//! Host-owned JSON encoding. No provider resources, requests or credentials.
use doremi_core::{
    media::{Media, MediaId, ProviderId},
    queue::{Queue, Repeat, checkpoint as core},
};
use serde::{Deserialize, Serialize};

pub const MAX_BYTES: usize = 1024 * 1024;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    TooLarge,
    Encoding,
    UnsupportedSchema,
    InvalidIdentity,
    InvalidQueue,
}
#[derive(Serialize, Deserialize)]
struct Entry {
    occurrence: u64,
    provider: String,
    id: String,
    title: String,
    duration_ms: Option<u64>,
}
#[derive(Serialize, Deserialize)]
struct Occurrence {
    entry: Entry,
    context_index: Option<usize>,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Mode {
    Off,
    Context,
    Track,
}
#[derive(Serialize, Deserialize)]
struct Document {
    schema: u32,
    current: Option<Occurrence>,
    manual: Vec<Entry>,
    context: Vec<Entry>,
    order: Vec<usize>,
    cursor: Option<usize>,
    history: Vec<Occurrence>,
    forward: Vec<Occurrence>,
    repeat: Mode,
    shuffled: bool,
    position_ms: u64,
    next_entry: u64,
    request_watermark: u64,
}
impl From<core::EntryRecord> for Entry {
    fn from(entry: core::EntryRecord) -> Self {
        Self {
            occurrence: entry.id,
            provider: entry.media.id.provider.as_str().into(),
            id: entry.media.id.local().into(),
            title: entry.media.title,
            duration_ms: entry.media.duration_ms,
        }
    }
}
impl TryFrom<Entry> for core::EntryRecord {
    type Error = Error;
    fn try_from(entry: Entry) -> Result<Self, Error> {
        let provider = ProviderId::new(entry.provider).map_err(|_| Error::InvalidIdentity)?;
        let id = MediaId::new(provider, entry.id).map_err(|_| Error::InvalidIdentity)?;
        Ok(Self {
            id: entry.occurrence,
            media: Media {
                id,
                title: entry.title,
                duration_ms: entry.duration_ms,
            },
        })
    }
}
impl From<core::Occurrence> for Occurrence {
    fn from(value: core::Occurrence) -> Self {
        Self {
            entry: value.entry.into(),
            context_index: value.context_index,
        }
    }
}
impl TryFrom<Occurrence> for core::Occurrence {
    type Error = Error;
    fn try_from(value: Occurrence) -> Result<Self, Error> {
        Ok(Self {
            entry: value.entry.try_into()?,
            context_index: value.context_index,
        })
    }
}
impl From<core::Checkpoint> for Document {
    fn from(s: core::Checkpoint) -> Self {
        Self {
            schema: 1,
            current: s.current.map(Into::into),
            manual: s.manual.into_iter().map(Into::into).collect(),
            context: s.context.into_iter().map(Into::into).collect(),
            order: s.order,
            cursor: s.cursor,
            history: s.history.into_iter().map(Into::into).collect(),
            forward: s.forward.into_iter().map(Into::into).collect(),
            repeat: match s.repeat {
                Repeat::Off => Mode::Off,
                Repeat::Context => Mode::Context,
                Repeat::Track => Mode::Track,
            },
            shuffled: s.shuffled,
            position_ms: s.position_ms,
            next_entry: s.next_entry,
            request_watermark: s.request_watermark,
        }
    }
}
impl TryFrom<Document> for core::Checkpoint {
    type Error = Error;
    fn try_from(s: Document) -> Result<Self, Error> {
        if s.schema != 1 {
            return Err(Error::UnsupportedSchema);
        }
        let checkpoint = Self {
            current: s.current.map(TryInto::try_into).transpose()?,
            manual: s
                .manual
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<_, _>>()?,
            context: s
                .context
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<_, _>>()?,
            order: s.order,
            cursor: s.cursor,
            history: s
                .history
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<_, _>>()?,
            forward: s
                .forward
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<_, _>>()?,
            repeat: match s.repeat {
                Mode::Off => Repeat::Off,
                Mode::Context => Repeat::Context,
                Mode::Track => Repeat::Track,
            },
            shuffled: s.shuffled,
            position_ms: s.position_ms,
            next_entry: s.next_entry,
            request_watermark: s.request_watermark,
        };
        checkpoint.validate().map_err(|_| Error::InvalidQueue)?;
        Ok(checkpoint)
    }
}
pub fn encode(queue: &Queue) -> Result<Vec<u8>, Error> {
    let checkpoint = queue.checkpoint();
    checkpoint.validate().map_err(|_| Error::InvalidQueue)?;
    let bytes =
        serde_json::to_vec_pretty(&Document::from(checkpoint)).map_err(|_| Error::Encoding)?;
    if bytes.len() > MAX_BYTES {
        return Err(Error::TooLarge);
    }
    Ok(bytes)
}
pub fn decode(bytes: &[u8]) -> Result<core::Checkpoint, Error> {
    if bytes.len() > MAX_BYTES {
        return Err(Error::TooLarge);
    }
    let document: Document = serde_json::from_slice(bytes).map_err(|_| Error::Encoding)?;
    document.try_into()
}

use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc,
    },
};
static NEXT_WRITE: AtomicUsize = AtomicUsize::new(1);
#[derive(Default)]
struct Status {
    completed: AtomicUsize,
    failed: AtomicBool,
}
struct Write {
    id: usize,
    prefs: Option<Vec<u8>>,
    queue: Option<Vec<u8>>,
    state: Vec<u8>,
}
impl Write {
    fn supersede(self, mut newer: Self) -> Self {
        // An omitted document means unchanged, not cancellation of an older save.
        if newer.prefs.is_none() {
            newer.prefs = self.prefs;
        }
        if newer.queue.is_none() {
            newer.queue = self.queue;
        }
        newer
    }
}
pub struct Writer {
    sender: mpsc::SyncSender<Write>,
    worker: std::thread::JoinHandle<()>,
    status: Arc<Status>,
}
#[derive(Clone, Copy, Debug)]
pub enum SubmitError {
    Busy,
    Closed,
}
impl Writer {
    pub fn new(directory: PathBuf) -> Self {
        let (sender, receiver) = mpsc::sync_channel::<Write>(1);
        let status = Arc::new(Status::default());
        let output = status.clone();
        let worker = std::thread::spawn(move || {
            let mut previous = std::collections::BTreeMap::new();
            while let Ok(mut write) = receiver.recv() {
                while let Ok(newer) = receiver.try_recv() {
                    write = write.supersede(newer);
                }
                let mut failed = false;
                for (name, bytes) in [
                    ("preferences-v1.json", write.prefs),
                    ("queue-v1.json", write.queue),
                    ("qa-state.json", Some(write.state)),
                ] {
                    let Some(bytes) = bytes else {
                        continue;
                    };
                    if previous.get(name) == Some(&bytes) {
                        continue;
                    }
                    match atomic_write(&directory, name, &bytes, name != "qa-state.json") {
                        Ok(()) => {
                            previous.insert(name, bytes);
                        }
                        Err(error) => {
                            failed = true;
                            eprintln!("Doremi persistence: {error}");
                        }
                    }
                }
                output.failed.store(failed, Ordering::Release);
                output.completed.store(write.id, Ordering::Release);
            }
        });
        Self {
            sender,
            worker,
            status,
        }
    }
    pub fn submit(
        &self,
        prefs: Option<Vec<u8>>,
        queue: Option<Vec<u8>>,
        state: Vec<u8>,
    ) -> Result<usize, SubmitError> {
        let id = NEXT_WRITE
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .map_err(|_| SubmitError::Closed)?;
        self.sender
            .try_send(Write {
                id,
                prefs,
                queue,
                state,
            })
            .map_err(|error| match error {
                mpsc::TrySendError::Full(_) => SubmitError::Busy,
                mpsc::TrySendError::Disconnected(_) => SubmitError::Closed,
            })?;
        Ok(id)
    }
    /// None means pending, Some(true) means a failed durable write.
    pub fn completed(&self, id: usize) -> Option<bool> {
        (self.status.completed.load(Ordering::Acquire) >= id)
            .then(|| self.status.failed.load(Ordering::Acquire))
    }
    /// Join only after the desktop event loop exits, never during drawing.
    pub fn shutdown(self) {
        drop(self.sender);
        let _ = self.worker.join();
    }
}
fn atomic_write(directory: &Path, name: &str, bytes: &[u8], durable: bool) -> std::io::Result<()> {
    use std::io::Write;
    std::fs::create_dir_all(directory)?;
    let path = directory.join(name);
    let temporary = path.with_extension("tmp");
    let mut file = std::fs::File::create(&temporary)?;
    file.write_all(bytes)?;
    if durable {
        file.sync_all()?;
    }
    std::fs::rename(temporary, path)?;
    #[cfg(unix)]
    if durable {
        std::fs::File::open(directory)?.sync_all()?;
    }
    Ok(())
}
pub fn read(directory: &Path, name: &str) -> Result<Option<Vec<u8>>, Error> {
    if !matches!(name, "preferences-v1.json" | "queue-v1.json") {
        return Err(Error::Encoding);
    }
    let path = directory.join(name);
    let metadata = match path.metadata() {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(Error::Encoding),
    };
    if metadata.len() > MAX_BYTES as u64 || !metadata.is_file() {
        return Err(Error::TooLarge);
    }
    std::fs::read(path).map(Some).map_err(|_| Error::Encoding)
}

