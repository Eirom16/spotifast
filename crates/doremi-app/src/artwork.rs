//! One bounded artwork worker, retained across Android Activities. Only CPU
//! pixels survive a GL context; texture handles belong to the current view.
use super::*;
use doremi_core::{catalog::ProviderError, media::EntityKind};
use doremi_musicbrainz::cover_art;
use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, Receiver, SyncSender, TrySendError},
    },
    thread::JoinHandle,
};

// At 250×250 RGBA this bounds decoded artwork to 6 MiB, plus GPU textures.
const CAPACITY: usize = 24;
struct Job {
    generation: u64,
    id: MediaId,
    url: Option<String>,
    ctx: egui::Context,
}
struct Pixels {
    image: Arc<egui::ColorImage>,
    expires: Instant,
    reusable: bool,
}
struct Completion {
    generation: u64,
    result: Result<Pixels, ProviderError>,
}
struct Worker {
    commands: SyncSender<Job>,
    events: Receiver<Completion>,
    generation: Arc<AtomicU64>,
    closed: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}
impl Worker {
    fn new() -> Result<Self, ProviderError> {
        let (commands, receive) = mpsc::sync_channel::<Job>(1);
        let (send, events) = mpsc::sync_channel(1);
        let generation = Arc::new(AtomicU64::new(0));
        let closed = Arc::new(AtomicBool::new(false));
        let active = generation.clone();
        let stopped = closed.clone();
        let thread = std::thread::Builder::new()
            .name("doremi-artwork".into())
            .spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .max_blocking_threads(1)
                    .enable_all()
                    .build();
                let client = cover_art::Client::new();
                'jobs: while let Ok(job) = receive.recv() {
                    let cancelled = || {
                        stopped.load(Ordering::Acquire)
                            || job.generation != active.load(Ordering::Acquire)
                    };
                    if cancelled() {
                        continue;
                    }
                    let result = client.as_ref().map_err(|error| *error).and_then(|client| {
                        let source = runtime
                            .as_ref()
                            .map_err(|_| ProviderError::Unavailable)?
                            .block_on(async {
                                if let Some(url)=&job.url {
                                    tokio::select! {
                                        result=doremi_youtube::artwork(url)=>result.map(|bytes|cover_art::Image {bytes,lifetime:Duration::from_secs(300),reusable:true}),
                                        _=async {while !cancelled() {tokio::time::sleep(Duration::from_millis(20)).await;}}=>Err(ProviderError::Cancelled),
                                    }
                                } else {client.fetch(&job.id, cancelled).await}
                            })?;
                        let image = doremi_ui::bitmap::decode(
                            &source.bytes,
                            doremi_ui::bitmap::DecodeLimits {
                                encoded_bytes: cover_art::MAX_BYTES,
                                side: 1024,
                                allocation_bytes: 8 * 1024 * 1024,
                            },
                        )
                        .map_err(|_| ProviderError::UnsupportedFormat)?;
                        if cancelled() {
                            return Err(ProviderError::Cancelled);
                        }
                        Ok(Pixels {
                            image: Arc::new(doremi_ui::bitmap::scaled(image, cover_art::PIXELS)),
                            expires: Instant::now() + source.lifetime,
                            reusable: source.reusable,
                        })
                    });
                    let mut event = Completion {
                        generation: job.generation,
                        result,
                    };
                    while !cancelled() {
                        match send.try_send(event) {
                            Ok(()) => {
                                job.ctx.request_repaint();
                                break;
                            }
                            Err(TrySendError::Full(pending)) => {
                                event = pending;
                                job.ctx.request_repaint();
                                std::thread::sleep(Duration::from_millis(20));
                            }
                            Err(TrySendError::Disconnected(_)) => break 'jobs,
                        }
                    }
                }
                // OS DNS may outlive a cancelled async request. Its pool has one
                // thread; never wait indefinitely for it during desktop shutdown.
                drop(client);
                if let Ok(runtime) = runtime {
                    runtime.shutdown_timeout(Duration::from_millis(100));
                }
            })
            .map_err(|_| ProviderError::Unavailable)?;
        Ok(Self {
            commands,
            events,
            generation,
            closed,
            thread: Some(thread),
        })
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.closed.store(true, Ordering::Release);
        // No join on a drawing/lifecycle thread. Desktop explicitly shuts down
        // after the event loop. Android retains this owner while the process lives.
    }
}
struct Cached {
    id: MediaId,
    version: u64,
    image: Arc<egui::ColorImage>,
    expires: Instant,
}
pub(super) struct State {
    selected: Option<MediaId>,
    url: Option<String>,
    generation: u64,
    version: u64,
    pixels: Option<Arc<egui::ColorImage>>,
    error: Option<ProviderError>,
    waiting: bool,
    pending: bool,
    cache_hit: bool,
    cache: VecDeque<Cached>,
    requested: Vec<(MediaId, String)>,
    failures: VecDeque<(MediaId, Instant)>,
    worker: Option<Worker>,
}
impl State {
    pub fn new() -> Self {
        Self {
            selected: None,
            url: None,
            generation: 0,
            version: 0,
            pixels: None,
            error: None,
            waiting: false,
            pending: false,
            cache_hit: false,
            cache: VecDeque::new(),
            requested: Vec::new(),
            failures: VecDeque::new(),
            worker: None,
        }
    }
    /// Visible rows register work; selection and HTTP happen on the next poll.
    pub fn request_thumbnail(&mut self, id: &MediaId, url: Option<String>, ctx: &egui::Context) {
        let Some(url) = url else { return };
        if self.requested.len() < CAPACITY && self.requested.iter().all(|(known, _)| known != id) {
            self.requested.push((id.clone(), url));
            if self.needs_thumbnail(id) {
                ctx.request_repaint_after(Duration::from_millis(50));
            }
        }
    }
    fn needs_thumbnail(&self, id: &MediaId) -> bool {
        let now = Instant::now();
        !self
            .cache
            .iter()
            .any(|item| item.id == *id && item.expires > now)
            && !self
                .failures
                .iter()
                .any(|(known, expiry)| known == id && *expiry > now)
    }
    fn next_thumbnail(&mut self) -> Option<(MediaId, String)> {
        let requested = std::mem::take(&mut self.requested);
        // Finish the visible in-flight image instead of restarting it every frame.
        if self.waiting
            && let Some(current) = requested
                .iter()
                .find(|(id, _)| Some(id) == self.selected.as_ref())
        {
            return Some(current.clone());
        }
        requested
            .into_iter()
            .find(|(id, _)| self.needs_thumbnail(id))
    }
    pub fn select(&mut self, id: Option<MediaId>) {
        if self.selected == id {
            return;
        }
        self.generation = self
            .generation
            .checked_add(1)
            .expect("artwork generation exhausted");
        if let Some(worker) = &self.worker {
            worker.generation.store(self.generation, Ordering::Release);
        }
        self.selected = id;
        self.pixels = None;
        self.error = None;
        self.waiting = self.selected.is_some();
        self.pending = self.waiting;
        self.cache_hit = false;
        self.cache.retain(|item| item.expires > Instant::now());
        if let Some(index) = self
            .cache
            .iter()
            .position(|item| Some(&item.id) == self.selected.as_ref())
        {
            let item = self.cache.remove(index).unwrap();
            self.pixels = Some(item.image.clone());
            self.version = item.version;
            self.cache.push_back(item);
            self.waiting = false;
            self.pending = false;
            self.cache_hit = true;
        }
    }
    pub fn poll(&mut self, ctx: &egui::Context) {
        if self.pending && self.worker.is_none() {
            match Worker::new() {
                Ok(worker) => {
                    worker.generation.store(self.generation, Ordering::Release);
                    self.worker = Some(worker);
                }
                Err(error) => {
                    self.error = Some(error);
                    self.pending = false;
                    self.waiting = false;
                }
            }
        }
        let Some(worker) = &self.worker else { return };
        // At most one completion and one queued command. A stale completion
        // cannot replace the pixels belonging to a different selected release.
        if let Ok(event) = worker.events.try_recv()
            && event.generation == self.generation
        {
            self.waiting = false;
            match event.result {
                Ok(pixels) => {
                    self.version = self.generation;
                    self.pixels = Some(pixels.image.clone());
                    if pixels.reusable && pixels.expires > Instant::now() {
                        let id = self.selected.clone().unwrap();
                        self.cache.retain(|item| item.id != id);
                        if self.cache.len() == CAPACITY {
                            self.cache.pop_front();
                        }
                        self.cache.push_back(Cached {
                            id,
                            version: self.version,
                            image: pixels.image,
                            expires: pixels.expires,
                        });
                    }
                }
                Err(error) => {
                    self.error = Some(error);
                    if error != ProviderError::Cancelled {
                        let id = self.selected.clone().unwrap();
                        self.failures
                            .retain(|(known, expiry)| *known != id && *expiry > Instant::now());
                        if self.failures.len() == CAPACITY {
                            self.failures.pop_front();
                        }
                        self.failures
                            .push_back((id, Instant::now() + Duration::from_secs(300)));
                    }
                }
            }
        }
        if self.pending {
            let job = Job {
                generation: self.generation,
                id: self.selected.clone().unwrap(),
                url: self.url.clone(),
                ctx: ctx.clone(),
            };
            match worker.commands.try_send(job) {
                Ok(()) => self.pending = false,
                Err(TrySendError::Full(_)) => {}
                Err(TrySendError::Disconnected(_)) => {
                    self.pending = false;
                    self.waiting = false;
                    self.error = Some(ProviderError::Unavailable);
                }
            }
        }
        if self.waiting {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
    }
    pub fn snapshot(&self) -> serde_json::Value {
        serde_json::json!({
            "id": self.selected.as_ref().map(MediaId::local), "generation": self.generation,
            "waiting": self.waiting, "dimensions": self.pixels.as_ref().map(|image| image.size),
            "cache_hit": self.cache_hit, "cached_images": self.cache.len(),
            "error": self.error.map(|error| format!("{error:?}")),
        })
    }
    pub fn caption(&self, expected: &MediaId) -> &'static str {
        if self
            .cache
            .iter()
            .any(|item| item.id == *expected && item.expires > Instant::now())
            || (self.selected.as_ref() == Some(expected) && self.pixels.is_some())
        {
            if expected.provider.as_str() == doremi_youtube::PROVIDER {
                "Carátula: YouTube Music · derechos de sus autores"
            } else {
                "Carátula: Cover Art Archive · derechos de sus autores"
            }
        } else if self
            .failures
            .iter()
            .any(|(id, expiry)| id == expected && *expiry > Instant::now())
        {
            "No se pudo cargar la carátula de esta edición."
        } else if self.selected.as_ref() != Some(expected) || self.waiting {
            "Cargando carátula…"
        } else if self.error == Some(ProviderError::Unavailable) {
            "Esta edición no tiene carátula disponible."
        } else {
            "No se pudo cargar la carátula de esta edición."
        }
    }
    pub fn shutdown(&mut self) {
        self.select(None);
        if let Some(mut worker) = self.worker.take() {
            worker.closed.store(true, Ordering::Release);
            // Wake an idle receiver by dropping the sender before joining.
            let thread = worker.thread.take();
            drop(worker);
            if let Some(thread) = thread {
                let _ = thread.join();
            }
        }
    }
}

