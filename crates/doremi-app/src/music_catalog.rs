//! One cancelable metadata actor shared by Linux, Windows and Android.
use super::*;
use doremi_core::{
    catalog::ProviderError,
    media::{CatalogEntry, EntityKind},
};
use doremi_musicbrainz::Client;
#[derive(Clone, Debug)]
pub(super) struct Results {
    items: Vec<CatalogEntry>,
    offset: u32,
    more: bool,
    detail: Option<CatalogEntry>,
    continuation: Option<String>,
}
impl From<doremi_musicbrainz::Page> for Results {
    fn from(page: doremi_musicbrainz::Page) -> Self {
        Self {
            items: page.items,
            offset: page.offset,
            more: page.more,
            detail: page.detail,
            continuation: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Request {
    Search {
        query: String,
        kind: EntityKind,
        offset: u32,
    },
    ArtistRecordings {
        id: MediaId,
        offset: u32,
    },
    ArtistReleases {
        id: MediaId,
        offset: u32,
    },
    ReleaseTracks {
        id: MediaId,
    },
    YouTube(doremi_youtube::Request),
}
impl Request {
    fn musicbrainz(&self) -> Option<doremi_musicbrainz::Request> {
        use doremi_musicbrainz::Request as Mb;
        Some(match self {
            Self::Search {
                query,
                kind,
                offset,
            } => Mb::Search {
                query: query.clone(),
                kind: *kind,
                offset: *offset,
            },
            Self::ArtistRecordings { id, offset } => Mb::ArtistRecordings {
                id: id.clone(),
                offset: *offset,
            },
            Self::ArtistReleases { id, offset } => Mb::ArtistReleases {
                id: id.clone(),
                offset: *offset,
            },
            Self::ReleaseTracks { id } => Mb::ReleaseTracks { id: id.clone() },
            Self::YouTube(_) => return None,
        })
    }
}

use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::mpsc;

type Completion = (u64, Result<Arc<Results>, ProviderError>);
type HistoryEntry = (
    Option<CatalogEntry>,
    Option<Arc<Results>>,
    Option<Request>,
    Vec<u32>,
    bool,
);
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
    fn new(youtube: youtube_session::SharedClient) -> Result<Self, ProviderError> {
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
                            if let Request::YouTube(request) = &request {
                                match youtube
                                    .lock()
                                    .await
                                    .get_or_insert_with(doremi_youtube::Client::new)
                                {
                                    Ok(client) => client.fetch(request).await.map(|page| {
                                        Arc::new(Results {
                                            items: page
                                                .sections
                                                .iter()
                                                .flat_map(|s| &s.items)
                                                .filter_map(doremi_youtube::entry)
                                                .take(200)
                                                .collect(),
                                            offset: 0,
                                            more: page.continuation.is_some(),
                                            detail: None,
                                            continuation: page.continuation,
                                        })
                                    }),
                                    Err(error) => Err(*error),
                                }
                            } else {
                                match &mut client {
                                    Ok(client) => client
                                        .fetch(&request.musicbrainz().unwrap())
                                        .await
                                        .map(|page| Arc::new(Results::from(page))),
                                    Err(error) => Err(*error),
                                }
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
    pub youtube: bool,
    pub session: youtube_session::State,
    pub kind: EntityKind,
    pub page: Option<Arc<Results>>,
    pub detail: Option<CatalogEntry>,
    pub error: Option<ProviderError>,
    pub waiting: bool,
    request: Option<Request>,
    retry_at: Option<Instant>,
    history: Vec<HistoryEntry>,
    previous_offsets: Vec<u32>,
    generation: u64,
    pending: Option<(Option<Request>, Instant)>,
    worker: Option<Worker>,
}
impl State {
    pub fn new() -> Self {
        Self {
            selected: false,
            youtube: true,
            session: youtube_session::State::new(),
            kind: EntityKind::Recording,
            page: None,
            detail: None,
            error: None,
            waiting: false,
            request: None,
            retry_at: None,
            history: vec![],
            previous_offsets: vec![],
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
        self.previous_offsets.clear();
        self.dispatch(
            (!query.trim().is_empty()).then(|| {
                if self.youtube {
                    Request::YouTube(doremi_youtube::Request::Search {
                        query: query.into(),
                        kind: self.kind,
                    })
                } else {
                    Request::Search {
                        query: query.into(),
                        kind: self.kind,
                        offset: 0,
                    }
                }
            }),
            Duration::from_millis(350),
        );
    }
    pub fn cancel(&mut self) {
        self.page = None;
        self.detail = None;
        self.history.clear();
        self.previous_offsets.clear();
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
        self.history.push((
            self.detail.take(),
            self.page.clone(),
            self.request.clone(),
            std::mem::take(&mut self.previous_offsets),
            self.youtube,
        ));
        self.youtube = entry.media.id.provider.as_str() == doremi_youtube::PROVIDER;
        let request = if self.youtube {
            (entry.kind != EntityKind::Recording).then(|| {
                Request::YouTube(doremi_youtube::Request::Browse {
                    id: entry.media.id.local().into(),
                })
            })
        } else {
            match entry.kind {
                EntityKind::Recording | EntityKind::Playlist => None,
                EntityKind::Artist => Some(Request::ArtistRecordings {
                    id: entry.media.id.clone(),
                    offset: 0,
                }),
                EntityKind::Release => Some(Request::ReleaseTracks {
                    id: entry.media.id.clone(),
                }),
            }
        };
        self.dispatch(request, Duration::ZERO);
        self.detail = Some(entry);
        self.page = None;
    }
    pub fn back(&mut self) {
        if let Some((detail, page, request, previous_offsets, youtube)) = self.history.pop() {
            self.dispatch(None, Duration::ZERO);
            self.youtube = youtube;
            self.detail = detail;
            self.page = page;
            self.previous_offsets = previous_offsets;
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
    pub fn artist_releases(&self) -> bool {
        matches!(self.request, Some(Request::ArtistReleases { .. }))
    }
    pub fn browse_artist(&mut self, releases: bool) {
        if self.youtube {
            return;
        }
        let Some(entry) = &self.detail else { return };
        if entry.kind != EntityKind::Artist || releases == self.artist_releases() {
            return;
        }
        let id = entry.media.id.clone();
        self.page = None;
        self.previous_offsets.clear();
        self.dispatch(
            Some(if releases {
                Request::ArtistReleases { id, offset: 0 }
            } else {
                Request::ArtistRecordings { id, offset: 0 }
            }),
            Duration::ZERO,
        );
    }
    pub fn page_by(&mut self, forward: bool) {
        if self.youtube {
            let token = self
                .page
                .as_ref()
                .and_then(|page| page.continuation.clone());
            if forward && let Some(token) = token {
                let search = matches!(
                    self.request,
                    Some(Request::YouTube(
                        doremi_youtube::Request::Search { .. }
                            | doremi_youtube::Request::Continue { search: true, .. }
                    ))
                );
                if self.history.len() == 8 {
                    self.history.remove(0);
                }
                self.history.push((
                    self.detail.clone(),
                    self.page.clone(),
                    self.request.clone(),
                    vec![],
                    self.youtube,
                ));
                self.dispatch(
                    Some(Request::YouTube(doremi_youtube::Request::Continue {
                        token,
                        search,
                    })),
                    Duration::ZERO,
                );
                self.page = None;
            }
            return;
        }
        let Some(page) = &self.page else {
            return;
        };
        if forward && !page.more {
            return;
        }
        let offset = if forward {
            page.offset
                + if matches!(self.request, Some(Request::Search { .. })) {
                    doremi_musicbrainz::PAGE_SIZE as u32
                } else {
                    page.items.len() as u32
                }
        } else {
            let Some(offset) = self.previous_offsets.last().copied() else {
                return;
            };
            offset
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
            Some(Request::ArtistReleases { id, .. }) => Request::ArtistReleases {
                id: id.clone(),
                offset,
            },
            _ => return,
        };
        if forward {
            self.previous_offsets.push(page.offset);
        } else {
            self.previous_offsets.pop();
        }
        self.dispatch(Some(request), Duration::ZERO);
        self.page = None;
    }
    pub fn poll(&mut self, ctx: &egui::Context) {
        let revision = self.session.revision;
        let signed_in = self.session.account.is_some();
        self.session.poll(ctx);
        if (signed_in && self.session.account.is_none())
            || (revision != self.session.revision && self.session.account.is_some())
        {
            self.cancel();
        }
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
                        Ok(page) => {
                            if let Some(detail) = &page.detail
                                && self
                                    .detail
                                    .as_ref()
                                    .is_some_and(|current| current.media.id == detail.media.id)
                            {
                                self.detail = Some(detail.clone());
                            }
                            self.page = Some(page);
                        }
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
            match Worker::new(self.session.client.clone()) {
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
    pub fn youtube_page(&mut self, request: doremi_youtube::Request) {
        self.cancel();
        self.selected = true;
        self.youtube = true;
        self.dispatch(Some(Request::YouTube(request)), Duration::ZERO);
    }
    pub fn home(&mut self) {
        self.cancel();
        self.selected = true;
        self.youtube = true;
        self.dispatch(
            Some(Request::YouTube(doremi_youtube::Request::Home)),
            Duration::ZERO,
        );
    }
    pub fn snapshot(&self) -> serde_json::Value {
        serde_json::json!({"selected":self.selected,"provider":if self.youtube {"youtube-music"} else {"musicbrainz"},"kind":format!("{:?}",self.kind),"generation":self.generation,"waiting":self.waiting,"count":self.page.as_ref().map(|page|page.items.len()),"offset":self.page.as_ref().map(|page|page.offset),"artist_releases":self.artist_releases(),"detail":self.detail.as_ref().map(|item|item.media.id.local()),"error":self.error.map(|error|format!("{error:?}"))})
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
            ui.label(if state.youtube {
                "Consultando YouTube Music…"
            } else {
                "Consultando MusicBrainz…"
            });
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
                ui.push_id(index, |ui| self.music_row(ui, item, index));
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
                description: Some(
                    if entry.media.id.provider.as_str() == doremi_youtube::PROVIDER {
                        "YouTube Music"
                    } else {
                        "MusicBrainz · metadatos públicos"
                    },
                ),
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
                                edition: None,
                            });
                        }
                    }
                    if let Some(ms) = entry.media.duration_ms {
                        if !entry.artists.is_empty() {
                            ui.weak("•");
                        }
                        ui.weak(duration(ms));
                    }
                    let edition = edition_label(entry);
                    if !edition.is_empty() {
                        if !entry.artists.is_empty() || entry.media.duration_ms.is_some() {
                            ui.weak("•");
                        }
                        ui.weak(edition);
                    }
                }
            },
        );
        let youtube = entry.media.id.provider.as_str() == doremi_youtube::PROVIDER;
        if !youtube {
            ui.weak("MusicBrainz proporciona metadatos; esta selección no incluye audio.");
        }
        if entry.kind == EntityKind::Release {
            ui.small(
                self.model
                    .as_ref()
                    .unwrap()
                    .artwork
                    .caption(&entry.media.id),
            );
        }
        ui.small(format!(
            "ID de {}: {}",
            if youtube {
                "YouTube Music"
            } else {
                "MusicBrainz"
            },
            entry.media.id.local()
        ));
        {
            let saved = self
                .model
                .as_ref()
                .unwrap()
                .library
                .contains(&entry.media.id);
            if self.button(
                ui,
                &format!("catalog-save:{}", entry.media.id.local()),
                if saved {
                    "Quitar de biblioteca"
                } else {
                    match entry.kind {
                        EntityKind::Artist => "Guardar artista",
                        EntityKind::Release => "Guardar álbum",
                        EntityKind::Playlist => "Guardar playlist",
                        EntityKind::Recording => "Guardar",
                    }
                },
                190.0,
            ) {
                let model = self.model.as_mut().unwrap();
                if saved {
                    model.remove_saved(&entry.media.id);
                } else {
                    model.save_media(entry.media.clone());
                }
            }
        }
        if youtube {
            let tracks: Vec<_> = if entry.kind == EntityKind::Recording {
                vec![entry.media.clone()]
            } else {
                self.model
                    .as_ref()
                    .unwrap()
                    .music
                    .page
                    .as_ref()
                    .into_iter()
                    .flat_map(|page| &page.items)
                    .filter(|item| item.kind == EntityKind::Recording)
                    .map(|item| item.media.clone())
                    .collect()
            };
            if !tracks.is_empty() {
                ui.horizontal_wrapped(|ui| {
                    if self.button(ui, "youtube-play-context", "Reproducir", 160.0) {
                        self.model
                            .as_mut()
                            .unwrap()
                            .start_context(tracks.clone(), 0);
                        self.navigate(Page::Player);
                    }
                    if self.button(ui, "youtube-enqueue-context", "Añadir a la cola", 190.0) {
                        self.model
                            .as_mut()
                            .unwrap()
                            .command(Command::Append(tracks.clone()));
                    }
                });
            }
        }
        if entry.kind == EntityKind::Artist && !youtube {
            ui.add_space(12.0);
            let releases = self.model.as_ref().unwrap().music.artist_releases();
            ui.horizontal_wrapped(|ui| {
                for (selected_releases, name, label) in [
                    (false, "artist-recordings", "Grabaciones"),
                    (true, "artist-releases", "Álbumes y ediciones"),
                ] {
                    let response = ui.add(
                        egui::Button::new(label)
                            .selected(releases == selected_releases)
                            .min_size(egui::vec2(0.0, TOUCH)),
                    );
                    self.record(name, &response);
                    if response.clicked() {
                        self.model
                            .as_mut()
                            .unwrap()
                            .music
                            .browse_artist(selected_releases);
                    }
                }
            });
        } else if entry.kind == EntityKind::Release {
            ui.add_space(12.0);
            ui.label(egui::RichText::new("Canciones").font(theme::bold(20.0)));
        }
    }

    fn music_row(&mut self, ui: &mut egui::Ui, entry: &CatalogEntry, row_index: usize) {
        use doremi_ui::{self as theme, Icon};
        let prefs = &self.model.as_ref().unwrap().prefs;
        let palette = theme::palette(prefs.light, prefs.green);
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
        let edition = edition_label(entry);
        if !edition.is_empty() {
            subtitle = format!("{edition} · {subtitle}");
        }
        let row = theme::list_row(
            ui,
            &palette,
            ui.id().with(("catalog-row", entry.media.id.local())),
            theme::ListRow {
                title: &entry.media.title,
                subtitle: &subtitle,
                round: entry.kind == EntityKind::Artist,
                actions: 1,
                action_label: if entry.media.id.provider.as_str() == doremi_youtube::PROVIDER
                    && entry.kind == EntityKind::Recording
                {
                    "Reproducir"
                } else {
                    "Explorar"
                },
            },
        );
        self.record(
            &format!("catalog-open:{}", entry.media.id.local()),
            &row.body,
        );
        if entry.media.id.provider.as_str() == doremi_youtube::PROVIDER
            && ui.is_rect_visible(row.cover_rect())
        {
            let model = self.model.as_mut().unwrap();
            let url = model
                .music
                .session
                .client
                .try_lock()
                .ok()
                .and_then(|client| {
                    client
                        .as_ref()
                        .and_then(|client| client.as_ref().ok())
                        .and_then(|client| client.thumbnail(&entry.media.id))
                });
            model
                .artwork
                .request_thumbnail(&entry.media.id, url, ui.ctx());
            if let Some(cover) =
                self.artwork_textures
                    .current(&model.artwork, &entry.media.id, ui.ctx())
            {
                egui::Image::new((cover.id(), cover.size_vec2()))
                    .fit_to_exact_size(row.cover_rect().size())
                    .corner_radius(if entry.kind == EntityKind::Artist {
                        20
                    } else {
                        4
                    })
                    .paint_at(ui, row.cover_rect());
            }
        }
        {
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
            if entry.media.id.provider.as_str() == doremi_youtube::PROVIDER
                && entry.kind == EntityKind::Recording
            {
                let model = self.model.as_mut().unwrap();
                let tracks: Vec<_> = model
                    .music
                    .page
                    .as_ref()
                    .into_iter()
                    .flat_map(|page| &page.items)
                    .filter(|item| item.kind == EntityKind::Recording)
                    .map(|item| item.media.clone())
                    .collect();
                let index = model
                    .music
                    .page
                    .as_ref()
                    .map(|page| {
                        page.items
                            .iter()
                            .take(row_index)
                            .filter(|item| item.kind == EntityKind::Recording)
                            .count()
                    })
                    .unwrap_or(0);
                model.start_context(tracks, index);
                self.navigate(Page::Player);
            } else {
                self.model.as_mut().unwrap().music.open(entry.clone());
            }
        }
    }
}

fn kind_label(kind: EntityKind) -> &'static str {
    match kind {
        EntityKind::Recording => "Grabación",
        EntityKind::Artist => "Artista",
        EntityKind::Release => "Álbum / edición",
        EntityKind::Playlist => "Playlist",
    }
}
fn duration(ms: u64) -> String {
    format!("{}:{:02}", ms / 60000, ms / 1000 % 60)
}
fn edition_label(entry: &CatalogEntry) -> String {
    entry.edition.as_ref().map_or_else(String::new, |edition| {
        [
            edition.date.as_deref(),
            edition.country.as_deref(),
            edition.description.as_deref(),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" · ")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn youtube_ids_and_source_survive_navigation_into_optional_metadata_and_back() {
        let mut state = State::new();
        state.search("Chopin");
        assert!(matches!(
            state.request,
            Some(Request::YouTube(doremi_youtube::Request::Search { .. }))
        ));
        let prior = state.request.clone();
        let entry = CatalogEntry {
            media: Media {
                id: MediaId::new(
                    ProviderId::new("musicbrainz").unwrap(),
                    "artist:00000000-0000-4000-8000-000000000001",
                )
                .unwrap(),
                title: "Optional metadata".into(),
                duration_ms: None,
            },
            kind: EntityKind::Artist,
            artists: vec![],
            edition: None,
        };
        state.open(entry);
        assert!(!state.youtube);
        state.back();
        assert!(state.youtube);
        assert_eq!(state.request, prior);
        let album = doremi_youtube::id("MPREb_exact_album").unwrap();
        state.open(CatalogEntry {
            media: Media {
                id: album.clone(),
                title: "Album".into(),
                duration_ms: None,
            },
            kind: EntityKind::Release,
            artists: vec![],
            edition: None,
        });
        assert_eq!(
            state.request,
            Some(Request::YouTube(doremi_youtube::Request::Browse {
                id: album.local().into()
            }))
        );
    }
    #[test]
    fn youtube_pagination_preserves_the_provider_token_and_previous_results() {
        let mut state = State::new();
        state.search("Chopin");
        let page = Arc::new(Results {
            items: vec![],
            offset: 0,
            more: true,
            detail: None,
            continuation: Some("provider-token".into()),
        });
        state.page = Some(page.clone());
        state.page_by(true);
        assert_eq!(
            state.request,
            Some(Request::YouTube(doremi_youtube::Request::Continue {
                token: "provider-token".into(),
                search: true
            }))
        );
        state.back();
        assert!(Arc::ptr_eq(state.page.as_ref().unwrap(), &page));
        assert!(state.youtube);
    }
    #[test]
    fn artist_edition_paging_and_back_preserve_the_actual_page_and_cancel_old_queries() {
        let make_entry = |kind, title: &str| CatalogEntry {
            media: Media {
                id: MediaId::new(
                    ProviderId::new("musicbrainz").unwrap(),
                    format!(
                        "{}:00000000-0000-4000-8000-000000000001",
                        match kind {
                            EntityKind::Artist => "artist",
                            _ => "release",
                        }
                    ),
                )
                .unwrap(),
                title: title.into(),
                duration_ms: None,
            },
            kind,
            artists: vec![],
            edition: None,
        };
        let artist = make_entry(EntityKind::Artist, "QA artist");
        let release = make_entry(EntityKind::Release, "QA edition");
        let mut state = State::new();
        state.youtube = false;
        state.open(artist.clone());
        let recordings_generation = state.generation;
        state.browse_artist(true);
        assert!(state.generation > recordings_generation);
        assert!(state.artist_releases());
        state.page = Some(Arc::new(Results {
            continuation: None,
            detail: None,
            items: vec![release.clone(); 3],
            offset: 0,
            more: true,
        }));
        state.page_by(true);
        assert_eq!(
            state.request,
            Some(Request::ArtistReleases {
                id: artist.media.id.clone(),
                offset: 3
            })
        );
        state.page = Some(Arc::new(Results {
            continuation: None,
            detail: None,
            items: vec![release.clone(); 2],
            offset: 3,
            more: true,
        }));
        state.open(release.clone());
        state.back();
        assert_eq!(state.detail, Some(artist.clone()));
        assert!(state.artist_releases());
        assert_eq!(state.page.as_ref().unwrap().offset, 3);
        state.page_by(false);
        assert_eq!(
            state.request,
            Some(Request::ArtistReleases {
                id: artist.media.id.clone(),
                offset: 0
            })
        );
        state.browse_artist(false);
        assert!(!state.artist_releases());
        assert!(state.page.is_none() && state.previous_offsets.is_empty());
        assert_eq!(
            state.request,
            Some(Request::ArtistRecordings {
                id: artist.media.id,
                offset: 0
            })
        );
        state.cancel();
        assert!(state.history.is_empty() && state.previous_offsets.is_empty());
    }
    #[test]
    fn a_worker_that_exits_after_accepting_a_query_reports_unavailable() {
        let mut state = State::new();
        state.youtube = false;
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
        state.youtube = false;
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
            continuation: None,
            detail: None,
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
