//! Independent Doremi host. The core owns queue and catalog rules.
use doremi_core::{
    catalog::{Capabilities, Catalog, ProviderCommand, ProviderEvent, Tick},
    media::{Media, MediaId, ProviderId},
    queue::{Command, Effect, Event, Order, Playback, Queue, Repeat},
    requests::SessionId,
};
#[cfg(target_os = "android")]
mod android_native;
#[cfg(any(target_os = "linux", target_os = "android", target_os = "windows"))]
mod artwork;
#[cfg(any(target_os = "linux", target_os = "android", target_os = "windows"))]
mod music_catalog;
#[cfg(any(target_os = "linux", target_os = "android", target_os = "windows"))]
mod online;
#[cfg(any(target_os = "linux", target_os = "android", target_os = "windows"))]
mod online_model;
#[cfg(target_os = "linux")]
mod online_probe;
#[cfg(any(target_os = "linux", target_os = "android", target_os = "windows"))]
mod youtube_session;
use doremi_library as persistence;
#[cfg(any(target_os = "linux", target_os = "android"))]
mod file_input;
mod listening;
mod local_media;
mod playlists;
mod presentation;
mod queue_ui;
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
    let mut palette = if ui.visuals().dark_mode {
        doremi_ui::Palette::dark()
    } else {
        doremi_ui::Palette::light()
    };
    palette.accent = ui.visuals().selection.stroke.color;
    let value = if duration == 0 {
        0.0
    } else {
        *position as f32 / duration as f32
    };
    let (event, mut response) = doremi_ui::thin_slider_sized(
        ui,
        &palette,
        identity,
        "Posición",
        value,
        egui::vec2(ui.available_width(), ui.spacing().interact_size.y.max(16.0)),
        None,
    );
    match event {
        doremi_ui::SliderEvent::Dragging(value) => {
            *position = (f64::from(value) * duration as f64).round() as u64
        }
        doremi_ui::SliderEvent::Committed(value) => {
            *position = (f64::from(value) * duration as f64).round() as u64;
            response.mark_changed();
        }
        doremi_ui::SliderEvent::None => {}
    }
    response
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
    Playlist,
    History,
}

