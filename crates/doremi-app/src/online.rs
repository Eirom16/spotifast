//! Linux host IO actor. Queue/playing intent remain solely in Model/core.
//! One search and one source future, bounded commands/results and two PCMs.
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

#[derive(Clone, Copy, Debug)]
pub(super) enum Error {
    Provider(ProviderError),
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
        result: Result<Arc<Pcm>, Error>,
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
    pub fn new(origin: Instant) -> Result<Self, ProviderError> {
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
                    .enable_all()
                    .build()
                {
                    Ok(runtime) => runtime.block_on(async move {
                        let client = commons::http::client();
                        actor(receiver, state, client, origin).await;
                    }),
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
        // Model is retained until after the desktop UI event loop exits.
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
async fn actor(
    mut receiver: mpsc::Receiver<Job>,
    shared: Arc<Shared>,
    client: Result<commons::Client, ProviderError>,
    origin: Instant,
) {
    let mut tasks: [Task; 2] = [None, None];
    let cache = Arc::new(Mutex::new(Vec::<(MediaId, Arc<Pcm>)>::new()));
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
                        let result = match transport {
                            Ok(client) => commons::search(&client, &request.query, request.limit)
                                .await
                                .map(|rows| {
                                    rows.into_iter()
                                        .filter(|row| {
                                            commons::audited(row.media.id.local()).is_some()
                                        })
                                        .map(|row| row.media)
                                        .collect()
                                }),
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
    client: Result<commons::Client, ProviderError>,
    media: &MediaId,
    generation: u64,
    state: &Shared,
    cache: &Mutex<Vec<(MediaId, Arc<Pcm>)>>,
    origin: Instant,
    ctx: &egui::Context,
) -> Result<Arc<Pcm>, Error> {
    if let Some(pcm) = cache
        .lock()
        .unwrap()
        .iter()
        .find(|(id, _)| id == media)
        .map(|(_, pcm)| pcm.clone())
    {
        return Ok(pcm);
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
    let mut buffers = cache.lock().unwrap();
    if buffers.len() >= 2 {
        return Err(ProviderError::TooLarge.into());
    }
    buffers.push((media.clone(), pcm.clone()));
    state.phase(generation, 4, ctx);
    Ok(pcm)
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
                Err(ProviderError::Transport),
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