#[derive(Default)]
pub(super) struct Textures(VecDeque<(MediaId, u64, egui::TextureHandle)>);
impl Textures {
    pub fn prune(&mut self, state: &State) {
        self.0.retain(|(id, version, _)| {
            (state.selected.as_ref() == Some(id)
                && state.version == *version
                && state.pixels.is_some())
                || state.cache.iter().any(|item| {
                    item.id == *id && item.version == *version && item.expires > Instant::now()
                })
        });
    }
    pub fn current(
        &mut self,
        state: &State,
        expected: &MediaId,
        ctx: &egui::Context,
    ) -> Option<egui::TextureHandle> {
        let (pixels, version) = match (state.selected.as_ref(), &state.pixels) {
            (Some(selected), Some(pixels)) if selected == expected => (pixels, state.version),
            _ => {
                let cached = state
                    .cache
                    .iter()
                    .find(|item| item.id == *expected && item.expires > Instant::now())?;
                (&cached.image, cached.version)
            }
        };
        let id = expected;
        if let Some(index) = self
            .0
            .iter()
            .position(|(key, known_version, _)| key == id && *known_version == version)
        {
            let item = self.0.remove(index).unwrap();
            let handle = item.2.clone();
            self.0.push_back(item);
            return Some(handle);
        }
        self.0.retain(|(key, _, _)| key != id);
        let handle = ctx.load_texture(
            "release-cover",
            egui::ImageData::Color(pixels.clone()),
            doremi_ui::bitmap::COVER_TEXTURE,
        );
        if self.0.len() == CAPACITY {
            self.0.pop_front();
        }
        self.0.push_back((id.clone(), version, handle.clone()));
        Some(handle)
    }
}

