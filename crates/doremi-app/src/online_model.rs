//! Host intent tracking only. Catalog and Queue remain the authoritative core.
use super::online::{Completion, Job, Prepared, Worker};
use super::*;
use doremi_core::{
    catalog::{AudioFormat, ProviderError, SearchRequest, Status},
    queue::RequestId,
};

struct SearchIntent {
    generation: u64,
    request: SearchRequest,
}
struct LoadIntent {
    generation: u64,
    request: RequestId,
    media: MediaId,
    position: u64,
    deadline: Instant,
    sent: bool,
}
struct PrefetchIntent {
    generation: u64,
    media: MediaId,
    deadline: Instant,
    started: Instant,
    sent: bool,
    result: Option<bool>,
    elapsed_ms: Option<u64>,
}
pub(super) struct State {
    pub selected: bool,
    pub(super) prefetch_enabled: bool,
    pub(super) eof_at: Option<Instant>,
    pub(super) transition_ms: Option<u64>,
    pub origin: Instant,
    session: u64,
    worker: Option<Worker>,
    debounce: Option<Instant>,
    search: Option<SearchIntent>,
    load: Option<LoadIntent>,
    prefetch: Option<PrefetchIntent>,
    phase_seen: &'static str,
    network: Option<(RequestId, Arc<doremi_commons::progressive::Metrics>)>,
    retired_network: Option<Arc<doremi_commons::progressive::Metrics>>,
}
impl State {
    pub fn failure_message(&self) -> Option<&'static str> {
        self.network
            .as_ref()
            .and_then(|(_, metrics)| metrics.message())
    }
    pub fn new() -> Self {
        Self {
            selected: false,
            prefetch_enabled: true,
            eof_at: None,
            transition_ms: None,
            origin: Instant::now(),
            session: 1,
            worker: None,
            debounce: None,
            search: None,
            load: None,
            prefetch: None,
            phase_seen: "idle",
            network: None,
            retired_network: None,
        }
    }
    pub fn phase(&self) -> &'static str {
        self.load
            .as_ref()
            .and_then(|load| {
                self.worker
                    .as_ref()
                    .map(|worker| worker.phase(load.generation))
            })
            .unwrap_or("idle")
    }
    pub fn snapshot(&self) -> serde_json::Value {
        let mut value = serde_json::json!({"request_timeout_ms":doremi_commons::progressive::REQUEST_TIMEOUT_MS,"search_source":if self.selected {"commons"}else{"demo"},"phase":self.phase(),"loading":self.load.is_some(),"search_debounce":self.debounce.is_some(),"worker_initialized":self.worker.is_some(),"retired_network":self.retired_network.as_ref().map(|metrics|{let (bytes,requests)=metrics.snapshot();serde_json::json!({"received_bytes":bytes,"requests":requests})}),"network":self.network.as_ref().map(|(_,metrics)| {let (bytes,requests)=metrics.snapshot();serde_json::json!({"received_bytes":bytes,"requests":requests,"total_bytes":metrics.total_bytes.load(std::sync::atomic::Ordering::Acquire),"encoded_buffers_bound_bytes":2*(doremi_commons::progressive::CHUNK+1)})})});
        value["transition_ms"] = serde_json::json!(self.transition_ms);
        value["source_cache"] = self
            .worker
            .as_ref()
            .map(Worker::diagnostics)
            .unwrap_or_default();
        value["prefetch"] = self.prefetch.as_ref().map(|p| serde_json::json!({
            "media":p.media.local(), "generation":p.generation, "sent":p.sent,
            "ready":p.result==Some(true), "failed":p.result==Some(false), "elapsed_ms":p.elapsed_ms
        })).unwrap_or_default();
        value
    }
    pub fn feedback(&self, status: Status) -> Option<(bool, String)> {
        if !self.selected {
            return None;
        }
        if self.debounce.is_some() || status == Status::Pending {
            return Some((false, "Consultando Commons…".into()));
        }
        if let Status::Failed(error) = status {
            return Some((true, format!("La consulta no se pudo completar: {error:?}")));
        }
        None
    }
}
pub(super) fn candidate(id: &MediaId) -> bool {
    doremi_commons::candidate(id)
        || doremi_youtube::entity_kind(id) == Some(doremi_core::media::EntityKind::Recording)
}
impl Model {
    // Keep the owner/worker across Android Activity recreation, but fence all
    // pending IO. The next foreground frame cannot accept retired completions.
    pub fn suspend_online(&mut self) {
        self.cancel_online_search();
        if self.catalog.search_status() == Status::Pending {
            self.catalog.cancel();
        }
        if self.online.load.is_some() {
            self.command_with_import_policy(Command::Cancel, false);
        }
        self.revision += 1;
    }
    fn online_worker(&mut self) -> Result<&Worker, ProviderError> {
        if self.online.worker.is_none() {
            self.online.worker = Some(Worker::new(
                self.online.origin,
                self.music.session.client.clone(),
            )?);
        }
        Ok(self.online.worker.as_ref().unwrap())
    }
    pub fn set_online_source(&mut self, selected: bool) {
        let was_music = self.music.selected;
        self.music.selected = false;
        self.music.cancel();
        if self.online.selected == selected && !was_music {
            return;
        }
        self.cancel_online_search();
        self.catalog.cancel();
        let Some(session) = self.online.session.checked_add(1) else {
            self.notice = "Se agotó la sesión del catálogo.".into();
            return;
        };
        self.online.session = session;
        self.online.selected = selected;
        self.catalog = Catalog::new(
            SessionId(session),
            Capabilities {
                provider: ProviderId::new(if selected {
                    doremi_commons::PROVIDER
                } else {
                    "demo"
                })
                .unwrap(),
                search: true,
                resolve: selected,
                max_results: if selected { 10 } else { 20 },
                formats: if selected {
                    vec![AudioFormat::OggVorbis]
                } else {
                    vec![]
                },
            },
        )
        .unwrap();
        self.search();
        self.revision += 1;
    }
    pub fn cancel_online_search(&mut self) {
        self.online.debounce = None;
        self.online.search = None;
        if let Some(worker) = &self.online.worker {
            let _ = worker.next(0);
        }
    }
    pub fn schedule_online_search(&mut self) {
        self.cancel_online_search();
        self.catalog.cancel();
        if !self.query.trim().is_empty() {
            self.online.debounce = Some(Instant::now() + Duration::from_millis(300));
        }
        self.revision += 1;
    }
    pub fn begin_online_load(&mut self, request: RequestId, id: &MediaId, position: u64) -> bool {
        if !candidate(id) {
            return false;
        }
        // The single source lane can be adopted by a new queue request. Its
        // generation/media identify the IO; only the current load owns playback.
        let promoted = self
            .online
            .prefetch
            .take()
            .filter(|p| p.media == *id && p.result != Some(false));
        let generation = if let Some(p) = &promoted {
            Ok(p.generation)
        } else {
            self.online_worker().and_then(|worker| worker.next(1))
        };
        match generation {
            Ok(generation) => {
                self.online.load = Some(LoadIntent {
                    generation,
                    request,
                    media: id.clone(),
                    position,
                    deadline: promoted
                        .as_ref()
                        .filter(|p| p.result.is_none())
                        .map(|p| p.deadline)
                        .unwrap_or_else(|| {
                            Instant::now()
                                + Duration::from_secs(
                                    if id.provider.as_str() == doremi_youtube::PROVIDER {
                                        600
                                    } else {
                                        12
                                    },
                                )
                        }),
                    sent: promoted
                        .as_ref()
                        .is_some_and(|p| p.sent && p.result.is_none()),
                });
                self.notice = if id.provider.as_str() == doremi_youtube::PROVIDER {
                    "Preparando audio de YouTube Music…"
                } else {
                    "Preparando audio CC0 de Commons…"
                }
                .into();
            }
            Err(error) => {
                self.notice = format!("No se pudo iniciar la fuente: {error:?}");
                self.event(Event::Failed(request));
            }
        }
        true
    }
    pub fn cancel_online_load(&mut self, request: Option<RequestId>) {
        if request.is_none() {
            self.cancel_prefetch();
        }
        if self
            .online
            .network
            .as_ref()
            .is_some_and(|(active, _)| request.is_none_or(|request| request == *active))
        {
            self.online.retired_network = self.online.network.take().map(|(_, metrics)| metrics);
        }
        if self
            .online
            .load
            .as_ref()
            .is_some_and(|load| request.is_none_or(|request| load.request == request))
        {
            self.online.load = None;
            if let Some(worker) = &self.online.worker {
                let _ = worker.next(1);
            }
        }
    }
    fn cancel_prefetch(&mut self) {
        if self.online.prefetch.take().is_some()
            && let Some(worker) = &self.online.worker
        {
            let _ = worker.next(1);
        }
    }
    /// Called after queue effects and by the process-owned Android actor, never
    /// by the renderer alone. One source lane, one bounded encoded cache.
    pub(super) fn sync_prefetch(&mut self) {
        // Prefetch only whole encoded YouTube audio. Other source contracts
        // retain their existing loading behavior (no speculative PCM decode).
        let target = (self.online.prefetch_enabled
            && self.queue.playback() == Playback::Playing
            && self.online.load.is_none()
            && std::env::var("DOREMI_PLAYBACK_BACKEND").as_deref() != Ok("legacy"))
        .then(|| self.queue.next_on_end())
        .flatten()
        .filter(|e| e.media.id.provider.as_str() == doremi_youtube::PROVIDER)
        .map(|e| e.media.id.clone());
        if self.online.prefetch.as_ref().map(|p| &p.media) == target.as_ref() {
            return;
        }
        self.cancel_prefetch();
        if let Some(media) = target
            && let Ok(generation) = self.online_worker().and_then(|worker| worker.next(1))
        {
            let started = Instant::now();
            self.online.prefetch = Some(PrefetchIntent {
                generation,
                media,
                deadline: started + Duration::from_secs(600),
                started,
                sent: false,
                result: None,
                elapsed_ms: None,
            });
        }
    }
    pub fn poll_online(&mut self, ctx: &egui::Context) {
        self.sync_prefetch();
        if let Some(p) = &mut self.online.prefetch {
            if p.result.is_none() && Instant::now() >= p.deadline {
                p.result = Some(false);
                let _ = self.online.worker.as_ref().unwrap().next(1);
            } else if !p.sent && p.result.is_none() {
                p.sent = self.online.worker.as_ref().unwrap().send(Job::Source {
                    generation: p.generation,
                    media: p.media.clone(),
                    context: ctx.clone(),
                });
            }
        }
        if self.online.debounce.is_some_and(|at| Instant::now() >= at) {
            self.online.debounce = None;
            let result = self
                .catalog
                .search(self.query.clone(), 10, self.catalog_now(), 8000);
            match result {
                Ok(effects) => {
                    for effect in effects {
                        if let ProviderCommand::Search(request) = effect {
                            match self.online_worker().and_then(|worker| worker.next(0)) {
                                Ok(generation) => {
                                    self.online.search = Some(SearchIntent {
                                        generation,
                                        request,
                                    })
                                }
                                Err(error) => {
                                    let _ = self.catalog.complete(
                                        ProviderEvent::Search {
                                            ticket: request.ticket,
                                            result: Err(error),
                                        },
                                        self.catalog_now(),
                                    );
                                }
                            }
                        }
                    }
                }
                Err(error) => self.notice = format!("Búsqueda online: {error:?}"),
            }
        }
        if let Some(at) = self.online.debounce {
            ctx.request_repaint_after(at.saturating_duration_since(Instant::now()));
        }
        if self.online.selected {
            if let Ok(effects) = self.catalog.advance(self.catalog_now())
                && !effects.is_empty()
            {
                self.cancel_online_search();
                self.revision += 1;
            }
            if self.catalog.search_status() == Status::Pending {
                ctx.request_repaint_after(Duration::from_millis(50));
            }
        }
        if let Some(intent) = self.online.search.take() {
            let job = Job::Search {
                generation: intent.generation,
                request: intent.request.clone(),
                context: ctx.clone(),
            };
            if !self.online.worker.as_ref().unwrap().send(job) {
                self.online.search = Some(intent);
                ctx.request_repaint_after(Duration::from_millis(20));
            }
        }
        if let Some(load) = &mut self.online.load
            && !load.sent
        {
            load.sent = self.online.worker.as_ref().unwrap().send(Job::Source {
                generation: load.generation,
                media: load.media.clone(),
                context: ctx.clone(),
            });
            ctx.request_repaint_after(Duration::from_millis(50));
        }
        let events = self
            .online
            .worker
            .as_ref()
            .map(Worker::poll)
            .unwrap_or([None, None]);
        for event in events.into_iter().flatten() {
            match event {
                Completion::Search {
                    generation,
                    ticket,
                    result,
                } => {
                    if !self.online.selected
                        || !self.online.worker.as_ref().unwrap().current(0, generation)
                    {
                        continue;
                    }
                    let _ = self
                        .catalog
                        .complete(ProviderEvent::Search { ticket, result }, self.catalog_now());
                    self.revision += 1;
                }
                Completion::Prepared {
                    generation,
                    media,
                    result,
                } => {
                    if !self.online.worker.as_ref().unwrap().current(1, generation) {
                        continue;
                    }
                    if let Some(p) = &mut self.online.prefetch
                        && p.generation == generation
                        && p.media == media
                    {
                        p.result = Some(result.is_ok());
                        p.elapsed_ms = Some(p.started.elapsed().as_millis() as u64);
                        // The worker's existing cache retains the compressed
                        // bytes. No decoder, PCM buffer, notice or queue event.
                        continue;
                    }
                    if !self
                        .online
                        .load
                        .as_ref()
                        .is_some_and(|load| load.generation == generation && load.media == media)
                    {
                        continue;
                    }
                    let request = self.online.load.as_ref().unwrap().request;
                    if self
                        .online
                        .load
                        .as_ref()
                        .is_some_and(|load| Instant::now() >= load.deadline)
                    {
                        self.cancel_online_load(Some(request));
                        self.notice = "La fuente agotó el tiempo de espera.".into();
                        self.event(Event::Failed(request));
                        self.revision += 1;
                        continue;
                    }
                    let load = self.online.load.take().unwrap();
                    match result {
                        Ok(Prepared::BufferedYouTube(bytes)) => {
                            let length = bytes.len() as u64;
                            let factory: doremi_player::StreamFactory = Box::new(move |cancel| {
                                if cancel.cancelled() {
                                    return Err(EngineError::Closed);
                                }
                                let source = doremi_player::ogg_stream::Vorbis::aac_from_read_seek(
                                    std::io::Cursor::new(bytes),
                                    length,
                                )
                                .map_err(EngineError::Decode)?;
                                Ok(Box::new(source))
                            });
                            if let Err(error) = self
                                .audio
                                .as_mut()
                                .ok_or(EngineError::Closed)
                                .and_then(|engine| {
                                    engine.load_stream(request, factory, load.position)
                                })
                            {
                                self.notice = format!(
                                    "No se pudo preparar el audio de YouTube Music: {error:?}"
                                );
                                self.event(Event::Failed(request));
                            } else {
                                self.notice = "Audio preparado de YouTube Music".into();
                            }
                        }
                        Ok(Prepared::YouTube(stream)) => {
                            let length = stream.length();
                            let factory: doremi_player::StreamFactory = Box::new(move |cancel| {
                                let reader = stream
                                    .reader(move || cancel.cancelled())
                                    .map_err(|_| EngineError::Input)?;
                                let source = doremi_player::ogg_stream::Vorbis::aac_from_read_seek(
                                    reader, length,
                                )
                                .map_err(EngineError::Decode)?;
                                Ok(Box::new(source))
                            });
                            if let Err(error) = self
                                .audio
                                .as_mut()
                                .ok_or(EngineError::Closed)
                                .and_then(|engine| {
                                    engine.load_stream(request, factory, load.position)
                                })
                            {
                                self.notice = format!(
                                    "No se pudo preparar el stream de YouTube Music: {error:?}"
                                );
                                self.event(Event::Failed(request));
                            } else {
                                self.notice = "Buffering de YouTube Music…".into();
                            }
                        }
                        Ok(Prepared::Stream(stream)) => {
                            let (resource, grant) = *stream;
                            let metrics = Arc::new(doremi_commons::progressive::Metrics::default());
                            self.online.network = Some((request, metrics.clone()));
                            let origin = self.online.origin;
                            let factory: doremi_player::StreamFactory = Box::new(move |cancel| {
                                let now = Tick(origin.elapsed().as_millis() as u64);
                                let info = doremi_player::wav_stream::Info {
                                    rate: grant.rate(),
                                    channels: grant.channels(),
                                    frames: grant.frames(),
                                };
                                let byte_len = grant.byte_len();
                                let reader = doremi_commons::progressive::Reader::open_stream(
                                    resource,
                                    grant,
                                    now,
                                    move || cancel.cancelled(),
                                    metrics,
                                )
                                .map_err(|_| EngineError::Input)?;
                                let mut source = doremi_player::ogg_stream::Vorbis::from_read_seek(
                                    reader, byte_len, info,
                                )
                                .map_err(EngineError::Decode)?;
                                // Prime HTTP seeking before the PCM feed's
                                // short prebuffer deadline, on this worker.
                                doremi_player::wav_stream::FrameReader::seek_frame(
                                    &mut source,
                                    load.position * u64::from(info.rate) / 1000,
                                )
                                .map_err(EngineError::Decode)?;
                                Ok(Box::new(source))
                            });
                            if let Err(error) = self
                                .audio
                                .as_mut()
                                .ok_or(EngineError::Closed)
                                .and_then(|engine| {
                                    engine.load_stream(request, factory, load.position)
                                })
                            {
                                self.notice = format!("No se pudo preparar el stream: {error:?}");
                                self.event(Event::Failed(request));
                            } else {
                                self.notice = "Buffering de música CC0…".into();
                            }
                        }
                        Ok(Prepared::Pcm(pcm)) => {
                            if self.prepared.len() >= 2
                                && !self.prepared.iter().any(|(id, _)| *id == media)
                            {
                                self.notice =
                                    "Se alcanzó el límite del experimento de audio.".into();
                                self.event(Event::Failed(request));
                                continue;
                            }
                            if !self.prepared.iter().any(|(id, _)| *id == media) {
                                self.prepared.push((media, pcm.clone()));
                            }
                            if let Err(error) = self
                                .audio
                                .as_mut()
                                .ok_or(EngineError::Closed)
                                .and_then(|engine| engine.load_pcm(request, pcm, load.position))
                            {
                                self.notice = format!("No se pudo abrir la salida: {error:?}");
                                self.event(Event::Failed(request));
                            } else {
                                self.notice = "Preparando salida de audio…".into();
                            }
                        }
                        Err(error) => {
                            self.notice = error.message();
                            self.event(Event::Failed(request));
                        }
                    }
                    self.revision += 1;
                }
            }
        }
        if let Some(load) = &self.online.load
            && Instant::now() >= load.deadline
        {
            let request = load.request;
            self.cancel_online_load(Some(request));
            self.notice = "La fuente agotó el tiempo de espera.".into();
            self.event(Event::Failed(request));
            self.revision += 1;
        }
        let phase = self.online.phase();
        if phase != self.online.phase_seen {
            self.online.phase_seen = phase;
            self.revision += 1;
            match phase {
                "resolve" => self.notice = "Resolviendo audio…".into(),
                "download" => self.notice = "Descargando audio…".into(),
                "decode" => self.notice = "Preparando audio…".into(),
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn playing_queue() -> Model {
        let mut m = Model::new(Preferences::default());
        let rows = ["qQzdAsjWGPg", "_d3SaQI_Ag4", "aaaaaaaaaaa"]
            .into_iter()
            .map(|id| Media {
                id: doremi_youtube::id(id).unwrap(),
                title: id.into(),
                duration_ms: None,
                music: None,
            })
            .collect();
        // Exercise real queue effects without touching the network/output device.
        let effects = m
            .queue
            .command(Command::StartContext {
                media: rows,
                start: 0,
                order: Order::Sequential,
            })
            .unwrap();
        let request = effects
            .iter()
            .find_map(|e| {
                if let Effect::Load { request, .. } = e {
                    Some(*request)
                } else {
                    None
                }
            })
            .unwrap();
        m.event(Event::Ready(request));
        m
    }
    #[test]
    fn in_flight_next_adopts_one_source_generation_and_the_new_queue_request() {
        let mut m = playing_queue();
        let p = m.online.prefetch.as_mut().unwrap();
        p.sent = true;
        let generation = p.generation;
        let media = p.media.clone();
        m.command(Command::Next);
        let load = m.online.load.as_ref().unwrap();
        assert_eq!(load.generation, generation);
        assert!(load.sent && m.online.prefetch.is_none());
        // Failure must address the new queue request, never the old occurrence.
        m.online.worker.as_ref().unwrap().inject_completion(
            1,
            Completion::Prepared {
                generation,
                media,
                result: Err(online::Error::YouTube(ProviderError::Unavailable)),
            },
        );
        m.poll_online(&egui::Context::default());
        assert_eq!(m.queue.playback(), Playback::Failed);
    }
    #[test]
    fn failed_prefetch_is_silent_and_not_retried_but_next_can_load_normally() {
        let mut m = playing_queue();
        let p = m.online.prefetch.as_mut().unwrap();
        p.sent = true;
        let generation = p.generation;
        let media = p.media.clone();
        let notice = m.notice.clone();
        m.online.worker.as_ref().unwrap().inject_completion(
            1,
            Completion::Prepared {
                generation,
                media,
                result: Err(online::Error::YouTube(ProviderError::AccessDenied)),
            },
        );
        for _ in 0..10 {
            m.poll_online(&egui::Context::default());
        }
        assert_eq!(m.notice, notice);
        assert_eq!(m.queue.playback(), Playback::Playing);
        assert_eq!(m.online.prefetch.as_ref().unwrap().result, Some(false));
        assert!(m.online.worker.as_ref().unwrap().current(1, generation));
        m.command(Command::Next);
        assert_ne!(m.online.load.as_ref().unwrap().generation, generation);
    }
    #[test]
    fn queue_changes_fence_late_preparation_and_cancel_releases_the_lane() {
        let mut m = playing_queue();
        let p = m.online.prefetch.as_ref().unwrap();
        let generation = p.generation;
        let media = p.media.clone();
        m.command(Command::SetOrder(Order::Shuffled(vec![0, 2, 1])));
        assert_eq!(
            m.online.prefetch.as_ref().unwrap().media.local(),
            "aaaaaaaaaaa"
        );
        m.online.worker.as_ref().unwrap().inject_completion(
            1,
            Completion::Prepared {
                generation,
                media,
                result: Ok(Prepared::BufferedYouTube(vec![1].into())),
            },
        );
        // Do not dispatch network in this fixture.
        m.online.prefetch.as_mut().unwrap().sent = true;
        m.poll_online(&egui::Context::default());
        assert_eq!(m.online.prefetch.as_ref().unwrap().result, None);
        m.command(Command::Cancel);
        assert!(m.online.prefetch.is_none() && m.online.load.is_none());
        assert!(!m.online.worker.as_ref().unwrap().current(1, generation));
    }
    #[test]
    fn ready_prefetch_retains_only_cache_bytes_and_deadline_never_fails_current_audio() {
        let mut m = playing_queue();
        let p = m.online.prefetch.as_mut().unwrap();
        p.sent = true;
        p.deadline = Instant::now();
        m.poll_online(&egui::Context::default());
        assert_eq!(m.online.prefetch.as_ref().unwrap().result, Some(false));
        assert_eq!(m.queue.playback(), Playback::Playing);
        m.command(Command::Pause);
        assert!(m.online.prefetch.is_none());
        m.command(Command::Play);
        let p = m.online.prefetch.as_mut().unwrap();
        p.sent = true;
        let generation = p.generation;
        let media = p.media.clone();
        let bytes: Arc<[u8]> = vec![1, 2, 3].into();
        m.online.worker.as_ref().unwrap().inject_completion(
            1,
            Completion::Prepared {
                generation,
                media,
                result: Ok(Prepared::BufferedYouTube(bytes.clone())),
            },
        );
        m.poll_online(&egui::Context::default());
        assert_eq!(m.online.prefetch.as_ref().unwrap().result, Some(true));
        assert_eq!(Arc::strong_count(&bytes), 1); // no extra host audio buffer
        assert!(m.prepared.is_empty() && m.audio.is_none());
        m.command(Command::Next);
        assert_eq!(m.online.load.as_ref().unwrap().generation, generation);
        assert!(!m.online.load.as_ref().unwrap().sent); // reuse worker cache
    }
    #[test]
    fn suspension_preserves_completed_results_without_starting_a_worker() {
        let mut model = Model::new(Preferences::default());
        model.query = "Café".into();
        model.search();
        let results = model.catalog.results().to_vec();
        assert!(!results.is_empty());
        model.suspend_online();
        assert_eq!(model.catalog.results(), results);
        assert_eq!(model.catalog.search_status(), Status::Ready);
        assert!(model.online.worker.is_none());
    }
    #[test]
    fn a_completion_after_its_deadline_cannot_win_the_same_frame_as_timeout() {
        let mut model = Model::new(Preferences::default());
        let media = Media {
            id: MediaId::new(
                ProviderId::new(doremi_commons::PROVIDER).unwrap(),
                "21923157",
            )
            .unwrap(),
            title: "Audited test tone".into(),
            duration_ms: Some(10000),
            music: None,
        };
        model.start_context(vec![media.clone()], 0);
        let load = model.online.load.as_mut().unwrap();
        load.sent = true;
        load.deadline = Instant::now();
        let generation = load.generation;
        let pcm = Arc::new(doremi_player::decode_wav(Builtin::Scale.bytes()).unwrap());
        model.online.worker.as_ref().unwrap().inject_completion(
            1,
            Completion::Prepared {
                generation,
                media: media.id,
                result: Ok(Prepared::Pcm(pcm)),
            },
        );
        model.poll_online(&egui::Context::default());
        assert_eq!(model.queue.playback(), Playback::Failed);
        assert!(model.audio.is_none() && model.prepared.is_empty());
    }
    #[test]
    fn suspension_fences_search_and_rejects_late_pcm_without_opening_audio() {
        let mut model = Model::new(Preferences::default());
        let media = Media {
            id: MediaId::new(
                ProviderId::new(doremi_commons::PROVIDER).unwrap(),
                "21923157",
            )
            .unwrap(),
            title: "Audited test tone".into(),
            duration_ms: Some(10000),
            music: None,
        };
        model.start_context(vec![media.clone()], 0);
        let pending = model.online.load.as_ref().unwrap();
        let generation = pending.generation;
        model.online.selected = true;
        model.query = "sine".into();
        model.schedule_online_search();
        assert!(model.online.debounce.is_some());
        model.suspend_online();
        assert!(model.online.search.is_none() && model.online.debounce.is_none());
        let pcm = Arc::new(doremi_player::decode_wav(Builtin::Scale.bytes()).unwrap());
        model.online.worker.as_ref().unwrap().inject_completion(
            1,
            Completion::Prepared {
                generation,
                media: media.id,
                result: Ok(Prepared::Pcm(pcm)),
            },
        );
        model.poll_online(&egui::Context::default());
        assert_eq!(model.queue.playback(), Playback::Paused);
        assert!(model.online.load.is_none() && model.audio.is_none() && model.prepared.is_empty());
    }
}
