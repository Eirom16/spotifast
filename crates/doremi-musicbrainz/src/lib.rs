//! Public MusicBrainz metadata. No credentials, audio extraction or title matching.
//! https://musicbrainz.org/doc/MusicBrainz_API
use doremi_core::{
    catalog::ProviderError as Error,
    media::{ArtistCredit, CatalogEntry, EntityKind, Media, MediaId, ProviderId},
};
use reqwest::{Client as HttpClient, redirect::Policy};
use serde_json::Value;
use std::{collections::VecDeque, time::Duration};
use tokio::time::{Instant, sleep_until, timeout};

pub const PAGE_SIZE: usize = 20;
static NEXT_REQUEST: std::sync::Mutex<Option<Instant>> = std::sync::Mutex::new(None);
const MAX_BYTES: usize = 1024 * 1024;
const MAX_OFFSET: u32 = 1000;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Request {
    Search {
        query: String,
        kind: EntityKind,
        offset: u32,
    },
    ArtistRecordings {
        id: MediaId,
        offset: u32,
    },
    ReleaseTracks {
        id: MediaId,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Page {
    pub items: Vec<CatalogEntry>,
    pub offset: u32,
    pub more: bool,
}

pub struct Client {
    http: HttpClient,
    // Eight metadata pages, at most 160 entries; no durable provider cache.
    cache: VecDeque<(Request, Instant, Page)>,
}
impl Client {
    pub fn new() -> Result<Self, Error> {
        Ok(Self {
            http: HttpClient::builder()
                .user_agent("Doremi/0.1.0 (https://github.com/Eirom16/spotifast)")
                .connect_timeout(Duration::from_secs(4))
                .timeout(Duration::from_secs(12))
                .redirect(Policy::none())
                .build()
                .map_err(network_error)?,
            cache: VecDeque::new(),
        })
    }

    /// Drop the future to cancel. Rate spacing survives cancellation/replacement.
    pub async fn fetch(&mut self, request: &Request) -> Result<Page, Error> {
        let (path, params) = parameters(request)?;
        if let Some((_, _, page)) = self
            .cache
            .iter()
            .find(|(key, time, _)| key == request && time.elapsed() < Duration::from_secs(300))
        {
            return Ok(page.clone());
        }
        let page = timeout(Duration::from_secs(15), async {
            // MusicBrainz requires at most one request per second per app.
            let due = {
                let mut next = NEXT_REQUEST.lock().map_err(|_| Error::Unavailable)?;
                let due = next.unwrap_or_else(Instant::now).max(Instant::now());
                *next = Some(due + Duration::from_millis(1100));
                due
            };
            sleep_until(due).await;
            let mut response = self
                .http
                .get(format!("https://musicbrainz.org/ws/2/{path}"))
                .query(&params)
                .send()
                .await
                .map_err(network_error)?;
            match response.status().as_u16() {
                200 => {}
                401 | 403 | 300..=399 => return Err(Error::AccessDenied),
                404 | 410 => return Err(Error::Unavailable),
                429 | 503 => {
                    return Err(Error::RateLimited {
                        retry_after_ms: response
                            .headers()
                            .get("retry-after")
                            .and_then(|v| v.to_str().ok())
                            .and_then(|v| v.parse::<u64>().ok())
                            .and_then(|v| v.checked_mul(1000)),
                    });
                }
                _ => return Err(Error::Transport),
            }
            if response
                .content_length()
                .is_some_and(|size| size > MAX_BYTES as u64)
            {
                return Err(Error::TooLarge);
            }
            let mut bytes = Vec::new();
            while let Some(chunk) = response.chunk().await.map_err(network_error)? {
                if chunk.len() > MAX_BYTES.saturating_sub(bytes.len()) {
                    return Err(Error::TooLarge);
                }
                bytes.extend_from_slice(&chunk);
            }
            parse(request, &bytes)
        })
        .await
        .map_err(|_| Error::Timeout)??;
        if page.items.len() <= PAGE_SIZE {
            if self.cache.len() == 8 {
                self.cache.pop_front();
            }
            self.cache
                .push_back((request.clone(), Instant::now(), page.clone()));
        }
        Ok(page)
    }
}

fn endpoint(kind: EntityKind) -> &'static str {
    match kind {
        EntityKind::Recording => "recording",
        EntityKind::Artist => "artist",
        EntityKind::Release => "release",
    }
}
fn network_error(error: reqwest::Error) -> Error {
    if error.is_timeout() {
        Error::Timeout
    } else {
        Error::Transport
    }
}
fn uuid(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(i, c)| {
            if [8, 13, 18, 23].contains(&i) {
                c == b'-'
            } else {
                c.is_ascii_hexdigit() && !c.is_ascii_uppercase()
            }
        })
}
fn identity(kind: EntityKind, id: &str) -> Result<MediaId, Error> {
    if !uuid(id) {
        return Err(Error::InvalidResponse);
    }
    MediaId::new(
        ProviderId::new("musicbrainz").unwrap(),
        format!("{}:{id}", endpoint(kind)),
    )
    .map_err(|_| Error::InvalidResponse)
}
fn local_id(id: &MediaId, kind: EntityKind) -> Result<&str, Error> {
    if id.provider.as_str() != "musicbrainz" {
        return Err(Error::InvalidRequest);
    }
    let (prefix, uuid_value) = id.local().split_once(':').ok_or(Error::InvalidRequest)?;
    if prefix != endpoint(kind) || !uuid(uuid_value) {
        return Err(Error::InvalidRequest);
    }
    Ok(uuid_value)
}
fn escape_query(query: &str) -> Result<String, Error> {
    let query = query.trim();
    if query.is_empty() || query.len() > 512 || query.chars().any(char::is_control) {
        return Err(Error::InvalidRequest);
    }
    let mut escaped = String::new();
    for ch in query.chars() {
        if "+-!(){}[]^\"~*?:\\/|&".contains(ch) {
            escaped.push('\\');
        }
        escaped.push(ch);
    }
    Ok(escaped)
}
fn parameters(request: &Request) -> Result<(String, Vec<(String, String)>), Error> {
    let mut params = vec![("fmt".into(), "json".into())];
    let path = match request {
        Request::Search {
            query,
            kind,
            offset,
        } => {
            if *offset > MAX_OFFSET || *offset % PAGE_SIZE as u32 != 0 {
                return Err(Error::InvalidRequest);
            }
            params.extend([
                ("query".into(), escape_query(query)?),
                ("limit".into(), PAGE_SIZE.to_string()),
                ("offset".into(), offset.to_string()),
            ]);
            endpoint(*kind).to_string()
        }
        Request::ArtistRecordings { id, offset } => {
            if *offset > MAX_OFFSET || *offset % PAGE_SIZE as u32 != 0 {
                return Err(Error::InvalidRequest);
            }
            params.extend([
                ("artist".into(), local_id(id, EntityKind::Artist)?.into()),
                ("inc".into(), "artist-credits".into()),
                ("limit".into(), PAGE_SIZE.to_string()),
                ("offset".into(), offset.to_string()),
            ]);
            "recording".into()
        }
        Request::ReleaseTracks { id } => {
            params.push(("inc".into(), "recordings+artist-credits".into()));
            format!("release/{}", local_id(id, EntityKind::Release)?)
        }
    };
    Ok((path, params))
}
fn text(value: &Value, name: &str) -> Result<String, Error> {
    let result = value[name].as_str().ok_or(Error::InvalidResponse)?;
    if result.trim().is_empty() || result.len() > 1024 || result.chars().any(char::is_control) {
        return Err(Error::InvalidResponse);
    }
    Ok(result.to_owned())
}
fn entry(value: &Value, kind: EntityKind) -> Result<CatalogEntry, Error> {
    let id = identity(kind, &text(value, "id")?)?;
    let title = text(
        value,
        if kind == EntityKind::Artist {
            "name"
        } else {
            "title"
        },
    )?;
    let artists = value["artist-credit"]
        .as_array()
        .map(|credits| {
            if credits.len() > 32 {
                return Err(Error::TooLarge);
            }
            credits
                .iter()
                .map(|credit| {
                    Ok(ArtistCredit {
                        id: identity(EntityKind::Artist, &text(&credit["artist"], "id")?)?,
                        name: text(credit, "name").or_else(|_| text(&credit["artist"], "name"))?,
                    })
                })
                .collect::<Result<Vec<_>, Error>>()
        })
        .transpose()?
        .unwrap_or_default();
    let duration_ms = if kind == EntityKind::Recording {
        value["length"].as_u64()
    } else {
        None
    };
    Ok(CatalogEntry {
        media: Media {
            id,
            title,
            duration_ms,
        },
        kind,
        artists,
    })
}

