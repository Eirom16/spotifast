//! Independent Doremi host. The core owns queue and catalog rules.
use doremi_core::{
    catalog::{Capabilities, Catalog, ProviderCommand, ProviderEvent, Tick},
    media::{Media, MediaId, ProviderId},
    queue::{Command, Effect, Event, Order, Playback, Queue, Repeat},
    requests::SessionId,
};
#[cfg(target_os = "android")]
mod android_native;
#[cfg(any(target_os = "linux", target_os = "android"))]
mod online;
#[cfg(any(target_os = "linux", target_os = "android"))]
mod online_model;
#[cfg(target_os = "linux")]
mod online_probe;
use doremi_library as persistence;
#[cfg(target_os = "linux")]
mod file_input;
mod local_media;
mod saved;
#[cfg(not(target_os = "android"))]
mod wav_probe;
use doremi_player::{Builtin, Engine, EngineError, EngineEvent, Pcm};
#[cfg(target_os = "linux")]
pub use online_probe::run as online_audio_probe;
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
#[cfg(not(target_os = "android"))]
pub use wav_probe::run as wav_audio_probe;

static NEXT_INSTANCE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(1);
const TOUCH: f32 = 48.0;
const GAP: f32 = 8.0;

// A pointer gesture belongs to one occurrence. Reusing the widget's auto ID
// after EOF would apply a still-held seek to the following song as well.
fn seek_slider(
    ui: &mut egui::Ui,
    identity: egui::Id,
    position: &mut u64,
    duration: u64,
) -> egui::Response {
    ui.push_id(identity, |ui| {
        ui.add_sized(
            [ui.available_width(), TOUCH],
            egui::Slider::new(position, 0..=duration)
                .text("Posición")
                .show_value(false),
        )
    })
    .inner
}
static RETAINED: Mutex<Option<Retained>> = Mutex::new(None);

