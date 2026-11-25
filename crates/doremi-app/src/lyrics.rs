//! LRCLIB adapter adapted from Spotifast src/lyrics.rs (Carmine Paolino, MIT).
use super::*;

use serde::de::DeserializeOwned;
use sha1::{Digest, Sha1};
use std::io::Read;
use tokio::sync::{mpsc, watch};

const API: &str = "https://lrclib.net/api";
const CACHE_LIFETIME: Duration = Duration::from_secs(30 * 24 * 60 * 60);
const MISS_LIFETIME: Duration = Duration::from_secs(24 * 60 * 60);
const LIMIT: usize = 512 * 1024;
const DURATION_TOLERANCE: f64 = 2.0;

/// Track metadata used for lyrics lookup.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Query {
    pub artist: String,
    pub title: String,
    pub album: String,
    pub duration_ms: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Line {
    /// When the line starts; `None` in lyrics that carry no timing.
    pub at_ms: Option<u32>,
    pub text: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lyrics {
    pub lines: Vec<Line>,
    /// Every line has a time, so the panel can follow the song.
    pub synced: bool,
    /// The database knows there are no words.
    pub instrumental: bool,
}

impl Lyrics {
    /// The line being sung at `position_ms`, or `None` before the first one
    /// starts and for lyrics without timing.
    pub fn active_line(&self, position_ms: u32) -> Option<usize> {
        if !self.synced {
            return None;
        }
        let started = self
            .lines
            .partition_point(|line| line.at_ms.is_some_and(|at| at <= position_ms));
        started.checked_sub(1)
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Record {
    track_name: String,
    artist_name: String,
    album_name: String,
    duration: Option<f64>,
    instrumental: bool,
    synced_lyrics: Option<String>,
    plain_lyrics: Option<String>,
}

impl Record {
    fn synced(&self) -> Option<&str> {
        self.synced_lyrics
            .as_deref()
            .filter(|text| !text.trim().is_empty())
    }

    fn plain(&self) -> Option<&str> {
        self.plain_lyrics
            .as_deref()
            .filter(|text| !text.trim().is_empty())
    }

    fn lyrics(&self) -> Option<Lyrics> {
        if self.instrumental {
            return Some(Lyrics {
                instrumental: true,
                ..Lyrics::default()
            });
        }
        if let Some(synced) = self.synced() {
            let lines = parse_lrc(synced);
            if !lines.is_empty() {
                return Some(Lyrics {
                    lines,
                    synced: true,
                    instrumental: false,
                });
            }
        }
        let plain = self.plain()?;
        Some(Lyrics {
            lines: plain
                .lines()
                .map(|line| Line {
                    at_ms: None,
                    text: line.trim_end().to_string(),
                })
                .collect(),
            synced: false,
            instrumental: false,
        })
    }
}

fn normalize(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut space = false;
    for c in text.chars() {
        let folded = fold(c);
        if folded == '\'' || folded == '’' || folded == '`' {
            continue;
        }
        if folded == '&' {
            for word in [' ', 'a', 'n', 'd', ' '] {
                push_normalized(&mut out, &mut space, word);
            }
            continue;
        }
        push_normalized(&mut out, &mut space, folded);
    }
    out.trim().to_string()
}

fn push_normalized(out: &mut String, space: &mut bool, c: char) {
    if c.is_alphanumeric() {
        out.extend(c.to_lowercase());
        *space = false;
    } else if !*space {
        out.push(' ');
        *space = true;
    }
}

/// The plain letter behind the Latin accents titles most often carry.
fn fold(c: char) -> char {
    match c {
        'À'..='Å' | 'à'..='å' => 'a',
        'Ç' | 'ç' => 'c',
        'È'..='Ë' | 'è'..='ë' => 'e',
        'Ì'..='Ï' | 'ì'..='ï' => 'i',
        'Ñ' | 'ñ' => 'n',
        'Ò'..='Ö' | 'Ø' | 'ò'..='ö' | 'ø' => 'o',
        'Ù'..='Ü' | 'ù'..='ü' => 'u',
        'Ý' | 'ý' | 'ÿ' => 'y',
        'ß' => 's',
        _ => c,
    }
}

/// Parses `[mm:ss.xx]` lines. A line may open with several stamps when the
/// same words repeat; tags such as `[ar:...]` carry no digits and are
/// skipped. The result is sorted by time.
pub fn parse_lrc(text: &str) -> Vec<Line> {
    let offset = text
        .lines()
        .find_map(|line| {
            line.trim()
                .strip_prefix("[offset:")?
                .strip_suffix(']')?
                .trim()
                .parse::<i32>()
                .ok()
        })
        .unwrap_or(0);
    let mut lines = Vec::new();
    for raw in text.trim_start_matches('\u{feff}').lines() {
        let mut rest = raw.trim_start();
        let mut times = Vec::new();
        while let Some(stamp) = leading_stamp(rest) {
            times.push(stamp.0);
            rest = &rest[stamp.1..];
        }
        if times.is_empty() {
            continue;
        }
        let body = rest.trim();
        for at_ms in times {
            lines.push(Line {
                at_ms: Some(at_ms.saturating_add_signed(offset)),
                text: body.to_string(),
            });
        }
    }
    lines.sort_by_key(|line| line.at_ms);
    lines
}

/// A timestamp at the head of `text`: its milliseconds and its length.
fn leading_stamp(text: &str) -> Option<(u32, usize)> {
    let inner = text.strip_prefix('[')?;
    let close = inner.find(']')?;
    let stamp = &inner[..close];
    let (minutes, seconds) = stamp.split_once(':')?;
    let (seconds, fraction) = match seconds.find(['.', ':']) {
        Some(dot) => (&seconds[..dot], Some(&seconds[dot + 1..])),
        None => (seconds, None),
    };
    let minutes: u32 = minutes.parse().ok()?;
    let seconds: u32 = seconds.parse().ok()?;
    let fraction_ms = match fraction {
        Some(digits) if !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()) => {
            let value: u32 = digits.chars().take(3).collect::<String>().parse().ok()?;
            value * 10u32.pow(3u32.saturating_sub(digits.len().min(3) as u32))
        }
        Some(_) => return None,
        None => 0,
    };
    if seconds >= 60 {
        return None;
    }
    let time = minutes
        .checked_mul(60_000)?
        .checked_add(seconds * 1_000 + fraction_ms)?;
    Some((time, close + 2))
}

impl Query {
    fn for_media(media: &Media) -> Option<Self> {
        let music = media.music.as_ref()?;
        let artist = if music.artists.is_empty() {
            music.byline.clone()?
        } else {
            music
                .artists
                .iter()
                .map(|artist| artist.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        };
        let query = Self {
            artist,
            title: media.title.clone(),
            album: music
                .album
                .as_ref()
                .map_or_else(String::new, |album| album.name.clone()),
            duration_ms: u32::try_from(media.duration_ms?).ok()?,
        };
        (!normalize(&query.artist).is_empty()
            && !normalize(&query.title).is_empty()
            && (1000..=3_600_000).contains(&query.duration_ms))
        .then_some(query)
    }

    fn accepts(&self, candidate: &Record) -> bool {
        normalize(&self.title) == normalize(&candidate.track_name)
            && normalize(&self.artist) == normalize(&candidate.artist_name)
            && (self.album.is_empty() || normalize(&self.album) == normalize(&candidate.album_name))
            && candidate.duration.is_some_and(|duration| {
                (duration - f64::from(self.duration_ms) / 1000.0).abs() <= DURATION_TOLERANCE
            })
    }

    fn choose(&self, candidates: Vec<Record>) -> Option<Lyrics> {
        let mut matches = candidates
            .into_iter()
            .filter(|candidate| self.accepts(candidate));
        let first = matches.next()?.lyrics()?;
        // Keep uncertain results out, including distinct versions of equal length
        // where the catalog omitted the album.
        matches
            .all(|candidate| candidate.lyrics().as_ref() == Some(&first))
            .then_some(first)
    }
}

enum Failure {
    Network,
    Limited(tokio::time::Instant),
}

async fn get<T: DeserializeOwned>(
    http: &reqwest::Client,
    path: &str,
    params: &[(&str, &str)],
) -> Result<Option<T>, Failure> {
    let mut response = http
        .get(format!("{API}/{path}"))
        .query(params)
        .send()
        .await
        .map_err(|_| Failure::Network)?;
    if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
        let wait = response
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<u32>().ok())
            .unwrap_or(60);
        return Err(Failure::Limited(
            tokio::time::Instant::now() + Duration::from_secs(u64::from(wait)),
        ));
    }
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if !response.status().is_success() {
        return Err(Failure::Network);
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| Failure::Network)? {
        if chunk.len() > LIMIT.saturating_sub(body.len()) {
            return Err(Failure::Network);
        }
        body.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&body)
        .map(Some)
        .map_err(|_| Failure::Network)
}

async fn lookup(http: &reqwest::Client, query: &Query) -> Result<Option<Lyrics>, Failure> {
    let mut parameters = vec![
        ("artist_name", query.artist.as_str()),
        ("track_name", query.title.as_str()),
    ];
    if !query.album.is_empty() {
        let duration = ((query.duration_ms + 500) / 1000).to_string();
        let mut exact = parameters.clone();
        exact.extend([
            ("album_name", query.album.as_str()),
            ("duration", &duration),
        ]);
        if let Some(record) = get::<Record>(http, "get-cached", &exact).await?
            && query.accepts(&record)
            && let Some(lyrics) = record.lyrics()
        {
            return Ok(Some(lyrics));
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
        parameters.push(("album_name", &query.album));
    }
    let candidates = get::<Vec<Record>>(http, "search", &parameters)
        .await?
        .unwrap_or_default();
    Ok(query.choose(candidates))
}

#[derive(Deserialize, Serialize)]
struct Cached {
    found: Option<Lyrics>,
}

fn cache_path(directory: &std::path::Path, query: &Query) -> PathBuf {
    let key = serde_json::to_vec(&(&query.artist, &query.title, &query.album, query.duration_ms))
        .expect("lyrics metadata serializes");
    directory.join(format!("{:x}.json", Sha1::digest(key)))
}

fn read_cache(path: &std::path::Path) -> Option<Option<Lyrics>> {
    let file = std::fs::File::open(path).ok()?;
    let age = file.metadata().ok()?.modified().ok()?.elapsed().ok()?;
    if age >= CACHE_LIFETIME {
        return None;
    }
    let cached: Cached = serde_json::from_reader(file.take(LIMIT as u64)).ok()?;
    if cached.found.is_none() && age >= MISS_LIFETIME {
        return None;
    }
    Some(cached.found)
}

fn write_cache(path: &std::path::Path, found: &Option<Lyrics>) {
    let Ok(bytes) = serde_json::to_vec(&Cached {
        found: found.clone(),
    }) else {
        return;
    };
    if bytes.len() <= LIMIT
        && let Some(directory) = path.parent()
        && std::fs::create_dir_all(directory).is_ok()
        && let Ok(mut file) = tempfile::NamedTempFile::new_in(directory)
    {
        use std::io::Write;
        if file.write_all(&bytes).is_ok() {
            let _ = file.persist(path);
        }
        // Keep at most the most recent 128 cache entries.
        if let Ok(entries) = std::fs::read_dir(directory) {
            let mut files: Vec<_> = entries
                .filter_map(Result::ok)
                .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "json"))
                .filter_map(|entry| Some((entry.metadata().ok()?.modified().ok()?, entry.path())))
                .collect();
            files.sort_unstable_by(|a, b| b.0.cmp(&a.0));
            for (_, path) in files.into_iter().skip(128) {
                let _ = std::fs::remove_file(path);
            }
        }
    }
}

