//! Linux file ingress. Only a requested path is inspected; no library scanning.
//! An owned native selector is bounded, cancellable and always reaped here.
use doremi_player::wav_stream::Wav;
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::PathBuf,
    process::{Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU8, AtomicU32, AtomicU64, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

const LIMIT: usize = doremi_library::local_files::MAX_PATH_BYTES;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Cancelled,
    Deadline,
    Input,
    Format,
    Name,
    Picker,
    Busy,
}
pub struct Metadata {
    pub path: PathBuf,
    pub title: String,
    pub duration_ms: u64,
}
pub enum Source {
    Picker(PathBuf),
    Path(PathBuf),
}
struct Job {
    id: u64,
    source: Source,
    ctx: egui::Context,
}
pub struct Event {
    pub id: u64,
    pub result: Result<Metadata, Error>,
}
#[derive(Default)]
struct Control {
    generation: AtomicU64,
    active: AtomicU64,
    phase: AtomicU8,
    picker: AtomicU32,
}
pub struct Worker {
    sender: Option<mpsc::SyncSender<Job>>,
    worker: Option<JoinHandle<()>>,
    control: Arc<Control>,
    event: Arc<Mutex<Option<Event>>>,
}
fn check(control: &Control, id: u64, deadline: Instant) -> Result<(), Error> {
    if control.generation.load(Ordering::Acquire) != id {
        Err(Error::Cancelled)
    } else if Instant::now() >= deadline {
        Err(Error::Deadline)
    } else {
        Ok(())
    }
}
struct Guarded<'a> {
    file: File,
    control: &'a Control,
    id: u64,
    deadline: Instant,
}
impl Guarded<'_> {
    fn check(&self) -> std::io::Result<()> {
        check(self.control, self.id, self.deadline)
            .map_err(|_| std::io::Error::other("File operation cancelled"))
    }
}
impl Read for Guarded<'_> {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        self.check()?;
        self.file.read(bytes)
    }
}
impl Seek for Guarded<'_> {
    fn seek(&mut self, at: SeekFrom) -> std::io::Result<u64> {
        self.check()?;
        self.file.seek(at)
    }
}
fn inspect(path: PathBuf, control: &Control, id: u64) -> Result<Metadata, Error> {
    let deadline = Instant::now() + Duration::from_secs(8);
    check(control, id, deadline)?;
    let text = path.to_str().ok_or(Error::Name)?;
    if text.len() > LIMIT || text.chars().any(char::is_control) {
        return Err(Error::Name);
    }
    let path = path.canonicalize().map_err(|_| Error::Input)?;
    check(control, id, deadline)?;
    let text = path.to_str().ok_or(Error::Name)?;
    if text.len() > LIMIT || text.chars().any(char::is_control) {
        return Err(Error::Name);
    }
    if !std::fs::metadata(&path)
        .map_err(|_| Error::Input)?
        .is_file()
    {
        return Err(Error::Input);
    }
    let file = File::open(&path).map_err(|_| Error::Input)?;
    if !file.metadata().map_err(|_| Error::Input)?.is_file() {
        return Err(Error::Input);
    }
    let wav = Wav::open(Guarded {
        file,
        control,
        id,
        deadline,
    });
    check(control, id, deadline)?;
    let info = wav.map_err(|_| Error::Format)?.info();
    let title = path
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or("Archivo WAV")
        .chars()
        .filter(|c| !c.is_control())
        .take(256)
        .collect();
    Ok(Metadata {
        path,
        title,
        duration_ms: info.duration_ms(),
    })
}
fn select(
    mut command: Command,
    control: &Control,
    id: u64,
    on_open: impl FnOnce(u32),
) -> Result<PathBuf, Error> {
    let deadline = Instant::now() + Duration::from_secs(600);
    check(control, id, deadline)?;
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| Error::Picker)?;
    control.picker.store(child.id(), Ordering::Release);
    on_open(child.id());
    let stdout = child.stdout.take().unwrap();
    let overflow = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let excess = overflow.clone();
    let reader = thread::Builder::new()
        .name("doremi-picker".into())
        .spawn(move || {
            let mut bytes = Vec::new();
            let result = stdout.take((LIMIT + 2) as u64).read_to_end(&mut bytes);
            if bytes.len() > LIMIT + 1 {
                excess.store(true, Ordering::Release);
            }
            (result, bytes)
        });
    let reader = match reader {
        Ok(reader) => reader,
        Err(_) => {
            let _ = child.kill();
            let _ = child.wait();
            control.picker.store(0, Ordering::Release);
            return Err(Error::Busy);
        }
    };
    let status = loop {
        if let Err(error) = check(control, id, deadline) {
            let _ = child.kill();
            let _ = child.wait();
            break Err(error);
        }
        if overflow.load(Ordering::Acquire) {
            let _ = child.kill();
            let _ = child.wait();
            break Err(Error::Name);
        }
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) => thread::sleep(Duration::from_millis(20)),
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                break Err(Error::Picker);
            }
        }
    };
    control.picker.store(0, Ordering::Release);
    let (read, mut bytes) = reader.join().map_err(|_| Error::Picker)?;
    let status = status?;
    read.map_err(|_| Error::Picker)?;
    if !status.success() {
        return Err(if status.code() == Some(1) {
            Error::Cancelled
        } else {
            Error::Picker
        });
    }
    if bytes.last() == Some(&b'\n') {
        bytes.pop();
    }
    if bytes.is_empty() || bytes.len() > LIMIT {
        return Err(Error::Name);
    }
    let path = String::from_utf8(bytes).map_err(|_| Error::Name)?;
    Ok(PathBuf::from(path))
}
impl Worker {
    pub fn new() -> Result<Self, Error> {
        let (sender, receiver) = mpsc::sync_channel::<Job>(1);
        let control = Arc::new(Control::default());
        let state = control.clone();
        let event = Arc::new(Mutex::new(None));
        let output = event.clone();
        let worker = thread::Builder::new()
            .name("doremi-files".into())
            .spawn(move || {
                while let Ok(job) = receiver.recv() {
                    state.active.store(job.id, Ordering::Release);
                    let result = (|| {
                        let path = match job.source {
                            Source::Picker(directory) => {
                                state.phase.store(1, Ordering::Release);
                                job.ctx.request_repaint();
                                check(&state, job.id, Instant::now() + Duration::from_secs(1))?;
                                std::fs::create_dir_all(&directory).map_err(|_| Error::Input)?;
                                let mut initial = directory.into_os_string();
                                initial.push("/");
                                let mut command = Command::new("zenity");
                                command
                                    .args([
                                        "--file-selection",
                                        "--title=Doremi: importar WAV",
                                        "--file-filter=WAV PCM16 | *.wav *.WAV",
                                        "--filename",
                                    ])
                                    .arg(initial)
                                    .env("GTK_USE_PORTAL", "0");
                                select(command, &state, job.id, |_| job.ctx.request_repaint())?
                            }
                            Source::Path(path) => path,
                        };
                        state.phase.store(2, Ordering::Release);
                        job.ctx.request_repaint();
                        inspect(path, &state, job.id)
                    })();
                    state.phase.store(0, Ordering::Release);
                    if state.generation.load(Ordering::Acquire) == job.id {
                        *output.lock().unwrap() = Some(Event { id: job.id, result });
                        job.ctx.request_repaint();
                    }
                }
            })
            .map_err(|_| Error::Busy)?;
        Ok(Self {
            sender: Some(sender),
            worker: Some(worker),
            control,
            event,
        })
    }
    pub fn start(&self, source: Source, ctx: &egui::Context) -> Result<u64, Error> {
        let id = self
            .control
            .generation
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| n.checked_add(1))
            .map_err(|_| Error::Busy)?
            + 1;
        self.sender
            .as_ref()
            .ok_or(Error::Busy)?
            .try_send(Job {
                id,
                source,
                ctx: ctx.clone(),
            })
            .map_err(|_| Error::Busy)?;
        Ok(id)
    }
    pub fn cancel(&self) {
        let _ = self
            .control
            .generation
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| n.checked_add(1));
    }
    pub fn poll(&self, ctx: &egui::Context) -> Option<Event> {
        match self.event.try_lock() {
            Ok(mut event) => event.take(),
            Err(_) => {
                ctx.request_repaint_after(Duration::from_millis(5));
                None
            }
        }
    }
    pub fn phase(&self, id: u64) -> (&'static str, u32) {
        if self.control.active.load(Ordering::Acquire) != id {
            return ("queued", 0);
        }
        match self.control.phase.load(Ordering::Acquire) {
            1 => ("selector", self.control.picker.load(Ordering::Acquire)),
            2 => ("metadata", 0),
            _ => ("finishing", 0),
        }
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.cancel();
        drop(self.sender.take());
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selected_wav_metadata_is_neutral_and_cancellation_precedes_any_file_read() {
        let control = Control::default();
        control.generation.store(1, Ordering::Release);
        assert!(matches!(
            inspect(PathBuf::from("never-read.wav"), &control, 0),
            Err(Error::Cancelled)
        ));
        let meta = inspect(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/audio/scale.wav"),
            &control,
            1,
        )
        .unwrap();
        assert_eq!(meta.duration_ms, 8000);
        assert_eq!(meta.title, "scale");
        assert!(meta.path.is_absolute());
    }
    #[test]
    fn a_cancelled_native_child_is_reaped_with_its_bounded_reader() {
        let control = Arc::new(Control::default());
        control.generation.store(1, Ordering::Release);
        let token = control.clone();
        let stop = thread::spawn(move || {
            thread::sleep(Duration::from_millis(80));
            token.generation.store(2, Ordering::Release);
        });
        let mut command = Command::new("sleep");
        command.arg("30");
        let started = Instant::now();
        let mut pid = 0;
        assert!(matches!(
            select(command, &control, 1, |n| pid = n),
            Err(Error::Cancelled)
        ));
        stop.join().unwrap();
        assert!(pid > 0);
        assert!(!PathBuf::from(format!("/proc/{pid}")).exists());
        assert_eq!(control.picker.load(Ordering::Acquire), 0);
        assert!(started.elapsed() < Duration::from_secs(3));
    }
}
