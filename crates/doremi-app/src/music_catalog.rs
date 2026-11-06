//! One cancelable metadata actor shared by Linux, Windows and Android.
use super::*;
use doremi_core::{
    catalog::ProviderError,
    media::{CatalogEntry, EntityKind},
};
use doremi_musicbrainz::{Client, Page as Results, Request};
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::mpsc;

type Completion = (u64, Result<Arc<Results>, ProviderError>);
type HistoryEntry = (Option<CatalogEntry>, Option<Arc<Results>>, Option<Request>);
struct Job {
    generation: u64,
    request: Option<Request>,
    ctx: egui::Context,
}
struct Worker {
    commands: mpsc::Sender<Job>,
    events: mpsc::Receiver<Completion>,
    generation: Arc<AtomicU64>,
}
impl Worker {
    fn new() -> Result<Self, ProviderError> {
        let (commands, mut receive) = mpsc::channel::<Job>(2);
        let (send, events) = mpsc::channel(1);
        let generation = Arc::new(AtomicU64::new(0));
        let active = generation.clone();
        std::thread::Builder::new()
            .name("doremi-catalog".into())
            .spawn(move || {
                let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
                    // OS DNS can outlive a dropped future; keep that work bounded.
                    .max_blocking_threads(1)
                    .enable_all()
                    .build()
                else {
                    return;
                };
                runtime.block_on(async move {
                    let mut client = Client::new();
                    let mut job = receive.recv().await;
                    while let Some(current) = job {
                        let Some(request) = current.request else {
                            job = receive.recv().await;
                            continue;
                        };
                        if current.generation != active.load(Ordering::Acquire) {
                            job = receive.recv().await;
                            continue;
                        }
                        let cancelled = async {
                            while current.generation == active.load(Ordering::Acquire) {
                                tokio::time::sleep(Duration::from_millis(20)).await;
                            }
                        };
                        let result = async {
                            match &mut client {
                                Ok(client) => client.fetch(&request).await.map(Arc::new),
                                Err(error) => Err(*error),
                            }
                        };
                        tokio::select! {
                            biased;
                            next = receive.recv() => {job = next;},
                            _ = cancelled => {job=receive.recv().await;},
                            result = result => {
                                current.ctx.request_repaint();
                                tokio::select! {
                                    biased;
                                    next = receive.recv() => {job=next;},
                                    sent = send.send((current.generation, result)) => {
                                        if sent.is_err() {break;}
                                        current.ctx.request_repaint();
                                        job=receive.recv().await;
                                    }
                                }
                            }
                        }
                    }
                });
            })
            .map_err(|_| ProviderError::Unavailable)?;
        Ok(Self {
            commands,
            events,
            generation,
        })
    }
}

