//! Shared Linux/Android host IO actor. Queue/playing intent remain solely in Model/core.
//! One search and one source future, bounded commands/results and source caches.
use doremi_commons as commons;
use doremi_core::{
    catalog::{ProviderError, SearchRequest, Tick},
    media::{Media, MediaId},
    queue::RequestId,
    requests::Ticket,
};
use doremi_player::{DecodeError, Pcm};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering},
};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
pub(super) enum Prepared {
    YouTube(Box<doremi_youtube::stream::Stream>),
    BufferedYouTube(Arc<[u8]>),
    Pcm(Arc<Pcm>),
    Stream(
        Box<(
            doremi_core::catalog::TemporaryResource,
            commons::StreamGrant,
        )>,
    ),
}

#[derive(Clone, Copy, Debug)]
pub(super) enum Error {
    Provider(ProviderError),
    YouTube(ProviderError),
    Decode(DecodeError),
}
impl From<ProviderError> for Error {
    fn from(value: ProviderError) -> Self {
        Self::Provider(value)
    }
}
impl Error {
    pub fn message(self) -> String {
        match self {
            Self::YouTube(ProviderError::AccessDenied)=>"YouTube Music denegó el acceso a esta pista. Comprueba la sesión y los permisos de la fuente.".into(),
            Self::YouTube(ProviderError::Unavailable)=>"YouTube Music indica que el contenido no está disponible.".into(),
            Self::YouTube(error)=>format!("No se pudo reproducir desde YouTube Music: {error:?}"),
            Self::Provider(error) => format!("Fuente online: {error:?}"),
            Self::Decode(error) => format!("No se pudo decodificar el audio: {error:?}"),
        }
    }
}
pub(super) enum Completion {
    Search {
        generation: u64,
        ticket: Ticket,
        result: Result<Vec<Media>, ProviderError>,
    },
    Prepared {
        generation: u64,
        request: RequestId,
        media: MediaId,
        result: Result<Prepared, Error>,
    },
}
pub(super) enum Job {
    Search {
        generation: u64,
        request: SearchRequest,
        context: egui::Context,
    },
    Source {
        generation: u64,
        request: RequestId,
        media: MediaId,
        context: egui::Context,
    },
}
impl Job {
    fn lane(&self) -> usize {
        match self {
            Self::Search { .. } => 0,
            Self::Source { .. } => 1,
        }
    }
    fn generation(&self) -> u64 {
        match self {
            Self::Search { generation, .. } | Self::Source { generation, .. } => *generation,
        }
    }
}
struct Shared {
    generation: [AtomicU64; 2],
    closed: AtomicBool,
    events: Mutex<[Option<Completion>; 2]>,
    phase_generation: AtomicU64,
    phase: AtomicU8,
}
impl Shared {
    fn current(&self, lane: usize, generation: u64) -> bool {
        !self.closed.load(Ordering::Acquire)
            && self.generation[lane].load(Ordering::Acquire) == generation
    }
    fn publish(&self, lane: usize, generation: u64, event: Completion, ctx: &egui::Context) {
        if self.current(lane, generation) {
            self.events.lock().unwrap()[lane] = Some(event);
            ctx.request_repaint();
        }
    }
    fn phase(&self, generation: u64, phase: u8, ctx: &egui::Context) {
        if self.current(1, generation) {
            self.phase.store(phase, Ordering::Release);
            self.phase_generation.store(generation, Ordering::Release);
            ctx.request_repaint();
        }
    }
}
pub(super) struct Worker {
    sender: Option<mpsc::Sender<Job>>,
    shared: Arc<Shared>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Worker {
    pub fn new(
        origin: Instant,
        youtube: super::youtube_session::SharedClient,
    ) -> Result<Self, ProviderError> {
        let (sender, receiver) = mpsc::channel(4);
        let shared = Arc::new(Shared {
            generation: [AtomicU64::new(0), AtomicU64::new(0)],
            closed: AtomicBool::new(false),
            events: Mutex::new([None, None]),
            phase_generation: AtomicU64::new(0),
            phase: AtomicU8::new(0),
        });
        let state = shared.clone();
        let thread = std::thread::Builder::new()
            .name("doremi-source".into())
            .spawn(move || {
                // Client construction loads trust roots only on this worker.
                match tokio::runtime::Builder::new_current_thread()
                    .max_blocking_threads(1)
                    .enable_all()
                    .build()
                {
                    Ok(runtime) => {
                        runtime.block_on(async move {
                            let client = Sources {
                                commons: commons::http::client(),
                                youtube,
                            };
                            actor(receiver, state, client, origin).await;
                        });
                        // Cancellation stops the download at the next read;
                        // never join a blocked HTTP read from host shutdown.
                        runtime.shutdown_timeout(Duration::from_millis(100));
                    }
                    Err(_) => fail_jobs(receiver, state),
                }
            })
            .map_err(|_| ProviderError::Transport)?;
        Ok(Self {
            sender: Some(sender),
            shared,
            thread: Some(thread),
        })
    }
    pub fn next(&self, lane: usize) -> Result<u64, ProviderError> {
        self.shared.generation[lane]
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| n.checked_add(1))
            .map(|old| old + 1)
            .map_err(|_| {
                self.shared.closed.store(true, Ordering::Release);
                ProviderError::Exhausted
            })
    }
    pub fn current(&self, lane: usize, generation: u64) -> bool {
        self.shared.current(lane, generation)
    }
    pub fn send(&self, job: Job) -> bool {
        self.sender.as_ref().unwrap().try_send(job).is_ok()
    }
    pub fn poll(&self) -> [Option<Completion>; 2] {
        self.shared
            .events
            .try_lock()
            .map(|mut slots| [slots[0].take(), slots[1].take()])
            .unwrap_or([None, None])
    }
    pub fn phase(&self, generation: u64) -> &'static str {
        if self.shared.phase_generation.load(Ordering::Acquire) != generation {
            return "queued";
        }
        match self.shared.phase.load(Ordering::Acquire) {
            1 => "resolve",
            2 => "download",
            3 => "decode",
            4 => "prepared",
            _ => "idle",
        }
    }
    #[cfg(test)]
    pub fn inject_completion(&self, lane: usize, event: Completion) {
        self.shared.events.lock().unwrap()[lane] = Some(event);
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        // Model/worker survive Android Activity lifetimes; desktop drops after its UI loop.
        self.shared.closed.store(true, Ordering::Release);
        self.sender.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
type Task = Option<(u64, tokio::task::JoinHandle<()>)>;
async fn stop(task: &mut Task) {
    if let Some((_, handle)) = task.take() {
        handle.abort();
        let _ = handle.await;
    }
}
fn fail_jobs(mut receiver: mpsc::Receiver<Job>, shared: Arc<Shared>) {
    while let Some(job) = receiver.blocking_recv() {
        match job {
            Job::Search {
                generation,
                request,
                context,
            } => shared.publish(
                0,
                generation,
                Completion::Search {
                    generation,
                    ticket: request.ticket,
                    result: Err(ProviderError::Transport),
                },
                &context,
            ),
            Job::Source {
                generation,
                request,
                media,
                context,
            } => shared.publish(
                1,
                generation,
                Completion::Prepared {
                    generation,
                    request,
                    media,
                    result: Err(Error::Provider(ProviderError::Transport)),
                },
                &context,
            ),
        }
    }
}
#[derive(Clone)]
struct Sources {
    commons: Result<commons::Client, ProviderError>,
    youtube: super::youtube_session::SharedClient,
}
#[derive(Default)]
struct SourceCache {
    pcm: Mutex<Vec<(MediaId, Arc<Pcm>)>>,
    youtube: Mutex<EncodedCache>,
}

/// Two complete encoded songs (each bounded by rescue::MAX_BYTES), shared with
/// the decoder. Seeks and previous/repeat reuse bytes without copying on the UI.
#[derive(Default)]
struct EncodedCache(Vec<(MediaId, Arc<[u8]>)>);
impl EncodedCache {
    fn get(&mut self, media: &MediaId) -> Option<Arc<[u8]>> {
        let index = self.0.iter().position(|(id, _)| id == media)?;
        let entry = self.0.remove(index);
        let bytes = entry.1.clone();
        self.0.push(entry);
        Some(bytes)
    }

    fn insert(&mut self, media: MediaId, bytes: Arc<[u8]>) {
        self.0.retain(|(id, _)| *id != media);
        if self.0.len() >= 2 {
            self.0.remove(0);
        }
        self.0.push((media, bytes));
    }
}
async fn actor(
    mut receiver: mpsc::Receiver<Job>,
    shared: Arc<Shared>,
    client: Sources,
    origin: Instant,
) {
    let mut tasks: [Task; 2] = [None, None];
    let cache = Arc::new(SourceCache::default());
    loop {
        if shared.closed.load(Ordering::Acquire) {
            break;
        }
        for (lane, task) in tasks.iter_mut().enumerate() {
            if task.as_ref().is_some_and(|(generation, handle)| {
                !shared.current(lane, *generation) || handle.is_finished()
            }) {
                stop(task).await;
            }
        }
        let job = if tasks.iter().any(Option::is_some) {
            tokio::select! { job=receiver.recv()=>job,_=tokio::time::sleep(Duration::from_millis(20))=>continue }
        } else {
            receiver.recv().await
        };
        let Some(job) = job else { break };
        let lane = job.lane();
        let generation = job.generation();
        if !shared.current(lane, generation) {
            continue;
        }
        stop(&mut tasks[lane]).await;
        let state = shared.clone();
        let transport = client.clone();
        let buffers = cache.clone();
        tasks[lane] = Some((
            generation,
            tokio::spawn(async move {
                match job {
                    Job::Search {
                        generation,
                        request,
                        context,
                    } => {
                        let result = match transport.commons {
                            Ok(client) => commons::search(&client, &request.query, request.limit)
                                .await
                                .map(|rows| rows.into_iter().map(|row| row.media).collect()),
                            Err(error) => Err(error),
                        };
                        state.publish(
                            0,
                            generation,
                            Completion::Search {
                                generation,
                                ticket: request.ticket,
                                result,
                            },
                            &context,
                        );
                    }
                    Job::Source {
                        generation,
                        request,
                        media,
                        context,
                    } => {
                        let result = source(
                            transport, &media, generation, &state, &buffers, origin, &context,
                        )
                        .await;
                        state.publish(
                            1,
                            generation,
                            Completion::Prepared {
                                generation,
                                request,
                                media,
                                result,
                            },
                            &context,
                        );
                    }
                }
            }),
        ));
    }
    for task in &mut tasks {
        stop(task).await;
    }
}
async fn source(
    client: Sources,
    media: &MediaId,
    generation: u64,
    state: &Shared,
    cache: &SourceCache,
    origin: Instant,
    ctx: &egui::Context,
) -> Result<Prepared, Error> {
    if media.provider.as_str() == doremi_youtube::PROVIDER {
        let legacy = std::env::var("DOREMI_PLAYBACK_BACKEND").as_deref() == Ok("legacy");
        // A queue seek reloads the current item; the whole encoded song is
        // already in memory, so reuse it instead of fetching it again.
        if !legacy && let Some(bytes) = cache.youtube.lock().unwrap().get(media) {
            return Ok(Prepared::BufferedYouTube(bytes));
        }
        state.phase(generation, 1, ctx);
        let result = match client
            .youtube
            .lock()
            .await
            .get_or_insert_with(doremi_youtube::Client::new)
        {
            Ok(client) => {
                // Rescue is the default on both platforms. The previous HTTP
                // range source remains selectable without changing UI or data.
                let result = if legacy {
                    client
                        .resolve(media)
                        .await
                        .map(|stream| Prepared::YouTube(Box::new(stream)))
                } else {
                    state.phase(generation, 2, ctx);
                    doremi_youtube::rescue::prepare(client, media)
                        .await
                        .map(|audio| {
                            let bytes: Arc<[u8]> = audio.bytes.into();
                            cache
                                .youtube
                                .lock()
                                .unwrap()
                                .insert(media.clone(), bytes.clone());
                            Prepared::BufferedYouTube(bytes)
                        })
                };
                if result.is_err() {
                    let message =
                        format!("YTM resolve failure stage={}", client.resolution_stage());
                    #[cfg(target_os = "android")]
                    log::warn!("{message}");
                    #[cfg(not(target_os = "android"))]
                    eprintln!("{message}");
                }
                if result.is_err()
                    && let Some(d) = client.player_diagnostic()
                {
                    let message = format!(
                        "YTM player failure http={} api={:?} status={} reason={} aac={} direct={} cipher={} drm={}",
                        d.http_status,
                        d.api_code,
                        d.status,
                        d.reason,
                        d.aac_formats,
                        d.direct_aac,
                        d.ciphered_aac,
                        d.drm
                    );
                    #[cfg(target_os = "android")]
                    log::warn!("{message}");
                    #[cfg(not(target_os = "android"))]
                    eprintln!("{message}");
                }
                result
            }
            Err(error) => Err(*error),
        };
        return result.map_err(Error::YouTube);
    }
    let client = client.commons;
    if commons::candidate(media) && commons::audited(media.local()).is_none() {
        state.phase(generation, 1, ctx);
        let info = commons::resolve(&client?, media).await?;
        let resource = commons::temporary(&info, Tick(origin.elapsed().as_millis() as u64))?;
        let grant = info.stream.ok_or(ProviderError::UnsupportedFormat)?;
        return Ok(Prepared::Stream(Box::new((resource, grant))));
    }
    if let Some(pcm) = cache
        .pcm
        .lock()
        .unwrap()
        .iter()
        .find(|(id, _)| id == media)
        .map(|(_, pcm)| pcm.clone())
    {
        return Ok(Prepared::Pcm(pcm));
    }
    let client = client?;
    state.phase(generation, 1, ctx);
    let info = commons::resolve(&client, media).await?;
    let now = || Tick(origin.elapsed().as_millis() as u64);
    let resource = commons::temporary(&info, now())?;
    state.phase(generation, 2, ctx);
    let bytes = commons::download(&client, &resource, now()).await?;
    state.phase(generation, 3, ctx);
    let pcm = Arc::new(
        doremi_player::decode_ogg_cancellable(&bytes, || !state.current(1, generation))
            .map_err(Error::Decode)?,
    );
    if !state.current(1, generation) {
        return Err(ProviderError::Cancelled.into());
    }
    let mut buffers = cache.pcm.lock().unwrap();
    if buffers.len() >= 2 {
        return Err(ProviderError::TooLarge.into());
    }
    buffers.push((media.clone(), pcm.clone()));
    state.phase(generation, 4, ctx);
    Ok(Prepared::Pcm(pcm))
}

#[cfg(test)]
mod cache_tests {
    use super::*;

    #[test]
    fn encoded_cache_reuses_recent_songs_and_evicts_the_least_recent() {
        let first = doremi_youtube::id("qQzdAsjWGPg").unwrap();
        let second = doremi_youtube::id("_d3SaQI_Ag4").unwrap();
        let third = doremi_youtube::id("aaaaaaaaaaa").unwrap();
        let bytes: Arc<[u8]> = vec![1, 2, 3].into();
        let mut cache = EncodedCache::default();
        cache.insert(first.clone(), bytes.clone());
        cache.insert(second.clone(), vec![4].into());
        assert!(Arc::ptr_eq(&cache.get(&first).unwrap(), &bytes));
        cache.insert(third.clone(), vec![5].into());
        assert!(cache.get(&second).is_none());
        assert!(Arc::ptr_eq(&cache.get(&first).unwrap(), &bytes));
        assert!(cache.get(&third).is_some());
        // Replacing an entry must release its old buffer and keep the bound.
        cache.insert(first.clone(), vec![6].into());
        assert_eq!(cache.get(&first).unwrap().as_ref(), &[6]);
        assert_eq!(Arc::strong_count(&bytes), 1);
        assert_eq!(cache.0.len(), 2);
    }

    #[test]
    fn seek_reload_uses_encoded_audio_without_resolving_again() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let cache = SourceCache::default();
            let media = doremi_youtube::id("qQzdAsjWGPg").unwrap();
            let bytes: Arc<[u8]> = vec![1, 2, 3].into();
            cache
                .youtube
                .lock()
                .unwrap()
                .insert(media.clone(), bytes.clone());
            let state = Shared {
                generation: [AtomicU64::new(0), AtomicU64::new(2)],
                closed: AtomicBool::new(false),
                events: Mutex::new([None, None]),
                phase_generation: AtomicU64::new(0),
                phase: AtomicU8::new(0),
            };
            // Both transports fail if touched: the new load generation must
            // succeed solely from the previous load's prepared audio.
            let client = Sources {
                commons: Err(ProviderError::Transport),
                youtube: Arc::new(tokio::sync::Mutex::new(Some(Err(ProviderError::Transport)))),
            };
            let prepared = source(
                client,
                &media,
                2,
                &state,
                &cache,
                Instant::now(),
                &egui::Context::default(),
            )
            .await
            .unwrap();
            let Prepared::BufferedYouTube(reused) = prepared else {
                panic!("seek did not reuse encoded audio");
            };
            assert!(Arc::ptr_eq(&reused, &bytes));
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use doremi_core::{
        catalog::{Capabilities, Catalog, ProviderCommand},
        media::ProviderId,
        requests::SessionId,
    };
    fn runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
    }
    #[test]
    fn stale_queued_search_is_skipped_and_actor_joins_without_network() {
        runtime().block_on(async {
            let shared = Arc::new(Shared {
                generation: [AtomicU64::new(2), AtomicU64::new(0)],
                closed: AtomicBool::new(false),
                events: Mutex::new([None, None]),
                phase_generation: AtomicU64::new(0),
                phase: AtomicU8::new(0),
            });
            let mut catalog = Catalog::new(
                SessionId(8),
                Capabilities {
                    provider: ProviderId::new(commons::PROVIDER).unwrap(),
                    search: true,
                    resolve: false,
                    max_results: 2,
                    formats: vec![],
                },
            )
            .unwrap();
            let (sender, receiver) = mpsc::channel(4);
            for generation in 1..=2 {
                let request = catalog
                    .search("sine".into(), 2, Tick(generation), 1000)
                    .unwrap()
                    .into_iter()
                    .find_map(|effect| match effect {
                        ProviderCommand::Search(request) => Some(request),
                        _ => None,
                    })
                    .unwrap();
                sender
                    .send(Job::Search {
                        generation,
                        request,
                        context: egui::Context::default(),
                    })
                    .await
                    .unwrap_or_else(|_| panic!("actor channel closed"));
            }
            let task = tokio::spawn(actor(
                receiver,
                shared.clone(),
                Sources {
                    commons: Err(ProviderError::Transport),
                    youtube: Arc::new(tokio::sync::Mutex::new(None)),
                },
                Instant::now(),
            ));
            tokio::time::timeout(Duration::from_secs(2), async {
                loop {
                    if shared.events.lock().unwrap()[0].is_some() {
                        break;
                    }
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            assert!(matches!(
                shared.events.lock().unwrap()[0],
                Some(Completion::Search {
                    generation: 2,
                    result: Err(ProviderError::Transport),
                    ..
                })
            ));
            drop(sender);
            tokio::time::timeout(Duration::from_secs(2), task)
                .await
                .unwrap()
                .unwrap();
        });
    }
    #[test]
    fn cancelling_a_pending_future_drops_its_resources_before_join_returns() {
        struct Guard(Arc<AtomicBool>);
        impl Drop for Guard {
            fn drop(&mut self) {
                self.0.store(true, Ordering::Release);
            }
        }
        runtime().block_on(async {
            let dropped = Arc::new(AtomicBool::new(false));
            let ready = Arc::new(AtomicBool::new(false));
            let own = dropped.clone();
            let started = ready.clone();
            let mut task = Some((
                1,
                tokio::spawn(async move {
                    let _guard = Guard(own);
                    started.store(true, Ordering::Release);
                    std::future::pending::<()>().await;
                }),
            ));
            while !ready.load(Ordering::Acquire) {
                tokio::task::yield_now().await;
            }
            stop(&mut task).await;
            assert!(task.is_none() && dropped.load(Ordering::Acquire));
        });
    }
}