enum Status {
    Loading,
    Unavailable,
    Error,
    Ready(Lyrics),
}

struct Worker {
    request: watch::Sender<Option<(u64, Query)>>,
    result: mpsc::Receiver<(u64, Status)>,
}

impl Worker {
    fn start(directory: PathBuf, ctx: egui::Context) -> Option<Self> {
        let (request, mut requests) = watch::channel::<Option<(u64, Query)>>(None);
        let (results, result) = mpsc::channel(2);
        std::thread::Builder::new()
            .name("doremi-lyrics".into())
            .spawn(move || {
                let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                else {
                    ctx.request_repaint();
                    return;
                };
                runtime.block_on(async {
                    let http = reqwest::Client::builder()
                        .user_agent(concat!(
                            "Doremi/",
                            env!("CARGO_PKG_VERSION"),
                            " (https://github.com/Eirom16/spotifast)"
                        ))
                        .timeout(Duration::from_secs(5))
                        .build();
                    let mut retry_after = tokio::time::Instant::now();
                    loop {
                        let next = requests.borrow_and_update().clone();
                        let Some((generation, query)) = next else {
                            if requests.changed().await.is_err() {
                                break;
                            }
                            continue;
                        };
                        let path = cache_path(&directory, &query);
                        let cached = read_cache(&path);
                        let mut work = std::pin::pin!(async {
                            if let Some(found) = cached {
                                return Ok((found, false));
                            }
                            let http = http.as_ref().map_err(|_| Failure::Network)?;
                            if retry_after > tokio::time::Instant::now() {
                                return Err(Failure::Limited(retry_after));
                            }
                            tokio::time::sleep(Duration::from_millis(250)).await;
                            lookup(http, &query).await.map(|found| (found, true))
                        });
                        let result = tokio::select! {
                            biased;
                            result = &mut work => result,
                            change = requests.changed() => {
                                if change.is_err() { break; }
                                continue;
                            }
                        };
                        let status = match result {
                            Ok((found, needs_cache)) => {
                                if needs_cache {
                                    write_cache(&path, &found);
                                }
                                found.map_or(Status::Unavailable, Status::Ready)
                            }
                            Err(Failure::Network) => Status::Error,
                            Err(Failure::Limited(until)) => {
                                // Keep the provider's backoff across track changes.
                                retry_after = until;
                                Status::Error
                            }
                        };
                        tokio::select! {
                            result = results.send((generation, status)) => {
                                if result.is_err() { break; }
                                ctx.request_repaint();
                                if requests.changed().await.is_err() { break; }
                            }
                            change = requests.changed() => {
                                if change.is_err() { break; }
                            }
                        }
                    }
                });
            })
            .ok()?;
        Some(Self { request, result })
    }
}

