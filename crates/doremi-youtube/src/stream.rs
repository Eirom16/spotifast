//! Desktop yt-dlp resolution and bounded native HTTP range input.
//! The original native resolver is retained for explicit diagnostics and Android,
//! never as an automatic fallback after an extractor or media access refusal.
use crate::{Client, Error, PROVIDER, id};
use doremi_core::media::MediaId;
use reqwest::{Url, redirect::Policy};
use serde_json::{Value, json};
use std::{
    io::{self, Read, Seek, SeekFrom},
    time::{Duration, Instant},
};
const PLAYER: &str = "https://www.youtube.com/youtubei/v1/player?prettyPrint=false";
const UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 15_7_3) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/26.0 Safari/605.1.15";
const CHUNK: usize = 64 * 1024;
/// Cold public-player download plus account/player requests and bounded JS work.
/// Hosts must still cancel the future immediately on a superseding user intent.
pub const RESOLVE_TIMEOUT: Duration = Duration::from_secs(120);
fn playback_nonce() -> Result<String, Error> {
    const ALPHABET: &[u8; 64] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_";
    let mut bytes = [0; 16];
    getrandom::fill(&mut bytes).map_err(|_| Error::Unavailable)?;
    Ok(bytes
        .iter()
        .map(|b| ALPHABET[usize::from(b & 63)] as char)
        .collect())
}
/// Redacted protocol outcome, never a response body, URL or session value.
#[derive(Clone, Copy, Debug)]
pub struct PlayerDiagnostic {
    pub http_status: u16,
    pub api_code: Option<u16>,
    pub status: &'static str,
    pub reason: &'static str,
    pub aac_formats: usize,
    pub direct_aac: usize,
    pub ciphered_aac: usize,
    pub drm: bool,
}
impl PlayerDiagnostic {
    pub(crate) fn http(http_status: u16) -> Self {
        Self {
            http_status,
            api_code: None,
            status: "unread",
            reason: "none",
            aac_formats: 0,
            direct_aac: 0,
            ciphered_aac: 0,
            drm: false,
        }
    }
    pub(crate) fn response(http_status: u16, value: &Value) -> Self {
        let status = match value["playabilityStatus"]["status"].as_str() {
            Some("OK") => "ok",
            Some("UNPLAYABLE") => "unplayable",
            Some("ERROR") => "error",
            Some("LOGIN_REQUIRED") => "login_required",
            Some("AGE_CHECK_REQUIRED") => "age_check",
            Some("CONTENT_CHECK_REQUIRED") => "content_check",
            Some(_) => "other",
            None => "missing",
        };
        let text = value["playabilityStatus"]["reason"]
            .as_str()
            .unwrap_or_default();
        let text = text
            .get(..text.len().min(512))
            .unwrap_or_default()
            .to_ascii_lowercase();
        let reason = if text.contains("bot") {
            "bot_verification"
        } else if text.contains("sign in") || text.contains("sign-in") {
            "sign_in"
        } else if text.contains("age-restrict")
            || text.contains("verify your age")
            || text.contains("confirm your age")
        {
            "age"
        } else if text.contains("country") || text.contains("region") {
            "region"
        } else if text.contains("premium") || text.contains("member") {
            "membership"
        } else if text.contains("private") {
            "private"
        } else if text.contains("copyright") || text.contains("rights") {
            "rights"
        } else if text.contains("removed") || text.contains("deleted") {
            "removed"
        } else if text.contains("client") || text.contains("this app") || text.contains("browser") {
            "client"
        } else if text.contains("unavailable") || text.contains("not available") {
            "unavailable"
        } else if text.is_empty() {
            "none"
        } else {
            "other"
        };
        let mut result = Self {
            status,
            reason,
            ..Self::http(http_status)
        };
        result.api_code = value["error"]["code"]
            .as_u64()
            .filter(|code| (100..=599).contains(code))
            .map(|code| code as u16);
        result.drm = value["streamingData"]["licenseInfos"]
            .as_array()
            .is_some_and(|items| !items.is_empty());
        if let Some(formats) = value["streamingData"]["adaptiveFormats"].as_array() {
            for format in formats {
                if format["mimeType"].as_str().is_some_and(|mime| {
                    mime.starts_with("audio/mp4;") && mime.contains("mp4a.40.2")
                }) {
                    result.aac_formats += 1;
                    if format["url"].is_string() && format["signatureCipher"].is_null() {
                        result.direct_aac += 1;
                    }
                    if format["signatureCipher"].is_string() || format["cipher"].is_string() {
                        result.ciphered_aac += 1;
                    }
                }
            }
        }
        result
    }
}
/// Ephemeral, non-serializable grant. Its transport URL is never a media ID.
pub struct Stream {
    url: String,
    user_agent: String,
    length: u64,
    expires: Instant,
    pub duration_ms: u64,
}
impl Stream {
    pub fn length(&self) -> u64 {
        self.length
    }
    pub fn reader(
        self,
        cancelled: impl Fn() -> bool + Send + Sync + 'static,
    ) -> Result<Reader, Error> {
        let http = reqwest::blocking::Client::builder()
            .redirect(Policy::custom(|attempt| {
                // CDN routing is part of the same media grant. Never follow
                // consent pages, third-party hosts, downgrade or redirect loops.
                if attempt.previous().len() > 3 || media_url(attempt.url().as_str()).is_err() {
                    attempt.stop()
                } else {
                    attempt.follow()
                }
            }))
            .connect_timeout(Duration::from_secs(4))
            .timeout(Duration::from_secs(6))
            .user_agent(&self.user_agent)
            .build()
            .map_err(|_| Error::Transport)?;
        Ok(Reader {
            http,
            grant: self,
            position: 0,
            cached_at: 0,
            bytes: Vec::new(),
            cancelled: Box::new(cancelled),
        })
    }
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    pub(crate) fn extracted(
        url: String,
        length: u64,
        duration_ms: u64,
        user_agent: String,
    ) -> Result<Self, Error> {
        use std::time::{SystemTime, UNIX_EPOCH};
        let uri = media_url(&url)?;
        if !(1..=512 * 1024 * 1024).contains(&length)
            || !(1..=6 * 3600 * 1000).contains(&duration_ms)
        {
            return Err(Error::TooLarge);
        }
        if user_agent.is_empty()
            || user_agent.len() > 4096
            || reqwest::header::HeaderValue::from_str(&user_agent).is_err()
        {
            return Err(Error::InvalidResponse);
        }
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| Error::Unavailable)?
            .as_secs();
        let expires = uri
            .query_pairs()
            .find(|(key, _)| key == "expire")
            .and_then(|(_, value)| value.parse::<u64>().ok())
            .and_then(|expires| expires.checked_sub(now))
            .filter(|seconds| *seconds > 0)
            .ok_or(Error::AccessDenied)?;
        Ok(Self {
            url,
            user_agent,
            length,
            duration_ms,
            expires: Instant::now() + Duration::from_secs(expires.min(6 * 3600)),
        })
    }
}
impl Client {
    pub async fn resolve(&mut self, media: &MediaId) -> Result<Stream, Error> {
        #[cfg(any(target_os = "linux", target_os = "windows"))]
        {
            self.player_diagnostic = None;
            self.resolution_stage = "identity";
            if media.provider.as_str() != PROVIDER
                || media.local().len() != 11
                || id(media.local()).is_err()
            {
                return Err(Error::InvalidRequest);
            }
            if self.signed_in() {
                self.resolution_stage = "account";
                self.account().await?;
            }
            self.resolution_stage = "extractor";
            let stream = crate::extractor::resolve(media, self.session.as_ref()).await?;
            self.resolution_stage = "ready";
            Ok(stream)
        }
        #[cfg(not(any(target_os = "linux", target_os = "windows")))]
        self.resolve_native(media).await
    }
    /// Preserved native experiment. It is not retried after a desktop failure.
    pub async fn resolve_native(&mut self, media: &MediaId) -> Result<Stream, Error> {
        self.player_diagnostic = None;
        self.resolution_stage = "identity";
        if media.provider.as_str() != PROVIDER
            || media.local().len() != 11
            || id(media.local()).is_err()
        {
            return Err(Error::InvalidRequest);
        }
        let nonce = playback_nonce()?;
        if self.signed_in() {
            // VISIONOS is a guest client in the reference implementation.
            // Verify expiry, then use the account's Music client and identity.
            self.resolution_stage = "account";
            self.account().await?;
            self.resolution_stage = "bootstrap";
            let timestamp = self.signature_timestamp().await?;
            self.resolution_stage = "player";
            let value = self
                .post("player", json!({"videoId":media.local(),"cpn":nonce,"playbackContext":{"contentPlaybackContext":{"html5Preference":"HTML5_PREF_WANTS","signatureTimestamp":timestamp}}}))
                .await?;
            validate(media, &value)?;
            self.resolution_stage = "signature";
            let mut url = self.resolve_format_url(candidate(&value)?).await?;
            with_nonce(&mut url, &nonce);
            let mut stream = parse_with_url(media, &value, Some(url))?;
            stream.user_agent = crate::USER_AGENT.into();
            self.resolution_stage = "ready";
            return Ok(stream);
        }
        let body = json!({"context":{"client":{"clientName":"VISIONOS","clientVersion":"1.02","deviceMake":"Apple","deviceModel":"RealityDevice17,1","osName":"visionOS","osVersion":"26.5.23O471","hl":"en"}},"videoId":media.local(),"cpn":nonce});
        self.resolution_stage = "player";
        let request = self
            .http
            .post(PLAYER)
            .header("User-Agent", UA)
            .header("X-YouTube-Client-Name", "101")
            .header("X-YouTube-Client-Version", "1.02")
            .header("Origin", crate::ORIGIN)
            .json(&body);
        let request = self.authenticate(request)?;
        let mut response = request.send().await.map_err(crate::network_error)?;
        self.player_diagnostic = Some(PlayerDiagnostic::http(response.status().as_u16()));
        match response.status().as_u16() {
            200 => {}
            401 | 403 => {
                // A content/player refusal is not proof that the musical
                // account expired. Account verification owns that decision.
                return Err(Error::AccessDenied);
            }
            429 => {
                return Err(Error::RateLimited {
                    retry_after_ms: Some(30_000),
                });
            }
            _ => return Err(Error::Unavailable),
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(crate::network_error)? {
            if chunk.len() > crate::MAX_BYTES.saturating_sub(bytes.len()) {
                return Err(Error::TooLarge);
            }
            bytes.extend_from_slice(&chunk);
        }
        let value = serde_json::from_slice(&bytes).map_err(|_| Error::InvalidResponse)?;
        self.player_diagnostic = Some(PlayerDiagnostic::response(200, &value));
        let mut stream = parse(media, &value)?;
        let mut url = media_url(&stream.url)?;
        with_nonce(&mut url, &nonce);
        stream.url = url.into();
        self.resolution_stage = "ready";
        Ok(stream)
    }
}
fn with_nonce(url: &mut Url, nonce: &str) {
    let pairs: Vec<_> = url
        .query_pairs()
        .filter(|(key, _)| key != "cpn")
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect();
    url.query_pairs_mut()
        .clear()
        .extend_pairs(pairs)
        .append_pair("cpn", nonce);
}
fn range_url(value: &str, start: u64, end: u64) -> io::Result<Url> {
    let mut url = Url::parse(value).map_err(|_| io::ErrorKind::InvalidInput)?;
    // googlevideo uses a query range (Tubefast's download protocol), not an
    // HTTP Range header. Preserve the signed query byte-for-byte, replacing
    // only an existing range so a seek cannot reuse the resolver's old range.
    let query = url
        .query()
        .unwrap_or_default()
        .split('&')
        .filter(|pair| !pair.is_empty() && pair.split('=').next() != Some("range"))
        .collect::<Vec<_>>()
        .join("&");
    url.set_query(if query.is_empty() { None } else { Some(&query) });
    url.query_pairs_mut()
        .append_pair("range", &format!("{start}-{end}"));
    Ok(url)
}
fn parse(media: &MediaId, value: &Value) -> Result<Stream, Error> {
    parse_with_url(media, value, None)
}
fn validate(media: &MediaId, value: &Value) -> Result<(), Error> {
    match value["playabilityStatus"]["status"].as_str() {
        Some("OK") => {}
        Some("LOGIN_REQUIRED" | "AGE_CHECK_REQUIRED" | "CONTENT_CHECK_REQUIRED") => {
            return Err(Error::AccessDenied);
        }
        _ => return Err(Error::Unavailable),
    }
    if value["videoDetails"]["videoId"].as_str() != Some(media.local()) {
        return Err(Error::InvalidResponse);
    }
    if value["streamingData"]["licenseInfos"]
        .as_array()
        .is_some_and(|v| !v.is_empty())
    {
        return Err(Error::AccessDenied);
    }
    Ok(())
}
fn candidate(value: &Value) -> Result<&Value, Error> {
    let formats = value["streamingData"]["adaptiveFormats"]
        .as_array()
        .ok_or(Error::UnsupportedFormat)?;
    formats
        .iter()
        .filter(|f| {
            f["mimeType"]
                .as_str()
                .is_some_and(|mime| mime.starts_with("audio/mp4;") && mime.contains("mp4a.40.2"))
        })
        .filter(|f| {
            (f["url"].is_string() || f["signatureCipher"].is_string() || f["cipher"].is_string())
                && f["drmFamilies"].as_array().is_none_or(|v| v.is_empty())
        })
        .min_by_key(|f| !f["url"].is_string())
        .ok_or(Error::UnsupportedFormat)
}
pub(crate) fn media_url(value: &str) -> Result<Url, Error> {
    if value.len() > 32 * 1024 {
        return Err(Error::TooLarge);
    }
    let uri = Url::parse(value).map_err(|_| Error::InvalidResponse)?;
    let host = uri.host_str().ok_or(Error::InvalidResponse)?;
    if uri.scheme() != "https"
        || uri.port().is_some()
        || !uri.username().is_empty()
        || uri.password().is_some()
        || uri.fragment().is_some()
        || !(host == "googlevideo.com" || host.ends_with(".googlevideo.com"))
    {
        return Err(Error::AccessDenied);
    }
    Ok(uri)
}
fn parse_with_url(media: &MediaId, value: &Value, resolved: Option<Url>) -> Result<Stream, Error> {
    validate(media, value)?;
    let format = candidate(value)?;
    let uri = match resolved {
        Some(uri) => media_url(uri.as_str())?,
        None => {
            if !format["signatureCipher"].is_null() || !format["cipher"].is_null() {
                return Err(Error::UnsupportedFormat);
            }
            media_url(format["url"].as_str().ok_or(Error::UnsupportedFormat)?)?
        }
    };
    let length = format["contentLength"]
        .as_str()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|v| (1..=512 * 1024 * 1024).contains(v))
        .ok_or(Error::TooLarge)?;
    let duration_ms = value["videoDetails"]["lengthSeconds"]
        .as_str()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|v| (1..=6 * 3600).contains(v))
        .and_then(|v| v.checked_mul(1000))
        .ok_or(Error::InvalidResponse)?;
    let seconds = value["streamingData"]["expiresInSeconds"]
        .as_str()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(60)
        .clamp(1, 6 * 3600);
    Ok(Stream {
        url: uri.into(),
        user_agent: UA.into(),
        length,
        expires: Instant::now() + Duration::from_secs(seconds),
        duration_ms,
    })
}
/// Owned by the decoder worker; neither the UI nor audio callback reads HTTP.
pub struct Reader {
    http: reqwest::blocking::Client,
    grant: Stream,
    position: u64,
    cached_at: u64,
    bytes: Vec<u8>,
    cancelled: Box<dyn Fn() -> bool + Send + Sync>,
}
impl Reader {
    fn check(&self) -> io::Result<()> {
        if (self.cancelled)() {
            return Err(io::ErrorKind::ConnectionAborted.into());
        }
        if Instant::now() >= self.grant.expires {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        Ok(())
    }
    fn fill(&mut self) -> io::Result<()> {
        self.check()?;
        let start = self.position / CHUNK as u64 * CHUNK as u64;
        let end = (start + CHUNK as u64).min(self.grant.length) - 1;
        let mut response = self
            .http
            .get(range_url(&self.grant.url, start, end)?)
            .header("Accept-Encoding", "identity")
            .send()
            .map_err(|_| {
                range_event(0, "transport", 0, start);
                io::Error::other("YouTube audio transport failed")
            })?;
        self.check()?;
        let status = response.status().as_u16();
        match response.status().as_u16() {
            200 | 206 => {}
            401 | 403 => {
                range_event(status, "refused", 0, start);
                return Err(io::ErrorKind::PermissionDenied.into());
            }
            _ => {
                range_event(status, "range_status", 0, start);
                return Err(io::Error::other("YouTube audio range unavailable"));
            }
        }
        let expected = format!("bytes {start}-{end}/{}", self.grant.length);
        // Query-range replies can be 200 without Content-Range. When the
        // server supplies that header, it must still identify the exact bytes.
        // A 206 without it is not a valid partial-content response.
        let content_range = response.headers().get("content-range");
        if content_range.is_some_and(|value| value.to_str().ok() != Some(expected.as_str()))
            || (status == 206 && content_range.is_none())
        {
            range_event(status, "content_range", 0, start);
            return Err(io::Error::other("YouTube audio range changed"));
        }
        let size = (end - start + 1) as usize;
        self.bytes.clear();
        self.bytes.reserve_exact(size + 1);
        let result = (|| {
            // Never accept an ignored range as a complete-track download.
            // Read one extra byte to detect overflow, checking cancellation
            // between blocks as well as at the request's bounded deadline.
            let mut block = [0; 16 * 1024];
            while self.bytes.len() <= size {
                self.check()?;
                let limit = block.len().min(size + 1 - self.bytes.len());
                let read = response.read(&mut block[..limit]).map_err(|_| {
                    range_event(status, "read", self.bytes.len(), start);
                    io::Error::other("YouTube audio read failed")
                })?;
                self.check()?;
                if read == 0 {
                    break;
                }
                self.bytes.extend_from_slice(&block[..read]);
            }
            Ok(())
        })();
        if let Err(error) = result {
            // A failed fill must not leave partial bytes eligible as a cache
            // hit on the next read.
            self.bytes.clear();
            return Err(error);
        }
        if self.bytes.len() != size {
            range_event(status, "truncated", self.bytes.len(), start);
            self.bytes.clear();
            return Err(io::ErrorKind::ConnectionReset.into());
        }
        if start == 0 {
            range_event(
                status,
                if self.bytes.get(4..8) == Some(b"ftyp") {
                    "mp4"
                } else {
                    "other_container"
                },
                self.bytes.len(),
                start,
            );
        }
        self.cached_at = start;
        Ok(())
    }
}
fn range_event(http: u16, stage: &'static str, bytes: usize, offset: u64) {
    // Fixed categories and counts only. Never inspect/log URL query, response
    // headers, body text, player identifiers or credentials.
    #[cfg(target_os = "android")]
    log::info!("YTM range http={http} stage={stage} bytes={bytes} offset={offset}");
    #[cfg(not(target_os = "android"))]
    eprintln!("YTM range http={http} stage={stage} bytes={bytes} offset={offset}");
}
impl Read for Reader {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        self.check()?;
        if output.is_empty() || self.position == self.grant.length {
            return Ok(0);
        }
        if self.position < self.cached_at
            || self.position - self.cached_at >= self.bytes.len() as u64
        {
            self.fill()?;
        }
        let index = (self.position - self.cached_at) as usize;
        let size = output.len().min(self.bytes.len() - index);
        output[..size].copy_from_slice(&self.bytes[index..index + size]);
        self.position += size as u64;
        Ok(size)
    }
}
impl Seek for Reader {
    fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
        self.check()?;
        let position = match from {
            SeekFrom::Start(n) => i128::from(n),
            SeekFrom::Current(n) => i128::from(self.position) + i128::from(n),
            SeekFrom::End(n) => i128::from(self.grant.length) + i128::from(n),
        };
        if !(0..=i128::from(self.grant.length)).contains(&position) {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        self.position = position as u64;
        Ok(self.position)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn media_redirect_boundary_and_nonce_preserve_the_same_grant() {
        for value in [
            "http://rr.googlevideo.com/videoplayback",
            "https://googlevideo.com.evil.invalid/",
            "https://accounts.google.com/",
            "https://secret@rr.googlevideo.com/",
            "https://rr.googlevideo.com:8443/",
            "https://rr.googlevideo.com/#other",
        ] {
            assert!(media_url(value).is_err());
        }
        let mut url = media_url(
            "https://rr.googlevideo.com/videoplayback?sig=fixture%3D&n=fixtureN&cpn=old&cpn=older",
        )
        .unwrap();
        with_nonce(&mut url, "abcdefghijklmnop");
        let pairs: Vec<_> = url.query_pairs().collect();
        assert_eq!(pairs.iter().filter(|(key, _)| key == "cpn").count(), 1);
        assert!(
            pairs
                .iter()
                .any(|(key, value)| key == "sig" && value == "fixture=")
        );
        assert!(
            pairs
                .iter()
                .any(|(key, value)| key == "n" && value == "fixtureN")
        );
        assert!(
            pairs
                .iter()
                .any(|(key, value)| key == "cpn" && value == "abcdefghijklmnop")
        );
    }
    fn response() -> Value {
        json!({"playabilityStatus":{"status":"OK"},"videoDetails":{"videoId":"abcdefghijk","lengthSeconds":"180"},"streamingData":{"expiresInSeconds":"600","adaptiveFormats":[{"mimeType":"audio/mp4; codecs=\"mp4a.40.2\"","contentLength":"100000","url":"https://r1.googlevideo.com/videoplayback?expire=123&token=fixture"}]}})
    }
    #[test]
    fn player_diagnostics_cannot_capture_urls_tokens_or_unrecognized_provider_text() {
        let mut value = response();
        value["playabilityStatus"]["reason"] =
            "NEVER_PRINT_THIS_SECRET https://signed.invalid/?token=NEVER_PRINT_THIS_SECRET".into();
        let report = PlayerDiagnostic::response(200, &value);
        assert_eq!(report.status, "ok");
        assert_eq!(report.direct_aac, 1);
        let rendered = format!("{report:?}");
        assert!(!rendered.contains("NEVER_PRINT_THIS_SECRET"));
        assert!(!rendered.contains("signed.invalid") && !rendered.contains("googlevideo.com"));
        value["playabilityStatus"]["status"] = "UNPLAYABLE".into();
        value["playabilityStatus"]["reason"] = "Sign in to confirm you are not a bot".into();
        let report = PlayerDiagnostic::response(200, &value);
        assert_eq!(report.status, "unplayable");
        assert_eq!(report.reason, "bot_verification");
    }
    #[tokio::test]
    async fn authenticated_playback_uses_music_identity_and_never_falls_back_after_refusal() {
        use crate::protocol_tests::{account, server};
        for (denied, ciphered) in [(false, false), (true, false), (false, true)] {
            let mut player = response();
            player["responseContext"] = json!({});
            if ciphered {
                player["streamingData"]["adaptiveFormats"][0]["url"] = Value::Null;
                player["streamingData"]["adaptiveFormats"][0]["signatureCipher"] = "s=fixture&sp=sig&url=https%3A%2F%2Frr.googlevideo.com%2Fvideoplayback%3Fn%3Dabc".into();
            }
            let (base, server) = server(vec![
                (200, account()),
                (200, account()),
                (if denied { 403 } else { 200 }, player),
            ])
            .await;
            let mut client = Client::new().unwrap();
            client.base = base;
            client.player_config = Some(crate::player_config::PlayerConfig {
                timestamp: 20760,
                loaded: Instant::now(),
                code: "_result.sig=s=>s.split('').reverse().join('');_result.n=s=>s+'X';".into(),
                prepared: true,
            });
            let session = json!({"schema":1,"cookie":"SAPISID=fixture-only","visitor_data":"fixture-visitor","data_sync_id":"fixture-channel","auth_user":"1"});
            client.sign_in(session.to_string()).await.unwrap();
            let result = client.resolve_native(&id("abcdefghijk").unwrap()).await;
            let mut delivered_nonce = None;
            if denied {
                assert!(matches!(result, Err(Error::AccessDenied)));
            } else {
                let grant = result.unwrap();
                assert_eq!(grant.length(), 100000);
                assert_eq!(grant.user_agent, crate::USER_AGENT);
                delivered_nonce = Url::parse(&grant.url)
                    .unwrap()
                    .query_pairs()
                    .find(|(key, _)| key == "cpn")
                    .map(|(_, value)| value.into_owned());
                if ciphered {
                    let url = Url::parse(&grant.url).unwrap();
                    let pairs: std::collections::HashMap<_, _> = url.query_pairs().collect();
                    assert_eq!(pairs.get("sig").map(|s| s.as_ref()), Some("erutxif"));
                    assert_eq!(pairs.get("n").map(|s| s.as_ref()), Some("abcX"));
                }
            }
            assert!(
                client.signed_in(),
                "A content refusal is not session expiry"
            );
            let requests = server.await.unwrap();
            assert_eq!(requests.len(), 3);
            assert!(requests[2].starts_with("POST /player?"));
            let headers = requests[2]
                .split("\r\n\r\n")
                .next()
                .unwrap()
                .to_ascii_lowercase();
            assert!(headers.contains("x-youtube-client-name: 67"));
            assert!(headers.contains("x-goog-api-format-version: 1"));
            assert!(headers.contains("cookie: sapisid=fixture-only"));
            assert!(headers.contains("x-goog-authuser: 1"));
            assert!(headers.contains("x-goog-visitor-id: fixture-visitor"));
            let body: Value =
                serde_json::from_str(requests[2].split_once("\r\n\r\n").unwrap().1).unwrap();
            assert_eq!(body["context"]["client"]["clientName"], "WEB_REMIX");
            assert_eq!(body["context"]["user"]["onBehalfOfUser"], "fixture-channel");
            assert_eq!(body["videoId"], "abcdefghijk");
            let nonce = body["cpn"].as_str().unwrap();
            assert!(
                nonce.len() == 16
                    && nonce
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b))
            );
            assert!(denied || delivered_nonce.as_deref() == Some(nonce));
            assert_eq!(
                body["playbackContext"]["contentPlaybackContext"]["html5Preference"],
                "HTML5_PREF_WANTS"
            );
            assert_eq!(
                body["playbackContext"]["contentPlaybackContext"]["signatureTimestamp"],
                20760
            );
            assert!(body.get("racyCheckOk").is_none() && body.get("contentCheckOk").is_none());
        }
    }
    #[tokio::test]
    async fn expired_music_sessions_stop_before_any_player_request() {
        use crate::protocol_tests::{account, server};
        let (base, server) = server(vec![(200, account()), (403, json!({}))]).await;
        let mut client = Client::new().unwrap();
        client.base = base;
        client.sign_in("SAPISID=fixture-only".into()).await.unwrap();
        assert!(matches!(
            client.resolve_native(&id("abcdefghijk").unwrap()).await,
            Err(Error::AccessDenied)
        ));
        assert!(!client.signed_in());
        let requests = server.await.unwrap();
        assert_eq!(requests.len(), 2);
        assert!(
            requests
                .iter()
                .all(|request| request.starts_with("POST /account/account_menu?"))
        );
    }
    #[test]
    fn rejects_denials_drm_ciphers_and_wrong_media_identity() {
        let media = id("abcdefghijk").unwrap();
        assert_eq!(parse(&media, &response()).unwrap().length(), 100000);
        let mut value = response();
        value["playabilityStatus"]["status"] = "LOGIN_REQUIRED".into();
        assert!(matches!(parse(&media, &value), Err(Error::AccessDenied)));
        let mut value = response();
        value["streamingData"]["licenseInfos"] =
            json!([{"licenseServerUrl":"https://example.com"}]);
        assert!(matches!(parse(&media, &value), Err(Error::AccessDenied)));
        let mut value = response();
        value["streamingData"]["adaptiveFormats"][0]["signatureCipher"] = "encrypted".into();
        assert!(matches!(
            parse(&media, &value),
            Err(Error::UnsupportedFormat)
        ));
        assert!(matches!(
            parse(&id("other-track").unwrap(), &response()),
            Err(Error::InvalidResponse)
        ));
    }
}
#[cfg(test)]
mod range_tests {
    use super::*;
    use std::net::TcpListener;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    #[test]
    fn query_ranges_preserve_signed_octets_and_replace_previous_ranges() {
        let url = range_url(
            "https://rr.googlevideo.com/videoplayback?sig=fixture%2f%3D&n=a%20b&range=0-9&cpn=fixture&range=2-4",
            65536,
            99999,
        )
        .unwrap();
        assert_eq!(
            url.query(),
            Some("sig=fixture%2f%3D&n=a%20b&cpn=fixture&range=65536-99999")
        );
    }
    #[test]
    fn refusals_and_broken_ranges_fail_once_without_reading_a_full_file() {
        for (status, range, body, expected) in [
            (403, "", b"".as_slice(), io::ErrorKind::PermissionDenied),
            (200, "", b"".as_slice(), io::ErrorKind::ConnectionReset),
            (206, "", b"".as_slice(), io::ErrorKind::Other),
            (
                206,
                "bytes 1-2/100000",
                b"".as_slice(),
                io::ErrorKind::Other,
            ),
            (
                200,
                "bytes 1-2/100000",
                b"".as_slice(),
                io::ErrorKind::Other,
            ),
            (
                206,
                "bytes 0-65535/100000",
                b"x".as_slice(),
                io::ErrorKind::ConnectionReset,
            ),
        ] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let thread = std::thread::spawn(move || {
                let (mut socket, _) = listener.accept().unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                let mut request = Vec::new();
                let mut byte = [0];
                while !request.ends_with(b"\r\n\r\n") {
                    socket.read_exact(&mut byte).unwrap();
                    request.push(byte[0]);
                    assert!(request.len() < 8192);
                }
                use std::io::Write;
                write!(
                    socket,
                    "HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nConnection: close\r\n",
                    body.len()
                )
                .unwrap();
                if !range.is_empty() {
                    write!(socket, "Content-Range: {range}\r\n").unwrap();
                }
                write!(socket, "\r\n").unwrap();
                socket.write_all(body).unwrap();
            });
            let grant = Stream {
                url: format!("http://{address}/fixture"),
                user_agent: crate::USER_AGENT.into(),
                length: 100000,
                expires: Instant::now() + Duration::from_secs(10),
                duration_ms: 60000,
            };
            let mut reader = grant.reader(|| false).unwrap();
            assert_eq!(reader.read(&mut [0; 1]).unwrap_err().kind(), expected);
            assert!(reader.bytes.is_empty());
            thread.join().unwrap();
        }
    }
    #[test]
    fn ignored_ranges_and_partial_read_errors_never_publish_cached_bytes() {
        for (sent, declared, expected) in [
            (CHUNK + 1, CHUNK + 1, io::ErrorKind::ConnectionReset),
            (CHUNK - 1, CHUNK, io::ErrorKind::Other),
        ] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let thread = std::thread::spawn(move || {
                use std::io::Write;
                let (mut socket, _) = listener.accept().unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                let mut request = Vec::new();
                let mut byte = [0];
                while !request.ends_with(b"\r\n\r\n") {
                    socket.read_exact(&mut byte).unwrap();
                    request.push(byte[0]);
                    assert!(request.len() < 8192);
                }
                write!(
                    socket,
                    "HTTP/1.1 200 OK\r\nContent-Length: {declared}\r\nConnection: close\r\n\r\n"
                )
                .unwrap();
                socket.write_all(&vec![1; sent]).unwrap();
            });
            let grant = Stream {
                url: format!("http://{address}/fixture"),
                user_agent: crate::USER_AGENT.into(),
                length: 100000,
                expires: Instant::now() + Duration::from_secs(10),
                duration_ms: 60000,
            };
            let mut reader = grant.reader(|| false).unwrap();
            assert_eq!(reader.read(&mut [0; 1]).unwrap_err().kind(), expected);
            assert_eq!(reader.position, 0);
            assert!(reader.bytes.is_empty());
            assert_eq!(reader.bytes.capacity(), CHUNK + 1);
            thread.join().unwrap();
        }
    }
    #[test]
    fn ranges_start_playback_without_downloading_the_whole_track_and_seek_is_cancelable() {
        for status in [200, 206] {
            query_range_round_trip(status);
        }
    }
    fn query_range_round_trip(status: u16) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let length = 4 * CHUNK;
        let thread = std::thread::spawn(move || {
            let mut transferred = 0;
            for (start, end) in [(0, CHUNK - 1), (3 * CHUNK, length - 1)] {
                let (mut socket, _) = listener.accept().unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                let mut request = Vec::new();
                let mut byte = [0];
                while !request.ends_with(b"\r\n\r\n") {
                    socket.read_exact(&mut byte).unwrap();
                    request.push(byte[0]);
                    assert!(request.len() < 8192);
                }
                let headers = String::from_utf8(request).unwrap().to_ascii_lowercase();
                assert!(
                    headers.starts_with(&format!("get /fixture?range={start}-{end} http/1.1\r\n"))
                );
                assert!(!headers.contains("\r\nrange:"));
                assert!(headers.contains(&format!(
                    "user-agent: {}",
                    crate::USER_AGENT.to_ascii_lowercase()
                )));
                assert!(!headers.contains("cookie:") && !headers.contains("authorization:"));
                use std::io::Write;
                write!(
                    socket,
                    "HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nConnection: close\r\n",
                    end - start + 1
                )
                .unwrap();
                if status == 206 {
                    write!(socket, "Content-Range: bytes {start}-{end}/{length}\r\n").unwrap();
                }
                write!(socket, "\r\n").unwrap();
                let data: Vec<_> = (start..=end).map(|i| (i % 251) as u8).collect();
                socket.write_all(&data).unwrap();
                transferred += data.len();
            }
            transferred
        });
        let cancel = Arc::new(AtomicBool::new(false));
        let signal = cancel.clone();
        let grant = Stream {
            url: format!("http://{address}/fixture"),
            user_agent: crate::USER_AGENT.into(),
            length: length as u64,
            expires: Instant::now() + Duration::from_secs(10),
            duration_ms: 60_000,
        };
        let mut reader = grant
            .reader(move || signal.load(Ordering::Acquire))
            .unwrap();
        let mut first = [0; 17];
        reader.read_exact(&mut first).unwrap();
        assert_eq!(first, std::array::from_fn(|i| i as u8));
        reader.seek(SeekFrom::End(-20)).unwrap();
        let mut last = [0; 20];
        reader.read_exact(&mut last).unwrap();
        assert_eq!(
            last,
            std::array::from_fn(|i| ((length - 20 + i) % 251) as u8)
        );
        assert!(thread.join().unwrap() < length);
        cancel.store(true, Ordering::Release);
        assert_eq!(
            reader.seek(SeekFrom::Start(0)).unwrap_err().kind(),
            io::ErrorKind::ConnectionAborted
        );
    }
}
