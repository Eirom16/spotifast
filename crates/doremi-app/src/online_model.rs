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
    trace: Arc<doremi_youtube::latency::Trace>,
    prepared: Option<Box<doremi_youtube::stream::Stream>>,
}
struct PrefetchIntent {
    current: doremi_core::queue::EntryId,
    media: MediaId,
    revision: u64,
    generation: u64,
    sent: bool,
}
pub(super) struct State {
    pub selected: bool,
    pub origin: Instant,
    pub intent_origin: Instant,
    session: u64,
    worker: Option<Worker>,
    debounce: Option<Instant>,
    search: Option<SearchIntent>,
    load: Option<LoadIntent>,
    phase_seen: &'static str,
    network: Option<(RequestId, Arc<doremi_commons::progressive::Metrics>)>,
    retired_network: Option<Arc<doremi_commons::progressive::Metrics>>,
    trace: Option<(RequestId, Arc<doremi_youtube::latency::Trace>)>,
    prefetch: Option<PrefetchIntent>,
    prefetch_hit: bool,
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
            origin: Instant::now(),
            intent_origin: Instant::now(),
            session: 1,
            worker: None,
            debounce: None,
            search: None,
            load: None,
            phase_seen: "idle",
            network: None,
            retired_network: None,
            trace: None,
            prefetch: None,
            prefetch_hit: false,
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
        let latency = self.trace.as_ref().map(|(request, trace)| {
            trace.enable_http();
            let mut value = serde_json::json!({"http_reads":trace.http_snapshot(),"request":format!("{request:?}"),"requested_us":0,"origin":"host_queue_intent","acoustic_capture":false,"prefetched":self.prefetch_hit});
            for (name, time) in doremi_youtube::latency::STAGES.iter().zip(trace.snapshot()) {
                value[*name] = serde_json::json!(time);
            }
            value
        });
        serde_json::json!({"latency":latency,"request_timeout_ms":doremi_commons::progressive::REQUEST_TIMEOUT_MS,"search_source":if self.selected {"commons"}else{"demo"},"phase":self.phase(),"loading":self.load.is_some(),"search_debounce":self.debounce.is_some(),"worker_initialized":self.worker.is_some(),"retired_network":self.retired_network.as_ref().map(|metrics|{let (bytes,requests)=metrics.snapshot();serde_json::json!({"received_bytes":bytes,"requests":requests})}),"network":self.network.as_ref().map(|(_,metrics)| {let (bytes,requests)=metrics.snapshot();serde_json::json!({"received_bytes":bytes,"requests":requests,"total_bytes":metrics.total_bytes.load(std::sync::atomic::Ordering::Acquire),"encoded_buffers_bound_bytes":2*(doremi_commons::progressive::CHUNK+1)})})})
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
        self.cancel_prefetch();
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
        match self.online_worker().and_then(|worker| worker.next(1)) {
            Ok(generation) => {
                let trace = Arc::new(doremi_youtube::latency::Trace::new(
                    self.online.intent_origin,
                ));
                let prepared = self
                    .online
                    .prefetch
                    .as_ref()
                    .filter(|intent| {
                        intent.revision == self.music.session.revision && !self.music.session.busy
                    })
                    .and_then(|_| self.online.worker.as_ref()?.take_prefetched(id));
                self.online.prefetch_hit = prepared.is_some();
                if prepared.is_some() {
                    trace.mark(doremi_youtube::latency::Stage::Verified);
                }
                self.cancel_prefetch();
                self.online.trace = Some((request, trace.clone()));
                self.online.load = Some(LoadIntent {
                    generation,
                    request,
                    media: id.clone(),
                    position,
                    deadline: Instant::now()
                        + if id.provider.as_str() == doremi_youtube::PROVIDER {
                            doremi_youtube::stream::RESOLVE_TIMEOUT
                        } else {
                            Duration::from_secs(12)
                        },
                    sent: false,
                    trace,
                    prepared,
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
    pub fn cancel_prefetch(&mut self) {
        self.online.prefetch = None;
        if let Some(worker) = &self.online.worker {
            let _ = worker.next(2);
        }
    }
    fn poll_prefetch(&mut self, ctx: &egui::Context) {
        // Resolution only, once in the last 30 seconds. No speculative audio
        // download, no periodic refresh and no work while paused/backgrounded.
        let target = self.queue.current().and_then(|current| {
            (self.queue.playback() == Playback::Playing
                && self.online.load.is_none()
                && !self.music.session.busy
                && current.media.duration_ms.is_some_and(|duration| {
                    duration.saturating_sub(self.queue.position_ms()) <= 30_000
                }))
            .then(|| self.queue.next_on_end())
            .flatten()
            .filter(|next| next.media.id.provider.as_str() == doremi_youtube::PROVIDER)
            .map(|next| (current.id, next.media.id.clone()))
        });
        if self.online.prefetch.as_ref().is_some_and(|intent| {
            target
                .as_ref()
                .is_none_or(|(current, media)| intent.current != *current || intent.media != *media)
                || intent.revision != self.music.session.revision
        }) {
            self.cancel_prefetch();
        }
        let Some((current, media)) = target else {
            return;
        };
        if self.online.prefetch.is_none()
            && let Ok(generation) = self.online_worker().and_then(|worker| worker.next(2))
        {
            self.online.prefetch = Some(PrefetchIntent {
                current,
                media,
                revision: self.music.session.revision,
                generation,
                sent: false,
            });
        }
        if let Some(intent) = &mut self.online.prefetch
            && !intent.sent
        {
            intent.sent = self
                .online
                .worker
                .as_ref()
                .unwrap()
                .send(Job::Prefetch {
                    generation: intent.generation,
                    media: intent.media.clone(),
                    context: ctx.clone(),
                })
                .is_ok();
        }
    }
    pub fn poll_online(&mut self, ctx: &egui::Context) {
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
            if self.online.worker.as_ref().unwrap().send(job).is_err() {
                self.online.search = Some(intent);
                ctx.request_repaint_after(Duration::from_millis(20));
            }
        }
        if let Some(load) = &mut self.online.load
            && !load.sent
        {
            load.trace.mark(doremi_youtube::latency::Stage::Queued);
            let result = self.online.worker.as_ref().unwrap().send(Job::Source {
                generation: load.generation,
                request: load.request,
                media: load.media.clone(),
                trace: load.trace.clone(),
                prepared: load.prepared.take(),
                context: ctx.clone(),
            });
            match result {
                Ok(()) => load.sent = true,
                Err(Job::Source { prepared, .. }) => load.prepared = prepared,
                Err(_) => unreachable!("source send returned another job"),
            }
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
                    request,
                    media,
                    result,
                } => {
                    if !self.online.worker.as_ref().unwrap().current(1, generation) {
                        continue;
                    }
                    if !self.online.load.as_ref().is_some_and(|load| {
                        load.generation == generation
                            && load.request == request
                            && load.media == media
                    }) {
                        continue;
                    }
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
                        Ok(Prepared::YouTube(stream)) => {
                            let length = stream.length();
                            let started = load.trace.origin();
                            let trace = load.trace.clone();
                            let factory: doremi_player::StreamFactory = Box::new(move |cancel| {
                                let reader = stream
                                    .reader(move || cancel.cancelled())
                                    .map_err(|_| EngineError::Input)?
                                    .with_trace(trace)
                                    .read_ahead()
                                    .map_err(|_| doremi_player::EngineError::Input)?
                                    .with_startup_prebuffer();
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
                                    engine.load_stream_since(
                                        request,
                                        factory,
                                        load.position,
                                        started,
                                    )
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
                "resolve" => self.notice = "Resolviendo audio CC0…".into(),
                "download" => self.notice = "Descargando tono CC0…".into(),
                "decode" => self.notice = "Preparando audio CC0…".into(),
                _ => {}
            }
        }
        self.poll_prefetch(ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn speculative_resolution_is_single_bounded_and_fenced_by_pause_repeat_and_session() {
        let mut model = Model::new(Preferences::default());
        *model.music.session.client.try_lock().unwrap() = Some(Err(ProviderError::Transport));
        model.start_context(
            ["abcdefghijk", "bcdefghijkl", "cdefghijklm"]
                .into_iter()
                .map(|local| Media {
                    id: doremi_youtube::id(local).unwrap(),
                    title: "Synthetic selection".into(),
                    duration_ms: Some(20_000),
                })
                .collect(),
            0,
        );
        let request = model.online.load.take().unwrap().request;
        model.event(Event::Ready(request));
        let ctx = egui::Context::default();
        model.poll_prefetch(&ctx);
        let intent = model.online.prefetch.as_ref().unwrap();
        assert_eq!(intent.media, doremi_youtube::id("bcdefghijkl").unwrap());
        let generation = intent.generation;
        model.poll_prefetch(&ctx);
        assert_eq!(
            model.online.prefetch.as_ref().unwrap().generation,
            generation
        );
        model.command(Command::Pause);
        model.poll_prefetch(&ctx);
        assert!(model.online.prefetch.is_none());
        assert!(!model.online.worker.as_ref().unwrap().current(2, generation));
        model.command(Command::SetRepeat(doremi_core::queue::Repeat::Track));
        model.command(Command::Play);
        model.poll_prefetch(&ctx);
        assert_eq!(
            model.online.prefetch.as_ref().unwrap().media,
            doremi_youtube::id("abcdefghijk").unwrap(),
        );
        let generation = model.online.prefetch.as_ref().unwrap().generation;
        model.music.session.revision += 1;
        model.poll_prefetch(&ctx);
        assert!(!model.online.worker.as_ref().unwrap().current(2, generation));
        model.suspend_online();
        assert!(model.online.prefetch.is_none());
        assert!(model.audio.is_none());
    }

    #[test]
    fn cold_youtube_resolution_keeps_its_budget_and_can_still_be_cancelled() {
        let mut model = Model::new(Preferences::default());
        let before = Instant::now();
        model.start_context(
            vec![Media {
                id: doremi_youtube::id("abcdefghijk").unwrap(),
                title: "Synthetic selection".into(),
                duration_ms: Some(180000),
            }],
            0,
        );
        let load = model.online.load.as_ref().unwrap();
        let budget = doremi_youtube::stream::RESOLVE_TIMEOUT;
        assert!(load.deadline >= before + budget);
        assert!(load.deadline <= Instant::now() + budget);
        let generation = load.generation;
        model.cancel_online_load(None);
        assert!(model.online.load.is_none());
        assert!(!model.online.worker.as_ref().unwrap().current(1, generation));
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
        };
        model.start_context(vec![media.clone()], 0);
        let load = model.online.load.as_mut().unwrap();
        load.sent = true;
        load.deadline = Instant::now();
        let generation = load.generation;
        let request = load.request;
        let pcm = Arc::new(doremi_player::decode_wav(Builtin::Scale.bytes()).unwrap());
        model.online.worker.as_ref().unwrap().inject_completion(
            1,
            Completion::Prepared {
                generation,
                request,
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
        };
        model.start_context(vec![media.clone()], 0);
        let pending = model.online.load.as_ref().unwrap();
        let generation = pending.generation;
        let request = pending.request;
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
                request,
                media: media.id,
                result: Ok(Prepared::Pcm(pcm)),
            },
        );
        model.poll_online(&egui::Context::default());
        assert_eq!(model.queue.playback(), Playback::Paused);
        assert!(model.online.load.is_none() && model.audio.is_none() && model.prepared.is_empty());
    }
}