pub(super) struct Panel {
    directory: PathBuf,
    worker: Option<Worker>,
    pub open: bool,
    generation: u64,
    query: Option<(doremi_core::queue::EntryId, Query)>,
    status: Status,
    last_line: Option<usize>,
}

impl Panel {
    pub fn new(directory: &std::path::Path) -> Self {
        Self {
            directory: directory.join("lyrics-v1"),
            worker: None,
            open: false,
            generation: 0,
            query: None,
            status: Status::Unavailable,
            last_line: None,
        }
    }

    fn select(&mut self, query: Option<(doremi_core::queue::EntryId, Query)>, ctx: &egui::Context) {
        if self.query == query {
            return;
        }
        self.query = query;
        self.last_line = None;
        self.generation += 1;
        self.status = Status::Unavailable;
        if let Some((_, query)) = &self.query {
            if self.worker.is_none() {
                self.worker = Worker::start(self.directory.clone(), ctx.clone());
            }
            self.status = if let Some(worker) = &self.worker {
                worker
                    .request
                    .send_replace(Some((self.generation, query.clone())));
                Status::Loading
            } else {
                Status::Error
            };
        } else if let Some(worker) = &self.worker {
            worker.request.send_replace(None);
        }
    }

    fn retry(&mut self, ctx: &egui::Context) {
        if self
            .worker
            .as_ref()
            .is_some_and(|worker| worker.request.is_closed())
        {
            self.worker = None;
        }
        let query = self.query.take();
        self.select(query, ctx);
    }