impl DoremiApp {
    pub(super) fn poll_artwork(&mut self, ctx: &egui::Context) {
        let model = self.model.as_mut().unwrap();
        let catalog_visible =
            model.page == Page::Search || model.page == Page::Home && model.music.home_root();
        let detail = (catalog_visible && model.music.selected)
            .then_some(model.music.detail.as_ref())
            .flatten()
            .filter(|entry| {
                entry.kind == EntityKind::Release
                    || entry.media.id.provider.as_str() == doremi_youtube::PROVIDER
            })
            .map(|entry| entry.media.id.clone());
        let url = if let Some(id) = detail
            .as_ref()
            .filter(|id| id.provider.as_str() == doremi_youtube::PROVIDER)
        {
            match youtube_detail_url(&model.music.session.client, id, model.music.waiting) {
                std::task::Poll::Ready(url) => url,
                std::task::Poll::Pending => {
                    // A catalog request owns the shared client until its
                    // metadata arrives. That is not a missing/refused image:
                    // do not send a URL-less YouTube ID to Cover Art Archive
                    // and poison the image cooldown before the browse ends.
                    ctx.request_repaint_after(Duration::from_millis(50));
                    return;
                }
            }
        } else {
            None
        };
        let thumbnail = model.artwork.next_thumbnail();
        let (id, url) = if detail.as_ref().is_some_and(|id| {
            id.provider.as_str() != doremi_youtube::PROVIDER || model.artwork.needs_thumbnail(id)
        }) {
            (detail, url)
        } else {
            // Player, queue, library and history register visible thumbnails
            // too. Offscreen work is still dropped by next_thumbnail().
            thumbnail.map_or((None, None), |(id, url)| (Some(id), Some(url)))
        };
        if model.artwork.selected == id && model.artwork.url != url {
            model.artwork.select(None);
        }
        model.artwork.select(id);
        model.artwork.url = url;
        model.artwork.poll(ctx);
        self.artwork_textures.prune(&model.artwork);
    }
}

