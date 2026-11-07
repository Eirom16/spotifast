//! Native YouTube Music catalog and session verification, shared by all hosts.
//! Renderer parsing and SAPISID signing adapted from Tubefast, MIT, revision
//! 928ebbb52b57c227a566a9680b3eccd208a14664. See LICENSE-TUBEFAST.
//! No password handling, alternate-client retries or title-based resolution.
#[cfg(any(target_os = "linux", target_os = "windows"))]
pub mod browser;
#[cfg(any(target_os = "linux", target_os = "windows"))]
pub mod credentials;
pub mod parser;
pub mod stream;
#[cfg(any(target_os = "linux", target_os = "windows"))]
pub mod embedded;
use doremi_core::{
    catalog::ProviderError as Error,
    media::{ArtistCredit, CatalogEntry, EntityKind, Media, MediaId, ProviderId},
};
use reqwest::{Client as Http, redirect::Policy};
use serde_json::{Value, json};
use sha1::{Digest, Sha1};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const ORIGIN: &str = "https://music.youtube.com";
const VERSION: &str = "1.20260707.12.00";
const MAX_BYTES: usize = 4 * 1024 * 1024;
pub const PROVIDER: &str = "youtube-music";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    Search { query: String, kind: EntityKind },
    Browse { id: String },
    Continue { token: String, search: bool },
    Home,
    Library,
    LikedSongs,
    SavedAlbums,
    SavedArtists,
}
impl Request {
    fn body(&self) -> Result<(&'static str, Value), Error> {
        Ok(match self {
            Self::Search { query, kind } => {
                if query.trim().is_empty() || query.len() > 512 {
                    return Err(Error::InvalidRequest);
                }
                let params = match kind {
                    EntityKind::Playlist => "EgWKAQIoAWoKEAkQChAFEAMQBA%3D%3D",
                    EntityKind::Recording => "EgWKAQIIAWoKEAkQBRAKEAMQBA%3D%3D",
                    EntityKind::Artist => "EgWKAQIgAWoKEAkQChAFEAMQBA%3D%3D",
                    EntityKind::Release => "EgWKAQIYAWoKEAkQChAFEAMQBA%3D%3D",
                };
                ("search", json!({"query": query, "params": params}))
            }
            Self::Browse { id } => {
                if id.is_empty()
                    || id.len() > 256
                    || !id
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c))
                {
                    return Err(Error::InvalidRequest);
                }
                ("browse", json!({"browseId": id}))
            }
            Self::Continue { token, search } => {
                if token.is_empty()
                    || token.len() > 16 * 1024
                    || token.chars().any(char::is_control)
                {
                    return Err(Error::InvalidRequest);
                }
                (
                    if *search { "search" } else { "browse" },
                    json!({"continuation":token}),
                )
            }
            Self::Home => ("browse", json!({"browseId":"FEmusic_home"})),
            Self::Library => ("browse", json!({"browseId":"FEmusic_liked_playlists"})),
            Self::LikedSongs => ("browse", json!({"browseId":"VLLM"})),
            Self::SavedAlbums => ("browse", json!({"browseId":"FEmusic_liked_albums"})),
            Self::SavedArtists => (
                "browse",
                json!({"browseId":"FEmusic_library_corpus_track_artists"}),
            ),
        })
    }
    fn personal(&self) -> bool {
        matches!(
            self,
            Self::Library | Self::LikedSongs | Self::SavedAlbums | Self::SavedArtists
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Account {
    pub name: String,
}

pub struct Client {
    http: Http,
    // Deliberately no Debug/Serialize: this never enters catalog snapshots/logs.
    session: Option<String>,
    base: String,
    thumbnails: std::collections::VecDeque<(MediaId, String)>,
}
impl Client {
    pub fn new() -> Result<Self, Error> {
        let http = Http::builder()
            .user_agent("Mozilla/5.0 Doremi/0.1.0")
            .connect_timeout(Duration::from_secs(4))
            .timeout(Duration::from_secs(15))
            .redirect(Policy::none())
            .build()
            .map_err(network_error)?;
        Ok(Self {
            http,
            session: None,
            base: format!("{ORIGIN}/youtubei/v1/"),
            thumbnails: std::collections::VecDeque::new(),
        })
    }
    pub fn clear_session(&mut self) {
        self.session = None;
    }
    pub fn signed_in(&self) -> bool {
        self.session.is_some()
    }
    pub async fn sign_in(&mut self, cookie: String) -> Result<Account, Error> {
        if authorization(&cookie, 0).is_none() {
            return Err(Error::InvalidRequest);
        }
        self.session = Some(cookie);
        match self.account().await {
            Ok(account) => Ok(account),
            Err(error) => {
                self.clear_session();
                Err(error)
            }
        }
    }
    pub async fn account(&mut self) -> Result<Account, Error> {
        if self.session.is_none() {
            return Err(Error::AccessDenied);
        }
        let response = self.post("account/account_menu", json!({})).await?;
        let name = find(&response, "activeAccountHeaderRenderer")
            .map(|header| text(&header["accountName"]))
            .filter(|name| !name.is_empty());
        match name {
            Some(name) => Ok(Account { name }),
            None => {
                self.clear_session();
                Err(Error::AccessDenied)
            }
        }
    }
    pub async fn fetch(&mut self, request: &Request) -> Result<parser::Page, Error> {
        if request.personal() {
            // An empty public response must never masquerade as a private library.
            self.account().await?;
        }
        let (endpoint, body) = request.body()?;
        let response = self.post(endpoint, body).await?;
        let page = parser::parse_page(&response);
        for item in page.sections.iter().flat_map(|section| &section.items) {
            if let Ok(id) = id(item.key()) {
                self.remember_thumbnail(id, &item.thumb);
            }
        }
        if let (Request::Browse { id: local }, Some(header)) = (request, &page.header)
            && let Ok(id) = id(local)
        {
            self.remember_thumbnail(id, &header.thumb);
        }
        Ok(page)
    }
    fn remember_thumbnail(&mut self, id: MediaId, url: &str) {
        if url.is_empty() {
            return;
        }
        self.thumbnails.retain(|(known, _)| known != &id);
        if self.thumbnails.len() == 256 {
            self.thumbnails.pop_front();
        }
        self.thumbnails.push_back((id, url.into()));
    }
    pub fn thumbnail(&self, id: &MediaId) -> Option<String> {
        self.thumbnails
            .iter()
            .find(|(known, _)| known == id)
            .map(|(_, url)| url.clone())
    }
    async fn post(&mut self, endpoint: &str, mut body: Value) -> Result<Value, Error> {
        body["context"] =
            json!({"client":{"clientName":"WEB_REMIX","clientVersion":VERSION,"hl":"en"}});
        let continuation = body["continuation"].as_str().map(str::to_owned);
        let mut request = self
            .http
            .post(format!("{}{endpoint}?prettyPrint=false", self.base))
            .header("Origin", ORIGIN)
            .header("X-YouTube-Client-Name", "67")
            .header("X-YouTube-Client-Version", VERSION)
            .json(&body);
        if let Some(token) = continuation {
            request = request.query(&[
                ("ctoken", token.as_str()),
                ("continuation", token.as_str()),
                ("type", "next"),
            ]);
        }
        if let Some(cookie) = &self.session {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|_| Error::Unavailable)?
                .as_secs();
            request = request
                .header("Cookie", cookie)
                .header(
                    "Authorization",
                    authorization(cookie, now).ok_or(Error::InvalidRequest)?,
                )
                .header("X-Origin", ORIGIN)
                .header("X-Goog-AuthUser", "0");
        }
        let mut response = request.send().await.map_err(network_error)?;
        match response.status().as_u16() {
            200 => {}
            401 | 403 => {
                self.clear_session();
                return Err(Error::AccessDenied);
            }
            429 => {
                return Err(Error::RateLimited {
                    retry_after_ms: Some(30_000),
                });
            }
            _ => return Err(Error::Unavailable),
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
        let value: Value = serde_json::from_slice(&bytes).map_err(|_| Error::InvalidResponse)?;
        if value.get("error").is_some() {
            return Err(Error::InvalidResponse);
        }
        // Consent/login HTML or an unrelated JSON response is not an empty catalog.
        if !value.is_object() || value.get("responseContext").is_none() {
            return Err(Error::InvalidResponse);
        }
        Ok(value)
    }
}
fn network_error(error: reqwest::Error) -> Error {
    if error.is_timeout() {
        Error::Timeout
    } else {
        Error::Transport
    }
}
pub fn authorization(cookie: &str, seconds: u64) -> Option<String> {
    let value = |name| {
        cookie
            .split(';')
            .map(str::trim)
            .find_map(|pair| pair.strip_prefix(name))
    };
    let secret = value("__Secure-3PAPISID=").or_else(|| value("SAPISID="))?;
    if secret.is_empty() || cookie.contains(['\r', '\n']) {
        return None;
    }
    let digest = Sha1::digest(format!("{seconds} {secret} {ORIGIN}").as_bytes());
    Some(format!("SAPISIDHASH {seconds}_{digest:x}"))
}
fn find<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    match value {
        Value::Object(map) => map
            .get(key)
            .or_else(|| map.values().find_map(|v| find(v, key))),
        Value::Array(list) => list.iter().find_map(|v| find(v, key)),
        _ => None,
    }
}
fn text(node: &Value) -> String {
    node["simpleText"]
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| {
            node["runs"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|run| run["text"].as_str())
                .collect()
        })
}
pub fn id(local: &str) -> Result<MediaId, Error> {
    if local.is_empty()
        || local.len() > 256
        || !local
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c))
    {
        return Err(Error::InvalidRequest);
    }
    MediaId::new(ProviderId::new(PROVIDER).unwrap(), local).map_err(|_| Error::InvalidRequest)
}
pub fn entry(item: &parser::Item) -> Option<CatalogEntry> {
    Some(CatalogEntry {
        media: Media {
            id: id(item.key()).ok()?,
            title: item.title.clone(),
            duration_ms: duration(&item.duration),
        },
        kind: if item.is_song() {
            EntityKind::Recording
        } else if item.is_artist {
            EntityKind::Artist
        } else if item.browse_id.starts_with("MPRE") {
            EntityKind::Release
        } else {
            EntityKind::Playlist
        },
        artists: item
            .artists
            .iter()
            .filter_map(|(name, local)| {
                Some(ArtistCredit {
                    id: id(local).ok()?,
                    name: name.clone(),
                })
            })
            .collect(),
        edition: None,
    })
}
fn duration(value: &str) -> Option<u64> {
    if value.is_empty() {
        return None;
    }
    value
        .split(':')
        .try_fold(0_u64, |acc, part| {
            acc.checked_mul(60)?.checked_add(part.parse::<u64>().ok()?)
        })?
        .checked_mul(1000)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn signatures_match_tubefast() {
        assert_eq!(
            authorization(
                "SID=a; SAPISID=first; __Secure-3PAPISID=abc/123",
                1_700_000_000
            )
            .as_deref(),
            Some("SAPISIDHASH 1700000000_149e3c5c81746e107a88a2e0103de8a4b63040a3")
        );
        assert!(authorization("SID=anon", 0).is_none());
        assert!(authorization("SAPISID=\r\nsecret", 0).is_none());
    }
    #[test]
    fn stable_provider_identity_and_duration() {
        let item = parser::Item {
            video_id: "abcdefghijk".into(),
            title: "Title".into(),
            duration: "1:02:03".into(),
            ..Default::default()
        };
        let entry = entry(&item).unwrap();
        assert_eq!(entry.media.id.provider.as_str(), PROVIDER);
        assert_eq!(entry.media.id.local(), "abcdefghijk");
        assert_eq!(entry.media.duration_ms, Some(3_723_000));
        assert!(id("https://example.com/stream?token=secret").is_err());
    }
}