    pub fn hide(&mut self, ctx: &egui::Context) {
        self.open = false;
        self.select(None, ctx);
    }
}

impl DoremiApp {
    pub(super) fn show_lyrics(&mut self, ui: &mut egui::Ui) {
        let model = self.model.as_ref().unwrap();
        let current = model.queue.current();
        let query = current
            .and_then(|current| Query::for_media(&current.media).map(|query| (current.id, query)));
        let position = model.queue.position_ms().min(u64::from(u32::MAX)) as u32;
        let title = current
            .map(|entry| entry.media.title.clone())
            .unwrap_or_default();
        let seekable = model.real_audio();
        self.lyrics.select(query, ui.ctx());
        if let Some(worker) = &mut self.lyrics.worker {
            loop {
                match worker.result.try_recv() {
                    Ok((generation, status)) if generation == self.lyrics.generation => {
                        self.lyrics.status = status
                    }
                    Ok(_) => {}
                    Err(mpsc::error::TryRecvError::Disconnected) => {
                        self.lyrics.status = Status::Error;
                        break;
                    }
                    Err(mpsc::error::TryRecvError::Empty) => break,
                }
            }
        }

        if self.choice_button(ui, "lyrics-back", "Volver al reproductor", 205.0, false) {
            self.lyrics.hide(ui.ctx());
            return;
        }
        ui.heading(title);
        let mut seek = None;
        match &self.lyrics.status {
            Status::Loading => {
                ui.spinner();
                ui.label("Buscando letras en LRCLIB…");
            }
            Status::Unavailable => {
                ui.label("No hay letras verificadas para esta canción.");
            }
            Status::Error => {
                ui.label("LRCLIB no está disponible en este momento.");
                if ui.button("Reintentar").clicked() {
                    self.lyrics.retry(ui.ctx());
                }
            }
            Status::Ready(lyrics) if lyrics.instrumental => {
                ui.label("Pista instrumental");
            }
            Status::Ready(lyrics) => {
                ui.weak(if lyrics.synced {
                    "Letra sincronizada · LRCLIB"
                } else {
                    "Letra · LRCLIB"
                });
                let active = lyrics.active_line(position);
                egui::ScrollArea::vertical()
                    .id_salt(("lrclib", self.lyrics.generation))
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for (index, line) in lyrics.lines.iter().enumerate() {
                            let mut text = egui::RichText::new(&line.text).size(17.0);
                            if active == Some(index) {
                                text = text.strong().color(ui.visuals().selection.stroke.color);
                            }
                            let response = if let Some(time) = line.at_ms.filter(|_| seekable) {
                                let response = ui.add(egui::Button::new(text).wrap().frame(false));
                                if response.clicked() {
                                    seek = Some(time);
                                }
                                response
                            } else {
                                ui.add(egui::Label::new(text).wrap())
                            };
                            // Same active-line anchoring as the original lyrics panel.
                            if active == Some(index) && active != self.lyrics.last_line {
                                response.scroll_to_me(Some(egui::Align::Center));
                            }
                        }
                    });
                self.lyrics.last_line = active;
            }
        }
        if let Some(position) = seek {
            self.model
                .as_mut()
                .unwrap()
                .command(Command::Seek(u64::from(position)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parsing_and_seeking_use_sorted_stamps_with_offsets() {
        let lines = parse_lrc(
            "[offset:-200]\n[00:10.5][00:15.500]repeat\n[00:03.250]first\n[99999999:10]overflow",
        );
        let lyrics = Lyrics {
            lines,
            synced: true,
            instrumental: false,
        };
        assert_eq!(lyrics.lines.len(), 3);
        assert_eq!(lyrics.active_line(3049), None);
        assert_eq!(lyrics.active_line(3050), Some(0));
        assert_eq!(lyrics.active_line(15_300), Some(2));
        assert_eq!(lyrics.active_line(10_500), Some(1));
    }

    #[test]
    fn superficial_matches_do_not_supply_lyrics() {
        let query = Query {
            title: "Canción".into(),
            artist: "Artista".into(),
            album: "Estudio".into(),
            duration_ms: 120_000,
        };
        let record = |title: &str, artist: &str, album: &str, duration| Record {
            track_name: title.into(),
            artist_name: artist.into(),
            album_name: album.into(),
            duration: Some(duration),
            plain_lyrics: Some("test fixture".into()),
            ..Record::default()
        };
        assert!(query.accepts(&record("Cancion", "ARTISTA", "Estudio", 121.0)));
        assert!(!query.accepts(&record("Canción en vivo", "Artista", "Estudio", 120.0)));
        assert!(!query.accepts(&record("Canción", "Otro Artista", "Estudio", 120.0)));
        assert!(!query.accepts(&record("Canción", "Artista", "Concierto", 120.0)));
        assert!(!query.accepts(&record("Canción", "Artista", "Estudio", 123.0)));
    }
}