#[cfg(test)]
mod storage_tests {
    use super::*;
    #[test]
    fn newer_diagnostics_cannot_discard_an_accepted_queue_save() {
        let save = Write {
            id: 1,
            prefs: Some(vec![1]),
            queue: Some(vec![2]),
            state: vec![3],
        };
        let diagnostics = Write {
            id: 2,
            prefs: None,
            queue: None,
            state: vec![4],
        };
        let merged = save.supersede(diagnostics);
        assert_eq!(merged.id, 2);
        assert_eq!(merged.prefs, Some(vec![1]));
        assert_eq!(merged.queue, Some(vec![2]));
        assert_eq!(merged.state, vec![4]);
        let update = Write {
            id: 3,
            prefs: Some(vec![5]),
            queue: Some(vec![6]),
            state: vec![7],
        };
        let merged = merged.supersede(update);
        assert_eq!(merged.prefs, Some(vec![5]));
        assert_eq!(merged.queue, Some(vec![6]));
    }
    fn directory(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target")
            .join(format!("{name}-{}", std::process::id()))
    }
    fn wait(writer: &Writer, id: usize) -> bool {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        loop {
            if let Some(failed) = writer.completed(id) {
                return failed;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "disk worker did not acknowledge"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
    #[test]
    fn an_omitted_document_preserves_unknown_future_data() {
        let directory = directory("storage-preserve-test");
        std::fs::create_dir_all(&directory).unwrap();
        let future = br#"{"schema":99,"future_data":"preserve"}"#;
        std::fs::write(directory.join("queue-v1.json"), future).unwrap();
        let writer = Writer::new(directory.clone());
        let id = writer.submit(None, None, b"{}".to_vec()).unwrap();
        assert!(!wait(&writer, id));
        writer.shutdown();
        assert_eq!(
            std::fs::read(directory.join("queue-v1.json")).unwrap(),
            future
        );
        assert!(!directory.join("queue-v1.tmp").exists());
        std::fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn worker_reports_real_write_failures_without_waiting_on_the_ui() {
        let directory = directory("storage-error-test");
        std::fs::write(&directory, b"not a directory").unwrap();
        let writer = Writer::new(directory.clone());
        let id = writer.submit(None, None, b"{}".to_vec()).unwrap();
        assert!(wait(&writer, id));
        writer.shutdown();
        assert_eq!(std::fs::read(&directory).unwrap(), b"not a directory");
        std::fs::remove_file(directory).unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use doremi_core::queue::{Command, Order, Playback};
    #[test]
    fn versioned_json_restores_duplicates_and_rejects_tampering() {
        let mut queue = Queue::default();
        let media: Vec<_> = (0..3)
            .map(|i| Media {
                id: MediaId::new(ProviderId::new("test").unwrap(), i.to_string()).unwrap(),
                title: format!("Café {i}"),
                duration_ms: Some(30_000),
            })
            .collect();
        queue
            .command(Command::StartContext {
                media: media.clone(),
                start: 1,
                order: Order::Sequential,
            })
            .unwrap();
        queue
            .command(Command::Append(vec![media[0].clone(), media[0].clone()]))
            .unwrap();
        queue.command(Command::Seek(4123)).unwrap();
        let bytes = encode(&queue).unwrap();
        let mut restored = Queue::default();
        restored
            .restore_checkpoint(decode(&bytes).unwrap())
            .unwrap();
        assert_eq!(restored.current(), queue.current());
        assert_eq!(restored.manual(), queue.manual());
        assert_eq!(restored.position_ms(), 4123);
        assert_eq!(restored.playback(), Playback::Paused);
        let mut json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        json["schema"] = 9.into();
        assert_eq!(
            decode(&serde_json::to_vec(&json).unwrap()),
            Err(Error::UnsupportedSchema)
        );
        json["schema"] = 1.into();
        json["manual"][1]["occurrence"] = json["manual"][0]["occurrence"].clone();
        assert_eq!(
            decode(&serde_json::to_vec(&json).unwrap()),
            Err(Error::InvalidQueue)
        );
        assert_eq!(decode(b"{truncated"), Err(Error::Encoding));
    }
}