/// The provider's stable browse/video identities retain their kind on restart.
pub fn entity_kind(media: &MediaId) -> Option<EntityKind> {
    if media.provider.as_str() != PROVIDER || id(media.local()).is_err() {
        return None;
    }
    let local = media.local();
    if local.len() == 11 {
        Some(EntityKind::Recording)
    } else if local.starts_with("UC") {
        Some(EntityKind::Artist)
    } else if local.starts_with("MPRE") {
        Some(EntityKind::Release)
    } else if local.starts_with("VL") || local.starts_with("PL") || local.starts_with("RD") {
        Some(EntityKind::Playlist)
    } else {
        None
    }
}

/// Artwork is fetched without session headers, from the provider image CDN only.
pub async fn artwork(url: &str) -> Result<Vec<u8>, Error> {
    let uri = reqwest::Url::parse(url).map_err(|_| Error::InvalidRequest)?;
    let host = uri.host_str().ok_or(Error::InvalidRequest)?;
    if uri.scheme() != "https"
        || uri.port().is_some()
        || !uri.username().is_empty()
        || uri.password().is_some()
        || !["ytimg.com", "googleusercontent.com", "ggpht.com"]
            .iter()
            .any(|domain| host == *domain || host.ends_with(&format!(".{domain}")))
    {
        return Err(Error::AccessDenied);
    }
    let client = Http::builder()
        .timeout(Duration::from_secs(8))
        .redirect(Policy::none())
        .build()
        .map_err(network_error)?;
    let mut response = client.get(uri).send().await.map_err(network_error)?;
    if response.status().as_u16() != 200 {
        return Err(Error::Unavailable);
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(network_error)? {
        if chunk.len() > (512_usize * 1024).saturating_sub(bytes.len()) {
            return Err(Error::TooLarge);
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

#[cfg(test)]
mod protocol_tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    async fn server(
        responses: Vec<(u16, Value)>,
    ) -> (String, tokio::task::JoinHandle<Vec<String>>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}/", listener.local_addr().unwrap());
        let task = tokio::spawn(async move {
            let mut requests = Vec::new();
            for (status, body) in responses {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut bytes = Vec::new();
                let mut buffer = [0; 4096];
                loop {
                    let n = stream.read(&mut buffer).await.unwrap();
                    assert!(n > 0);
                    bytes.extend_from_slice(&buffer[..n]);
                    if let Some(head) = bytes.windows(4).position(|s| s == b"\r\n\r\n") {
                        let headers = String::from_utf8_lossy(&bytes[..head]);
                        let size = headers
                            .lines()
                            .find_map(|line| {
                                line.to_ascii_lowercase()
                                    .strip_prefix("content-length:")
                                    .and_then(|v| v.trim().parse::<usize>().ok())
                            })
                            .unwrap_or(0);
                        if bytes.len() >= head + 4 + size {
                            break;
                        }
                    }
                    assert!(bytes.len() < 65536);
                }
                requests.push(String::from_utf8(bytes).unwrap());
                let body = body.to_string();
                stream.write_all(format!("HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
            }
            requests
        });
        (base, task)
    }
    fn account() -> Value {
        json!({"responseContext":{},"actions":[{"activeAccountHeaderRenderer":{"accountName":{"runs":[{"text":"Fixture account"}]}}}]})
    }
    #[tokio::test]
    async fn account_and_personal_library_use_the_same_verified_session() {
        let (base, task) = server(vec![
            (200, account()),
            (200, account()),
            (200, json!({"responseContext":{},"contents":{}})),
        ])
        .await;
        let mut client = Client::new().unwrap();
        client.base = base;
        assert_eq!(
            client
                .sign_in("SAPISID=fixture-only".into())
                .await
                .unwrap()
                .name,
            "Fixture account"
        );
        client.fetch(&Request::Library).await.unwrap();
        let requests = task.await.unwrap();
        assert_eq!(requests.len(), 3);
        for request in &requests {
            let lower = request.to_ascii_lowercase();
            assert!(lower.contains("authorization: sapisidhash "));
            assert!(lower.contains("cookie: sapisid=fixture-only"));
        }
        assert!(requests[2].contains("FEmusic_liked_playlists"));
        client.clear_session();
        assert!(!client.signed_in());
        assert_eq!(
            client.fetch(&Request::Library).await.err(),
            Some(Error::AccessDenied)
        );
    }
    #[tokio::test]
    async fn identity_without_a_music_account_is_rejected() {
        let (base, task) = server(vec![(
            200,
            json!({"responseContext":{},"accountName":"Generic Google identity"}),
        )])
        .await;
        let mut client = Client::new().unwrap();
        client.base = base;
        assert_eq!(
            client.sign_in("SAPISID=fixture-only".into()).await.err(),
            Some(Error::AccessDenied)
        );
        assert!(!client.signed_in());
        task.await.unwrap();
    }
    #[tokio::test]
    async fn expired_session_is_cleared_without_guest_retry() {
        let (base, task) =
            server(vec![(200, account()), (403, json!({"responseContext":{}}))]).await;
        let mut client = Client::new().unwrap();
        client.base = base;
        client.sign_in("SAPISID=fixture-only".into()).await.unwrap();
        assert_eq!(
            client.fetch(&Request::Home).await.err(),
            Some(Error::AccessDenied)
        );
        assert!(!client.signed_in());
        assert_eq!(task.await.unwrap().len(), 2);
    }
    #[test]
    fn parses_song_album_artist_playlist_and_artwork_without_id_substitution() {
        let card = |id: &str| json!({"musicTwoRowItemRenderer":{"title":{"runs":[{"text":id}]},"navigationEndpoint":{"browseEndpoint":{"browseId":id}},"thumbnailRenderer":{"musicThumbnailRenderer":{"thumbnail":{"thumbnails":[{"url":"https://lh3.googleusercontent.com/fixture","width":700}]}}}}});
        let value = json!({"contents":[{"musicCarouselShelfRenderer":{"contents":[card("UCfixture"),card("MPREb_fixture"),card("VLPLfixture0001")]}}]});
        let page = parser::parse_page(&value);
        let items = &page.sections[0].items;
        assert_eq!(items.len(), 3);
        assert_eq!(entry(&items[0]).unwrap().kind, EntityKind::Artist);
        assert_eq!(entry(&items[1]).unwrap().kind, EntityKind::Release);
        assert_eq!(entry(&items[2]).unwrap().kind, EntityKind::Playlist);
        assert!(items.iter().all(|item| !item.thumb.is_empty()));
        for item in items {
            let entry = entry(item).unwrap();
            assert_eq!(entity_kind(&entry.media.id), Some(entry.kind));
        }
    }
}

/// A pasted session starts from the user's own clipboard copy of their
/// music.youtube.com request headers (Tubefast's documented fallback path).
/// It accepts the raw cookie body, a `Cookie:` header line, or a curl command
/// that contains one. Authorization is always recomputed; nothing is granted
/// by markers inside the pasted text.
pub fn paste_cookie(input: &str) -> Result<String, Error> {
    let at = input.to_ascii_lowercase().find("cookie:");
    let header = at.map(|at| input[at + 7..].lines().next().unwrap_or_default());
    let raw = header
        .map_or_else(|| input.replace(['\r', '\n'], ""), str::to_owned)
        .trim()
        .trim_end_matches(['\\', ' '])
        .trim_matches(['\'', '"'])
        .to_owned();
    if raw.len() > 64 * 1024 || raw.contains(['\r', '\n']) {
        return Err(Error::InvalidRequest);
    }
    let cookie = raw
        .split(';')
        .map(str::trim)
        .filter(|pair| !pair.is_empty() && pair.contains('='))
        .collect::<Vec<_>>()
        .join("; ");
    if authorization(&cookie, 0).is_some() {
        Ok(cookie)
    } else if cookie.is_empty() {
        Err(Error::InvalidResponse)
    } else {
        Err(Error::InvalidRequest)
    }
}

#[cfg(test)]
mod paste_tests {
    use super::*;
    #[test]
    fn accepts_a_raw_cookie_a_header_line_or_a_curl_command() {
        let raw = "SID=a;  SAPISID=k ; junk; LOGIN_INFO=x";
        assert_eq!(paste_cookie(raw).unwrap(), "SID=a; SAPISID=k; LOGIN_INFO=x");
        assert_eq!(
            paste_cookie("accept: */*\r\nCookie: SID=a; SAPISID=k\r\nuser-agent: x").unwrap(),
            "SID=a; SAPISID=k"
        );
        assert_eq!(
            paste_cookie("curl 'https://x' \\\n  -H 'cookie: SID=a; SAPISID=k' \\\n  -H 'x: y'")
                .unwrap(),
            "SID=a; SAPISID=k"
        );
        assert_eq!(paste_cookie("SID=a; HSID=b"), Err(Error::InvalidRequest));
    }
}

/// Compatibility with headers saved by Doremi-Rust. Authorization is always
/// recomputed from the cookie; its legacy `SAPISIDHASH 1` marker is not a grant.
/// This parser performs no IO and never discovers/imports another app's session.
pub fn legacy_cookie(bytes: &[u8]) -> Result<String, Error> {
    if bytes.len() > 64 * 1024 {
        return Err(Error::TooLarge);
    }
    let value: Value = serde_json::from_slice(bytes).map_err(|_| Error::InvalidResponse)?;
    let user = value
        .get("x-goog-authuser")
        .and_then(Value::as_str)
        .unwrap_or("0");
    if user != "0" {
        return Err(Error::Unsupported);
    }
    let raw = value
        .get("cookie")
        .or_else(|| value.get("Cookie"))
        .and_then(Value::as_str)
        .ok_or(Error::InvalidResponse)?;
    if raw.contains(['\r', '\n']) {
        return Err(Error::InvalidRequest);
    }
    let cookie = raw
        .split(';')
        .map(str::trim)
        .filter(|pair| {
            pair.split_once('=').is_some_and(|(name, _)| {
                matches!(
                    name,
                    "SAPISID"
                        | "__Secure-3PAPISID"
                        | "__Secure-1PAPISID"
                        | "SID"
                        | "HSID"
                        | "SSID"
                        | "APISID"
                        | "__Secure-1PSID"
                        | "__Secure-3PSID"
                        | "LOGIN_INFO"
                        | "SIDCC"
                        | "__Secure-1PSIDTS"
                        | "__Secure-3PSIDTS"
                )
            })
        })
        .collect::<Vec<_>>()
        .join("; ");
    authorization(&cookie, 0).ok_or(Error::InvalidRequest)?;
    Ok(cookie)
}
#[cfg(test)]
mod legacy_tests {
    use super::*;
    #[test]
    fn imports_cookie_headers_without_reusing_the_legacy_authorization_marker() {
        let cookie=legacy_cookie(br#"{"cookie":"SAPISID=fixture; LOGIN_INFO=fixture; unrelated=discard","authorization":"SAPISIDHASH 1","x-goog-authuser":"0"}"#).unwrap();
        assert_eq!(cookie, "SAPISID=fixture; LOGIN_INFO=fixture");
        assert!(!authorization(&cookie, 123).unwrap().ends_with("HASH 1"));
        assert_eq!(
            legacy_cookie(br#"{"cookie":"SID=anonymous"}"#),
            Err(Error::InvalidRequest)
        );
        assert_eq!(
            legacy_cookie(br#"{"cookie":"SAPISID=fixture","x-goog-authuser":"1"}"#),
            Err(Error::Unsupported)
        );
    }
}
