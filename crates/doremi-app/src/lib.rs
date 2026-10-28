//! Independent Doremi host. The core owns queue and catalog rules.
use doremi_core::{
    catalog::{Capabilities, Catalog, ProviderCommand, ProviderEvent, Tick},
    media::{Media, MediaId, ProviderId},
    queue::{Command, Effect, Event, Order, Queue, Repeat},
    requests::SessionId,
};
use eframe::egui;
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    sync::{Mutex, mpsc},
    time::Duration,
};

const TOUCH: f32 = 48.0;
const GAP: f32 = 8.0;
static RETAINED: Mutex<Option<Model>> = Mutex::new(None);

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default)]
struct Preferences {
    schema: u32,
    green: bool,
    light: bool,
    library: Vec<String>,
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
        }
    }
    fn command(&mut self, command: Command) {
        match self.queue.command(command) {
            Ok(effects) => self.effects(effects),
            Err(error) => self.notice = format!("No se pudo cambiar la cola: {error:?}"),
        }
        self.revision += 1;
    }
    fn effects(&mut self, effects: Vec<Effect>) {
        // Fake transport is explicit. It never claims to produce sound.
        for effect in effects {
            if let Effect::Load { request, .. } = effect {
                let _ = self.queue.event(Event::Ready(request));
            }
        }
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

#[derive(Serialize)]
struct Control {
    name: String,
    rect: [f32; 4],
}
struct DiskWrite {
    prefs: Vec<u8>,
    state: Vec<u8>,
}

pub struct DoremiApp {
    model: Option<Model>,
    writer: Option<mpsc::SyncSender<DiskWrite>>,
    worker: Option<std::thread::JoinHandle<()>>,
    controls: Vec<Control>,
    logo: egui::TextureHandle,
    saved_state: Vec<u8>,
    pending: bool,
}

impl DoremiApp {
    pub fn new(cc: &eframe::CreationContext<'_>, directory: PathBuf) -> Self {
        // Startup reads precede the UI event loop. Writes always run on a worker.
        let prefs = std::fs::read(directory.join("preferences-v1.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        let model = RETAINED
            .lock()
            .unwrap()
            .take()
            .unwrap_or_else(|| Model::new(prefs));
        let (tx, rx) = mpsc::sync_channel::<DiskWrite>(1);
        let worker = std::thread::spawn(move || {
            let result = std::fs::create_dir_all(&directory);
            if result.is_err() {
                return;
            }
            while let Ok(mut write) = rx.recv() {
                while let Ok(newer) = rx.try_recv() {
                    write = newer;
                }
                for (name, bytes) in [
                    ("preferences-v1.json", write.prefs),
                    ("qa-state.json", write.state),
                ] {
                    let path = directory.join(name);
                    let temporary = path.with_extension("tmp");
                    let result = (|| -> std::io::Result<()> {
                        use std::io::Write;
                        let mut file = std::fs::File::create(&temporary)?;
                        file.write_all(&bytes)?;
                        file.sync_all()?;
                        std::fs::rename(&temporary, &path)?;
                        std::fs::File::open(&directory)?.sync_all()
                    })();
                    if let Err(error) = result {
                        eprintln!("Doremi persistence: {error}");
                    }
                }
            }
        });
        let image = image::load_from_memory(include_bytes!("../../../assets/doremi/Doremi.png"))
            .unwrap()
            .to_rgba8();
        let size = [image.width() as usize, image.height() as usize];
        let logo = cc.egui_ctx.load_texture(
            "Doremi",
            egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw()),
            Default::default(),
        );
        cc.egui_ctx.all_styles_mut(|style| {
            style.spacing.item_spacing = egui::vec2(GAP, GAP);
            style.spacing.button_padding = egui::vec2(12.0, 10.0);
            style.spacing.interact_size.y = TOUCH;
        });
        Self {
            model: Some(model),
            writer: Some(tx),
            worker: Some(worker),
            controls: vec![],
            logo,
            saved_state: vec![],
            pending: true,
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
        for media in &rows {
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.set_width((ui.available_width() - 4.0).max(0.0));
                ui.label(egui::RichText::new(&media.title).strong());
                ui.small(format!("demo:{} · 0:30 · sin audio", media.id.local()));
                ui.horizontal_wrapped(|ui| {
                    if self.button(
                        ui,
                        &format!("select:{}", media.id.local()),
                        "Seleccionar",
                        118.0,
                    ) {
                        let start = rows.iter().position(|s| s.id == media.id).unwrap();
                        self.model.as_mut().unwrap().command(Command::StartContext {
                            media: rows.clone(),
                            start,
                            order: Order::Sequential,
                        });
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
        let model = self.model.as_ref().unwrap();
        let state = serde_json::json!({
            "schema": 1, "page": model.page, "query": model.query,
            "revision": model.revision, "green": model.prefs.green, "light": model.prefs.light,
            "current": model.queue.current().map(|e| e.media.id.local()),
            "manual": model.queue.manual().iter().map(|e| e.media.id.local()).collect::<Vec<_>>(),
            "upcoming": model.queue.upcoming().map(|e| e.media.id.local()).collect::<Vec<_>>(),
            "playback": format!("{:?}", model.queue.playback()), "audio": false,
            "repeat": format!("{:?}", model.queue.repeat()), "shuffle": model.queue.shuffled(),
            "library": model.prefs.library, "controls": self.controls,
            "pixels_per_point": ctx.pixels_per_point(), "viewport": [ctx.content_rect().width(), ctx.content_rect().height()],
        });
        let state = serde_json::to_vec(&state).unwrap();
        if !self.pending && self.saved_state == state {
            return;
        }
        let write = DiskWrite {
            prefs: serde_json::to_vec(&model.prefs).unwrap(),
            state: state.clone(),
        };
        self.pending = self.writer.as_ref().unwrap().try_send(write).is_err();
        if !self.pending {
            self.saved_state = state;
        } else {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
    }
}

impl eframe::App for DoremiApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
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
                ui.colored_label(accent, "Preview Android");
            });
        });
        egui::Panel::bottom("navigation").show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                let width = ((ui.available_width() - 4.0 * GAP) / 5.0).max(48.0);
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
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                let page = self.model.as_ref().unwrap().page;
                match page {
                    Page::Home => { ui.heading("Tu música, a tu ritmo"); ui.label("Catálogo de prueba local. Sin Spotify ni conexión."); self.rows(ui, songs()); }
                    Page::Search => {
                        ui.heading("Buscar");
                        let response = ui.add_sized([ui.available_width(), TOUCH], egui::TextEdit::singleline(&mut self.model.as_mut().unwrap().query).hint_text("Busca en la demostración"));
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
                        let entries = self.model.as_ref().unwrap().queue.manual().to_vec();
                        ui.label("Añadidas por ti (cada copia es independiente)");
                        for (index, entry) in entries.iter().enumerate() {
                            ui.label(&entry.media.title);
                            ui.horizontal_wrapped(|ui| {
                                if self.button(ui,&format!("remove:{index}"),"Quitar",90.0) { self.model.as_mut().unwrap().command(Command::Remove(entry.id)); }
                                if index > 0 && self.button(ui,&format!("up:{index}"),"Subir",90.0) { self.model.as_mut().unwrap().command(Command::Move { entry: entry.id, gap: index - 1 }); }
                            });
                        }
                        ui.separator(); ui.label("Continuación");
                        for entry in self.model.as_ref().unwrap().queue.upcoming() { ui.label(&entry.media.title); }
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
                    Page::Player => { ui.heading("Reproducción de demostración"); ui.label("Simulación de controles. Esta versión no produce audio."); }
                }
                ui.separator();
                let title = self.model.as_ref().unwrap().queue.current().map(|e| e.media.title.clone()).unwrap_or_else(|| "Ninguna canción seleccionada".into());
                ui.strong(title);
                ui.horizontal_wrapped(|ui| {
                    for (name,label,command) in [("previous","Anterior",Command::Previous),("pause","Pausa",Command::Pause),("play","Play demo",Command::Play),("next","Siguiente",Command::Next),("stop","Cancelar",Command::Cancel)] {
                        if self.button(ui,name,label,100.0) { self.model.as_mut().unwrap().command(command); }
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    if self.button(ui,"shuffle","Shuffle",110.0) {
                        let queue = &self.model.as_ref().unwrap().queue;
                        let order = if queue.shuffled() { Order::Sequential } else { Order::Shuffled((0..queue.context().len()).rev().collect()) };
                        self.model.as_mut().unwrap().command(Command::SetOrder(order));
                    }
                    if self.button(ui,"repeat","Repeat",110.0) {
                        let repeat = match self.model.as_ref().unwrap().queue.repeat() { Repeat::Off => Repeat::Context, Repeat::Context => Repeat::Track, Repeat::Track => Repeat::Off };
                        self.model.as_mut().unwrap().command(Command::SetRepeat(repeat));
                    }
                });
                let model = self.model.as_ref().unwrap();
                ui.label(format!("Estado demo: {:?} · Repeat {:?} · Shuffle {}",model.queue.playback(),model.queue.repeat(),model.queue.shuffled()));
                ui.small(&model.notice);
            });
        });
        self.persist(&ctx);
    }
}
impl Drop for DoremiApp {
    fn drop(&mut self) {
        if let Some(mut model) = self.model.take() {
            model.command(Command::Cancel);
            *RETAINED.lock().unwrap() = Some(model);
        }
        self.writer.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(app: winit::platform::android::activity::AndroidApp) {
    android_logger::init_once(
        android_logger::Config::default()
            .with_max_level(log::LevelFilter::Info)
            .with_tag("Doremi"),
    );
    let directory = app
        .internal_data_path()
        .expect("Android private data directory");
    let options = eframe::NativeOptions {
        android_app: Some(app),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    if let Err(error) = eframe::run_native(
        "Doremi",
        options,
        Box::new(move |cc| Ok(Box::new(DoremiApp::new(cc, directory)))),
    ) {
        log::error!("Doremi startup: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
    fn next_pause_restore_and_stale_events_follow_core() {
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
}