pub(super) struct State {
    pub selected: bool,
    pub kind: EntityKind,
    pub page: Option<Arc<Results>>,
    pub detail: Option<CatalogEntry>,
    pub error: Option<ProviderError>,
    pub waiting: bool,
    request: Option<Request>,
    retry_at: Option<Instant>,
    history: Vec<HistoryEntry>,
    generation: u64,
    pending: Option<(Option<Request>, Instant)>,
    worker: Option<Worker>,
}
impl State {
    pub fn new() -> Self {
        Self {
            selected: false,
            kind: EntityKind::Recording,
            page: None,
            detail: None,
            error: None,
            waiting: false,
            request: None,
            retry_at: None,
            history: vec![],
            generation: 0,
            pending: None,
            worker: None,
        }
    }
    fn dispatch(&mut self, request: Option<Request>, delay: Duration) {
        self.generation = self
            .generation
            .checked_add(1)
            .expect("catalog generation exhausted");
        if let Some(worker) = &self.worker {
            worker.generation.store(self.generation, Ordering::Release);
        }
        self.waiting = request.is_some();
        self.error = None;
        self.request = request.clone();
        self.pending = Some((request, Instant::now() + delay));
    }
    pub fn search(&mut self, query: &str) {
        self.page = None;
        self.detail = None;
        self.history.clear();
        self.dispatch(
            (!query.trim().is_empty()).then(|| Request::Search {
                query: query.into(),
                kind: self.kind,
                offset: 0,
            }),
            Duration::from_millis(350),
        );
    }
    pub fn cancel(&mut self) {
        self.page = None;
        self.detail = None;
        self.history.clear();
        self.dispatch(None, Duration::ZERO);
    }
    pub fn suspend(&mut self) {
        self.pending = None;
        self.waiting = false;
        self.generation = self
            .generation
            .checked_add(1)
            .expect("catalog generation exhausted");
        if let Some(worker) = &self.worker {
            worker.generation.store(self.generation, Ordering::Release);
        }
    }
    pub fn open(&mut self, entry: CatalogEntry) {
        if self.history.len() == 8 {
            self.history.remove(0);
        }
        self.history
            .push((self.detail.take(), self.page.clone(), self.request.clone()));
        let request = match entry.kind {
            EntityKind::Recording => None,
            EntityKind::Artist => Some(Request::ArtistRecordings {
                id: entry.media.id.clone(),
                offset: 0,
            }),
            EntityKind::Release => Some(Request::ReleaseTracks {
                id: entry.media.id.clone(),
            }),
        };
        self.dispatch(request, Duration::ZERO);
        self.detail = Some(entry);
        self.page = None;
    }
    pub fn back(&mut self) {
        if let Some((detail, page, request)) = self.history.pop() {
            self.dispatch(None, Duration::ZERO);
            self.detail = detail;
            self.page = page;
            if self.page.is_none() && request.is_some() {
                self.dispatch(request, Duration::ZERO);
            } else {
                self.request = request;
            }
        }
    }
    pub fn retry(&mut self) {
        if self.retry_at.is_none_or(|due| Instant::now() >= due)
            && let Some(request) = self.request.clone()
        {
            self.dispatch(Some(request), Duration::ZERO);
        }
    }
    pub fn can_retry(&self) -> bool {
        self.request.is_some()
            && !self.waiting
            && self.retry_at.is_none_or(|due| Instant::now() >= due)
    }
    pub fn has_back(&self) -> bool {
        !self.history.is_empty()
    }
    pub fn page_by(&mut self, forward: bool) {
        let Some(page) = &self.page else {
            return;
        };
        if forward && !page.more {
            return;
        }
        let offset = if forward {
            page.offset + 20
        } else {
            page.offset.saturating_sub(20)
        };
        let request = match &self.request {
            Some(Request::Search { query, kind, .. }) => Request::Search {
                query: query.clone(),
                kind: *kind,
                offset,
            },
            Some(Request::ArtistRecordings { id, .. }) => Request::ArtistRecordings {
                id: id.clone(),
                offset,
            },
            _ => return,
        };
        self.dispatch(Some(request), Duration::ZERO);
        self.page = None;
    }
    pub fn poll(&mut self, ctx: &egui::Context) {
        let mut disconnected = false;
        if let Some(worker) = &mut self.worker {
            loop {
                let (generation, result) = match worker.events.try_recv() {
                    Ok(completion) => completion,
                    Err(mpsc::error::TryRecvError::Empty) => break,
                    Err(mpsc::error::TryRecvError::Disconnected) => {
                        disconnected = true;
                        break;
                    }
                };
                if generation == self.generation {
                    self.waiting = false;
                    match result {
                        Ok(page) => self.page = Some(page),
                        Err(error) => {
                            self.error = Some(error);
                            self.retry_at = match error {
                                ProviderError::RateLimited { retry_after_ms } => Instant::now()
                                    .checked_add(Duration::from_millis(
                                        retry_after_ms.unwrap_or(3000).max(1100),
                                    )),
                                _ => None,
                            };
                        }
                    }
                }
            }
        }
        if disconnected {
            self.worker = None;
            if self.waiting {
                self.pending = None;
                self.waiting = false;
                self.error = Some(ProviderError::Unavailable);
            }
        }
        let Some((request, due)) = &self.pending else {
            return;
        };
        if *due > Instant::now() {
            ctx.request_repaint_after(due.saturating_duration_since(Instant::now()));
            return;
        }
        if self.worker.is_none() {
            // Cancelling an unstarted search does not start a network runtime.
            if request.is_none() {
                self.pending = None;
                return;
            }
            match Worker::new() {
                Ok(worker) => self.worker = Some(worker),
                Err(error) => {
                    self.pending = None;
                    self.waiting = false;
                    self.error = Some(error);
                    return;
                }
            }
        }
        self.worker
            .as_ref()
            .unwrap()
            .generation
            .store(self.generation, Ordering::Release);
        let job = Job {
            generation: self.generation,
            request: request.clone(),
            ctx: ctx.clone(),
        };
        match self.worker.as_ref().unwrap().commands.try_send(job) {
            Ok(()) => self.pending = None,
            Err(mpsc::error::TrySendError::Full(_)) => {
                ctx.request_repaint_after(Duration::from_millis(20))
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                self.pending = None;
                self.waiting = false;
                self.error = Some(ProviderError::Unavailable);
                self.worker = None;
            }
        }
    }
    pub fn snapshot(&self) -> serde_json::Value {
        serde_json::json!({"selected":self.selected,"kind":format!("{:?}",self.kind),"generation":self.generation,"waiting":self.waiting,"count":self.page.as_ref().map(|page|page.items.len()),"detail":self.detail.as_ref().map(|item|item.media.id.local()),"error":self.error.map(|error|format!("{error:?}"))})
    }
}