struct Model {
    queue: Queue,
    catalog: Catalog,
    prefs: Preferences,
    page: Page,
    player_return: Page,
    query: String,
    notice: String,
    revision: u64,
    write_prefs: bool,
    write_queue: bool,
    library: persistence::collection::Collection,
    write_library: bool,
    history: persistence::history::History,
    write_history: bool,
    history_clear_pending: bool,
    listening: doremi_core::listening::Counter,
    listening_media: Option<Media>,
    playlists: persistence::playlists::Playlists,
    write_playlists: bool,
    selected_playlist: Option<u64>,
    playlist_name: String,
    playlist_edit_name: String,
    playlist_delete_confirmation: bool,
    playlist_notice: Option<String>,
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
    #[cfg(any(target_os = "linux", target_os = "android", target_os = "windows"))]
    online: online_model::State,
    #[cfg(any(target_os = "linux", target_os = "android", target_os = "windows"))]
    music: music_catalog::State,
    #[cfg(any(target_os = "linux", target_os = "android", target_os = "windows"))]
    artwork: artwork::State,
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
            player_return: Page::Home,
            query: String::new(),
            notice: "Demostración sin audio. No es reproducción real.".into(),
            revision: 0,
            write_prefs: true,
            write_queue: true,
            library,
            write_library,
            history: persistence::history::History::default(),
            write_history: true,
            history_clear_pending: false,
            listening: doremi_core::listening::Counter::default(),
            listening_media: None,
            playlists: persistence::playlists::Playlists::default(),
            write_playlists: true,
            selected_playlist: None,
            playlist_name: String::new(),
            playlist_edit_name: String::new(),
            playlist_delete_confirmation: false,
            playlist_notice: None,
            storage_warning: None,
            audio: None,
            focus: !cfg!(target_os = "android"),
            original_path: None,
            wav_input: None,
            local_files: local_media::State::new(),
            prepared: Vec::new(),
            #[cfg(any(target_os = "linux", target_os = "android", target_os = "windows"))]
            online: online_model::State::new(),
            #[cfg(any(target_os = "linux", target_os = "android", target_os = "windows"))]
            music: music_catalog::State::new(),
            #[cfg(any(target_os = "linux", target_os = "android", target_os = "windows"))]
            artwork: artwork::State::new(),
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
        #[cfg(any(target_os = "linux", target_os = "android", target_os = "windows"))]
        {
            model.music.selected = true;
        }
        model.load_library(directory);
        model.load_playlists(directory);
        model.load_history(directory);
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
        self.command_with_import_policy(command, true);
    }
    fn command_with_import_policy(&mut self, command: Command, cancel_import: bool) {
        self.note_history();
        let cancelled = matches!(&command, Command::Cancel);
        if cancelled && cancel_import {
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
        self.note_history();
        for effect in effects {
            match effect {
                Effect::Load {
                    request,
                    entry,
                    position_ms,
                    ..
                } => {
                    self.listening.bind(&entry, request);
                    self.listening_media = Some(entry.media.clone());
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
                                |engine| engine.load_file(request, path.to_path_buf(), position_ms),
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
        self.note_history();
        let event = self.audio.as_ref().and_then(Engine::poll);
        if let Some(event) = event {
            let request = match event {
                EngineEvent::Ready(r) | EngineEvent::Failed(r, _) | EngineEvent::Ended(r) => r,
            };
            if self.audio.as_ref().and_then(Engine::current_request) == Some(request) {
                match event {
                    EngineEvent::Ready(r) => {
                        self.notice = if self.queue.current().is_some_and(|entry| {
                            entry.media.id.provider.as_str() == "youtube-music"
                        }) {
                            "YouTube Music · reproducción nativa"
                        } else if self
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
                            "Archivo de audio local seleccionado"
                        }
                        .into();
                        self.event(Event::Ready(r));
                    }
                    EngineEvent::Ended(r) => {
                        self.listening.reset();
                        self.listening_media = None;
                        self.event(Event::Ended(r));
                    }
                    EngineEvent::Failed(r, error) => {
                        self.notice = match error {
                            EngineError::Decode(doremi_player::DecodeError::AccessDenied) => "El origen denegó el acceso al audio de esta pista.".into(),
                            EngineError::Input | EngineError::Decode(doremi_player::DecodeError::Input) => "No se pudo leer el audio desde su origen. Comprueba su disponibilidad o permisos.".into(),
                            EngineError::Decode(doremi_player::DecodeError::InvalidMp4) => "La pista no contiene un MP4 de audio que Doremi pueda decodificar.".into(),
                            _ => format!("La salida de audio falló: {error:?}"),
                        };
                        #[cfg(any(
                            target_os = "linux",
                            target_os = "android",
                            target_os = "windows"
                        ))]
                        if let Some(message) = self.online.failure_message() {
                            self.notice = message.into();
                        }
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
        #[cfg(any(target_os = "linux", target_os = "android", target_os = "windows"))]
        if self.music.selected {
            self.music.search(&self.query);
            self.revision += 1;
            return;
        }

        #[cfg(any(target_os = "linux", target_os = "android", target_os = "windows"))]
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
        #[cfg(any(target_os = "linux", target_os = "android", target_os = "windows"))]
        {
            Tick(self.online.origin.elapsed().as_millis() as u64)
        }
        #[cfg(not(any(target_os = "linux", target_os = "android", target_os = "windows")))]
        {
            Tick(self.revision)
        }
    }
    fn cancel_search(&mut self) {
        #[cfg(any(target_os = "linux", target_os = "android", target_os = "windows"))]
        self.music.cancel();

        self.query.clear();
        self.catalog.cancel();
        #[cfg(any(target_os = "linux", target_os = "android", target_os = "windows"))]
        self.cancel_online_search();
        self.revision += 1;
    }
    fn online_candidate(&self, _id: &MediaId) -> bool {
        #[cfg(any(target_os = "linux", target_os = "android", target_os = "windows"))]
        {
            online_model::candidate(_id)
        }
        #[cfg(not(any(target_os = "linux", target_os = "android", target_os = "windows")))]
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
        #[cfg(any(target_os = "linux", target_os = "android", target_os = "windows"))]
        {
            self.begin_online_load(_request, _id, _position)
        }
        #[cfg(not(any(target_os = "linux", target_os = "android", target_os = "windows")))]
        {
            false
        }
    }
    fn cancel_online(&mut self, _request: Option<doremi_core::queue::RequestId>) {
        #[cfg(any(target_os = "linux", target_os = "android", target_os = "windows"))]
        self.cancel_online_load(_request);
    }
    fn online_search_selected(&self) -> bool {
        #[cfg(any(target_os = "linux", target_os = "android", target_os = "windows"))]
        {
            self.online.selected
        }
        #[cfg(not(any(target_os = "linux", target_os = "android", target_os = "windows")))]
        {
            false
        }
    }
    fn source_diagnostics(&self) -> serde_json::Value {
        #[cfg(any(target_os = "linux", target_os = "android", target_os = "windows"))]
        {
            let mut state = self.online.snapshot();
            state["search_status"] =
                serde_json::json!(format!("{:?}", self.catalog.search_status()));
            state
        }
        #[cfg(not(any(target_os = "linux", target_os = "android", target_os = "windows")))]
        {
            serde_json::json!({"search_source":"demo","phase":"unavailable"})
        }
    }
    fn search_feedback(&self) -> Option<(bool, String)> {
        #[cfg(any(target_os = "linux", target_os = "android", target_os = "windows"))]
        {
            self.online.feedback(self.catalog.search_status())
        }
        #[cfg(not(any(target_os = "linux", target_os = "android", target_os = "windows")))]
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
#[derive(Clone, Copy, PartialEq, Eq)]
enum TextField {
    Search,
    PlaylistName,
    PlaylistRename,
    SessionCookie,
}
impl TextField {
    fn key(self) -> &'static str {
        match self {
            Self::Search => "search",
            Self::PlaylistName => "playlist-name",
            Self::PlaylistRename => "playlist-rename",
            Self::SessionCookie => "session-cookie",
        }
    }
}
pub struct DoremiApp {
    instance: usize,
    editor: TextField,
    model: Option<Model>,
    writer: Option<persistence::Writer>,
    pending_write: usize,
    storage_error: Option<StorageError>,
    controls: Vec<Control>,
    page_rect: [f32; 4],
    saved_state: Vec<u8>,
    pending: bool,
    last_persist: Instant,
    last_queue_persist: Instant,
    saved_revision: u64,
    saved_controls: Vec<Control>,
    #[cfg(any(target_os = "linux", target_os = "android", target_os = "windows"))]
    artwork_textures: artwork::Textures,
}

impl DoremiApp {
    #[cfg(not(target_os = "android"))]
    pub fn new(cc: &eframe::CreationContext<'_>, directory: PathBuf) -> Self {
        let app = Self::new_context(&cc.egui_ctx, directory);
        #[cfg(any(target_os = "linux", target_os = "windows"))]
        let mut app = app;
        #[cfg(any(target_os = "linux", target_os = "windows"))]
        if std::env::var_os("DOREMI_DATA_DIR").is_none() {
            app.model.as_mut().unwrap().music.session.restore();
            app.model
                .as_mut()
                .unwrap()
                .youtube_page(doremi_youtube::Request::Home);
        }
        app
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
        doremi_ui::install(ctx);
        Self {
            instance: NEXT_INSTANCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            editor: TextField::Search,
            model: Some(model),
            writer: Some(writer),
            pending_write: 0,
            storage_error: None,
            controls: vec![],
            page_rect: [0.0; 4],
            saved_state: vec![],
            pending: true,
            last_persist: Instant::now(),
            last_queue_persist: Instant::now(),
            saved_revision: u64::MAX,
            saved_controls: vec![],
            #[cfg(any(target_os = "linux", target_os = "android", target_os = "windows"))]
            artwork_textures: artwork::Textures::default(),
        }
    }
    fn button(&mut self, ui: &mut egui::Ui, name: &str, label: &str, width: f32) -> bool {
        let height = if ui.ctx().content_rect().width() >= 760.0 {
            32.0
        } else {
            TOUCH
        };
        let response = ui.add_sized([width.max(TOUCH), height], egui::Button::new(label));
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
            "streaming":m.streaming,"buffering":m.buffering,"buffer_capacity_frames":m.buffer_capacity_frames,"buffer_underruns":m.buffer_underruns,"nonzero_samples":m.nonzero_samples,
            "first_pcm_us":m.first_pcm_us,"first_callback_us":m.first_callback_us,"reported_xruns":m.reported_xruns,"first_xrun_frame":m.first_xrun_frame,"last_xrun_frame":m.last_xrun_frame,
        }));
        #[cfg(any(target_os = "linux", target_os = "android", target_os = "windows"))]
        let music_catalog = {
            let mut value = model.music.snapshot();
            value["artwork"] = model.artwork.snapshot();
            value
        };
        #[cfg(not(any(target_os = "linux", target_os = "android", target_os = "windows")))]
        let music_catalog = serde_json::Value::Null;
        let mut state = serde_json::json!({
            "input":ctx.input(|input|serde_json::json!({"focused":input.raw.focused,"primary_down":input.pointer.primary_down(),"position":input.pointer.hover_pos().map(|point|[point.x,point.y])})),
            "local_import":model.local_files.snapshot(),"notice":model.notice,
            "editor_target":self.editor.key(),"editor_text":if self.editor == TextField::SessionCookie {""} else {self.editor_text()},
            "playlists":model.playlists.lists().iter().map(|list| serde_json::json!({"id":list.id,"name":list.name,"entries":list.entries.iter().map(|media|serde_json::json!({"provider":media.id.provider.as_str(),"id":media.id.local()})).collect::<Vec<_>>()})).collect::<Vec<_>>(),
            "selected_playlist":model.selected_playlist,
            "schema": 1, "pid": std::process::id(), "instance": self.instance, "page": model.page, "player_return":model.player_return, "query": model.query,
            "revision": model.revision, "green": model.prefs.green, "light": model.prefs.light,
            "current": model.queue.current().map(|e| e.media.id.local()),
            "current_provider":model.queue.current().map(|e|e.media.id.provider.as_str()),"online":model.source_diagnostics(),
            "music_catalog":music_catalog,
            "current_occurrence": model.queue.current().map(|e|e.id.as_u64()),
            "context_occurrences":model.queue.context().iter().map(|e|e.id.as_u64()).collect::<Vec<_>>(),
            "manual_occurrences": model.queue.manual().iter().map(|e|e.id.as_u64()).collect::<Vec<_>>(),
            "position_ms":model.queue.position_ms(),
            "manual": model.queue.manual().iter().map(|e| e.media.id.local()).collect::<Vec<_>>(),
            "upcoming": model.queue.upcoming().map(|e| e.media.id.local()).collect::<Vec<_>>(),
            "playback": format!("{:?}", model.queue.playback()), "audio": model.real_audio(), "audio_metrics": audio_metrics, "volume": model.prefs.volume, "focus":model.focus,
            "repeat": format!("{:?}", model.queue.repeat()), "shuffle": model.queue.shuffled(),
            "library": model.library.items().iter().filter(|media|media.id.provider.as_str()=="demo").map(|media|media.id.local()).collect::<Vec<_>>(),
            "saved_media":model.library.items().iter().map(|media|serde_json::json!({"provider":media.id.provider.as_str(),"id":media.id.local()})).collect::<Vec<_>>(),
            "controls": self.controls,"page_rect":self.page_rect,
            "pixels_per_point": ctx.pixels_per_point(), "viewport": [ctx.content_rect().width(), ctx.content_rect().height()],
        });
        state["listening_history"] = serde_json::json!({"count":model.history.plays().len(),"writable":model.write_history,"recent":model.history.recent(16).iter().map(|media|serde_json::json!({"provider":media.id.provider.as_str(),"id":media.id.local()})).collect::<Vec<_>>()});
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
        match self.writer.as_ref().unwrap().submit_snapshot(
            prefs,
            queue,
            persistence::UserDocuments {
                library: model.write_library.then(|| model.library.clone()),
                local_files: (model.local_files.writable && !model.local_files.index.is_empty())
                    .then(|| model.local_files.index.clone()),
                playlists: model.write_playlists.then(|| model.playlists.clone()),
                history: model.write_history.then(|| model.history.clone()),
            },
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
                    && let Some(path) = dropped
                        .into_iter()
                        .next()
                        .map(|file| file.path().to_path_buf())
                {
                    self.model.as_mut().unwrap().open_local(Some(path), &ctx);
                } else {
                    let model = self.model.as_mut().unwrap();
                    model.notice = "Importa un archivo de audio cada vez.".into();
                    model.revision += 1;
                }
            }
        }
        #[cfg(any(target_os = "linux", target_os = "android"))]
        self.model.as_mut().unwrap().poll_local(&ctx);
        #[cfg(any(target_os = "linux", target_os = "android", target_os = "windows"))]
        {
            self.model.as_mut().unwrap().poll_online(&ctx);
            self.model.as_mut().unwrap().music.poll(&ctx);
            self.poll_artwork(&ctx);
        }
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
        let palette = doremi_ui::palette(prefs.light, prefs.green);
        let mut style = (*ctx.global_style()).clone();
        doremi_ui::apply_to_style(&mut style, &palette);
        if ctx.content_rect().width() < 760.0 {
            style.spacing.interact_size.y = TOUCH;
        }
        ctx.set_global_style(style);
        self.shell(ui, &palette);
        self.persist(&ctx);
    }

    pub(crate) fn search_field(&mut self, ui: &mut egui::Ui) {
        let online = self.model.as_ref().unwrap().online_search_selected();
        let response = ui.add_sized(
            [ui.available_width(), TOUCH],
            egui::TextEdit::singleline(&mut self.model.as_mut().unwrap().query)
                .id(egui::Id::new("catalog-query"))
                .hint_text(if online {
                    "Busca música CC0, por ejemplo Chopin"
                } else {
                    "Busca música por nombre o artista"
                })
                .char_limit(128),
        );
        self.controls.push(Control {
            name: "query".into(),
            rect: [
                response.rect.min.x,
                response.rect.min.y,
                response.rect.max.x,
                response.rect.max.y,
            ],
        });
        if response.has_focus() {
            self.editor = TextField::Search;
        }
        if response.changed() {
            self.model.as_mut().unwrap().search();
        }
        if ui.ctx().content_rect().width() >= 760.0 {
            self.follow_editor(ui, &response, TextField::Search);
        }
    }

    fn page_content(&mut self, ui: &mut egui::Ui) {
        #[cfg(any(target_os = "linux", target_os = "android"))]
        let ctx = ui.ctx().clone();
        let page = self.model.as_ref().unwrap().page;
        match page {
            Page::Home => {
                self.home_header(ui);
                self.recent_shelf(ui);
                #[cfg(any(target_os = "linux", target_os = "android", target_os = "windows"))]
                if self.model.as_ref().unwrap().music.home_root() {
                    ui.add_space(16.0);
                    ui.heading("YouTube Music");
                    self.music_results(ui);
                }
                ui.add_space(16.0);
                ui.heading("Tu biblioteca");
                let library = self.model.as_ref().unwrap().library.clone();
                if library.items().is_empty() {
                    ui.weak("Guarda música desde Buscar para encontrarla aquí.");
                } else {
                    self.rows(ui, library.items());
                }
                ui.add_space(16.0);
                let response=egui::CollapsingHeader::new("Muestras originales y demostración").id_salt("demo-examples").show(ui,|ui| {
                    ui.small("Muestras de prueba CC0 y datos de demostración. No son resultados del catálogo musical.");
                    if self.button(ui,"local-audio","Audio original",220.0) {
                        let model=self.model.as_mut().unwrap();model.start_context(vec![Builtin::Scale.media(),Builtin::Chord.media()],0);self.navigate(Page::Player);
                    }
                    if self.button(ui,"long-audio","Muestra original de 2 minutos",260.0) {
                        let model=self.model.as_mut().unwrap();model.start_context(vec![doremi_player::original_long::media(),Builtin::Chord.media()],0);self.navigate(Page::Player);
                    }
                    self.rows(ui,&songs());
                });
                self.record("demo-examples", &response.header_response);
            }
            Page::History => self.history_page(ui),
            Page::Search => {
                #[cfg(any(target_os = "linux", target_os = "android", target_os = "windows"))]
                if self.model.as_ref().unwrap().music.selected
                    && self.model.as_ref().unwrap().music.detail.is_some()
                {
                    self.music_results(ui);
                    return;
                }
                if ui.ctx().content_rect().width() >= 760.0 {
                    ui.heading("Buscar");
                }
                #[cfg(any(target_os = "linux", target_os = "android", target_os = "windows"))]
                {
                    let width = ((ui.available_width() - 3.0 * GAP) / 4.0)
                        .floor()
                        .max(TOUCH);
                    ui.horizontal_wrapped(|ui| {
                        if ui.ctx().content_rect().width() < 760.0 {
                            ui.spacing_mut().button_padding.x = 4.0;
                        }
                        if self.button(
                            ui,
                            "source-youtube",
                            if ui.ctx().content_rect().width() < 760.0 {
                                "YouTube\nMusic"
                            } else {
                                "YouTube Music"
                            },
                            width,
                        ) {
                            let model = self.model.as_mut().unwrap();
                            model.cancel_search();
                            model.set_online_source(false);
                            model.music.selected = true;
                            model.music.youtube = true;
                            model.search();
                            model.revision += 1;
                        }
                        if self.button(
                            ui,
                            "source-musicbrainz",
                            if ui.ctx().content_rect().width() < 760.0 {
                                "Music\nBrainz"
                            } else {
                                "MusicBrainz"
                            },
                            width,
                        ) {
                            let model = self.model.as_mut().unwrap();
                            model.cancel_search();
                            model.set_online_source(false);
                            model.music.selected = true;
                            model.music.youtube = false;
                            if model.music.kind == doremi_core::media::EntityKind::Playlist {
                                model.music.kind = doremi_core::media::EntityKind::Recording;
                            }
                            model.search();
                            model.revision += 1;
                        }
                        if self.button(ui, "source-commons", "Commons", width) {
                            self.model.as_mut().unwrap().set_online_source(true);
                        }
                        if self.button(ui, "source-demo", "Demo", width) {
                            self.model.as_mut().unwrap().set_online_source(false);
                        }
                    });
                    let model = self.model.as_ref().unwrap();
                    ui.small(if model.music.selected {
                        if model.music.youtube {
                            "Fuente: YouTube Music"
                        } else {
                            "Fuente: MusicBrainz · metadatos opcionales; no proporciona audio."
                        }
                    } else if model.online.selected {
                        "Fuente: Commons · audio CC0 declarado por su autor · Vorbis hasta 10 min."
                    } else {
                        "Fuente: demostración offline · sin audio."
                    });
                    if self.model.as_ref().unwrap().music.selected {
                        ui.horizontal_wrapped(|ui| {
                            for (kind, label) in [
                                (doremi_core::media::EntityKind::Recording, "Canciones"),
                                (doremi_core::media::EntityKind::Artist, "Artistas"),
                                (doremi_core::media::EntityKind::Release, "Álbumes"),
                                (doremi_core::media::EntityKind::Playlist, "Playlists"),
                            ] {
                                if kind == doremi_core::media::EntityKind::Playlist
                                    && !self.model.as_ref().unwrap().music.youtube
                                {
                                    continue;
                                }
                                let response = ui.selectable_label(
                                    self.model.as_ref().unwrap().music.kind == kind,
                                    label,
                                );
                                self.record(&format!("catalog-kind:{kind:?}"), &response);
                                if response.clicked() {
                                    let model = self.model.as_mut().unwrap();
                                    model.music.kind = kind;
                                    model.search();
                                }
                            }
                        });
                    }
                }
                if ui.ctx().content_rect().width() >= 760.0 {
                    self.search_field(ui);
                }
                if self.button(ui, "cancel-search", "Cancelar búsqueda", 200.0) {
                    self.model.as_mut().unwrap().cancel_search();
                }
                #[cfg(any(target_os = "linux", target_os = "android", target_os = "windows"))]
                if self.model.as_ref().unwrap().music.selected {
                    self.music_results(ui);
                    return;
                }
                let online = self.model.as_ref().unwrap().online_search_selected();
                let results = self.model.as_ref().unwrap().catalog.results().to_vec();
                if let Some((error, message)) = self.model.as_ref().unwrap().search_feedback() {
                    if error {
                        ui.colored_label(ui.visuals().error_fg_color, message);
                    } else {
                        ui.label(message);
                    }
                } else {
                    ui.label(format!(
                        "{} {} {}",
                        results.len(),
                        if results.len() == 1 {
                            "resultado"
                        } else {
                            "resultados"
                        },
                        if online {
                            "CC0 de Commons"
                        } else {
                            "de demostración"
                        }
                    ));
                }
                self.rows(ui, &results);
            }
            Page::Library => {
                ui.heading("Biblioteca");
                let library = self.model.as_ref().unwrap().library.clone();
                ui.label("Tu música y tus playlists, guardadas en este dispositivo.");
                if !self.model.as_ref().unwrap().write_library {
                    ui.small("Solo lectura: el archivo anterior se conserva.");
                }
                ui.add_space(8.0);
                self.playlist_library(ui);
                ui.add_space(16.0);
                ui.heading("Tu música guardada");
                self.rows(ui, library.items());
                if !library.items().is_empty() {
                    ui.add_space(12.0);
                }
                #[cfg(any(target_os = "linux", target_os = "android"))]
                {
                    if self.model.as_ref().unwrap().local_files.pending() {
                        ui.label("Importación en curso…");
                        if self.button(ui, "cancel-import", "Cancelar importación", 220.0) {
                            self.model.as_mut().unwrap().cancel_local_import();
                        }
                    } else if self.button(ui, "import-wav", "Importar audio", 220.0) {
                        self.model.as_mut().unwrap().open_local(None, &ctx);
                    }
                    if !self.model.as_ref().unwrap().local_files.pending()
                        && let Some((error, message)) =
                            self.model.as_ref().unwrap().local_files.feedback
                    {
                        let response = if error {
                            ui.colored_label(ui.visuals().error_fg_color, message)
                        } else {
                            ui.label(message)
                        };
                        self.controls.push(Control {
                            name: "import-feedback".into(),
                            rect: [
                                response.rect.min.x,
                                response.rect.min.y,
                                response.rect.max.x,
                                response.rect.max.y,
                            ],
                        });
                    }
                    #[cfg(target_os = "linux")]
                            ui.small("WAV PCM16, FLAC y MP3 mono o estéreo. También puedes arrastrarlos a Doremi.");
                    #[cfg(target_os = "android")]
                            ui.small("WAV PCM16, FLAC y MP3 locales, mono o estéreo, hasta 256 MiB. Se guarda una copia privada.");
                }
            }
            Page::Playlist => self.playlist_page(ui),
            Page::Queue => self.queue_page(ui),
            Page::Settings => {
                ui.heading("Configuración");
                #[cfg(any(target_os = "linux", target_os = "android", target_os = "windows"))]
                self.youtube_account(ui);
                ui.label("Acento");
                ui.horizontal_wrapped(|ui| {
                    for (name, label, value) in
                        [("purple", "Morado", false), ("green", "Verde", true)]
                    {
                        if self.button(ui, name, label, 130.0) {
                            let model = self.model.as_mut().unwrap();
                            model.prefs.green = value;
                            model.revision += 1;
                        }
                    }
                });
                ui.label("Tema");
                ui.horizontal_wrapped(|ui| {
                    for (name, label, value) in
                        [("dark", "Oscuro", false), ("light", "Claro", true)]
                    {
                        if self.button(ui, name, label, 130.0) {
                            let model = self.model.as_mut().unwrap();
                            model.prefs.light = value;
                            model.revision += 1;
                        }
                    }
                });
                ui.label(if cfg!(any(target_os = "linux", target_os = "android", target_os = "windows")) {
                    "Datos propios de Doremi. Commons es opcional y no requiere cuenta."
                } else {
                    "Datos propios de Doremi. Sin permisos de red ni acceso a archivos personales."
                });
            }
            Page::Player => {
                if ui.ctx().content_rect().width() < 760.0 {
                    let prefs = &self.model.as_ref().unwrap().prefs;
                    let palette = doremi_ui::palette(prefs.light, prefs.green);
                    self.mobile_player(ui, &palette);
                    return;
                }
                if self.model.as_ref().unwrap().queue.current().is_none() {
                    self.empty_player(ui);
                    return;
                }
                let model = self.model.as_ref().unwrap();
                let title = model
                    .queue
                    .current()
                    .map(|e| e.media.title.as_str())
                    .unwrap_or("Ninguna canción seleccionada");
                ui.heading(title);
                ui.label(if !model.real_audio() {
                    "Esta selección es una demostración sin audio."
                } else if model.queue.playback() == Playback::Loading {
                    "Preparando audio…"
                } else if model.queue.playback() == Playback::Failed {
                    "No se pudo reproducir esta selección."
                } else if model.queue.playback() == Playback::Paused {
                    "En pausa"
                } else {
                    "Reproducción nativa"
                });
                ui.label(&model.notice);
                #[cfg(target_os = "android")]
                ui.small("La reproducción se pausa al salir. MediaSession aún pendiente.");
            }
        }
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
            #[cfg(any(target_os = "linux", target_os = "android", target_os = "windows"))]
            {
                model.suspend_online();
                model.music.suspend();
                model.artwork.select(None);
            }
            // The Android system picker pauses the Activity. Keep its explicit
            // import intent while suspending playback and the GL renderer.
            model.command_with_import_policy(Command::Cancel, !cfg!(target_os = "android"));
            if model.write_history {
                let _ = self
                    .writer
                    .as_ref()
                    .unwrap()
                    .submit_history(model.history.clone());
            }
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
            #[cfg(any(target_os = "linux", target_os = "android", target_os = "windows"))]
            let mut retained = retained;
            #[cfg(any(target_os = "linux", target_os = "android", target_os = "windows"))]
            retained.model.artwork.shutdown();
            #[cfg(any(target_os = "linux", target_os = "android", target_os = "windows"))]
            retained.model.music.session.shutdown();
            retained.writer.shutdown_with_history(
                retained
                    .model
                    .write_history
                    .then(|| retained.model.history.clone()),
            );
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
        serde_json::json!({"current":model.queue.current().map(|e|e.media.id.local()),"position_ms":model.queue.position_ms(),"frames":m.frames,"callbacks":m.callbacks,"output_rate":m.output_rate,"pcm_bytes":m.pcm_bytes,"streaming":m.streaming,"buffering":m.buffering,"buffer_capacity_frames":m.buffer_capacity_frames,"buffer_underruns":m.buffer_underruns,"nonzero_samples":m.nonzero_samples,"reported_xruns":m.reported_xruns,"first_xrun_frame":m.first_xrun_frame,"last_xrun_frame":m.last_xrun_frame,"first_pcm_us":m.first_pcm_us,"first_callback_us":m.first_callback_us})
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
    pub(super) static UI_TEST_LOCK: Mutex<()> = Mutex::new(());
    #[test]
    fn session_cookie_editor_never_enters_durable_diagnostics() {
        let _guard = UI_TEST_LOCK.lock().unwrap();
        let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
            "../../target/session-cookie-diagnostics-{}",
            std::process::id()
        ));
        let ctx = egui::Context::default();
        let mut app = DoremiApp::new_context(&ctx, directory.clone());
        app.navigate(Page::Settings);
        app.editor = TextField::SessionCookie;
        const FIXTURE: &str = "SAPISID=synthetic-diagnostics-fixture-only";
        app.receive_editor_text("session-cookie", FIXTURE);
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| app.draw(ui));
        output.textures_delta.clear();
        assert!(!app.saved_state.is_empty());
        assert!(!String::from_utf8_lossy(&app.saved_state).contains(FIXTURE));
        let diagnostic: serde_json::Value = serde_json::from_slice(&app.saved_state).unwrap();
        assert_eq!(diagnostic["editor_text"], "");
        let state = &mut app.model.as_mut().unwrap().music.session;
        state.paste();
        assert!(
            state.pasted.is_empty(),
            "Submitting removes the editor copy"
        );
        state.cancel(); // Retire before polling: no real account or network.
        drop(app);
        DoremiApp::shutdown();
        for file in std::fs::read_dir(&directory).unwrap().flatten() {
            if file.file_type().unwrap().is_file() {
                assert!(
                    !String::from_utf8_lossy(&std::fs::read(file.path()).unwrap())
                        .contains(FIXTURE)
                );
            }
        }
        std::fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn editor_targets_fence_late_text_and_duplicate_playlist_selection_keeps_its_offset() {
        let _guard = UI_TEST_LOCK.lock().unwrap();
        let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
            "../../target/playlist-editor-{}",
            std::process::id()
        ));
        let ctx = egui::Context::default();
        let mut app = DoremiApp::new_context(&ctx, directory.clone());
        app.navigate(Page::Library);
        app.editor = TextField::PlaylistName;
        app.receive_editor_text("search", "must not search");
        assert!(app.model.as_ref().unwrap().query.is_empty());
        app.receive_editor_text("playlist-name", "Café · 音楽");
        assert_eq!(app.model.as_ref().unwrap().playlist_name, "Café · 音楽");
        app.navigate(Page::Search);
        app.receive_editor_text("playlist-name", "stale edit");
        assert_eq!(app.model.as_ref().unwrap().playlist_name, "Café · 音楽");
        let media = songs()[0].clone();
        let metadata = Media {
            id: MediaId::new(
                ProviderId::new("musicbrainz").unwrap(),
                "recording:00000000-0000-4000-8000-000000000001",
            )
            .unwrap(),
            title: "Metadata only".into(),
            duration_ms: None,
        };
        app.open_library_entry(&[media.clone(), metadata, media], 2);
        assert_eq!(
            app.model.as_ref().unwrap().queue.checkpoint().cursor,
            Some(1)
        );
        assert_eq!(app.model.as_ref().unwrap().queue.context().len(), 2);
        assert_eq!(
            app.model.as_ref().unwrap().queue.context()[0].media.id,
            app.model.as_ref().unwrap().queue.context()[1].media.id
        );
        drop(app);
        DoremiApp::shutdown();
        let _ = std::fs::remove_dir_all(directory);
    }
    #[test]
    fn artist_browse_tabs_fit_phone_widths_with_distinct_touch_targets() {
        use doremi_core::media::{CatalogEntry, EntityKind};
        let _guard = UI_TEST_LOCK.lock().unwrap();
        let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(format!("../../target/artist-tabs-{}", std::process::id()));
        let ctx = egui::Context::default();
        let mut app = DoremiApp::new_context(&ctx, directory.clone());
        app.model.as_mut().unwrap().music.detail = Some(CatalogEntry {
            media: Media {
                id: MediaId::new(
                    ProviderId::new("musicbrainz").unwrap(),
                    "artist:00000000-0000-4000-8000-000000000001",
                )
                .unwrap(),
                title: "Artista de prueba · título largo".into(),
                duration_ms: None,
            },
            kind: EntityKind::Artist,
            artists: vec![],
            edition: None,
        });
        for width in [360.0, 393.0, 430.0, 1180.0] {
            for _ in 0..3 {
                app.controls.clear();
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(width, 852.0),
                        )),
                        ..Default::default()
                    },
                    |ui| app.music_results(ui),
                );
                output.textures_delta.clear();
            }
            let tabs: Vec<_> = app
                .controls
                .iter()
                .filter(|control| control.name.starts_with("artist-"))
                .collect();
            assert_eq!(tabs.len(), 2);
            for tab in &tabs {
                let [x1, y1, x2, y2] = tab.rect;
                assert!(x1 >= 0.0 && x2 <= width && y1 >= 0.0 && y2 <= 852.0);
                assert!(x2 - x1 >= TOUCH && y2 - y1 >= TOUCH);
            }
            let [a, b] = [tabs[0].rect, tabs[1].rect];
            assert!(a[2] <= b[0] || a[3] <= b[1]);
        }
        drop(app);
        DoremiApp::shutdown();
        let _ = std::fs::remove_dir_all(directory);
    }
    #[test]
    fn recent_cards_and_full_history_render_bounded_rows_at_phone_and_desktop_sizes() {
        let _guard = UI_TEST_LOCK.lock().unwrap();
        let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
            "../../target/listening-layout-{}",
            std::process::id()
        ));
        let ctx = egui::Context::default();
        let mut app = DoremiApp::new_context(&ctx, directory.clone());
        // Own synthetic metadata exercises the layout only. The opt-in GUI
        // journey separately proves history produced by real CC0 playback.
        for index in 0..500 {
            app.model
                .as_mut()
                .unwrap()
                .history
                .record(
                    Media {
                        id: MediaId::new(ProviderId::new("qa").unwrap(), index.to_string())
                            .unwrap(),
                        title: format!("QA · Música original {index}"),
                        duration_ms: Some(90_000),
                    },
                    None,
                )
                .unwrap();
        }
        let mut time = 0.0;
        for light in [false, true] {
            for size in [
                [360.0, 800.0],
                [393.0, 852.0],
                [430.0, 932.0],
                [1180.0, 820.0],
            ] {
                app.model.as_mut().unwrap().prefs.light = light;
                for page in [Page::Home, Page::History] {
                    app.navigate(page);
                    for _ in 0..3 {
                        time += 0.1;
                        let mut input = egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(
                                egui::Pos2::ZERO,
                                size.into(),
                            )),
                            time: Some(time),
                            ..Default::default()
                        };
                        input
                            .viewports
                            .get_mut(&egui::ViewportId::ROOT)
                            .unwrap()
                            .native_pixels_per_point = Some(1.875);
                        let mut output = ctx.run_ui(input, |ui| app.draw(ui));
                        output.textures_delta.clear();
                    }
                    if page == Page::Home {
                        assert_eq!(
                            app.controls
                                .iter()
                                .filter(|c| c.name.starts_with("history-play:"))
                                .count(),
                            16
                        );
                        let heading = app
                            .controls
                            .iter()
                            .find(|c| c.name == "history-open")
                            .unwrap();
                        assert!(
                            heading.rect[3] - heading.rect[1]
                                >= if size[0] < 760.0 { 48.0 } else { 32.0 }
                        );
                    } else {
                        let visible = app
                            .controls
                            .iter()
                            .filter(|c| c.name.starts_with("select:"))
                            .count();
                        assert!(
                            (1..25).contains(&visible),
                            "Measured {visible} history rows at {size:?}"
                        );
                        assert_eq!(app.model.as_ref().unwrap().history.plays().len(), 500);
                    }
                }
            }
        }
        drop(app);
        DoremiApp::shutdown();
        let _ = std::fs::remove_dir_all(directory);
    }
    #[test]
    fn future_history_stays_untouched_during_real_ui_persistence_and_shutdown() {
        let _guard = UI_TEST_LOCK.lock().unwrap();
        let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
            "../../target/future-listening-history-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let future = br#"{"schema":2,"plays":[],"future":"keep"}"#;
        std::fs::write(directory.join("history-v1.json"), future).unwrap();
        let ctx = egui::Context::default();
        let mut app = DoremiApp::new_context(&ctx, directory.clone());
        assert!(!app.model.as_ref().unwrap().write_history);
        for _ in 0..3 {
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| app.draw(ui));
            output.textures_delta.clear();
        }
        drop(app);
        DoremiApp::shutdown();
        assert_eq!(
            std::fs::read(directory.join("history-v1.json")).unwrap(),
            future
        );
        std::fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn mobile_search_is_pinned_above_results_through_ime_resize_and_scroll() {
        let _guard = UI_TEST_LOCK.lock().unwrap();
        let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
            "../../target/mobile-search-header-{}",
            std::process::id()
        ));
        let ctx = egui::Context::default();
        let mut app = DoremiApp::new_context(&ctx, directory.clone());
        #[cfg(any(target_os = "linux", target_os = "windows", target_os = "android"))]
        {
            app.model.as_mut().unwrap().music.selected = false;
        }
        app.navigate(Page::Search);
        // Keep a mini-player visible as well: the IME leaves even less room.
        app.model
            .as_mut()
            .unwrap()
            .start_context(vec![songs()[0].clone()], 0);
        app.model.as_mut().unwrap().command(Command::Pause);
        let mut time = 0.0;
        fn render(
            app: &mut DoremiApp,
            ctx: &egui::Context,
            time: &mut f64,
            size: [f32; 2],
            focused: bool,
            events: Vec<egui::Event>,
        ) -> [f32; 4] {
            *time += 0.1;
            let mut input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size.into())),
                focused,
                time: Some(*time),
                events,
                ..Default::default()
            };
            input
                .viewports
                .get_mut(&egui::ViewportId::ROOT)
                .unwrap()
                .native_pixels_per_point = Some(1.875);
            let mut output = ctx.run_ui(input, |ui| app.draw(ui));
            output.textures_delta.clear();
            let rect = app
                .controls
                .iter()
                .find(|control| control.name == "query")
                .unwrap()
                .rect;
            assert!(
                rect[1] >= 0.0 && rect[1] <= 16.0,
                "Search must be at the top: {rect:?}"
            );
            assert!(
                rect[3] - rect[1] >= TOUCH && rect[3] <= app.page_rect[1],
                "Search is a fixed header above the scrolling results: {rect:?} {:?}",
                app.page_rect
            );
            assert!(rect[0] >= 0.0 && rect[2] <= size[0] && rect[3] <= size[1]);
            rect
        }
        for light in [false, true] {
            app.model.as_mut().unwrap().prefs.light = light;
            for width in [360.0, 393.0, 430.0] {
                for _ in 0..3 {
                    render(&mut app, &ctx, &mut time, [width, 740.0], true, vec![]);
                }
                app.editor = TextField::Search;
                app.receive_editor_text("search", "Café · búsqueda");
                let rect = render(&mut app, &ctx, &mut time, [width, 300.0], false, vec![]);
                let point = egui::pos2(width / 2.0, (app.page_rect[1] + app.page_rect[3]) / 2.0);
                for _ in 0..3 {
                    let scrolled = render(
                        &mut app,
                        &ctx,
                        &mut time,
                        [width, 300.0],
                        false,
                        vec![
                            egui::Event::PointerMoved(point),
                            egui::Event::MouseWheel {
                                unit: egui::MouseWheelUnit::Point,
                                phase: egui::TouchPhase::Move,
                                delta: egui::vec2(0.0, -240.0),
                                modifiers: egui::Modifiers::NONE,
                            },
                        ],
                    );
                    assert_eq!(rect, scrolled, "Scrolling results moved the search field");
                }
                assert_eq!(app.model.as_ref().unwrap().query, "Café · búsqueda");
            }
        }
        drop(app);
        DoremiApp::shutdown();
        let _ = std::fs::remove_dir_all(directory);
    }
    #[test]
    fn playlist_editor_stays_visible_when_android_ime_takes_window_focus() {
        let _guard = UI_TEST_LOCK.lock().unwrap();
        let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(format!("../../target/playlist-ime-{}", std::process::id()));
        let ctx = egui::Context::default();
        let mut app = DoremiApp::new_context(&ctx, directory.clone());
        let model = app.model.as_mut().unwrap();
        let id = model.playlists.create("Café nocturno").unwrap();
        for _ in 0..8 {
            model.playlists.append(id, songs()[0].clone()).unwrap();
        }
        model.start_context(vec![songs()[0].clone()], 0);
        model.command(Command::Pause);
        app.open_playlist(id);
        let mut time = 0.0;
        fn frame(
            app: &mut DoremiApp,
            ctx: &egui::Context,
            time: &mut f64,
            height: f32,
            focused: bool,
            events: Vec<egui::Event>,
        ) {
            *time += 0.1;
            let mut input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(384.0, height),
                )),
                focused,
                time: Some(*time),
                events,
                ..Default::default()
            };
            input
                .viewports
                .get_mut(&egui::ViewportId::ROOT)
                .unwrap()
                .native_pixels_per_point = Some(1.875);
            let mut output = ctx.run_ui(input, |ui| app.draw(ui));
            output.textures_delta.clear();
        }
        for _ in 0..4 {
            frame(&mut app, &ctx, &mut time, 658.65625, true, vec![]);
        }
        let edit = app
            .controls
            .iter()
            .find(|control| control.name == "playlist-edit")
            .unwrap();
        let point = egui::pos2(
            (edit.rect[0] + edit.rect[2]) / 2.0,
            (edit.rect[1] + edit.rect[3]) / 2.0,
        );
        for pressed in [true, false] {
            frame(
                &mut app,
                &ctx,
                &mut time,
                658.65625,
                true,
                vec![
                    egui::Event::PointerMoved(point),
                    egui::Event::PointerButton {
                        pos: point,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
        for _ in 0..4 {
            frame(&mut app, &ctx, &mut time, 658.65625, true, vec![]);
        }
        // Android's native editor owns focus while its IME is open. Its target
        // remains explicit even when egui's TextEdit no longer has focus.
        app.editor = TextField::PlaylistRename;
        app.receive_editor_text("playlist-rename", "Café · lectura");
        for _ in 0..6 {
            frame(&mut app, &ctx, &mut time, 332.28125, false, vec![]);
        }
        for name in ["playlist-rename", "playlist-rename-save"] {
            let control = app
                .controls
                .iter()
                .find(|control| control.name == name)
                .unwrap();
            assert!(
                control.rect[1] >= app.page_rect[1] && control.rect[3] <= app.page_rect[3],
                "{name} {:?} outside {:?} with native IME",
                control.rect,
                app.page_rect
            );
        }
        drop(app);
        DoremiApp::shutdown();
        let _ = std::fs::remove_dir_all(directory);
    }
    #[test]
    fn mobile_player_is_conditional_and_keeps_full_controls_reachable() {
        let _guard = UI_TEST_LOCK.lock().unwrap();
        fn render(app: &mut DoremiApp, ctx: &egui::Context, size: [f32; 2], scale: f32) {
            for _ in 0..3 {
                let mut input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size.into())),
                    ..Default::default()
                };
                input
                    .viewports
                    .get_mut(&egui::ViewportId::ROOT)
                    .unwrap()
                    .native_pixels_per_point = Some(scale);
                let mut output = ctx.run_ui(input, |ui| app.draw(ui));
                assert_eq!(ctx.pixels_per_point(), scale);
                assert!((ctx.content_rect().width() - size[0]).abs() < 1.0);
                output.textures_delta.clear();
            }
        }
        let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
            "../../target/mobile-player-layout-{}",
            std::process::id()
        ));
        let ctx = egui::Context::default();
        let mut app = DoremiApp::new_context(&ctx, directory.clone());
        for light in [false, true] {
            for (size, scale) in [
                [360.0, 640.0],
                [393.0, 852.0],
                [430.0, 932.0],
                [640.0, 360.0],
                [682.65625, 360.0],
                [720.0, 360.0],
            ]
            .into_iter()
            .map(|size| (size, 1.0))
            .chain([([682.65625, 360.0], 1.875), ([360.0, 640.0], 1.875)])
            {
                let model = app.model.as_mut().unwrap();
                model.queue = Queue::new(SessionId(90));
                model.page = Page::Home;
                model.prefs.light = light;
                render(&mut app, &ctx, size, scale);
                assert!(
                    !app.controls
                        .iter()
                        .any(|control| ["play", "pause", "player", "seek"]
                            .contains(&control.name.as_str())),
                    "Empty browsing screen has a fake player"
                );
                let empty_height = app.page_rect[3] - app.page_rect[1];
                let mut media = songs()[0].clone();
                media.title="Una canción con un título largo, con acentos y créditos de una interpretación que debe seguir dejando espacio a todos los controles".into();
                app.model.as_mut().unwrap().start_context(vec![media], 0);
                app.model.as_mut().unwrap().command(Command::Pause);
                render(&mut app, &ctx, size, scale);
                assert!(
                    (empty_height - (app.page_rect[3] - app.page_rect[1]) - 72.0).abs() < 1.0,
                    "Mini-player reserves the wrong height"
                );
                for name in ["player", "play", "next"] {
                    let control = app
                        .controls
                        .iter()
                        .find(|control| control.name == name)
                        .unwrap();
                    assert!(
                        control.rect[2] - control.rect[0] >= TOUCH
                            && control.rect[3] - control.rect[1] >= TOUCH
                    );
                }
                app.navigate(Page::Library);
                app.navigate(Page::Player);
                render(&mut app, &ctx, size, scale);
                let names = [
                    "play",
                    "previous",
                    "next",
                    "shuffle",
                    "repeat",
                    "seek",
                    "volume",
                    "stop",
                    "player-back",
                ];
                let controls: Vec<_> = names
                    .iter()
                    .map(|name| {
                        app.controls
                            .iter()
                            .find(|control| control.name == *name)
                            .unwrap()
                    })
                    .collect();
                for (index, left) in controls.iter().enumerate() {
                    assert!(
                        left.rect[0] >= app.page_rect[0]
                            && left.rect[2] <= app.page_rect[2]
                            && left.rect[1] >= app.page_rect[1]
                            && left.rect[3] <= app.page_rect[3],
                        "{} {:?} outside page {:?} at {:?}",
                        left.name,
                        left.rect,
                        app.page_rect,
                        size
                    );
                    assert!(left.rect[3] - left.rect[1] >= TOUCH);
                    for right in &controls[index + 1..] {
                        assert!(
                            left.rect[2] <= right.rect[0]
                                || right.rect[2] <= left.rect[0]
                                || left.rect[3] <= right.rect[1]
                                || right.rect[3] <= left.rect[1],
                            "{} overlaps {}",
                            left.name,
                            right.name
                        );
                    }
                }
                if size[0] > size[1] {
                    let notice = app
                        .controls
                        .iter()
                        .find(|control| control.name == "player-background-notice")
                        .unwrap();
                    assert!(
                        notice.rect[1] >= app.page_rect[1] && notice.rect[3] <= app.page_rect[3],
                        "Background notice clipped at {size:?}: {:?}",
                        notice.rect
                    );
                    for control in &controls {
                        assert!(
                            notice.rect[2] <= control.rect[0]
                                || control.rect[2] <= notice.rect[0]
                                || notice.rect[3] <= control.rect[1]
                                || control.rect[3] <= notice.rect[1]
                        );
                    }
                }
                assert!(
                    !app.controls.iter().any(|control| control.name == "player"),
                    "Full player duplicates the mini-player"
                );
                let checkpoint = app.model.as_ref().unwrap().queue.checkpoint();
                app.go_back();
                assert_eq!(app.model.as_ref().unwrap().page, Page::Library);
                assert_eq!(app.model.as_ref().unwrap().queue.checkpoint(), checkpoint);
            }
        }
        drop(app);
        DoremiApp::shutdown();
        let _ = std::fs::remove_dir_all(directory);
    }
    #[cfg(any(target_os = "linux", target_os = "android", target_os = "windows"))]
    #[test]
    fn opening_saved_metadata_preserves_playback_and_audio_contexts_skip_it() {
        let _guard = UI_TEST_LOCK.lock().unwrap();
        let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
            "../../target/metadata-routing-test-{}",
            std::process::id()
        ));
        let ctx = egui::Context::default();
        let mut app = DoremiApp::new_context(&ctx, directory.clone());
        let demo = songs();
        let metadata = Media {
            id: MediaId::new(
                ProviderId::new("musicbrainz").unwrap(),
                "recording:00000000-0000-4000-8000-000000000001",
            )
            .unwrap(),
            title: "Saved public metadata".into(),
            duration_ms: None,
        };
        let rows = vec![demo[0].clone(), metadata.clone(), demo[1].clone()];
        app.open_library_entry(&rows, 0);
        let before = app.model.as_ref().unwrap().queue.checkpoint();
        app.open_library_entry(&rows, 1);
        let model = app.model.as_ref().unwrap();
        assert_eq!(model.page, Page::Search);
        assert_eq!(model.music.detail.as_ref().unwrap().media.id, metadata.id);
        assert_eq!(model.queue.checkpoint(), before);
        for (prefix, kind) in [
            ("artist", doremi_core::media::EntityKind::Artist),
            ("release", doremi_core::media::EntityKind::Release),
        ] {
            let saved = Media {
                id: MediaId::new(
                    ProviderId::new("musicbrainz").unwrap(),
                    format!("{prefix}:00000000-0000-4000-8000-000000000001"),
                )
                .unwrap(),
                title: format!("Saved {prefix}"),
                duration_ms: None,
            };
            app.model.as_mut().unwrap().save_media(saved.clone());
            let restored = persistence::collection::Collection::decode(
                &app.model.as_ref().unwrap().library.encode().unwrap(),
            )
            .unwrap();
            app.open_library_entry(
                restored.items(),
                restored
                    .items()
                    .iter()
                    .position(|media| media.id == saved.id)
                    .unwrap(),
            );
            let model = app.model.as_ref().unwrap();
            assert_eq!(model.music.detail.as_ref().unwrap().kind, kind);
            assert_eq!(model.music.detail.as_ref().unwrap().media, saved);
            assert_eq!(model.queue.checkpoint(), before);
        }
        app.open_library_entry(&rows, 2);
        let model = app.model.as_ref().unwrap();
        assert_eq!(model.page, Page::Player);
        assert_eq!(model.queue.current().unwrap().media.id, demo[1].id);
        assert!(
            !model
                .queue
                .context()
                .iter()
                .any(|entry| entry.media.id == metadata.id)
        );
        drop(app);
        DoremiApp::shutdown();
        let _ = std::fs::remove_dir_all(directory);
    }
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
        let pointer = egui::pos2(rect.min.x + rect.width() * 0.75, rect.center().y);
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
    #[cfg(any(target_os = "linux", target_os = "android", target_os = "windows"))]
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
            for size in [
                [384.0, 614.0],
                [384.0, 332.0],
                [682.0, 336.0],
                [440.0, 800.0],
            ] {
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
                assert_eq!(controls.len(), 4);
                for control in &controls {
                    let [x1, y1, x2, y2] = control.rect;
                    assert!(x1 >= 0.0 && y1 >= 0.0 && x2 <= size[0] && y2 <= size[1]);
                    assert!(x2 - x1 >= TOUCH && y2 - y1 >= TOUCH);
                }
                for pair in controls.windows(2) {
                    assert_eq!(
                        pair[0].rect[1],
                        pair[1].rect[1],
                        "Viewport {size:?}: {:?}",
                        controls
                            .iter()
                            .map(|control| (&control.name, control.rect))
                            .collect::<Vec<_>>()
                    );
                    assert!((pair[1].rect[0] - pair[0].rect[2] - GAP).abs() < 0.1);
                }
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
        app.model.as_mut().unwrap().start_context(songs(), 0);
        app.model.as_mut().unwrap().command(Command::Pause);
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
        assert!(
            app.controls
                .iter()
                .any(|control| control.name == "browse-catalog")
        );
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
        let play = state["controls"]
            .as_array()
            .unwrap()
            .iter()
            .find(|control| control["name"] == "play")
            .unwrap();
        let actual = app
            .controls
            .iter()
            .find(|control| control.name == "play")
            .unwrap();
        assert_eq!(play["rect"], serde_json::json!(actual.rect));
        assert_eq!(state["page"], "Player");
        assert!(
            play["rect"][3].as_f64().unwrap() < 614.0,
            "Published transport must remain inside the viewport"
        );
        assert!(
            !state["controls"]
                .as_array()
                .unwrap()
                .iter()
                .any(|c| c["name"] == "browse-catalog")
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
        app.model.as_mut().unwrap().start_context(songs(), 0);
        app.model.as_mut().unwrap().command(Command::Pause);
        app.model.as_mut().unwrap().page = Page::Home;
        for light in [false, true] {
            app.model.as_mut().unwrap().prefs.light = light;
            for size in [
                [360.0, 640.0],
                [393.0, 852.0],
                [430.0, 932.0],
                [384.0, 614.0],
                [384.0, 332.0],
                [682.0, 336.0],
                [440.0, 800.0],
            ] {
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
                let transport: Vec<_> = app
                    .controls
                    .iter()
                    .filter(|control| {
                        [
                            "play",
                            "pause",
                            "previous",
                            "next",
                            "shuffle",
                            "repeat",
                            "seek",
                            "volume",
                            "player",
                            "stop",
                            "player-more",
                            "elapsed-time",
                            "total-time",
                            "playing-title",
                        ]
                        .contains(&control.name.as_str())
                    })
                    .collect();
                for (index, left) in transport.iter().enumerate() {
                    assert!(
                        left.rect[0] >= 0.0
                            && left.rect[2] <= size[0]
                            && left.rect[1] >= 0.0
                            && left.rect[3] <= size[1],
                        "{} {:?} at {:?}",
                        left.name,
                        left.rect,
                        size
                    );
                    assert!(
                        left.rect[3] <= navigation[0].rect[1],
                        "{} runs into navigation at {:?}",
                        left.name,
                        size
                    );
                    for right in &transport[index + 1..] {
                        let overlap = left.rect[0] < right.rect[2]
                            && right.rect[0] < left.rect[2]
                            && left.rect[1] < right.rect[3]
                            && right.rect[1] < left.rect[3];
                        assert!(
                            !overlap,
                            "{} overlaps {} at {:?}",
                            left.name, right.name, size
                        );
                    }
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