#[derive(Clone, Deserialize, Serialize)]
#[serde(default)]
struct Preferences {
    schema: u32,
    green: bool,
    light: bool,
    library: Vec<String>,
    volume: f32,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            schema: 1,
            green: false,
            light: false,
            library: vec![],
            volume: 0.7,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
enum Page {
    #[default]
    Home,
    Search,
    Library,
    Queue,
    Settings,
    Player,
}

struct Model {
    queue: Queue,
    catalog: Catalog,
    prefs: Preferences,
    page: Page,
    query: String,
    notice: String,
    revision: u64,
    write_prefs: bool,
    write_queue: bool,
    library: persistence::collection::Collection,
    write_library: bool,
    storage_warning: Option<&'static str>,
    audio: Option<Engine>,
    focus: bool,
    original_path: Option<PathBuf>,
    // Transient CLI-selected file capability, separate from saved metadata.
    wav_input: Option<(MediaId, PathBuf)>,
    local_files: local_media::State,
    // Source data, not a second queue. Only the explicit bounded CLI corpus
    // populates this cache; no arbitrary files or persisted transport URIs.
    prepared: Vec<(MediaId, Arc<Pcm>)>,
    #[cfg(any(target_os = "linux", target_os = "android"))]
    online: online_model::State,
}

const DEMO_TRACKS: [(&str, &str); 4] = [
    ("dawn", "Amanecer violeta"),
    ("rain", "Lluvia de cristal"),
    ("cafe", "Café en órbita"),
    ("night", "Noche tranquila"),
];

fn demo_candidate(id: &MediaId) -> bool {
    id.provider.as_str() == "demo" && DEMO_TRACKS.iter().any(|(local, _)| *local == id.local())
}

fn songs() -> Vec<Media> {
    DEMO_TRACKS
        .into_iter()
        .map(|(id, title)| Media {
            id: MediaId::new(ProviderId::new("demo").unwrap(), id).unwrap(),
            title: title.into(),
            duration_ms: Some(30_000),
        })
        .chain([
            Builtin::Scale.media(),
            Builtin::Chord.media(),
            doremi_player::original_long::media(),
        ])
        .collect()
}

impl Model {
    fn new(prefs: Preferences) -> Self {
        let legacy = saved::legacy(&prefs);
        let write_library = legacy.is_ok();
        let library = legacy.unwrap_or_default();
        Self {
            queue: Queue::new(SessionId(1)),
            catalog: Catalog::new(
                SessionId(1),
                Capabilities {
                    provider: ProviderId::new("demo").unwrap(),
                    search: true,
                    resolve: false,
                    max_results: 20,
                    formats: vec![],
                },
            )
            .unwrap(),
            prefs,
            page: Page::Home,
            query: String::new(),
            notice: "Demostración sin audio. No es reproducción real.".into(),
            revision: 0,
            write_prefs: true,
            write_queue: true,
            library,
            write_library,
            storage_warning: None,
            audio: None,
            focus: !cfg!(target_os = "android"),
            original_path: None,
            wav_input: None,
            local_files: local_media::State::new(),
            prepared: Vec::new(),
            #[cfg(any(target_os = "linux", target_os = "android"))]
            online: online_model::State::new(),
        }
    }
    fn load(directory: &std::path::Path) -> Self {
        let mut model = Self::new(Preferences::default());
        model.original_path = Some(doremi_player::original_long::path(directory));
        match persistence::read(directory, "preferences-v1.json") {
            Ok(Some(bytes)) => match serde_json::from_slice::<Preferences>(&bytes) {
                Ok(mut prefs) if prefs.schema <= 1 => {
                    prefs.schema = 1;
                    prefs.volume = if prefs.volume.is_finite() {
                        prefs.volume.clamp(0.0, 1.0)
                    } else {
                        0.7
                    };
                    model.prefs = prefs;
                }
                _ => {
                    model.write_prefs = false;
                    model.storage_warning = Some(
                        "Preferencias incompatibles. El archivo se conserva y los cambios no se guardarán.",
                    );
                }
            },
            Ok(None) => {
                model.prefs.schema = 1;
            }
            Err(_) => {
                model.write_prefs = false;
                model.storage_warning =
                    Some("No se pudieron leer las preferencias. El archivo se conserva.");
            }
        }
        model.load_library(directory);
        model.load_local_files(directory);
        match persistence::read(directory, "queue-v1.json") {
            Ok(Some(bytes)) => match persistence::decode(&bytes).and_then(|checkpoint| {
                model
                    .queue
                    .restore_checkpoint(checkpoint)
                    .map_err(|_| persistence::Error::InvalidQueue)
            }) {
                Ok(_) => {
                    model.notice = "Sesión restaurada en pausa.".into();
                }
                Err(_) => {
                    model.write_queue = false;
                    model.storage_warning = Some(
                        "Cola guardada incompatible. Se conserva; la sesión actual no se guardará.",
                    );
                }
            },
            Ok(None) => {}
            Err(_) => {
                model.write_queue = false;
                model.storage_warning =
                    Some("No se pudo leer la cola guardada. Se conserva el archivo.");
            }
        }
        model
    }
    fn command(&mut self, command: Command) {
        let cancelled = matches!(&command, Command::Cancel);
        if cancelled {
            self.cancel_local_import();
        }
        if let Command::Append(media) = &command
            && self.queue.manual().len().saturating_add(media.len())
                > doremi_core::queue::checkpoint::ENTRY_LIMIT
        {
            self.notice = "La cola manual admite hasta 4096 canciones.".into();
            self.revision += 1;
            return;
        }
        match self.queue.command(command) {
            Ok(effects) => {
                self.effects(effects);
                if cancelled {
                    self.notice = "Operación cancelada.".into();
                }
            }
            Err(error) => self.notice = format!("No se pudo cambiar la cola: {error:?}"),
        }
        self.revision += 1;
    }
    fn enable_audio(&mut self) {
        if self.audio.is_none() {
            match Engine::new() {
                Ok(engine) => {
                    engine.set_volume(self.prefs.volume);
                    self.audio = Some(engine);
                }
                Err(_) => self.notice = "No se pudo iniciar el worker de audio.".into(),
            }
        }
    }
    fn real_audio(&self) -> bool {
        self.queue.current().is_some_and(|entry| {
            Builtin::from_media(&entry.media.id).is_some()
                || doremi_player::original_long::matches(&entry.media.id)
                || self.local_files.index.get(&entry.media.id).is_some()
                || self
                    .wav_input
                    .as_ref()
                    .is_some_and(|(id, _)| *id == entry.media.id)
                || self.prepared.iter().any(|(id, _)| *id == entry.media.id)
                || self.online_candidate(&entry.media.id)
        })
    }
    #[cfg(target_os = "android")]
    fn needs_focus(&self) -> bool {
        self.real_audio() && self.queue.playback() == Playback::Playing
    }
    fn event(&mut self, event: Event) {
        match self.queue.event(event) {
            Ok(effects) => self.effects(effects),
            Err(error) => self.notice = format!("Audio: {error:?}"),
        }
    }
    fn effects(&mut self, effects: Vec<Effect>) {
        for effect in effects {
            match effect {
                Effect::Load {
                    request,
                    entry,
                    position_ms,
                    ..
                } => {
                    let result =
                        if let Some((_, pcm)) =
                            self.prepared.iter().find(|(id, _)| *id == entry.media.id)
                        {
                            Some(self.audio.as_mut().ok_or(EngineError::Closed).and_then(
                                |engine| engine.load_pcm(request, pcm.clone(), position_ms),
                            ))
                        } else if let Some(builtin) = Builtin::from_media(&entry.media.id) {
                            Some(
                                self.audio
                                    .as_mut()
                                    .ok_or(EngineError::Closed)
                                    .and_then(|engine| engine.load(request, builtin, position_ms)),
                            )
                        } else if doremi_player::original_long::matches(&entry.media.id) {
                            Some(
                                self.original_path
                                    .as_ref()
                                    .ok_or(EngineError::Input)
                                    .and_then(|path| {
                                        self.audio.as_mut().ok_or(EngineError::Closed).and_then(
                                            |engine| {
                                                engine.load_original_long(
                                                    request,
                                                    path.clone(),
                                                    position_ms,
                                                )
                                            },
                                        )
                                    }),
                            )
                        } else if let Some(path) = self.local_files.index.get(&entry.media.id) {
                            Some(self.audio.as_mut().ok_or(EngineError::Closed).and_then(
                                |engine| engine.load_wav(request, path.to_path_buf(), position_ms),
                            ))
                        } else if let Some((_, path)) = self
                            .wav_input
                            .as_ref()
                            .filter(|(id, _)| *id == entry.media.id)
                        {
                            Some(self.audio.as_mut().ok_or(EngineError::Closed).and_then(
                                |engine| engine.load_wav(request, path.clone(), position_ms),
                            ))
                        } else {
                            None
                        };
                    if let Some(result) = result {
                        match result {
                            Ok(()) => self.notice = "Preparando salida de audio…".into(),
                            Err(error) => {
                                self.notice = format!("No se pudo preparar el audio: {error:?}");
                                self.event(Event::Failed(request));
                            }
                        }
                    } else if self.load_online(request, &entry.media.id, position_ms) {
                        // The IO worker will prepare PCM; never report simulated Ready.
                    } else if demo_candidate(&entry.media.id) {
                        if let Some(engine) = self.audio.as_mut() {
                            engine.cancel();
                        }
                        self.notice = "Demostración sin audio. No es reproducción real.".into();
                        self.event(Event::Ready(request));
                    } else {
                        self.notice = "Fuente no disponible en esta versión.".into();
                        self.event(Event::Failed(request));
                    }
                }
                Effect::Cancel(request) => {
                    self.cancel_online(Some(request));
                    if let Some(engine) = self.audio.as_mut()
                        && engine.current_request() == Some(request)
                    {
                        engine.cancel();
                    }
                }
                Effect::SetPlaying(playing) => {
                    if let Some(engine) = &self.audio {
                        engine.set_playing(playing && self.focus);
                    }
                }
                Effect::Stop => {
                    self.cancel_online(None);
                    if let Some(engine) = self.audio.as_mut() {
                        engine.cancel();
                    }
                }
                Effect::ReadAppend(_) => {}
            }
        }
    }
    fn poll_audio(&mut self) {
        let event = self.audio.as_ref().and_then(Engine::poll);
        if let Some(event) = event {
            let request = match event {
                EngineEvent::Ready(r) | EngineEvent::Failed(r, _) | EngineEvent::Ended(r) => r,
            };
            if self.audio.as_ref().and_then(Engine::current_request) == Some(request) {
                match event {
                    EngineEvent::Ready(r) => {
                        self.notice = if self
                            .queue
                            .current()
                            .is_some_and(|entry| self.online_candidate(&entry.media.id))
                        {
                            "Audio CC0 de Commons · experimento"
                        } else if self.queue.current().is_some_and(|entry| {
                            Builtin::from_media(&entry.media.id).is_some()
                                || doremi_player::original_long::matches(&entry.media.id)
                        }) {
                            "Audio local original · CC0"
                        } else {
                            "Archivo WAV local seleccionado"
                        }
                        .into();
                        self.event(Event::Ready(r));
                    }
                    EngineEvent::Ended(r) => self.event(Event::Ended(r)),
                    EngineEvent::Failed(r, error) => {
                        self.notice = format!("La salida de audio falló: {error:?}");
                        self.event(Event::Failed(r));
                    }
                }
                self.revision += 1;
            }
        }
        if let Some(engine) = &self.audio
            && let Some(request) = engine.current_request()
            && let Some(metrics) = engine.metrics()
        {
            let _ = self.queue.event(Event::Position {
                request,
                position_ms: metrics.position_ms,
            });
        }
    }
    fn start_context(&mut self, rows: Vec<Media>, start: usize) {
        self.cancel_local_import();
        let order = if self.queue.shuffled() {
            shuffled(rows.len(), Some(start))
        } else {
            Order::Sequential
        };
        self.command(Command::StartContext {
            media: rows,
            start,
            order,
        });
    }
    fn search(&mut self) {
        #[cfg(any(target_os = "linux", target_os = "android"))]
        if self.online.selected {
            self.schedule_online_search();
            return;
        }
        if self.query.trim().is_empty() {
            self.catalog.cancel();
        } else {
            match self
                .catalog
                .search(self.query.clone(), 20, self.catalog_now(), 1000)
            {
                Ok(effects) => {
                    for effect in effects {
                        if let ProviderCommand::Search(request) = effect {
                            let query = request.query.to_lowercase();
                            let results = songs()
                                .into_iter()
                                .filter(|s| s.title.to_lowercase().contains(&query))
                                .collect();
                            let _ = self.catalog.complete(
                                ProviderEvent::Search {
                                    ticket: request.ticket,
                                    result: Ok(results),
                                },
                                self.catalog_now(),
                            );
                        }
                    }
                }
                Err(error) => self.notice = format!("Búsqueda: {error:?}"),
            }
        }
        self.revision += 1;
    }
    fn catalog_now(&self) -> Tick {
        #[cfg(any(target_os = "linux", target_os = "android"))]
        {
            Tick(self.online.origin.elapsed().as_millis() as u64)
        }
        #[cfg(not(any(target_os = "linux", target_os = "android")))]
        {
            Tick(self.revision)
        }
    }
    fn cancel_search(&mut self) {
        self.query.clear();
        self.catalog.cancel();
        #[cfg(any(target_os = "linux", target_os = "android"))]
        self.cancel_online_search();
        self.revision += 1;
    }
    fn online_candidate(&self, _id: &MediaId) -> bool {
        #[cfg(any(target_os = "linux", target_os = "android"))]
        {
            online_model::candidate(_id)
        }
        #[cfg(not(any(target_os = "linux", target_os = "android")))]
        {
            false
        }
    }
    fn load_online(
        &mut self,
        _request: doremi_core::queue::RequestId,
        _id: &MediaId,
        _position: u64,
    ) -> bool {
        #[cfg(any(target_os = "linux", target_os = "android"))]
        {
            self.begin_online_load(_request, _id, _position)
        }
        #[cfg(not(any(target_os = "linux", target_os = "android")))]
        {
            false
        }
    }
    fn cancel_online(&mut self, _request: Option<doremi_core::queue::RequestId>) {
        #[cfg(any(target_os = "linux", target_os = "android"))]
        self.cancel_online_load(_request);
    }
    fn online_search_selected(&self) -> bool {
        #[cfg(any(target_os = "linux", target_os = "android"))]
        {
            self.online.selected
        }
        #[cfg(not(any(target_os = "linux", target_os = "android")))]
        {
            false
        }
    }
    fn source_diagnostics(&self) -> serde_json::Value {
        #[cfg(any(target_os = "linux", target_os = "android"))]
        {
            let mut state = self.online.snapshot();
            state["search_status"] =
                serde_json::json!(format!("{:?}", self.catalog.search_status()));
            state
        }
        #[cfg(not(any(target_os = "linux", target_os = "android")))]
        {
            serde_json::json!({"search_source":"demo","phase":"unavailable"})
        }
    }
    fn search_feedback(&self) -> Option<(bool, String)> {
        #[cfg(any(target_os = "linux", target_os = "android"))]
        {
            self.online.feedback(self.catalog.search_status())
        }
        #[cfg(not(any(target_os = "linux", target_os = "android")))]
        {
            None
        }
    }
}

fn shuffled(length: usize, first: Option<usize>) -> Order {
    let mut indices: Vec<_> = (0..length).collect();
    indices.shuffle(&mut rand::rng());
    if let Some(first) = first
        && let Some(index) = indices.iter().position(|value| *value == first)
    {
        indices.swap(0, index);
    }
    Order::Shuffled(indices)
}

#[derive(Clone, PartialEq, Serialize)]
struct Control {
    name: String,
    rect: [f32; 4],
}
struct Retained {
    model: Model,
    writer: persistence::Writer,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum StorageError {
    Encoding,
    Io,
    Closed,
}
impl StorageError {
    fn message(self) -> &'static str {
        match self {
            Self::Encoding => "No se pudo guardar la cola. El estado anterior se conserva.",
            Self::Io => "No se pudieron guardar los cambios. El estado anterior se conserva.",
            Self::Closed => {
                "No se pudieron guardar los cambios. Reabre Doremi para volver a intentarlo."
            }
        }
    }
}
pub struct DoremiApp {
    instance: usize,
    model: Option<Model>,
    writer: Option<persistence::Writer>,
    pending_write: usize,
    storage_error: Option<StorageError>,
    controls: Vec<Control>,
    logo: egui::TextureHandle,
    saved_state: Vec<u8>,
    pending: bool,
    last_persist: Instant,
    last_queue_persist: Instant,
    saved_revision: u64,
    saved_controls: Vec<Control>,
}

impl DoremiApp {
    #[cfg(not(target_os = "android"))]
    pub fn new(cc: &eframe::CreationContext<'_>, directory: PathBuf) -> Self {
        Self::new_context(&cc.egui_ctx, directory)
    }