impl DoremiApp {
    pub(super) fn music_results(&mut self, ui: &mut egui::Ui) {
        let detail = self.model.as_ref().unwrap().music.detail.clone();
        let page = self.model.as_ref().unwrap().music.page.clone();
        if self.model.as_ref().unwrap().music.has_back()
            && self.button(ui, "catalog-back", "Volver a resultados", 200.0)
        {
            self.model.as_mut().unwrap().music.back();
            return;
        }
        if let Some(detail) = detail {
            self.music_detail(ui, &detail);
        }
        let state = &self.model.as_ref().unwrap().music;
        if state.waiting {
            ui.spinner();
            ui.label("Consultando MusicBrainz…");
        }
        if let Some(error) = state.error {
            let retry = state.can_retry();
            ui.colored_label(ui.visuals().error_fg_color,match error {
                ProviderError::RateLimited {..}=>"El servicio está ocupado o ha limitado las consultas. Espera antes de reintentar.",
                ProviderError::Timeout=>"La consulta tardó demasiado. Puedes volver a intentarlo.",
                ProviderError::AccessDenied=>"El proveedor rechazó la consulta. No se utilizará otro método de acceso.",
                ProviderError::Unavailable=>"El catálogo no está disponible en este momento.",
                ProviderError::Cancelled=>"Consulta cancelada.",
                ProviderError::Transport=>"No se pudo conectar con el catálogo. Revisa la conexión y vuelve a intentarlo.",
                _=>"La respuesta del catálogo no pudo utilizarse. Vuelve a intentarlo más tarde.",
            });
            if error != ProviderError::AccessDenied {
                let response = ui.add_enabled(retry, egui::Button::new("Reintentar"));
                self.record("catalog-retry", &response);
                if response.clicked() {
                    self.model.as_mut().unwrap().music.retry();
                }
                if !retry {
                    ui.ctx().request_repaint_after(Duration::from_millis(250));
                }
            }
        }
        if let Some(page) = page {
            if page.items.is_empty() {
                ui.label("No hay resultados para esta consulta.");
            }
            for (index, item) in page.items.iter().enumerate() {
                ui.push_id(index, |ui| self.music_row(ui, item));
            }
            ui.horizontal_wrapped(|ui| {
                if page.offset > 0 && self.button(ui, "catalog-previous", "Página anterior", 160.0)
                {
                    self.model.as_mut().unwrap().music.page_by(false);
                }
                if page.more && self.button(ui, "catalog-next", "Más resultados", 160.0) {
                    self.model.as_mut().unwrap().music.page_by(true);
                }
            });
        }
    }
    fn music_detail(&mut self, ui: &mut egui::Ui, entry: &CatalogEntry) {
        use doremi_ui::{
            self as theme, Icon,
            collection::{self, Hero, HeroSlot},
        };
        let prefs = &self.model.as_ref().unwrap().prefs;
        let palette = theme::palette(prefs.light, prefs.green);
        let cover = self.artwork_textures.current(
            &self.model.as_ref().unwrap().artwork,
            &entry.media.id,
            ui.ctx(),
        );
        collection::hero(
            ui,
            &palette,
            Hero {
                kind: kind_label(entry.kind),
                title: &entry.media.title,
                description: Some("MusicBrainz · metadatos públicos"),
                round: entry.kind == EntityKind::Artist,
                cover_clickable: false,
            },
            |ui, slot| match slot {
                HeroSlot::Cover {
                    rect,
                    radius,
                    response,
                } => {
                    if let Some(cover) = &cover {
                        egui::Image::new((cover.id(), cover.size_vec2()))
                            .fit_to_exact_size(rect.size())
                            .maintain_aspect_ratio(true)
                            .corner_radius(radius)
                            .paint_at(ui, rect);
                    } else {
                        theme::cover_placeholder(
                            ui,
                            &palette,
                            rect,
                            radius,
                            if entry.kind == EntityKind::Artist {
                                Icon::User
                            } else {
                                Icon::Music
                            },
                        );
                    }
                    self.record("catalog-artwork", response);
                }
                HeroSlot::Text { text, font, color } => {
                    ui.add(
                        egui::Label::new(egui::RichText::new(text).font(font).color(color)).wrap(),
                    );
                }
                HeroSlot::Byline => {
                    for (index, credit) in entry.artists.iter().enumerate() {
                        if index > 0 {
                            ui.weak("•");
                        }
                        let response = ui.link(&credit.name);
                        self.record(&format!("catalog-artist:{}", credit.id.local()), &response);
                        if response.clicked() {
                            self.model.as_mut().unwrap().music.open(CatalogEntry {
                                media: Media {
                                    id: credit.id.clone(),
                                    title: credit.name.clone(),
                                    duration_ms: None,
                                },
                                kind: EntityKind::Artist,
                                artists: vec![],
                            });
                        }
                    }
                    if let Some(ms) = entry.media.duration_ms {
                        ui.weak(duration(ms));
                    }
                }
            },
        );
        ui.weak("MusicBrainz proporciona metadatos; esta selección no incluye audio.");
        if entry.kind == EntityKind::Release {
            ui.small(
                self.model
                    .as_ref()
                    .unwrap()
                    .artwork
                    .caption(&entry.media.id),
            );
        }
        ui.small(format!("ID de MusicBrainz: {}", entry.media.id.local()));
        if entry.kind == EntityKind::Recording {
            let saved = self
                .model
                .as_ref()
                .unwrap()
                .library
                .contains(&entry.media.id);
            if self.button(
                ui,
                &format!("catalog-save:{}", entry.media.id.local()),
                if saved { "Guardada" } else { "Guardar" },
                120.0,
            ) {
                self.model.as_mut().unwrap().save_media(entry.media.clone());
            }
        } else {
            ui.add_space(12.0);
            ui.label(
                egui::RichText::new(if entry.kind == EntityKind::Artist {
                    "Grabaciones"
                } else {
                    "Canciones"
                })
                .font(theme::bold(20.0)),
            );
        }
    }

