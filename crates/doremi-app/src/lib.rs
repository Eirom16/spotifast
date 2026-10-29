//! Independent Doremi host. The core owns queue and catalog rules.
use doremi_core::{
    catalog::{Capabilities, Catalog, ProviderCommand, ProviderEvent, Tick},
    media::{Media, MediaId, ProviderId},
    queue::{Command, Effect, Event, Order, Playback, Queue, Repeat},
    requests::SessionId,
};
#[cfg(target_os = "android")]
mod android_native;
use doremi_library as persistence;
use doremi_player::{Builtin, Engine, EngineError, EngineEvent};
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    sync::Mutex,
    time::{Duration, Instant},
};

static NEXT_INSTANCE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(1);
const TOUCH: f32 = 48.0;
const GAP: f32 = 8.0;
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
    storage_warning: Option<&'static str>,
    audio: Option<Engine>,
    focus: bool,
}

fn songs() -> Vec<Media> {
    [
        ("dawn", "Amanecer violeta"),
        ("rain", "Lluvia de cristal"),
        ("cafe", "Café en órbita"),
        ("night", "Noche tranquila"),
    ]
    .into_iter()
    .map(|(id, title)| Media {
        id: MediaId::new(ProviderId::new("demo").unwrap(), id).unwrap(),
        title: title.into(),
        duration_ms: Some(30_000),
    })
    .chain([Builtin::Scale.media(), Builtin::Chord.media()])
    .collect()
}