    pub fn new_context(ctx: &egui::Context, directory: PathBuf) -> Self {
        let retained = RETAINED.lock().unwrap().take();
        let (mut model, writer) = if let Some(retained) = retained {
            (retained.model, retained.writer)
        } else {
            // Startup reads precede drawing. The writer does all durable I/O.
            let model = Model::load(&directory);
            (model, persistence::Writer::new(directory))
        };
        model.enable_audio();
        let image = image::load_from_memory(include_bytes!("../../../assets/doremi/Doremi.png"))
            .unwrap()
            .to_rgba8();
        let size = [image.width() as usize, image.height() as usize];
        let logo = ctx.load_texture(
            "Doremi",
            egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw()),
            Default::default(),
        );
        ctx.all_styles_mut(|style| {
            style.spacing.item_spacing = egui::vec2(GAP, GAP);
            style.spacing.button_padding = egui::vec2(12.0, 10.0);
            style.spacing.interact_size.y = TOUCH;
        });
        Self {
            instance: NEXT_INSTANCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            model: Some(model),
            writer: Some(writer),
            pending_write: 0,
            storage_error: None,
            controls: vec![],
            logo,
            saved_state: vec![],
            pending: true,
            last_persist: Instant::now(),
            last_queue_persist: Instant::now(),
            saved_revision: u64::MAX,
            saved_controls: vec![],
        }
    }
    fn button(&mut self, ui: &mut egui::Ui, name: &str, label: &str, width: f32) -> bool {
        let response = ui.add_sized([width.max(TOUCH), TOUCH], egui::Button::new(label));
        self.controls.push(Control {
            name: name.into(),
            rect: [
                response.rect.min.x,
                response.rect.min.y,
                response.rect.max.x,
                response.rect.max.y,
            ],
        });
        response.clicked()
    }
    fn rows(&mut self, ui: &mut egui::Ui, rows: &[Media]) {
        for (start, media) in rows.iter().enumerate() {
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.set_width((ui.available_width() - 4.0).max(0.0));
                ui.label(egui::RichText::new(&media.title).strong());
                ui.small(if doremi_player::original_long::matches(&media.id) {
                    "Audio local original · 2:00 · CC0".into()
                } else if Builtin::from_media(&media.id).is_some() {
                    "Audio local original · 0:08 · CC0".into()
                } else if self
                    .model
                    .as_ref()
                    .unwrap()
                    .local_files
                    .index
                    .get(&media.id)
                    .is_some()
                {
                    "Archivo WAV local".into()
                } else if self.model.as_ref().unwrap().online_candidate(&media.id) {
                    "Tono de prueba online · 0:10 · CC0".into()
                } else if demo_candidate(&media.id) {
                    format!("demo:{} · 0:30 · sin audio", media.id.local())
                } else {
                    "Fuente no disponible en esta versión".into()
                });
                ui.horizontal_wrapped(|ui| {
                    if self.button(
                        ui,
                        &format!("select:{}", media.id.local()),
                        "Seleccionar",
                        118.0,
                    ) {
                        self.model
                            .as_mut()
                            .unwrap()
                            .start_context(rows.to_vec(), start);
                        self.model.as_mut().unwrap().page = Page::Player;
                    }
                    if self.button(ui, &format!("add:{}", media.id.local()), "+ Cola", 90.0) {
                        self.model
                            .as_mut()
                            .unwrap()
                            .command(Command::Append(vec![media.clone()]));
                    }
                    let saved = self.model.as_ref().unwrap().library.contains(&media.id);
                    if self.button(
                        ui,
                        &format!("save:{}", media.id.local()),
                        if saved { "Guardada" } else { "Guardar" },
                        90.0,
                    ) {
                        self.model.as_mut().unwrap().save_media(media.clone());
                    }
                    if self.model.as_ref().unwrap().page == Page::Library
                        && self.button(ui, &format!("forget:{}", media.id.local()), "Quitar", 90.0)
                    {
                        self.model.as_mut().unwrap().remove_saved(&media.id);
                    }
                });
            });
        }
    }
    fn persist(&mut self, ctx: &egui::Context) {
        if self.pending_write > 0 {
            match self.writer.as_ref().unwrap().completed(self.pending_write) {
                None => ctx.request_repaint_after(Duration::from_millis(50)),
                Some(failed) => {
                    self.pending_write = 0;
                    if failed {
                        self.storage_error = Some(StorageError::Io);
                    } else if self.storage_error == Some(StorageError::Io) {
                        self.storage_error = None;
                    }
                    ctx.request_repaint();
                }
            }
        }
        let model = self.model.as_ref().unwrap();
        let urgent = self.saved_revision != model.revision || self.saved_controls != self.controls;
        if !self.pending && !urgent && self.last_persist.elapsed() < Duration::from_millis(250) {
            ctx.request_repaint_after(
                Duration::from_millis(250).saturating_sub(self.last_persist.elapsed()),
            );
            return;
        }
        let audio_metrics = model.audio.as_ref().and_then(Engine::metrics).map(|m| serde_json::json!({
            "frames":m.frames,"callbacks":m.callbacks,"output_rate":m.output_rate,"pcm_bytes":m.pcm_bytes,
            "streaming":m.streaming,"buffer_capacity_frames":m.buffer_capacity_frames,"buffer_underruns":m.buffer_underruns,"nonzero_samples":m.nonzero_samples,
            "first_pcm_us":m.first_pcm_us,"first_callback_us":m.first_callback_us,"reported_xruns":m.reported_xruns,"first_xrun_frame":m.first_xrun_frame,"last_xrun_frame":m.last_xrun_frame,
        }));
        let state = serde_json::json!({
            "local_import":model.local_files.snapshot(),"notice":model.notice,
            "schema": 1, "pid": std::process::id(), "instance": self.instance, "page": model.page, "query": model.query,
            "revision": model.revision, "green": model.prefs.green, "light": model.prefs.light,
            "current": model.queue.current().map(|e| e.media.id.local()),
            "current_provider":model.queue.current().map(|e|e.media.id.provider.as_str()),"online":model.source_diagnostics(),
            "current_occurrence": model.queue.current().map(|e|e.id.as_u64()),
            "manual_occurrences": model.queue.manual().iter().map(|e|e.id.as_u64()).collect::<Vec<_>>(),
            "position_ms":model.queue.position_ms(),
            "manual": model.queue.manual().iter().map(|e| e.media.id.local()).collect::<Vec<_>>(),
            "upcoming": model.queue.upcoming().map(|e| e.media.id.local()).collect::<Vec<_>>(),
            "playback": format!("{:?}", model.queue.playback()), "audio": model.real_audio(), "audio_metrics": audio_metrics, "volume": model.prefs.volume, "focus":model.focus,
            "repeat": format!("{:?}", model.queue.repeat()), "shuffle": model.queue.shuffled(),
            "library": model.library.items().iter().filter(|media|media.id.provider.as_str()=="demo").map(|media|media.id.local()).collect::<Vec<_>>(),
            "saved_media":model.library.items().iter().map(|media|serde_json::json!({"provider":media.id.provider.as_str(),"id":media.id.local()})).collect::<Vec<_>>(),
            "controls": self.controls,
            "pixels_per_point": ctx.pixels_per_point(), "viewport": [ctx.content_rect().width(), ctx.content_rect().height()],
        });
        let state = serde_json::to_vec(&state).unwrap();
        if !self.pending && self.saved_state == state {
            return;
        }
        let write_queue = model.write_queue
            && (urgent || self.last_queue_persist.elapsed() >= Duration::from_secs(2));
        let queue = if write_queue {
            match persistence::encode(&model.queue) {
                Ok(bytes) => {
                    if self.storage_error == Some(StorageError::Encoding) {
                        self.storage_error = None;
                    }
                    Some(bytes)
                }
                Err(_) => {
                    self.storage_error = Some(StorageError::Encoding);
                    None
                }
            }
        } else {
            None
        };
        let prefs = model
            .write_prefs
            .then(|| serde_json::to_vec(&model.prefs).unwrap());
        match self.writer.as_ref().unwrap().submit_documents(
            prefs,
            queue,
            model.write_library.then(|| model.library.clone()),
            (model.local_files.writable && !model.local_files.index.is_empty())
                .then(|| model.local_files.index.clone()),
            state.clone(),
        ) {
            Ok(id) => {
                self.pending = false;
                self.last_persist = Instant::now();
                if write_queue {
                    self.last_queue_persist = self.last_persist;
                }
                self.saved_revision = model.revision;
                self.saved_controls = self.controls.clone();
                self.pending_write = id;
                self.saved_state = state;
                ctx.request_repaint_after(Duration::from_millis(50));
            }
            Err(persistence::SubmitError::Busy) => {
                self.pending = true;
                ctx.request_repaint_after(Duration::from_millis(50));
            }
            Err(persistence::SubmitError::Closed) => {
                self.pending = false;
                self.saved_state = state;
                self.storage_error = Some(StorageError::Closed);
                ctx.request_repaint();
            }
        }
    }
}