    fn music_row(&mut self, ui: &mut egui::Ui, entry: &CatalogEntry) {
        use doremi_ui::{self as theme, Icon};
        let prefs = &self.model.as_ref().unwrap().prefs;
        let palette = theme::palette(prefs.light, prefs.green);
        let can_save = entry.kind == EntityKind::Recording;
        let artists = entry
            .artists
            .iter()
            .map(|credit| credit.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let mut subtitle = if artists.is_empty() {
            kind_label(entry.kind).to_owned()
        } else {
            artists
        };
        if let Some(ms) = entry.media.duration_ms {
            subtitle.push_str(&format!(" · {}", duration(ms)));
        }
        let row = theme::list_row(
            ui,
            &palette,
            ui.id().with(("catalog-row", entry.media.id.local())),
            theme::ListRow {
                title: &entry.media.title,
                subtitle: &subtitle,
                round: entry.kind == EntityKind::Artist,
                actions: usize::from(can_save),
                action_label: "Explorar",
            },
        );
        self.record(
            &format!("catalog-open:{}", entry.media.id.local()),
            &row.body,
        );
        if can_save {
            let saved = self
                .model
                .as_ref()
                .unwrap()
                .library
                .contains(&entry.media.id);
            let target = row.action_rect(0);
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(target).layout(
                egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
            ));
            let save = theme::icon_button_sized(
                &mut child,
                if saved {
                    Icon::HeartFilled
                } else {
                    Icon::Heart
                },
                [20.0, TOUCH],
                if saved {
                    palette.accent
                } else {
                    palette.secondary
                },
                palette.text,
                if saved {
                    "Quitar de la biblioteca"
                } else {
                    "Guardar en la biblioteca"
                },
            );
            self.record(&format!("catalog-save:{}", entry.media.id.local()), &save);
            if save.clicked() {
                let model = self.model.as_mut().unwrap();
                if saved {
                    model.remove_saved(&entry.media.id);
                } else {
                    model.save_media(entry.media.clone());
                }
            }
        }
        if row.body.clicked() {
            self.model.as_mut().unwrap().music.open(entry.clone());
        }
    }
}