impl Model {
    fn new(prefs: Preferences) -> Self {
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
            storage_warning: None,
            audio: None,
            focus: !cfg!(target_os = "android"),
        }
    }
    fn load(directory: &std::path::Path) -> Self {
        let mut model = Self::new(Preferences::default());
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
        if let Command::Append(media) = &command
            && self.queue.manual().len().saturating_add(media.len())
                > doremi_core::queue::checkpoint::ENTRY_LIMIT
        {
            self.notice = "La cola manual admite hasta 4096 canciones.".into();
            self.revision += 1;
            return;
        }
        match self.queue.command(command) {
            Ok(effects) => self.effects(effects),
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
        self.queue
            .current()
            .is_some_and(|entry| Builtin::from_media(&entry.media.id).is_some())
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
                    if let Some(builtin) = Builtin::from_media(&entry.media.id) {
                        let result = self
                            .audio
                            .as_mut()
                            .ok_or(EngineError::Closed)
                            .and_then(|engine| engine.load(request, builtin, position_ms));
                        match result {
                            Ok(()) => self.notice = "Preparando audio local original…".into(),
                            Err(error) => {
                                self.notice = format!("No se pudo preparar el audio: {error:?}");
                                self.event(Event::Failed(request));
                            }
                        }
                    } else if songs().iter().any(|s| s.id == entry.media.id) {
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
                        self.notice = "Audio local original · CC0 · 8 segundos".into();
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
        if self.query.trim().is_empty() {
            self.catalog.cancel();
        } else {
            match self
                .catalog
                .search(self.query.clone(), 20, Tick(self.revision), 1000)
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
                                Tick(self.revision),
                            );
                        }
                    }
                }
                Err(error) => self.notice = format!("Búsqueda: {error:?}"),
            }
        }
        self.revision += 1;
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
    fn rows(&mut self, ui: &mut egui::Ui, rows: Vec<Media>) {
        for (start, media) in rows.iter().enumerate() {
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.set_width((ui.available_width() - 4.0).max(0.0));
                ui.label(egui::RichText::new(&media.title).strong());
                ui.small(if Builtin::from_media(&media.id).is_some() {
                    "Audio local original · 0:08 · CC0".into()
                } else {
                    format!("demo:{} · 0:30 · sin audio", media.id.local())
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
                            .start_context(rows.clone(), start);
                        self.model.as_mut().unwrap().page = Page::Player;
                    }
                    if self.button(ui, &format!("add:{}", media.id.local()), "+ Cola", 90.0) {
                        self.model
                            .as_mut()
                            .unwrap()
                            .command(Command::Append(vec![media.clone()]));
                    }
                    if self.button(ui, &format!("save:{}", media.id.local()), "Guardar", 90.0) {
                        let model = self.model.as_mut().unwrap();
                        if !model.prefs.library.iter().any(|id| id == media.id.local()) {
                            model.prefs.library.push(media.id.local().into());
                            model.revision += 1;
                        }
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
            "first_pcm_us":m.first_pcm_us,"first_callback_us":m.first_callback_us,"reported_xruns":m.reported_xruns,"first_xrun_frame":m.first_xrun_frame,"last_xrun_frame":m.last_xrun_frame,
        }));
        let state = serde_json::json!({
            "schema": 1, "pid": std::process::id(), "instance": self.instance, "page": model.page, "query": model.query,
            "revision": model.revision, "green": model.prefs.green, "light": model.prefs.light,
            "current": model.queue.current().map(|e| e.media.id.local()),
            "current_occurrence": model.queue.current().map(|e|e.id.as_u64()),
            "manual_occurrences": model.queue.manual().iter().map(|e|e.id.as_u64()).collect::<Vec<_>>(),
            "position_ms":model.queue.position_ms(),
            "manual": model.queue.manual().iter().map(|e| e.media.id.local()).collect::<Vec<_>>(),
            "upcoming": model.queue.upcoming().map(|e| e.media.id.local()).collect::<Vec<_>>(),
            "playback": format!("{:?}", model.queue.playback()), "audio": model.real_audio(), "audio_metrics": audio_metrics, "volume": model.prefs.volume, "focus":model.focus,
            "repeat": format!("{:?}", model.queue.repeat()), "shuffle": model.queue.shuffled(),
            "library": model.prefs.library, "controls": self.controls,
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
        match self
            .writer
            .as_ref()
            .unwrap()
            .submit(prefs, queue, state.clone())
        {
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
                        ui.heading("Tu música, a tu ritmo");ui.label("Catálogo de prueba local. Sin Spotify ni conexión.");
                        if self.model.as_ref().unwrap().queue.current().is_some() && self.button(ui,"player","Abrir reproductor",220.0) {
                            let model=self.model.as_mut().unwrap();model.page=Page::Player;model.revision+=1;
                        }
                        if self.button(ui,"local-audio","Probar audio original",240.0) {
                            let model=self.model.as_mut().unwrap();
                            model.start_context(vec![Builtin::Scale.media(),Builtin::Chord.media()],0);
                            model.page=Page::Player;
                        }
                        self.rows(ui,songs());
                    }
                    Page::Search => {
                        ui.heading("Buscar");
                        let response = ui.add_sized([ui.available_width(), TOUCH], egui::TextEdit::singleline(&mut self.model.as_mut().unwrap().query).hint_text("Busca en la demostración").char_limit(128));
                        self.controls.push(Control { name: "query".into(), rect: [response.rect.min.x,response.rect.min.y,response.rect.max.x,response.rect.max.y] });
                        if response.changed() { self.model.as_mut().unwrap().search(); }
                        if self.button(ui,"cancel-search","Cancelar búsqueda",200.0) {
                            let model = self.model.as_mut().unwrap(); model.query.clear(); model.catalog.cancel(); model.revision += 1;
                        }
                        let results = self.model.as_ref().unwrap().catalog.results().to_vec();
                        ui.label(format!("{} resultados de demostración", results.len()));
                        self.rows(ui, results);
                    }
                    Page::Library => {
                        ui.heading("Biblioteca");
                        let prefs = &self.model.as_ref().unwrap().prefs;
                        let rows = songs().into_iter().filter(|s| prefs.library.iter().any(|id| id == s.id.local())).collect();
                        ui.label("Tus canciones guardadas en este dispositivo."); self.rows(ui, rows);
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
                        ui.label("Datos propios de Doremi. Sin permisos de red ni acceso a archivos personales.");
                    }
                    Page::Player => {
                        if self.model.as_ref().unwrap().real_audio() {
                            ui.heading("Audio local"); ui.label("Muestras originales de Doremi · CC0");
                            #[cfg(target_os = "android")]
                            ui.small("La reproducción se pausa al salir. MediaSession aún pendiente.");
                            let model = self.model.as_mut().unwrap();
                            let duration = model.queue.current().and_then(|e|e.media.duration_ms).unwrap_or(0);
                            let mut position = model.queue.position_ms();
                            ui.label(format!("{}:{:02} / {}:{:02}",position/60000,position/1000%60,duration/60000,duration/1000%60));
                            let response=ui.add_sized([ui.available_width(), TOUCH],egui::Slider::new(&mut position,0..=duration).text("Posición").show_value(false));
                            self.controls.push(Control{name:"seek".into(),rect:[response.rect.min.x,response.rect.min.y,response.rect.max.x,response.rect.max.y]});
                            if response.changed() { model.command(Command::Seek(position)); }
                            let response=ui.add_sized([ui.available_width(), TOUCH],egui::Slider::new(&mut model.prefs.volume,0.0..=1.0).text("Volumen").show_value(false));
                            self.controls.push(Control{name:"volume".into(),rect:[response.rect.min.x,response.rect.min.y,response.rect.max.x,response.rect.max.y]});
                            if response.changed() { if let Some(engine)=&model.audio {engine.set_volume(model.prefs.volume);} model.revision+=1; }
                        } else { ui.heading("Reproducción de demostración"); ui.label("Simulación de controles. Esta selección no produce audio."); }
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
        serde_json::json!({"current":model.queue.current().map(|e|e.media.id.local()),"position_ms":model.queue.position_ms(),"frames":m.frames,"callbacks":m.callbacks,"output_rate":m.output_rate,"pcm_bytes":m.pcm_bytes,"reported_xruns":m.reported_xruns,"first_xrun_frame":m.first_xrun_frame,"last_xrun_frame":m.last_xrun_frame,"first_pcm_us":m.first_pcm_us,"first_callback_us":m.first_callback_us})
    }
    let mut model = Model::new(Preferences::default());
    model.enable_audio();
    model.start_context(vec![Builtin::Scale.media(), Builtin::Chord.media()], 0);
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
    model.audio.as_ref().unwrap().set_volume(0.7);
    model.command(Command::Next);
    wait(&mut model, "next track", |m| {
        m.queue
            .current()
            .is_some_and(|e| e.media.id.local() == "tone-chord")
            && m.queue.position_ms() > 200
    })?;
    let next = metrics(&model);
    model.command(Command::Previous);
    wait(&mut model, "previous track", |m| {
        m.queue
            .current()
            .is_some_and(|e| e.media.id.local() == "tone-scale")
            && m.queue.position_ms() > 200
    })?;
    model.command(Command::Seek(7600));
    wait(&mut model, "end-of-track transition", |m| {
        m.queue
            .current()
            .is_some_and(|e| e.media.id.local() == "tone-chord")
            && m.queue.position_ms() > 200
    })?;
    let ended = metrics(&model);
    model.command(Command::Cancel);
    if model.queue.playback() != Playback::Paused
        || model.audio.as_ref().unwrap().metrics().is_some()
    {
        return Err("Cancellation left live output".into());
    }
    Ok(serde_json::json!({"result":"pass","output":"native device callback; acoustic output not captured","first":first,"zero_volume":muted,"next":next,"after_end":ended,"checks":["play","pause clock stable","seek paused","zero volume clock advances","next","previous","end advances queue","cancel"]}).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    static UI_TEST_LOCK: Mutex<()> = Mutex::new(());
    use doremi_core::queue::Playback;
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