impl DoremiApp {
    pub fn draw(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        #[cfg(target_os = "linux")]
        {
            let dropped = ctx.input_mut(|input| std::mem::take(&mut input.raw.dropped_files));
            if !dropped.is_empty() {
                if dropped.len() == 1
                    && let Some(path) = dropped.into_iter().next().and_then(|file| file.path)
                {
                    self.model.as_mut().unwrap().open_local(Some(path), &ctx);
                } else {
                    let model = self.model.as_mut().unwrap();
                    model.notice = "Importa un archivo WAV cada vez.".into();
                    model.revision += 1;
                }
            }
            self.model.as_mut().unwrap().poll_local(&ctx);
        }
        #[cfg(any(target_os = "linux", target_os = "android"))]
        self.model.as_mut().unwrap().poll_online(&ctx);
        self.model.as_mut().unwrap().poll_audio();
        let model = self.model.as_ref().unwrap();
        if model.queue.playback() == Playback::Loading {
            ctx.request_repaint_after(Duration::from_millis(50));
        } else if model.real_audio() && model.queue.playback() == Playback::Playing {
            // Audio runs independently. The position label needs only 5 updates/s.
            ctx.request_repaint_after(Duration::from_millis(200));
        }
        self.controls.clear();
        let prefs = &self.model.as_ref().unwrap().prefs;
        let accent = if prefs.green {
            egui::Color32::from_rgb(30, 215, 96)
        } else {
            egui::Color32::from_rgb(153, 102, 232)
        };
        let mut visuals = if prefs.light {
            egui::Visuals::light()
        } else {
            egui::Visuals::dark()
        };
        visuals.selection.bg_fill = accent;
        visuals.hyperlink_color = accent;
        ctx.set_visuals(visuals);
        egui::Panel::top("brand").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.image((self.logo.id(), egui::vec2(40.0, 40.0)));
                ui.heading("Doremi");
                ui.colored_label(accent, "Preview nativa");
            });
            if let Some(error) = self
                .model
                .as_ref()
                .unwrap()
                .storage_warning
                .or(self.storage_error.map(StorageError::message))
            {
                ui.colored_label(ui.visuals().error_fg_color, error);
            }
        });
        egui::Panel::bottom("navigation").show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                let width = ((ui.available_width() - 4.0 * GAP) / 5.0).floor().max(48.0);
                for (page, name, label) in [
                    (Page::Home, "home", "Inicio"),
                    (Page::Search, "search", "Buscar"),
                    (Page::Library, "library", "Biblio."),
                    (Page::Queue, "queue", "Cola"),
                    (Page::Settings, "settings", "Ajustes"),
                ] {
                    if self.button(ui, name, label, width) {
                        let model = self.model.as_mut().unwrap();
                        model.page = page;
                        model.revision += 1;
                    }
                }
            });
        });
        egui::CentralPanel::default().show(ui, |ui| {
            let page=self.model.as_ref().unwrap().page;
            egui::ScrollArea::vertical().id_salt(format!("page:{page:?}")).show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                match page {
                    Page::Home => {
                        ui.heading("Tu música, a tu ritmo");ui.label(if cfg!(any(target_os="linux",target_os="android")) {"Audio local y demostración. Commons experimental desde Buscar."}else{"Catálogo de prueba local. Sin Spotify ni conexión."});
                        if self.model.as_ref().unwrap().queue.current().is_some() && self.button(ui,"player","Abrir reproductor",220.0) {
                            let model=self.model.as_mut().unwrap();model.page=Page::Player;model.revision+=1;
                        }
                        if self.button(ui,"local-audio","Probar audio original",240.0) {
                            let model=self.model.as_mut().unwrap();
                            model.start_context(vec![Builtin::Scale.media(),Builtin::Chord.media()],0);
                            model.page=Page::Player;
                        }
                        if self.button(ui,"long-audio","Probar WAV de 2 minutos",260.0) {
                            let model=self.model.as_mut().unwrap();model.start_context(vec![doremi_player::original_long::media(),Builtin::Chord.media()],0);model.page=Page::Player;
                        }
                        self.rows(ui,&songs());
                    }
                    Page::Search => {
                        ui.heading("Buscar");
                        #[cfg(any(target_os="linux",target_os="android"))]
                        {
                            let width=((ui.available_width()-GAP)/2.0).floor().max(TOUCH);
                            ui.horizontal_wrapped(|ui| {
                                if self.button(ui,"source-demo","Demostración",width) {self.model.as_mut().unwrap().set_online_source(false);}
                                if self.button(ui,"source-commons","Commons · CC0",width) {self.model.as_mut().unwrap().set_online_source(true);}
                            });
                            ui.small(if self.model.as_ref().unwrap().online.selected {"Fuente: Commons · dos tonos CC0 de prueba. Guardar añade un favorito, sin descargar audio."}else{"Fuente: demostración offline"});
                        }
                        let online=self.model.as_ref().unwrap().online_search_selected();
                        let response = ui.add_sized([ui.available_width(), TOUCH], egui::TextEdit::singleline(&mut self.model.as_mut().unwrap().query).hint_text(if online {"Busca sine, 440 o 880"}else{"Busca en la demostración"}).char_limit(128));
                        self.controls.push(Control { name: "query".into(), rect: [response.rect.min.x,response.rect.min.y,response.rect.max.x,response.rect.max.y] });
                        if response.changed() { self.model.as_mut().unwrap().search(); }
                        if self.button(ui,"cancel-search","Cancelar búsqueda",200.0) {
                            self.model.as_mut().unwrap().cancel_search();
                        }
                        let results = self.model.as_ref().unwrap().catalog.results().to_vec();
                        if let Some((error,message))=self.model.as_ref().unwrap().search_feedback() {
                            if error {ui.colored_label(ui.visuals().error_fg_color,message);}else {ui.label(message);}
                        } else {ui.label(format!("{} resultados {}", results.len(),if online {"CC0 de Commons"}else{"de demostración"}));}
                        self.rows(ui, &results);
                    }
                    Page::Library => {
                        ui.heading("Biblioteca");
                        let library=self.model.as_ref().unwrap().library.clone();
                        ui.label("Favoritos de este dispositivo. Guardar no descarga el audio online.");
                        if !self.model.as_ref().unwrap().write_library {ui.small("Solo lectura: el archivo anterior se conserva.");}
                        #[cfg(target_os = "linux")]
                        {
                            if self.model.as_ref().unwrap().local_files.pending() {
                                ui.label("Importación en curso…");
                                if self.button(ui,"cancel-import","Cancelar importación",220.0) {self.model.as_mut().unwrap().cancel_local_import();}
                            } else if self.button(ui,"import-wav","Importar WAV",220.0) {self.model.as_mut().unwrap().open_local(None,&ctx);}
                            ui.small("WAV PCM16, mono o estéreo. También puedes arrastrarlo a Doremi.");
                        }
                        self.rows(ui, library.items());
                    }
                    Page::Queue => {
                        ui.heading("Cola");
                        if self.button(ui,"clear","Vaciar cola manual",210.0) { self.model.as_mut().unwrap().command(Command::ClearManual); }
                        ui.label("Las canciones que siguen, en orden.");
                        let entries=self.model.as_ref().unwrap().queue.upcoming().cloned().collect::<Vec<_>>();
                        let manual=self.model.as_ref().unwrap().queue.manual().iter().map(|e|e.id).collect::<Vec<_>>();
                        for entry in entries {
                            ui.strong(&entry.media.title);
                            if let Some(index)=manual.iter().position(|id|*id==entry.id) {
                                ui.small("Añadida por ti");
                                ui.horizontal_wrapped(|ui| {
                                    if self.button(ui,&format!("remove:{index}"),"Quitar",90.0) { self.model.as_mut().unwrap().command(Command::Remove(entry.id)); }
                                    if index>0 && self.button(ui,&format!("up:{index}"),"Subir",90.0) { self.model.as_mut().unwrap().command(Command::Move{entry:entry.id,gap:index-1}); }
                                });
                            } else {ui.small("Continuación");}
                            ui.separator();
                        }
                    }
                    Page::Settings => {
                        ui.heading("Configuración"); ui.label("Acento");
                        ui.horizontal_wrapped(|ui| {
                            for (name, label, value) in [("purple","Morado",false),("green","Verde",true)] {
                                if self.button(ui,name,label,130.0) { let model = self.model.as_mut().unwrap(); model.prefs.green = value; model.revision += 1; }
                            }
                        });
                        ui.label("Tema"); ui.horizontal_wrapped(|ui| {
                            for (name,label,value) in [("dark","Oscuro",false),("light","Claro",true)] {
                                if self.button(ui,name,label,130.0) { let model = self.model.as_mut().unwrap(); model.prefs.light = value; model.revision += 1; }
                            }
                        });
                        ui.label(if cfg!(any(target_os="linux",target_os="android")) {"Datos propios de Doremi. Commons es opcional y no requiere cuenta."}else{"Datos propios de Doremi. Sin permisos de red ni acceso a archivos personales."});
                    }
                    Page::Player => {
                        if self.model.as_ref().unwrap().real_audio() {
                            let online=self.model.as_ref().unwrap().queue.current().is_some_and(|entry|self.model.as_ref().unwrap().online_candidate(&entry.media.id));
                            let file=self.model.as_ref().unwrap().queue.current().is_some_and(|entry|self.model.as_ref().unwrap().local_files.index.get(&entry.media.id).is_some());
                            ui.heading(if online {"Audio CC0 · Commons"}else{"Audio local"}); ui.label(if online {"Tonos de Ubershmekel · CC0 · experimental"}else if file {"Archivo WAV de tu biblioteca"}else{"Muestras originales de Doremi · CC0"});
                            #[cfg(target_os = "android")]
                            ui.small("La reproducción se pausa al salir. MediaSession aún pendiente.");
                            let model = self.model.as_mut().unwrap();
                            let duration = model.queue.current().and_then(|e|e.media.duration_ms).unwrap_or(0);
                            let mut position = model.queue.position_ms();
                            ui.label(format!("{}:{:02} / {}:{:02}",position/60000,position/1000%60,duration/60000,duration/1000%60));
                            let identity=egui::Id::new(("seek",model.queue.current().map(|entry|(entry.id.as_u64(),&entry.media.id))));
                            let response=seek_slider(ui,identity,&mut position,duration);
                            self.controls.push(Control{name:"seek".into(),rect:[response.rect.min.x,response.rect.min.y,response.rect.max.x,response.rect.max.y]});
                            if response.changed() { model.command(Command::Seek(position)); }
                            let response=ui.add_sized([ui.available_width(), TOUCH],egui::Slider::new(&mut model.prefs.volume,0.0..=1.0).text("Volumen").show_value(false));
                            self.controls.push(Control{name:"volume".into(),rect:[response.rect.min.x,response.rect.min.y,response.rect.max.x,response.rect.max.y]});
                            if response.changed() { if let Some(engine)=&model.audio {engine.set_volume(model.prefs.volume);} model.revision+=1; }
                        } else if self.model.as_ref().unwrap().queue.current().is_some_and(|entry|demo_candidate(&entry.media.id)) {
                            ui.heading("Reproducción de demostración"); ui.label("Simulación de controles. Esta selección no produce audio.");
                        } else { ui.heading("Reproducción no disponible"); ui.label("Esta fuente no puede reproducirse en esta versión."); }
                    }
                }
                ui.separator();
                let title = self.model.as_ref().unwrap().queue.current().map(|e| e.media.title.clone()).unwrap_or_else(|| "Ninguna canción seleccionada".into());
                ui.strong(title);
                ui.horizontal_wrapped(|ui| {
                    for (name,label,command) in [("previous","Anterior",Command::Previous),("pause","Pausa",Command::Pause),("play",if self.model.as_ref().unwrap().real_audio() {"Play"} else {"Play demo"},Command::Play),("next","Siguiente",Command::Next),("stop","Cancelar",Command::Cancel)] {
                        if self.button(ui,name,label,100.0) { self.model.as_mut().unwrap().command(command); }
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    if self.button(ui,"shuffle","Shuffle",110.0) {
                        let queue = &self.model.as_ref().unwrap().queue;
                        let order = if queue.shuffled() { Order::Sequential } else { shuffled(queue.context().len(), queue.current().and_then(|current|queue.context().iter().position(|entry|entry.id==current.id))) };
                        self.model.as_mut().unwrap().command(Command::SetOrder(order));
                    }
                    if self.button(ui,"repeat","Repeat",110.0) {
                        let repeat = match self.model.as_ref().unwrap().queue.repeat() { Repeat::Off => Repeat::Context, Repeat::Context => Repeat::Track, Repeat::Track => Repeat::Off };
                        self.model.as_mut().unwrap().command(Command::SetRepeat(repeat));
                    }
                });
                let model = self.model.as_ref().unwrap();
                ui.label(format!("Estado: {:?} · Repeat {:?} · Shuffle {}",model.queue.playback(),model.queue.repeat(),model.queue.shuffled()));
                ui.small(&model.notice);

            });
        });
        self.persist(&ctx);
    }
}
#[cfg(not(target_os = "android"))]
impl eframe::App for DoremiApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.draw(ui);
    }
}
impl Drop for DoremiApp {
    fn drop(&mut self) {
        if let Some(mut model) = self.model.take() {
            #[cfg(any(target_os = "linux", target_os = "android"))]
            model.suspend_online();
            model.command(Command::Cancel);
            // Retain the owner and its idle writer across Activity lifetimes.
            // Never join disk work from the drawing/lifecycle thread.
            *RETAINED.lock().unwrap() = Some(Retained {
                model,
                writer: self.writer.take().unwrap(),
            });
        }
    }
}
impl DoremiApp {
    /// Desktop calls this only after its UI event loop has exited.
    pub fn shutdown() {
        if let Some(retained) = RETAINED.lock().unwrap().take() {
            retained.writer.shutdown();
        }
    }
}
/// Exercise the same host reducer and native output without opening a window.
/// No profile is read or written and no microphone/loopback recording is used.
#[cfg(not(target_os = "android"))]
pub fn audio_probe() -> Result<String, String> {
    probe_controls(
        Model::new(Preferences::default()),
        vec![Builtin::Scale.media(), Builtin::Chord.media()],
    )
    .map(|value| value.to_string())
}
#[cfg(not(target_os = "android"))]
fn probe_controls(mut model: Model, rows: Vec<Media>) -> Result<serde_json::Value, String> {
    let original_gain = model.prefs.volume;
    if rows.len() != 2 {
        return Err("Probe requires two explicit media identities".into());
    }
    let first_id = rows[0].id.clone();
    let second_id = rows[1].id.clone();
    let end_seek = rows[0]
        .duration_ms
        .ok_or("Probe requires known duration")?
        .saturating_sub(400);
    fn wait(
        model: &mut Model,
        label: &str,
        predicate: impl Fn(&Model) -> bool,
    ) -> Result<(), String> {
        let deadline = Instant::now() + Duration::from_secs(12);
        while Instant::now() < deadline {
            model.poll_audio();
            if model.queue.playback() == Playback::Failed {
                return Err(model.notice.clone());
            }
            if predicate(model) {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        Err(format!("Audio probe timed out: {label}; {}", model.notice))
    }
    fn metrics(model: &Model) -> serde_json::Value {
        let m = model
            .audio
            .as_ref()
            .and_then(Engine::metrics)
            .unwrap_or_default();
        serde_json::json!({"current":model.queue.current().map(|e|e.media.id.local()),"position_ms":model.queue.position_ms(),"frames":m.frames,"callbacks":m.callbacks,"output_rate":m.output_rate,"pcm_bytes":m.pcm_bytes,"streaming":m.streaming,"buffer_capacity_frames":m.buffer_capacity_frames,"buffer_underruns":m.buffer_underruns,"nonzero_samples":m.nonzero_samples,"reported_xruns":m.reported_xruns,"first_xrun_frame":m.first_xrun_frame,"last_xrun_frame":m.last_xrun_frame,"first_pcm_us":m.first_pcm_us,"first_callback_us":m.first_callback_us})
    }
    model.enable_audio();
    model.start_context(rows, 0);
    wait(&mut model, "first output", |m| m.queue.position_ms() > 400)?;
    let first = metrics(&model);
    model.command(Command::Pause);
    std::thread::sleep(Duration::from_millis(150));
    model.poll_audio();
    let paused = model.queue.position_ms();
    std::thread::sleep(Duration::from_millis(300));
    model.poll_audio();
    if model.queue.position_ms() != paused {
        return Err("PCM advanced while paused".into());
    }
    model.command(Command::Seek(2000));
    wait(&mut model, "paused seek", |m| {
        m.queue.playback() == Playback::Paused
    })?;
    if model.queue.position_ms() != 2000 {
        return Err("Paused seek did not retain its requested position".into());
    }
    model.audio.as_ref().unwrap().set_volume(0.0);
    model.command(Command::Play);
    wait(&mut model, "zero volume clock", |m| {
        m.queue.position_ms() > 2400
    })?;
    let muted = metrics(&model);
    model.audio.as_ref().unwrap().set_volume(original_gain);
    model.command(Command::Next);
    wait(&mut model, "next track", |m| {
        m.queue.current().is_some_and(|e| e.media.id == second_id) && m.queue.position_ms() > 200
    })?;
    let next = metrics(&model);
    model.command(Command::Previous);
    wait(&mut model, "previous track", |m| {
        m.queue.current().is_some_and(|e| e.media.id == first_id) && m.queue.position_ms() > 200
    })?;
    model.command(Command::Seek(end_seek));
    wait(&mut model, "end-of-track transition", |m| {
        m.queue.current().is_some_and(|e| e.media.id == second_id) && m.queue.position_ms() > 200
    })?;
    let ended = metrics(&model);
    model.command(Command::Cancel);
    if model.queue.playback() != Playback::Paused
        || model.audio.as_ref().unwrap().metrics().is_some()
    {
        return Err("Cancellation left live output".into());
    }
    Ok(
        serde_json::json!({"result":"pass","output":"native device callback; acoustic output not captured","first":first,"zero_volume":muted,"next":next,"after_end":ended,"checks":["play","pause clock stable","seek paused","zero volume clock advances","next","previous","end advances queue","cancel"]}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    static UI_TEST_LOCK: Mutex<()> = Mutex::new(());
    #[test]
    fn a_held_seek_does_not_transfer_to_the_next_occurrence() {
        let ctx = egui::Context::default();
        let render = |events, identity, value: &mut u64, duration| {
            let mut rect = egui::Rect::NOTHING;
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(440.0, 800.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| {
                    rect = seek_slider(ui, egui::Id::new(identity), value, duration).rect;
                },
            );
            output.textures_delta.clear();
            rect
        };
        let mut old = 0;
        let rect = render(vec![], 1_u64, &mut old, 120000);
        let pointer = egui::pos2(rect.min.x + 80.0, rect.center().y);
        let button = |pressed| egui::Event::PointerButton {
            pos: pointer,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::default(),
        };
        render(
            vec![egui::Event::PointerMoved(pointer), button(true)],
            1,
            &mut old,
            120000,
        );
        assert!(old > 60000, "The initial seek must really engage");
        let mut next = 0;
        render(vec![], 2, &mut next, 8000);
        assert_eq!(
            next, 0,
            "A held gesture from another occurrence changed the new track"
        );
        render(vec![button(false)], 2, &mut next, 8000);
        assert_eq!(next, 0);
        render(vec![button(true)], 2, &mut next, 8000);
        assert!(
            next > 4000,
            "A fresh press on the new occurrence must still work"
        );
    }
    use doremi_core::queue::Playback;
    #[cfg(any(target_os = "linux", target_os = "android"))]
    #[test]
    fn source_picker_uses_equal_touch_boxes_and_fits_narrow_and_landscape_views() {
        let _guard = UI_TEST_LOCK.lock().unwrap();
        let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/source-picker-layout-test");
        let ctx = egui::Context::default();
        let mut app = DoremiApp::new_context(&ctx, directory.clone());
        app.model.as_mut().unwrap().page = Page::Search;
        for light in [false, true] {
            app.model.as_mut().unwrap().prefs.light = light;
            for size in [[384.0, 614.0], [682.0, 336.0], [440.0, 800.0]] {
                for _ in 0..3 {
                    let mut output = ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(
                                egui::Pos2::ZERO,
                                size.into(),
                            )),
                            ..Default::default()
                        },
                        |ui| app.draw(ui),
                    );
                    output.textures_delta.clear();
                }
                let controls = app
                    .controls
                    .iter()
                    .filter(|control| control.name.starts_with("source-"))
                    .collect::<Vec<_>>();
                assert_eq!(controls.len(), 2);
                for control in &controls {
                    let [x1, y1, x2, y2] = control.rect;
                    assert!(x1 >= 0.0 && y1 >= 0.0 && x2 <= size[0] && y2 <= size[1]);
                    assert!(x2 - x1 >= TOUCH && y2 - y1 >= TOUCH);
                }
                assert_eq!(controls[0].rect[1], controls[1].rect[1]);
                assert!((controls[1].rect[0] - controls[0].rect[2] - GAP).abs() < 0.1);
            }
        }
        drop(app);
        DoremiApp::shutdown();
        let _ = std::fs::remove_dir_all(directory);
    }
    #[test]
    fn unicode_search_cancel_and_duplicate_occurrences_use_core() {
        let mut model = Model::new(Preferences::default());
        model.query = "Café".into();
        model.search();
        assert_eq!(model.catalog.results()[0].id.local(), "cafe");
        let song = songs()[0].clone();
        model.command(Command::Append(vec![song.clone(), song]));
        assert_ne!(model.queue.manual()[0].id, model.queue.manual()[1].id);
        let id = model.queue.manual()[0].id;
        model.command(Command::Remove(id));
        assert_eq!(model.queue.manual().len(), 1);
        model.catalog.cancel();
        assert!(model.catalog.results().is_empty());
    }
    #[test]
    fn next_pause_and_restore_follow_core() {
        let mut model = Model::new(Preferences::default());
        model.command(Command::StartContext {
            media: songs(),
            start: 1,
            order: Order::Sequential,
        });
        assert_eq!(model.queue.current().unwrap().media.id.local(), "rain");
        model.command(Command::Pause);
        assert_eq!(model.queue.playback(), Playback::Paused);
        let snapshot = model.queue.snapshot();
        model.command(Command::Next);
        assert_eq!(model.queue.current().unwrap().media.id.local(), "cafe");
        model.command(Command::Restore(Box::new(snapshot)));
        assert_eq!(model.queue.current().unwrap().media.id.local(), "rain");
        assert_eq!(model.queue.playback(), Playback::Paused);
    }
    #[test]
    fn duplicate_context_selection_and_shuffle_keep_occurrence_identity() {
        let mut model = Model::new(Preferences::default());
        let rows = vec![songs()[0].clone(), songs()[0].clone(), songs()[1].clone()];
        model.command(Command::SetOrder(Order::Shuffled(vec![])));
        model.start_context(rows, 1);
        assert_eq!(
            model.queue.current().unwrap().id,
            model.queue.context()[1].id
        );
        assert_ne!(
            model.queue.current().unwrap().id,
            model.queue.context()[0].id
        );
        assert!(model.queue.shuffled());
        let mut ids = model.queue.upcoming().map(|e| e.id).collect::<Vec<_>>();
        ids.push(model.queue.current().unwrap().id);
        ids.sort_by_key(|id| id.as_u64());
        ids.dedup();
        assert_eq!(ids.len(), 3);
    }
    #[test]
    fn unavailable_sources_and_missing_output_never_become_fake_playback() {
        let mut model = Model::new(Preferences::default());
        let media = Media {
            id: MediaId::new(ProviderId::new("unavailable").unwrap(), "track").unwrap(),
            title: "Unavailable".into(),
            duration_ms: None,
        };
        model.start_context(vec![media], 0);
        assert_eq!(model.queue.playback(), Playback::Failed);
        model.start_context(vec![Builtin::Scale.media()], 0);
        assert_eq!(model.queue.playback(), Playback::Failed);
        assert!(model.notice.contains("Closed"));
    }
    #[test]
    fn diagnostics_publish_new_page_geometry_before_the_audio_throttle() {
        let _guard = UI_TEST_LOCK.lock().unwrap();
        let directory =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/native-geometry-test");
        let ctx = egui::Context::default();
        DoremiApp::shutdown();
        let mut app = DoremiApp::new_context(&ctx, directory.clone());
        app.model.as_mut().unwrap().page = Page::Home;
        let input = || egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(384.0, 614.0),
            )),
            ..Default::default()
        };
        let mut output = ctx.run_ui(input(), |ui| app.draw(ui));
        output.textures_delta.clear();
        // Emulate an intent applied after drawing Home: the first publication
        // names Player, but still carries Home's controls from that frame.
        app.model.as_mut().unwrap().page = Page::Player;
        app.model.as_mut().unwrap().revision += 1;
        let deadline = Instant::now() + Duration::from_secs(3);
        while app
            .writer
            .as_ref()
            .unwrap()
            .completed(app.pending_write)
            .is_none()
        {
            assert!(
                Instant::now() < deadline,
                "Diagnostic worker failed to acknowledge"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        app.persist(&ctx);
        let deadline = Instant::now() + Duration::from_secs(3);
        while app
            .writer
            .as_ref()
            .unwrap()
            .completed(app.pending_write)
            .is_none()
        {
            assert!(
                Instant::now() < deadline,
                "Diagnostic worker failed to acknowledge"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        app.last_persist = Instant::now();
        let mut output = ctx.run_ui(input(), |ui| app.draw(ui));
        output.textures_delta.clear();
        let state: serde_json::Value = serde_json::from_slice(&app.saved_state).unwrap();
        let pause = state["controls"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == "pause")
            .unwrap();
        assert!(
            pause["rect"][3].as_f64().unwrap() < 550.0,
            "Published controls must belong to the visible Player page"
        );
        assert!(
            !state["controls"]
                .as_array()
                .unwrap()
                .iter()
                .any(|c| c["name"] == "select:dawn")
        );
        drop(app);
        DoremiApp::shutdown();
        let _ = std::fs::remove_dir_all(directory);
    }
    #[test]
    fn touch_navigation_fits_phone_and_landscape_in_both_themes() {
        let _guard = UI_TEST_LOCK.lock().unwrap();
        let directory =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/native-layout-test");
        let ctx = egui::Context::default();
        let mut app = DoremiApp::new_context(&ctx, directory.clone());
        for light in [false, true] {
            app.model.as_mut().unwrap().prefs.light = light;
            for size in [[384.0, 614.0], [682.0, 336.0], [440.0, 800.0]] {
                for _ in 0..3 {
                    let mut output = ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(
                                egui::Pos2::ZERO,
                                size.into(),
                            )),
                            ..Default::default()
                        },
                        |ui| app.draw(ui),
                    );
                    output.textures_delta.clear();
                }
                let navigation: Vec<_> = app
                    .controls
                    .iter()
                    .filter(|c| {
                        ["home", "search", "library", "queue", "settings"]
                            .contains(&c.name.as_str())
                    })
                    .collect();
                assert_eq!(navigation.len(), 5);
                for control in &navigation {
                    let [x1, y1, x2, y2] = control.rect;
                    assert!(
                        x1 >= 0.0 && x2 <= size[0] && y1 >= 0.0 && y2 <= size[1],
                        "{} {:?} at {:?}",
                        control.name,
                        control.rect,
                        size
                    );
                    assert!(x2 - x1 >= TOUCH && y2 - y1 >= TOUCH);
                }
                assert!(
                    navigation
                        .iter()
                        .all(|c| c.rect[1] == navigation[0].rect[1]),
                    "navigation unexpectedly wraps at {size:?}"
                );
                for pair in navigation.windows(2) {
                    assert!(
                        pair[0].rect[2] <= pair[1].rect[0] || pair[0].rect[3] <= pair[1].rect[1]
                    );
                }
            }
        }
        drop(app);
        DoremiApp::shutdown();
        let _ = std::fs::remove_dir_all(directory);
    }
    #[test]
    fn old_preferences_migrate_and_unknown_queue_data_is_preserved() {
        let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target")
            .join(format!("preferences-migration-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(
            directory.join("preferences-v1.json"),
            br#"{"schema":0,"green":true,"light":true,"library":["dawn"]}"#,
        )
        .unwrap();
        let future = br#"{"schema":99,"future_data":"preserve"}"#;
        std::fs::write(directory.join("queue-v1.json"), future).unwrap();
        let model = Model::load(&directory);
        assert!(model.prefs.green && model.prefs.light);
        assert_eq!(model.prefs.schema, 1);
        assert_eq!(model.prefs.library, ["dawn"]);
        assert!(!model.write_queue);
        assert!(model.storage_warning.is_some());
        assert_eq!(
            std::fs::read(directory.join("queue-v1.json")).unwrap(),
            future
        );
        std::fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn queue_view_draws_each_upcoming_occurrence_once() {
        let _guard = UI_TEST_LOCK.lock().unwrap();
        let directory =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/queue-render-test");
        let ctx = egui::Context::default();
        let mut app = DoremiApp::new_context(&ctx, directory.clone());
        let model = app.model.as_mut().unwrap();
        model.command(Command::ClearManual);
        model.command(Command::StartContext {
            media: songs(),
            start: 1,
            order: Order::Sequential,
        });
        model.command(Command::Append(vec![
            songs()[0].clone(),
            songs()[0].clone(),
        ]));
        model.page = Page::Queue;
        fn count(shape: &egui::epaint::Shape) -> usize {
            match shape {
                egui::epaint::Shape::Text(text) => {
                    usize::from(text.galley.text() == "Amanecer violeta")
                }
                egui::epaint::Shape::Vec(shapes) => shapes.iter().map(count).sum(),
                _ => 0,
            }
        }
        let mut copies = 0;
        for _ in 0..3 {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(440.0, 800.0),
                    )),
                    ..Default::default()
                },
                |ui| app.draw(ui),
            );
            copies = output.shapes.iter().map(|shape| count(&shape.shape)).sum();
            output.textures_delta.clear();
        }
        assert_eq!(copies, 2, "two queued copies must produce two rows");
        drop(app);
        DoremiApp::shutdown();
        let _ = std::fs::remove_dir_all(directory);
    }
}