fn youtube_detail_url(
    client: &super::youtube_session::SharedClient,
    id: &MediaId,
    metadata_waiting: bool,
) -> std::task::Poll<Option<String>> {
    let Ok(client) = client.try_lock() else {
        return std::task::Poll::Pending;
    };
    let url = client
        .as_ref()
        .and_then(|client| client.as_ref().ok())
        .and_then(|client| client.thumbnail(id));
    if url.is_none() && metadata_waiting {
        std::task::Poll::Pending
    } else {
        std::task::Poll::Ready(url)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::UI_TEST_LOCK;

    #[test]
    fn visible_music_images_dispatch_outside_the_catalog_without_offscreen_prefetch() {
        let _guard = UI_TEST_LOCK.lock().unwrap();
        DoremiApp::shutdown();
        let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target")
            .join(format!("visible-music-images-{}", std::process::id()));
        assert!(!directory.exists());
        std::fs::create_dir(&directory).unwrap();
        let ctx = egui::Context::default();
        let mut app = DoremiApp::new_context(&ctx, directory.clone());
        let id = doremi_youtube::id("abcdefghijk").unwrap();
        for page in [
            Page::Player,
            Page::Queue,
            Page::Library,
            Page::History,
            Page::Playlist,
        ] {
            let (commands, requests) = mpsc::sync_channel(1);
            let (_send, events) = mpsc::sync_channel(1);
            let model = app.model.as_mut().unwrap();
            model.page = page;
            model.music.selected = false;
            model.artwork = State::new();
            model.artwork.worker = Some(Worker {
                commands,
                events,
                generation: Arc::new(AtomicU64::new(0)),
                closed: Arc::new(AtomicBool::new(false)),
                thread: None,
            });
            model.artwork.request_thumbnail(
                &id,
                Some("https://i.ytimg.com/fixture.jpg".into()),
                &ctx,
            );
            app.poll_artwork(&ctx);
            assert_eq!(requests.try_recv().unwrap().id, id);
            // No visible request on the next frame: abandon the pending image.
            app.poll_artwork(&ctx);
            assert!(app.model.as_ref().unwrap().artwork.selected.is_none());
            assert!(requests.try_recv().is_err());
        }
        drop(app);
        DoremiApp::shutdown();
        std::fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn browsing_keeps_an_unknown_hero_source_pending_without_blocking_the_ui() {
        let client = Arc::new(tokio::sync::Mutex::new(None));
        let id = doremi_youtube::id("MPREfixture").unwrap();
        let catalog_guard = client.try_lock().unwrap();
        assert!(youtube_detail_url(&client, &id, false).is_pending());
        drop(catalog_guard);
        assert!(youtube_detail_url(&client, &id, true).is_pending());
        assert_eq!(
            youtube_detail_url(&client, &id, false),
            std::task::Poll::Ready(None),
            "Only completed metadata can establish that no thumbnail is known"
        );
    }
    fn id(value: &str) -> MediaId {
        MediaId::new(ProviderId::new("musicbrainz").unwrap(), value).unwrap()
    }
    #[test]
    fn visible_thumbnails_finish_in_order_and_remain_available_without_stealing_the_hero() {
        let ctx = egui::Context::default();
        let (commands, requests) = mpsc::sync_channel(1);
        let (send, events) = mpsc::sync_channel(1);
        let mut state = State::new();
        state.worker = Some(Worker {
            commands,
            events,
            generation: Arc::new(AtomicU64::new(0)),
            closed: Arc::new(AtomicBool::new(false)),
            thread: None,
        });
        let first = doremi_youtube::id("abcdefghijk").unwrap();
        let second = doremi_youtube::id("lmnopqrstuv").unwrap();
        let url = Some("https://i.ytimg.com/fixture.jpg".to_owned());
        state.request_thumbnail(&first, url.clone(), &ctx);
        state.request_thumbnail(&second, url.clone(), &ctx);
        assert_eq!(state.next_thumbnail().unwrap().0, first);
        state.select(Some(first.clone()));
        state.poll(&ctx);
        assert_eq!(requests.try_recv().unwrap().id, first);
        let generation = state.generation;
        // Layout/scroll frames cannot keep cancelling a visible image mid-read.
        state.request_thumbnail(&second, url.clone(), &ctx);
        state.request_thumbnail(&first, url.clone(), &ctx);
        assert_eq!(state.next_thumbnail().unwrap().0, first);
        assert_eq!(state.generation, generation);
        send.send(Completion {
            generation,
            result: Ok(Pixels {
                image: Arc::new(egui::ColorImage::filled([2, 2], egui::Color32::RED)),
                expires: Instant::now() + Duration::from_secs(60),
                reusable: true,
            }),
        })
        .unwrap();
        state.poll(&ctx);
        state.request_thumbnail(&first, url.clone(), &ctx);
        state.request_thumbnail(&second, url.clone(), &ctx);
        assert_eq!(state.next_thumbnail().unwrap().0, second);
        state.select(Some(second.clone()));
        state.poll(&ctx);
        assert_eq!(requests.try_recv().unwrap().id, second);
        let mut textures = Textures::default();
        assert!(textures.current(&state, &first, &ctx).is_some());
        assert!(state.caption(&first).starts_with("Carátula: YouTube Music"));
        send.send(Completion {
            generation: state.generation,
            result: Err(ProviderError::AccessDenied),
        })
        .unwrap();
        state.poll(&ctx);
        state.request_thumbnail(&second, url, &ctx);
        assert!(
            state.next_thumbnail().is_none(),
            "Do not retry refused artwork every frame"
        );
        state.select(None);
        assert!(textures.current(&state, &first, &ctx).is_some());
        assert!(textures.current(&state, &second, &ctx).is_none());
        assert!(state.caption(&second).starts_with("No se pudo"));
        assert!(requests.try_recv().is_err());
    }
    #[test]
    fn visible_thumbnail_work_is_bounded_and_drops_offscreen_candidates() {
        let ctx = egui::Context::default();
        let mut state = State::new();
        for i in 0..CAPACITY * 2 {
            let id = doremi_youtube::id(&format!("video{i:06}")).unwrap();
            state.request_thumbnail(&id, Some("https://i.ytimg.com/fixture.jpg".into()), &ctx);
        }
        assert_eq!(state.requested.len(), CAPACITY);
        let first = state.next_thumbnail().unwrap().0;
        state.select(Some(first));
        assert!(
            state.next_thumbnail().is_none(),
            "No visible rows means no background prefetch"
        );
        assert!(
            state.worker.is_none(),
            "Registering visible rows must never start network IO"
        );
    }
    #[test]
    fn stale_work_cannot_replace_release_and_only_reusable_pixels_survive_navigation() {
        let (commands, _requests) = mpsc::sync_channel(1);
        let (send, events) = mpsc::sync_channel(1);
        let mut state = State::new();
        state.worker = Some(Worker {
            commands,
            events,
            generation: Arc::new(AtomicU64::new(0)),
            closed: Arc::new(AtomicBool::new(false)),
            thread: None,
        });
        let first = id("release:12b57d46-a192-499e-a91f-7da66790a1c1");
        let second = id("release:00000000-0000-0000-0000-000000000002");
        state.select(Some(first.clone()));
        let old_generation = state.generation;
        state.select(Some(second.clone()));
        send.send(Completion {
            generation: old_generation,
            result: Err(ProviderError::AccessDenied),
        })
        .unwrap();
        let ctx = egui::Context::default();
        state.poll(&ctx);
        assert!(state.waiting && state.error.is_none() && state.pixels.is_none());
        let image = Arc::new(egui::ColorImage::filled([2, 2], egui::Color32::RED));
        send.send(Completion {
            generation: state.generation,
            result: Ok(Pixels {
                image: image.clone(),
                expires: Instant::now() + Duration::from_secs(60),
                reusable: false,
            }),
        })
        .unwrap();
        state.poll(&ctx);
        assert!(state.pixels.is_some() && state.cache.is_empty());
        let mut textures = Textures::default();
        assert!(textures.current(&state, &second, &ctx).is_some());
        assert!(textures.current(&state, &first, &ctx).is_none());
        state.select(None);
        textures.prune(&state);
        assert!(textures.0.is_empty() && state.pixels.is_none());
        state.select(Some(first.clone()));
        send.send(Completion {
            generation: state.generation,
            result: Ok(Pixels {
                image,
                expires: Instant::now() + Duration::from_secs(60),
                reusable: true,
            }),
        })
        .unwrap();
        state.poll(&ctx);
        state.select(None);
        state.select(Some(first));
        assert!(state.cache_hit && state.pixels.is_some() && !state.pending && !state.waiting);
        state.select(None);
        state.cache[0].expires = Instant::now();
        state.select(Some(second));
        assert!(state.cache.is_empty() && state.pixels.is_none() && state.pending);
    }
}