/// Provider response objects stop here; only neutral presentation leaves the adapter.
pub fn parse(request: &Request, bytes: &[u8]) -> Result<Page, Error> {
    parameters(request)?;
    if bytes.len() > MAX_BYTES {
        return Err(Error::TooLarge);
    }
    let root: Value = serde_json::from_slice(bytes).map_err(|_| Error::InvalidResponse)?;
    if let Request::ReleaseTracks { .. } = request {
        let media = root["media"].as_array().ok_or(Error::InvalidResponse)?;
        // A release may contain many discs. Keep a hard total limit and report
        // overflow rather than silently presenting a partial album as complete.
        let mut items = Vec::new();
        for disc in media {
            for track in disc["tracks"].as_array().ok_or(Error::InvalidResponse)? {
                if items.len() == 500 {
                    return Err(Error::TooLarge);
                }
                let mut item = entry(&track["recording"], EntityKind::Recording)?;
                if let Some(duration) = track["length"].as_u64() {
                    item.media.duration_ms = Some(duration);
                }
                items.push(item);
            }
        }
        return Ok(Page {
            items,
            offset: 0,
            more: false,
        });
    }
    let (kind, offset, count_key) = match request {
        Request::Search { kind, offset, .. } => (*kind, *offset, "count"),
        Request::ArtistRecordings { offset, .. } => {
            (EntityKind::Recording, *offset, "recording-count")
        }
        _ => unreachable!(),
    };
    let key = format!("{}s", endpoint(kind));
    let values = root[&key].as_array().ok_or(Error::InvalidResponse)?;
    if values.len() > PAGE_SIZE {
        return Err(Error::TooLarge);
    }
    let total = root[count_key].as_u64().ok_or(Error::InvalidResponse)?;
    let items = values
        .iter()
        .map(|value| entry(value, kind))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Page {
        more: !items.is_empty()
            && u64::from(offset) + (items.len() as u64) < total
            && offset < MAX_OFFSET,
        items,
        offset,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    const ID: &str = "def33242-688e-43ca-ada9-8f8d4f7b2e75";
    fn request() -> Request {
        Request::Search {
            query: "Café 東京".into(),
            kind: EntityKind::Recording,
            offset: 0,
        }
    }
    #[test]
    fn unicode_real_metadata_and_missing_duration_preserve_identity() {
        let bytes = format!(r#"{{"count":2,"recordings":[{{"id":"{ID}","title":"Café 東京"}}]}}"#);
        let page = parse(&request(), bytes.as_bytes()).unwrap();
        assert_eq!(page.items[0].media.title, "Café 東京");
        assert_eq!(page.items[0].media.id.provider.as_str(), "musicbrainz");
        assert_eq!(page.items[0].media.duration_ms, None);
        assert!(page.more);
        assert!(page.items[0].artists.is_empty());
        assert_eq!(
            parse(&request(), br#"{"count":0,"recordings":[]}"#)
                .unwrap()
                .items,
            []
        );
    }
    #[test]
    fn requests_and_responses_cannot_supply_paths_or_excess_results() {
        assert!(!uuid("../../cookie"));
        assert_eq!(
            escape_query("  Café + test:* ").unwrap(),
            "Café \\+ test\\:\\*"
        );
        assert_eq!(escape_query(""), Err(Error::InvalidRequest));
        assert_eq!(
            parse(
                &request(),
                br#"{"count":1,"recordings":[{"id":"bad","title":"bad"}]}"#
            ),
            Err(Error::InvalidResponse)
        );
        assert_eq!(parse(&request(), b"garbage"), Err(Error::InvalidResponse));
        let oversized = serde_json::json!({"count":21,"recordings":vec![serde_json::json!({"id":ID,"title":"test"});21]});
        assert_eq!(
            parse(&request(), &serde_json::to_vec(&oversized).unwrap()),
            Err(Error::TooLarge)
        );
        let bad = MediaId::new(
            ProviderId::new("musicbrainz").unwrap(),
            "artist:../../cookie",
        )
        .unwrap();
        assert_eq!(
            parameters(&Request::ArtistRecordings { id: bad, offset: 0 }),
            Err(Error::InvalidRequest)
        );
    }
    #[test]
    fn album_tracks_use_recording_identity_and_do_not_invent_credits() {
        let request = Request::ReleaseTracks {
            id: identity(EntityKind::Release, ID).unwrap(),
        };
        let bytes = format!(
            r#"{{"media":[{{"tracks":[{{"length":1234,"recording":{{"id":"{ID}","title":"Track"}}}}]}}]}}"#
        );
        let page = parse(&request, bytes.as_bytes()).unwrap();
        assert_eq!(
            page.items[0].media.id,
            identity(EntityKind::Recording, ID).unwrap()
        );
        assert_eq!(page.items[0].media.duration_ms, Some(1234));
        assert!(!page.more);
    }
}
