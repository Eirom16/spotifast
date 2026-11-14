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
    sections: Vec<ResultSection>,
    subtitles: Vec<String>,
}
#[derive(Clone, Debug)]
struct ResultSection {
    title: String,
    rows: std::ops::Range<usize>,
}
impl Results {
    fn play_context(&self, row: usize) -> (Vec<Media>, usize) {
        let range = self
            .sections
            .iter()
            .find(|section| section.rows.contains(&row))
            .map(|section| section.rows.clone())
            .unwrap_or(0..self.items.len());
        let tracks = self.items[range.clone()]
            .iter()
            .filter(|item| item.kind == EntityKind::Recording)
            .map(|item| item.media.clone())
            .collect();
        let selected = self.items[range.start..row.min(range.end)]
            .iter()
            .filter(|item| item.kind == EntityKind::Recording)
            .count();
        (tracks, selected)
    }
}
impl From<doremi_youtube::parser::Page> for Results {
    fn from(page: doremi_youtube::parser::Page) -> Self {
        let mut items = Vec::new();
        let mut sections = Vec::new();
        let mut subtitles = Vec::new();
        for section in page.sections {
            let start = items.len();
            for item in section.items {
                if items.len() == 200 {
                    break;
                }
                if let Some(entry) = doremi_youtube::entry(&item) {
                    items.push(entry);
                    subtitles.push(item.subtitle);
                }
            }
            if items.len() > start {
                sections.push(ResultSection {
                    title: section.title,
                    rows: start..items.len(),
                });
            }
            if items.len() == 200 {
                break;
            }
        }
        Self {
            items,
            sections,
            subtitles,
            offset: 0,
            more: page.continuation.is_some(),
            detail: None,
            continuation: page.continuation,
        }
    }
}
impl From<doremi_musicbrainz::Page> for Results {
    fn from(page: doremi_musicbrainz::Page) -> Self {
        Self {
            items: page.items,
            offset: page.offset,
            more: page.more,
            detail: page.detail,
            continuation: None,
            sections: vec![],
            subtitles: vec![],
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
struct HistoryEntry {
    detail: Option<CatalogEntry>,
    page: Option<Arc<Results>>,
    request: Option<Request>,
    previous_offsets: Vec<u32>,
    youtube: bool,
    home_context: bool,
}
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
                                    Ok(client) => client
                                        .fetch(request)
                                        .await
                                        .map(|page| Arc::new(Results::from(page))),
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
    home_context: bool,
    home_cache: Option<(Arc<Results>, Instant, u64)>,
    home_started: bool,
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
            home_context: false,
            home_cache: None,
            home_started: false,
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
        self.home_context = false;
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
        self.home_context = false;
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
        self.history.push(HistoryEntry {
            detail: self.detail.take(),
            page: self.page.clone(),
            request: self.request.clone(),
            previous_offsets: std::mem::take(&mut self.previous_offsets),
            youtube: self.youtube,
            home_context: self.home_context,
        });
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
        if let Some(HistoryEntry {
            detail,
            page,
            request,
            previous_offsets,
            youtube,
            home_context,
        }) = self.history.pop()
        {
            self.dispatch(None, Duration::ZERO);
            self.youtube = youtube;
            self.home_context = home_context;
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
                self.history.push(HistoryEntry {
                    detail: self.detail.clone(),
                    page: self.page.clone(),
                    request: self.request.clone(),
                    previous_offsets: vec![],
                    youtube: self.youtube,
                    home_context: self.home_context,
                });
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
            let home = self.home_context;
            self.forget_home();
            if home {
                self.home();
            } else {
                self.cancel();
            }
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
                            if matches!(
                                self.request,
                                Some(Request::YouTube(doremi_youtube::Request::Home))
                            ) {
                                self.home_cache = Some((
                                    page.clone(),
                                    Instant::now() + Duration::from_secs(300),
                                    self.session.revision,
                                ));
                            }
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
        if request == doremi_youtube::Request::Home {
            self.home();
            return;
        }
        self.cancel();
        self.selected = true;
        self.youtube = true;
        self.dispatch(Some(Request::YouTube(request)), Duration::ZERO);
    }
    pub fn home(&mut self) {
        self.home_started = true;
        self.cancel();
        self.selected = true;
        self.youtube = true;
        self.home_context = true;
        if let Some((page, expiry, revision)) = &self.home_cache
            && *expiry > Instant::now()
            && *revision == self.session.revision
        {
            self.page = Some(page.clone());
            self.request = Some(Request::YouTube(doremi_youtube::Request::Home));
            self.pending = None;
            return;
        }
        self.dispatch(
            Some(Request::YouTube(doremi_youtube::Request::Home)),
            Duration::ZERO,
        );
    }
    pub fn home_root(&self) -> bool {
        self.home_context && self.youtube && self.selected && self.detail.is_none()
    }
    pub fn home_started(&self) -> bool {
        self.home_started
    }
    pub fn forget_home(&mut self) {
        self.home_cache = None;
    }
    pub fn can_refresh_home(&self) -> bool {
        !self.home_root()
            || (!self.waiting && self.retry_at.is_none_or(|due| Instant::now() >= due))
    }
    pub fn snapshot(&self) -> serde_json::Value {
        serde_json::json!({"selected":self.selected,"provider":if self.youtube {"youtube-music"} else {"musicbrainz"},"kind":format!("{:?}",self.kind),"generation":self.generation,"waiting":self.waiting,"count":self.page.as_ref().map(|page|page.items.len()),"offset":self.page.as_ref().map(|page|page.offset),"artist_releases":self.artist_releases(),"detail":self.detail.as_ref().map(|item|item.media.id.local()),"error":self.error.map(|error|format!("{error:?}"))})
    }
}

impl Model {
    pub fn youtube_page(&mut self, request: doremi_youtube::Request) {
        // Cancel only the old catalog request; playback resolution is a
        // separate generation and keeps running when browsing another source.
        self.cancel_online_search();
        self.online.selected = false;
        self.catalog.cancel();
        self.music.youtube_page(request);
        self.revision += 1;
    }
    pub fn refresh_music_home(&mut self) {
        self.music.forget_home();
        self.youtube_page(doremi_youtube::Request::Home);
    }
}

impl DoremiApp {
    pub(super) fn music_results(&mut self, ui: &mut egui::Ui) {
        use doremi_ui::feedback::{self, Kind};
        let prefs = &self.model.as_ref().unwrap().prefs;
        let palette = doremi_ui::palette(prefs.light, prefs.green);
        let detail = self.model.as_ref().unwrap().music.detail.clone();
        let page = self.model.as_ref().unwrap().music.page.clone();
        if self.model.as_ref().unwrap().music.has_back()
            && self.button(ui, "catalog-back", "Volver a resultados", 200.0)
        {
            self.catalog_back();
            return;
        }
        if let Some(detail) = detail {
            self.music_detail(ui, &detail);
        }
        let state = &self.model.as_ref().unwrap().music;
        if state.waiting {
            feedback::panel(
                ui,
                &palette,
                Kind::Loading,
                if state.youtube {
                    "Consultando YouTube Music…"
                } else {
                    "Consultando MusicBrainz…"
                },
                "Los resultados aparecerán aquí cuando responda el catálogo.",
            );
        }
        if let Some(error) = state.error {
            let retry = state.can_retry();
            feedback::panel(
                ui,
                &palette,
                Kind::Error,
                "No se pudo cargar la música",
                crate::search_view::provider_error_message(error),
            );
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
        let state = &self.model.as_ref().unwrap().music;
        if page.is_none() && !state.waiting && state.error.is_none() {
            feedback::panel(
                ui,
                &palette,
                Kind::Info,
                "Encuentra tu próxima canción",
                "Escribe un título, artista o álbum y elige el tipo de resultado que quieres explorar.",
            );
        }
        if let Some(page) = page {
            let state = &self.model.as_ref().unwrap().music;
            if page.items.is_empty() && !state.waiting && state.error.is_none() {
                feedback::panel(
                    ui,
                    &palette,
                    Kind::Empty,
                    "No hay resultados para esta consulta",
                    "Prueba otro título o nombre de artista, o cambia el tipo de resultado.",
                );
            } else if !page.items.is_empty() {
                ui.add_space(12.0);
                ui.label(
                    egui::RichText::new(format!("{} resultados", page.items.len()))
                        .font(doremi_ui::semibold(14.0))
                        .color(palette.secondary),
                );
                ui.add_space(4.0);
            }
            let tiles = !page.items.is_empty()
                && page
                    .items
                    .iter()
                    .all(|item| item.kind != EntityKind::Recording);
            if tiles {
                if page.sections.is_empty() {
                    self.music_grid(ui, &page.items);
                } else {
                    for (index, section) in page.sections.iter().enumerate() {
                        ui.push_id(("catalog-section", index), |ui| {
                            ui.add_space(12.0);
                            ui.label(
                                egui::RichText::new(&section.title).font(doremi_ui::bold(20.0)),
                            );
                            self.music_grid(ui, &page.items[section.rows.clone()]);
                        });
                    }
                }
            } else {
                for (index, item) in page.items.iter().enumerate() {
                    if let Some(section) = page.sections.iter().find(|section| {
                        section.rows.start == index && !section.title.trim().is_empty()
                    }) {
                        ui.add_space(20.0);
                        ui.label(egui::RichText::new(&section.title).font(doremi_ui::bold(20.0)));
                    }
                    ui.push_id(index, |ui| self.music_row(ui, item, index));
                }
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

    fn music_grid(&mut self, ui: &mut egui::Ui, items: &[CatalogEntry]) {
        use doremi_ui::{self as theme, cards};
        let prefs = &self.model.as_ref().unwrap().prefs;
        let palette = theme::palette(prefs.light, prefs.green);
        let width = ui.available_width();
        let preferred = if ui.ctx().content_rect().width() < 760.0 {
            136.0
        } else {
            cards::CARD_WIDTH
        };
        let columns = ((width + cards::CARD_GAP) / (preferred + cards::CARD_GAP))
            .floor()
            .max(1.0) as usize;
        let card_width = (width - (columns - 1) as f32 * cards::CARD_GAP) / columns as f32;
        for (chunk_index, chunk) in items.chunks(columns).enumerate() {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = cards::CARD_GAP;
                for (index, entry) in chunk.iter().enumerate() {
                    ui.push_id(
                        ("catalog-card", chunk_index, index, &entry.media.id),
                        |ui| {
                            ui.vertical(|ui| {
                                ui.set_width(card_width);
                                let subtitle = if entry.artists.is_empty() {
                                    kind_label(entry.kind).to_owned()
                                } else {
                                    entry
                                        .artists
                                        .iter()
                                        .map(|artist| artist.name.as_str())
                                        .collect::<Vec<_>>()
                                        .join(", ")
                                };
                                let card = cards::card_sized(
                                    ui,
                                    &palette,
                                    cards::Card {
                                        title: &entry.media.title,
                                        subtitle: &subtitle,
                                        cover: cards::CardCover {
                                            circular: entry.kind == EntityKind::Artist,
                                            ..Default::default()
                                        },
                                        play_label: "",
                                        play_size: TOUCH,
                                    },
                                    card_width,
                                    |ui, slot| match slot {
                                        cards::Slot::Cover { rect, radius } => {
                                            theme::cover_placeholder(
                                                ui,
                                                &palette,
                                                rect,
                                                radius,
                                                if entry.kind == EntityKind::Artist {
                                                    theme::Icon::User
                                                } else {
                                                    theme::Icon::Disc
                                                },
                                            );
                                            self.media_artwork(
                                                ui,
                                                &entry.media.id,
                                                rect,
                                                radius.min(127.0) as u8,
                                            );
                                        }
                                        cards::Slot::Text {
                                            text,
                                            rect,
                                            font,
                                            color,
                                            rows,
                                        } => {
                                            let mut job = egui::text::LayoutJob::simple(
                                                text.into(),
                                                font,
                                                color,
                                                rect.width(),
                                            );
                                            job.wrap.max_rows = rows;
                                            let galley = ui.painter().layout_job(job);
                                            ui.painter().galley(rect.min, galley, color);
                                        }
                                    },
                                );
                                let response = card
                                    .response
                                    .on_hover_text(format!("{}\n{}", entry.media.title, subtitle));
                                self.record(
                                    &format!("catalog-open:{}", entry.media.id.local()),
                                    &response,
                                );
                                if card.clicked {
                                    self.model.as_mut().unwrap().music.open(entry.clone());
                                    self.navigate(Page::Search);
                                }
                                let saved = self
                                    .model
                                    .as_ref()
                                    .unwrap()
                                    .library
                                    .contains(&entry.media.id);
                                if self.choice_button(
                                    ui,
                                    &format!("catalog-save:{}", entry.media.id.local()),
                                    if saved { "Quitar" } else { "Guardar" },
                                    card_width,
                                    saved,
                                ) {
                                    let model = self.model.as_mut().unwrap();
                                    if saved {
                                        model.remove_saved(&entry.media.id);
                                    } else {
                                        model.save_media(entry.media.clone());
                                    }
                                }
                            });
                        },
                    );
                }
            });
            ui.add_space(12.0);
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
                        let response = ui.add(
                            egui::Button::new(&credit.name)
                                .frame(false)
                                .wrap()
                                .min_size(egui::vec2(TOUCH, TOUCH)),
                        );
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
        ui.collapsing("Información del catálogo", |ui| {
            ui.small(format!(
                "ID de {}: {}",
                if youtube {
                    "YouTube Music"
                } else {
                    "MusicBrainz"
                },
                entry.media.id.local()
            ))
        });
        {
            let saved = self
                .model
                .as_ref()
                .unwrap()
                .library
                .contains(&entry.media.id);
            if self.choice_button(
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
                saved,
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
                    .and_then(|page| {
                        page.items
                            .iter()
                            .position(|item| item.kind == EntityKind::Recording)
                            .map(|row| page.play_context(row).0)
                    })
                    .unwrap_or_default()
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
                            .corner_radius(18)
                            .min_size(egui::vec2(TOUCH, TOUCH)),
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
            self.model
                .as_ref()
                .unwrap()
                .music
                .page
                .as_ref()
                .and_then(|page| page.subtitles.get(row_index))
                .filter(|text| !text.is_empty())
                .cloned()
                .unwrap_or_else(|| kind_label(entry.kind).to_owned())
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
        let selected = entry.kind == EntityKind::Recording
            && self
                .model
                .as_ref()
                .unwrap()
                .queue
                .current()
                .is_some_and(|current| current.media.id == entry.media.id);
        let row = theme::selected_list_row(
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
            selected,
        );
        self.record(
            &format!("catalog-open:{}", entry.media.id.local()),
            &row.body,
        );
        self.media_artwork(
            ui,
            &entry.media.id,
            row.cover_rect(),
            if entry.kind == EntityKind::Artist {
                20
            } else {
                4
            },
        );
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
                let (tracks, index) = model
                    .music
                    .page
                    .as_ref()
                    .map(|page| page.play_context(row_index))
                    .unwrap_or_default();
                model.start_context(tracks, index);
                self.navigate(Page::Player);
            } else {
                self.model.as_mut().unwrap().music.open(entry.clone());
                self.navigate(Page::Search);
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
    fn home_fixture() -> Arc<Results> {
        use doremi_youtube::parser::{Item, Layout, Page, Section};
        let track = Item {
            title: "Fixture song".into(),
            subtitle: "Fixture artist without a browse ID".into(),
            video_id: "abcdefghijk".into(),
            duration: "1:23".into(),
            ..Default::default()
        };
        Arc::new(Results::from(Page {
            sections: vec![
                Section {
                    title: "Fixture first section".into(),
                    layout: Layout::Tracks,
                    items: vec![
                        Item {
                            title: "Invalid identity".into(),
                            browse_id: "https://invalid.test/".into(),
                            ..Default::default()
                        },
                        track.clone(),
                        Item {
                            title: "Fixture album".into(),
                            browse_id: "MPRE_exact_fixture".into(),
                            ..Default::default()
                        },
                        track,
                    ],
                },
                Section {
                    title: "Fixture second section".into(),
                    layout: Layout::Tracks,
                    items: vec![Item {
                        title: "Fixture other song".into(),
                        video_id: "lmnopqrstuv".into(),
                        ..Default::default()
                    }],
                },
            ],
            ..Default::default()
        }))
    }
    #[test]
    fn catalog_feedback_distinguishes_waiting_empty_and_access_refusal_without_network() {
        let _guard = crate::tests::UI_TEST_LOCK.lock().unwrap();
        let directory =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/catalog-feedback-ui-test");
        let ctx = egui::Context::default();
        let mut app = DoremiApp::new_context(&ctx, directory.clone());
        for (waiting, error, empty, expected) in [
            (false, None, false, "Encuentra tu próxima canción"),
            (true, None, true, "Consultando YouTube Music"),
            (false, None, true, "No hay resultados"),
            (
                false,
                Some(ProviderError::AccessDenied),
                false,
                "El proveedor rechazó",
            ),
        ] {
            let state = &mut app.model.as_mut().unwrap().music;
            state.youtube = true;
            state.waiting = waiting;
            state.error = error;
            state.page = if empty {
                Some(Arc::new(Results {
                    items: vec![],
                    offset: 0,
                    more: false,
                    detail: None,
                    continuation: None,
                    sections: vec![],
                    subtitles: vec![],
                }))
            } else {
                None
            };
            app.controls.clear();
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| app.music_results(ui));
            output.textures_delta.clear();
            let text = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) => Some(text.galley.text()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n");
            assert!(text.contains(expected), "{text}");
            if waiting {
                assert!(!text.contains("No hay resultados"));
            }
            if error == Some(ProviderError::AccessDenied) {
                assert!(
                    !app.controls
                        .iter()
                        .any(|control| control.name == "catalog-retry")
                );
            }
            assert!(app.model.as_ref().unwrap().music.worker.is_none());
        }
        drop(app);
        DoremiApp::shutdown();
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn catalog_tiles_keep_duplicate_entries_and_save_controls_inside_small_viewports() {
        let _guard = crate::tests::UI_TEST_LOCK.lock().unwrap();
        let directory =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/catalog-grid-ui-test");
        let ctx = egui::Context::default();
        let mut app = DoremiApp::new_context(&ctx, directory.clone());
        // Repeated IDs must still produce independent widgets at each position.
        let album = home_fixture().items[1].clone();
        let entries = vec![album.clone(), album.clone(), album];
        for light in [false, true] {
            app.model.as_mut().unwrap().prefs.light = light;
            for width in [296.0, 320.0, 360.0, 682.0, 1100.0] {
                app.controls.clear();
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(width, 900.0),
                        )),
                        ..Default::default()
                    },
                    |ui| app.music_grid(ui, &entries),
                );
                output.textures_delta.clear();
                let controls: Vec<_> = app
                    .controls
                    .iter()
                    .filter(|c| c.name.starts_with("catalog-"))
                    .collect();
                assert_eq!(controls.len(), 6);
                for control in &controls {
                    assert!(
                        control.rect[0] >= 0.0 && control.rect[2] <= width + 0.1,
                        "{width}: {:?}",
                        control.rect
                    );
                }
                for (index, control) in controls.iter().enumerate() {
                    let rect = egui::Rect::from_min_max(
                        egui::pos2(control.rect[0], control.rect[1]),
                        egui::pos2(control.rect[2], control.rect[3]),
                    );
                    for other in &controls[index + 1..] {
                        let other = egui::Rect::from_min_max(
                            egui::pos2(other.rect[0], other.rect[1]),
                            egui::pos2(other.rect[2], other.rect[3]),
                        );
                        assert!(!rect.intersects(other));
                    }
                }
                assert!(app.model.as_ref().unwrap().music.worker.is_none());
                assert!(app.model.as_ref().unwrap().queue.current().is_none());
            }
        }
        let save = app
            .controls
            .iter()
            .find(|control| control.name.starts_with("catalog-save:"))
            .unwrap()
            .rect;
        let pointer = egui::pos2((save[0] + save[2]) / 2.0, (save[1] + save[3]) / 2.0);
        for expected_saved in [true, false] {
            for pressed in [true, false] {
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1100.0, 900.0),
                        )),
                        events: vec![
                            egui::Event::PointerMoved(pointer),
                            egui::Event::PointerButton {
                                pos: pointer,
                                button: egui::PointerButton::Primary,
                                pressed,
                                modifiers: egui::Modifiers::NONE,
                            },
                        ],
                        ..Default::default()
                    },
                    |ui| app.music_grid(ui, &entries),
                );
                output.textures_delta.clear();
            }
            assert_eq!(
                app.model
                    .as_ref()
                    .unwrap()
                    .library
                    .contains(&entries[0].media.id),
                expected_saved
            );
            assert!(app.model.as_ref().unwrap().queue.current().is_none());
        }
        drop(app);
        DoremiApp::shutdown();
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn home_preserves_provider_sections_and_duplicate_track_contexts_after_invalid_entries_are_removed()
     {
        let page = home_fixture();
        assert_eq!(page.items.len(), 4);
        assert!(page.items[0].artists.is_empty());
        assert_eq!(page.subtitles[0], "Fixture artist without a browse ID");
        assert_eq!(page.sections[0].title, "Fixture first section");
        assert_eq!(page.sections[0].rows, 0..3);
        assert_eq!(page.sections[1].rows, 3..4);
        let (tracks, selected) = page.play_context(2);
        assert_eq!(tracks.len(), 2);
        assert_eq!(tracks[0].id, tracks[1].id);
        assert_eq!(tracks[0].duration_ms, Some(83_000));
        assert_eq!(
            selected, 1,
            "The selected duplicate is an occurrence, not a title match"
        );
        let (tracks, selected) = page.play_context(3);
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].id.local(), "lmnopqrstuv");
        assert_eq!(selected, 0);
    }
    #[test]
    fn home_cache_survives_search_and_back_but_not_a_changed_account_or_expiry() {
        let mut state = State::new();
        state.home();
        state.pending = None; // The worker accepted the initial home request.
        let (commands, _requests) = mpsc::channel(2);
        let (send, events) = mpsc::channel(1);
        state.worker = Some(Worker {
            commands,
            events,
            generation: Arc::new(AtomicU64::new(state.generation)),
        });
        let ctx = egui::Context::default();
        let page = home_fixture();
        send.try_send((state.generation, Ok(page.clone()))).unwrap();
        state.poll(&ctx);
        state.open(page.items[1].clone());
        assert!(!state.home_root() && state.detail.is_some());
        state.back();
        assert!(state.home_root());
        assert!(Arc::ptr_eq(state.page.as_ref().unwrap(), &page));
        state.search("A different query");
        assert!(
            state.home_started(),
            "Resuming an Activity must not restart home over an active search"
        );
        let old = state.generation;
        state.home();
        assert!(state.home_root() && !state.waiting && state.pending.is_none());
        send.try_send((
            old,
            Ok(Arc::new(Results {
                sections: vec![],
                subtitles: vec![],
                items: vec![],
                offset: 0,
                more: false,
                detail: None,
                continuation: None,
            })),
        ))
        .unwrap();
        state.poll(&ctx);
        assert!(
            Arc::ptr_eq(state.page.as_ref().unwrap(), &page),
            "A stale search cannot replace home"
        );
        state.session.revision += 1;
        state.home();
        assert!(state.page.is_none() && state.waiting);
        state.home_cache = Some((
            page,
            Instant::now() - Duration::from_secs(1),
            state.session.revision,
        ));
        state.home();
        assert!(state.page.is_none() && state.waiting);
        state.forget_home();
        assert!(state.home_cache.is_none());
    }
    #[test]
    fn shared_home_navigation_and_rows_fit_mobile_widths_without_implicit_test_network() {
        let _guard = crate::tests::UI_TEST_LOCK.lock().unwrap();
        let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(format!("../../target/home-layout-{}", std::process::id()));
        let ctx = egui::Context::default();
        let mut app = DoremiApp::new_context(&ctx, directory.clone());
        let page = home_fixture();
        let model = app.model.as_mut().unwrap();
        model.query = "Preserved query".into();
        model.music.home_cache = Some((
            page.clone(),
            Instant::now() + Duration::from_secs(60),
            model.music.session.revision,
        ));
        model.music.home();
        for light in [false, true] {
            app.model.as_mut().unwrap().prefs.light = light;
            for width in [360.0, 393.0, 430.0, 1240.0] {
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(width, 800.0),
                        )),
                        ..Default::default()
                    },
                    |ui| app.draw(ui),
                );
                output.textures_delta.clear();
                assert_eq!(app.model.as_ref().unwrap().page, Page::Home);
                let rows: Vec<_> = app
                    .controls
                    .iter()
                    .filter(|control| control.name.starts_with("catalog-open:"))
                    .collect();
                assert_eq!(rows.len(), 4);
                assert!(
                    rows.iter()
                        .all(|row| row.rect[0] >= 0.0 && row.rect[2] <= width)
                );
                assert!(app.model.as_ref().unwrap().music.worker.is_none());
            }
        }
        let more = Arc::make_mut(app.model.as_mut().unwrap().music.page.as_mut().unwrap());
        more.more = true;
        more.continuation = Some("fixture-home-continuation".into());
        app.model.as_mut().unwrap().music.page_by(true);
        assert!(
            app.can_go_back(),
            "Android Back must reach the previous home page"
        );
        app.go_back();
        assert_eq!(app.model.as_ref().unwrap().page, Page::Home);
        assert!(
            app.model
                .as_ref()
                .unwrap()
                .music
                .page
                .as_ref()
                .unwrap()
                .more,
            "Back must restore that page instead of resetting to the cached first page"
        );
        app.model
            .as_mut()
            .unwrap()
            .music
            .open(page.items[1].clone());
        app.navigate(Page::Search);
        assert!(app.can_go_back());
        app.go_back();
        assert_eq!(app.model.as_ref().unwrap().page, Page::Home);
        assert_eq!(app.model.as_ref().unwrap().query, "Preserved query");
        app.navigate(Page::Search);
        assert!(
            matches!(&app.model.as_ref().unwrap().music.request,Some(Request::YouTube(doremi_youtube::Request::Search {query,..})) if query=="Preserved query")
        );
        app.navigate(Page::Home);
        assert!(app.model.as_ref().unwrap().music.pending.is_none());
        drop(app);
        DoremiApp::shutdown();
        let _ = std::fs::remove_dir_all(directory);
    }
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
            sections: vec![],
            subtitles: vec![],
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
            sections: vec![],
            subtitles: vec![],
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
            sections: vec![],
            subtitles: vec![],
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
            sections: vec![],
            subtitles: vec![],
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