fn kind_label(kind: EntityKind) -> &'static str {
    match kind {
        EntityKind::Recording => "Grabación",
        EntityKind::Artist => "Artista",
        EntityKind::Release => "Álbum / edición",
    }
}
fn duration(ms: u64) -> String {
    format!("{}:{:02}", ms / 60000, ms / 1000 % 60)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_worker_that_exits_after_accepting_a_query_reports_unavailable() {
        let mut state = State::new();
        state.search("Moonlight Sonata");
        state.pending = None; // The command was accepted by the worker.
        let (commands, _receive) = mpsc::channel(2);
        let (send, events) = mpsc::channel(1);
        state.worker = Some(Worker {
            commands,
            events,
            generation: Arc::new(AtomicU64::new(state.generation)),
        });
        drop(send); // Includes a runtime initialization failure on that thread.
        state.poll(&egui::Context::default());
        assert!(!state.waiting);
        assert_eq!(state.error, Some(ProviderError::Unavailable));
        assert!(state.worker.is_none());
        assert!(state.can_retry(), "Recovery must remain a user action");
    }
    #[test]
    fn a_replaced_or_cancelled_query_cannot_publish_old_results() {
        let mut state = State::new();
        state.search("first");
        let old = state.generation;
        state.search("Café 東京");
        let current = state.generation;
        let (commands, _receive) = mpsc::channel(2);
        let (send, events) = mpsc::channel(1);
        state.worker = Some(Worker {
            commands,
            events,
            generation: Arc::new(AtomicU64::new(0)),
        });
        let page = Arc::new(Results {
            items: vec![],
            offset: 0,
            more: false,
        });
        send.try_send((old, Ok(page.clone()))).unwrap();
        state.poll(&egui::Context::default());
        assert!(state.page.is_none());
        assert!(state.waiting);
        send.try_send((current, Ok(page.clone()))).unwrap();
        state.poll(&egui::Context::default());
        assert!(state.page.is_some());
        assert!(!state.waiting);
        state.cancel();
        send.try_send((current, Ok(page))).unwrap();
        state.poll(&egui::Context::default());
        assert!(state.page.is_none());
        assert!(!state.waiting);
    }
    #[test]
    fn switching_from_musicbrainz_to_demo_rebuilds_the_correct_catalog() {
        let mut model = Model::new(Preferences::default());
        model.music.selected = true;
        model.online.selected = false;
        model.set_online_source(false);
        model.query = "Café".into();
        model.search();
        assert_eq!(model.catalog.results().len(), 1);
        assert_eq!(model.catalog.results()[0].id.provider.as_str(), "demo");
        assert!(!model.music.selected);
    }
}
