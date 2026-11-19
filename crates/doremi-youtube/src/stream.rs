//! Desktop yt-dlp resolution and bounded native HTTP range input.
//! The original native resolver is retained for explicit diagnostics and Android,
//! never as an automatic fallback after an extractor or media access refusal.
use crate::latency::{Stage, Trace};
use crate::{Client, Error, PROVIDER, id};
use doremi_core::media::MediaId;
use reqwest::{Url, redirect::Policy};
use serde_json::{Value, json};
use std::{
    io::{self, Read, Seek, SeekFrom},
    time::{Duration, Instant},
};
mod read_ahead;
pub use read_ahead::ReadAhead;
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
    pub fn valid_for(&self, margin: Duration) -> bool {
        Instant::now()
            .checked_add(margin)
            .is_some_and(|at| at < self.expires)
    }
    pub fn reader(
        self,
        cancelled: impl Fn() -> bool + Send + Sync + 'static,
    ) -> Result<Reader, Error> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|_| Error::Transport)?;
        let http = reqwest::Client::builder()
            .no_gzip()
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
            runtime,
            grant: self,
            position: 0,
            cached_at: 0,
            bytes: Vec::new(),
            cancelled: Box::new(cancelled),
            trace: None,
            sequence: 0,
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
        self.resolve_traced(media, &Trace::default()).await
    }
    pub async fn resolve_traced(
        &mut self,
        media: &MediaId,
        trace: &Trace,
    ) -> Result<Stream, Error> {
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
            trace.mark(Stage::Extraction);
            let stream = crate::extractor::resolve(media, self.session.as_deref()).await?;
            trace.mark(Stage::Extracted);
            trace.mark(Stage::Verified);
            self.resolution_stage = "ready";
            Ok(stream)
        }
        #[cfg(not(any(target_os = "linux", target_os = "windows")))]
        {
            trace.mark(Stage::Extraction);
            let stream = self.resolve_native(media).await?;
            trace.mark(Stage::Extracted);
            trace.mark(Stage::Verified);
            Ok(stream)
        }
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
fn resource_url(value: &str) -> io::Result<Url> {
    let mut url = Url::parse(value).map_err(|_| io::ErrorKind::InvalidInput)?;
    // Strip a resolver-supplied query range without re-encoding signed octets.
    // A standard Range header is the only interval selector. Never fall back
    // to query ranges after a refusal or an ignored Range.
    let query = url
        .query()
        .unwrap_or_default()
        .split('&')
        .filter(|pair| !pair.is_empty() && pair.split('=').next() != Some("range"))
        .collect::<Vec<_>>()
        .join("&");
    url.set_query(if query.is_empty() { None } else { Some(&query) });
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
    http: reqwest::Client,
    runtime: tokio::runtime::Runtime,
    grant: Stream,
    position: u64,
    cached_at: u64,
    bytes: Vec<u8>,
    cancelled: Box<dyn Fn() -> bool + Send + Sync>,
    trace: Option<std::sync::Arc<Trace>>,
    sequence: u64,
}
impl Reader {
    pub fn with_trace(mut self, trace: std::sync::Arc<Trace>) -> Self {
        self.trace = Some(trace);
        self
    }
    fn check(&self) -> io::Result<()> {
        if (self.cancelled)() {
            return Err(io::ErrorKind::ConnectionAborted.into());
        }
        if Instant::now() >= self.grant.expires {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        Ok(())
    }
    // Retain each future while polling the cancellation fence. Dropping it
    // closes an active body/request; cancellation need not await the 6 s IO budget.
    fn wait_http<T>(
        &self,
        future: impl std::future::Future<Output = Result<T, reqwest::Error>>,
    ) -> io::Result<T> {
        self.runtime.block_on(async {
            let mut future = std::pin::pin!(future);
            loop {
                self.check()?;
                if let Ok(result) =
                    tokio::time::timeout(Duration::from_millis(20), future.as_mut()).await
                {
                    self.check()?;
                    return result.map_err(|error| {
                        io::Error::new(
                            if error.is_timeout() {
                                io::ErrorKind::TimedOut
                            } else {
                                io::ErrorKind::ConnectionReset
                            },
                            "YouTube audio transport failed",
                        )
                    });
                }
            }
        })
    }
    fn fill(&mut self) -> io::Result<()> {
        // Retry only an interrupted operation on the SAME grant and interval.
        // Never retry access refusal, inconsistent identity, overflow or 429.
        for attempt in 0..=2 {
            self.check()?;
            let start = self.position / CHUNK as u64 * CHUNK as u64;
            let end = (start + CHUNK as u64).min(self.grant.length) - 1;
            self.sequence += 1;
            let origin = self
                .trace
                .as_ref()
                .map_or_else(Instant::now, |t| t.origin());
            let mut record = crate::latency::HttpRead {
                sequence: self.sequence,
                offset: start,
                requested: end - start + 1,
                started_us: origin.elapsed().as_micros() as u64,
                ended_us: 0,
                status: 0,
                received: 0,
                content_length: None,
                length_valid: None,
                range_valid: None,
                first_data_us: None,
                outcome: "transport",
                cancelled: false,
                timeout: false,
                eof: false,
                incomplete: false,
            };
            let result = self.fetch(start, end, origin, &mut record);
            record.ended_us = origin.elapsed().as_micros() as u64;
            record.cancelled = (self.cancelled)();
            if record.cancelled {
                record.outcome = "cancelled";
            }
            if let Some(trace) = &self.trace {
                trace.record_http(record.clone());
            }
            if record.cancelled {
                self.bytes.clear();
                return Err(io::ErrorKind::ConnectionAborted.into());
            }
            match result {
                Ok(()) => return Ok(()),
                Err(error) => {
                    self.bytes.clear();
                    if attempt == 2
                        || !matches!(record.outcome, "transport" | "timeout" | "incomplete")
                    {
                        return Err(error);
                    }
                    // 50/100 ms backoff, checked every 10 ms; no new request
                    // after cancellation or lease expiration.
                    for _ in 0..5 * (attempt + 1) {
                        self.check()?;
                        std::thread::sleep(Duration::from_millis(10));
                    }
                }
            }
        }
        unreachable!()
    }
    fn fetch(
        &mut self,
        start: u64,
        end: u64,
        origin: Instant,
        record: &mut crate::latency::HttpRead,
    ) -> io::Result<()> {
        if let Some(trace) = &self.trace {
            trace.mark(Stage::Http);
        }
        let url = resource_url(&self.grant.url)?;
        let mut response = self
            .wait_http(async {
                self.http
                    .get(url)
                    .header("Range", format!("bytes={start}-{end}"))
                    .header("Accept-Encoding", "identity")
                    .send()
                    .await
            })
            .map_err(|error| {
                record.timeout = error.kind() == io::ErrorKind::TimedOut;
                record.outcome = if record.timeout {
                    "timeout"
                } else {
                    "transport"
                };
                error
            })?;
        self.check()?;
        record.status = response.status().as_u16();
        record.outcome = "status";
        match record.status {
            206 => {}
            200 => {
                record.outcome = "ignored_range";
                return Err(io::Error::other("YouTube audio range ignored"));
            }
            401 | 403 => {
                record.outcome = "refused";
                return Err(io::ErrorKind::PermissionDenied.into());
            }
            _ => return Err(io::Error::other("YouTube audio range unavailable")),
        }
        let expected = format!("bytes {start}-{end}/{}", self.grant.length);
        let range = response.headers().get("content-range");
        record.range_valid = range.map(|v| v.to_str().ok() == Some(expected.as_str()));
        if record.range_valid == Some(false) || (record.status == 206 && range.is_none()) {
            record.outcome = "content_range";
            return Err(io::Error::other("YouTube audio range changed"));
        }
        let size = (end - start + 1) as usize;
        let length = response.headers().get("content-length");
        record.content_length = length.and_then(|v| v.to_str().ok()?.parse().ok());
        record.length_valid = length.map(|_| record.content_length == Some(size as u64));
        if record.length_valid == Some(false) {
            record.incomplete = record.content_length.is_some_and(|n| n < size as u64);
            record.outcome = if record.incomplete {
                "incomplete"
            } else {
                "content_length"
            };
            return Err(io::ErrorKind::ConnectionReset.into());
        }
        self.bytes.clear();
        if self.bytes.capacity() < size + 1 {
            self.bytes.reserve_exact(size + 1 - self.bytes.len());
        }
        loop {
            let chunk = self.wait_http(response.chunk()).map_err(|error| {
                record.timeout = error.kind() == io::ErrorKind::TimedOut;
                record.incomplete = true;
                record.outcome = if record.timeout {
                    "timeout"
                } else {
                    "incomplete"
                };
                error
            })?;
            let Some(chunk) = chunk else {
                break;
            };
            record.received += chunk.len() as u64;
            if !chunk.is_empty() && record.first_data_us.is_none() {
                record.first_data_us = Some(origin.elapsed().as_micros() as u64);
            }
            self.check()?;
            if chunk.len() > (size + 1).saturating_sub(self.bytes.len()) {
                record.outcome = "overflow";
                return Err(io::ErrorKind::ConnectionReset.into());
            }
            self.bytes.extend_from_slice(&chunk);
            if self.bytes.len() > size {
                break;
            }
        }
        if self.bytes.len() != size {
            record.incomplete = self.bytes.len() < size;
            record.outcome = if record.incomplete {
                "incomplete"
            } else {
                "overflow"
            };
            return Err(io::ErrorKind::ConnectionReset.into());
        }
        self.check()?;
        self.cached_at = start;
        record.outcome = "validated";
        record.eof = end + 1 == self.grant.length;
        if let Some(trace) = &self.trace {
            trace.mark(Stage::FirstBytes);
        }
        Ok(())
    }
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
    fn resource_url_preserves_signed_octets_and_removes_previous_ranges() {
        let url = resource_url(
            "https://rr.googlevideo.com/videoplayback?sig=fixture%2f%3D&n=a%20b&range=0-9&cpn=fixture&range=2-4",
        )
        .unwrap();
        assert_eq!(url.query(), Some("sig=fixture%2f%3D&n=a%20b&cpn=fixture"));
    }
    #[test]
    fn refusals_and_broken_ranges_fail_once_without_reading_a_full_file() {
        for (status, range, body, expected) in [
            (403, "", b"".as_slice(), io::ErrorKind::PermissionDenied),
            (200, "", b"".as_slice(), io::ErrorKind::Other),
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
            (CHUNK, CHUNK, io::ErrorKind::Other),
            (CHUNK + 1, CHUNK + 1, io::ErrorKind::Other),
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
            assert!(reader.bytes.capacity() <= CHUNK + 1);
            thread.join().unwrap();
        }
    }
    #[test]
    fn ranges_start_playback_without_downloading_the_whole_track_and_seek_is_cancelable() {
        query_range_round_trip(206);
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
                    headers.starts_with("get /fixture http/1.1\r\n")
                        && headers.contains(&format!("\r\nrange: bytes={start}-{end}\r\n"))
                );

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

#[cfg(test)]
mod reliability_tests {
    use super::*;
    use std::{
        io::Write,
        net::TcpListener,
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
            mpsc,
        },
    };
    fn request(socket: &mut std::net::TcpStream) -> String {
        socket
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut bytes = Vec::new();
        while !bytes.ends_with(b"\r\n\r\n") {
            let mut byte = [0];
            socket.read_exact(&mut byte).unwrap();
            bytes.push(byte[0]);
            assert!(bytes.len() < 8192);
        }
        String::from_utf8(bytes).unwrap()
    }
    fn grant(address: std::net::SocketAddr, length: u64) -> Stream {
        Stream {
            url: format!("http://{address}/fixture"),
            user_agent: UA.into(),
            length,
            expires: Instant::now() + Duration::from_secs(30),
            duration_ms: 60000,
        }
    }
    #[test]
    fn interrupted_block_retries_same_offset_and_never_duplicates_bytes() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            for (start, sent) in [(0, 17), (0, CHUNK), (CHUNK, 71)] {
                let (mut socket, _) = listener.accept().unwrap();
                let req = request(&mut socket);
                let end = if start == 0 { CHUNK - 1 } else { CHUNK + 70 };
                assert!(
                    req.starts_with("GET /fixture HTTP/1.1")
                        && req
                            .to_ascii_lowercase()
                            .contains(&format!("\r\nrange: bytes={start}-{end}\r\n"))
                );
                write!(socket,"HTTP/1.1 206 OK\r\nContent-Length: {}\r\nContent-Range: bytes {start}-{end}/{}\r\nConnection: close\r\n\r\n",end-start+1,CHUNK+71).unwrap();
                socket
                    .write_all(&vec![if start == 0 { 1 } else { 2 }; sent])
                    .unwrap();
            }
        });
        let trace = Arc::new(Trace::default());
        trace.enable_http();
        let mut reader = grant(address, (CHUNK + 71) as u64)
            .reader(|| false)
            .unwrap()
            .with_trace(trace.clone());
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).unwrap();
        assert_eq!(&bytes[..CHUNK], vec![1; CHUNK]);
        assert_eq!(&bytes[CHUNK..], vec![2; 71]);
        assert_eq!(reader.read(&mut [0; 1]).unwrap(), 0);
        let records = trace.http_snapshot();
        assert_eq!(records.len(), 3);
        assert_eq!(
            records.iter().map(|r| r.offset).collect::<Vec<_>>(),
            [0, 0, CHUNK as u64]
        );
        assert!(records[0].incomplete);
        assert_eq!(records[0].received, 17);
        assert_eq!(records[1].outcome, "validated");
        assert!(records[2].eof);
        server.join().unwrap();
    }
    #[test]
    fn cancellation_retires_blocked_headers_and_bodies_before_deadline() {
        for headers in [false, true] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let (ready_tx, ready_rx) = mpsc::channel();
            let (release_tx, release_rx) = mpsc::channel();
            let server = std::thread::spawn(move || {
                let (mut socket, _) = listener.accept().unwrap();
                request(&mut socket);
                if headers {
                    write!(socket, "HTTP/1.1 206 OK\r\nContent-Length: {CHUNK}\r\nContent-Range: bytes 0-65535/65536\r\n\r\n").unwrap();
                    socket.write_all(&[7; 16]).unwrap();
                }
                ready_tx.send(()).unwrap();
                release_rx.recv_timeout(Duration::from_secs(2)).unwrap();
                // A late response must not turn a cancelled read into success.
                let _ = socket.write_all(&vec![9; CHUNK]);
            });
            let cancelled = Arc::new(AtomicBool::new(false));
            let fence = cancelled.clone();
            let trace = Arc::new(Trace::default());
            trace.enable_http();
            let exported = trace.clone();
            let worker = std::thread::spawn(move || {
                let mut reader = grant(address, CHUNK as u64)
                    .reader(move || fence.load(Ordering::Acquire))
                    .unwrap()
                    .with_trace(trace);
                let result = reader.read(&mut [0; 1]);
                assert!(reader.bytes.is_empty());
                result
            });
            ready_rx.recv_timeout(Duration::from_secs(2)).unwrap();
            let start = Instant::now();
            cancelled.store(true, Ordering::Release);
            assert_eq!(
                worker.join().unwrap().unwrap_err().kind(),
                io::ErrorKind::ConnectionAborted
            );
            assert!(start.elapsed() < Duration::from_millis(250));
            let record = exported.http_snapshot().pop().unwrap();
            assert!(record.cancelled);
            release_tx.send(()).unwrap();
            server.join().unwrap();
        }
    }
    #[test]
    fn timeout_has_a_finite_retry_budget_and_no_false_eof() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let mut sockets = Vec::new();
            for _ in 0..3 {
                let (mut socket, _) = listener.accept().unwrap();
                request(&mut socket);
                sockets.push(socket);
            }
            std::thread::sleep(Duration::from_millis(100));
        });
        let trace = Arc::new(Trace::default());
        trace.enable_http();
        let mut reader = grant(address, CHUNK as u64)
            .reader(|| false)
            .unwrap()
            .with_trace(trace.clone());
        reader.http = reqwest::Client::builder()
            .timeout(Duration::from_millis(60))
            .build()
            .unwrap();
        assert_eq!(
            reader.read(&mut [0; 1]).unwrap_err().kind(),
            io::ErrorKind::TimedOut
        );
        let records = trace.http_snapshot();
        assert_eq!(records.len(), 3);
        assert!(records.iter().all(|r| r.timeout && !r.eof));
        assert_eq!(reader.position, 0);
        assert!(reader.bytes.is_empty());
        server.join().unwrap();
    }
}

#[cfg(test)]
mod read_ahead_tests {
    use super::*;
    use std::{io::Write, net::TcpListener, sync::mpsc};
    #[test]
    fn seek_cancels_active_prefetch_and_uses_only_requested_interval() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let (ready, wait) = mpsc::channel();
        let server = std::thread::spawn(move || {
            for (index, at) in [0, CHUNK, 3 * CHUNK, 0].into_iter().enumerate() {
                let (mut socket, _) = listener.accept().unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut request = Vec::new();
                while !request.ends_with(b"\r\n\r\n") {
                    let mut b = [0];
                    socket.read_exact(&mut b).unwrap();
                    request.push(b[0]);
                }
                let request = String::from_utf8(request).unwrap().to_ascii_lowercase();
                assert!(request.starts_with("get /fixture http/1.1"));
                assert!(request.contains(&format!("\r\nrange: bytes={at}-{}\r\n", at + CHUNK - 1)));
                if index == 1 {
                    ready.send(()).unwrap();
                    // The active request must close before another seek can run.
                    assert_eq!(socket.read(&mut [0; 1]).unwrap(), 0);
                } else {
                    write!(
                        socket,
                        "HTTP/1.1 206 OK\r\nContent-Length: {CHUNK}\r\nContent-Range: bytes {at}-{}/{}\r\nConnection: close\r\n\r\n",at+CHUNK-1,4*CHUNK
                    )
                    .unwrap();
                    socket.write_all(&vec![(at / CHUNK) as u8; CHUNK]).unwrap();
                }
            }
        });
        let grant = Stream {
            url: format!("http://{address}/fixture"),
            user_agent: UA.into(),
            length: (4 * CHUNK) as u64,
            expires: Instant::now() + Duration::from_secs(10),
            duration_ms: 60000,
        };
        let mut reader = grant.reader(|| false).unwrap().read_ahead().unwrap();
        let mut output = [7; 16];
        reader.read_exact(&mut output).unwrap();
        assert_eq!(output, [0; 16]);
        wait.recv_timeout(Duration::from_secs(2)).unwrap();
        let start = Instant::now();
        reader.seek(SeekFrom::Start((3 * CHUNK) as u64)).unwrap();
        reader.read_exact(&mut output).unwrap();
        assert_eq!(output, [3; 16]);
        assert!(start.elapsed() < Duration::from_millis(500));
        reader.seek(SeekFrom::Start(0)).unwrap();
        reader.read_exact(&mut output).unwrap();
        assert_eq!(output, [0; 16]);
        drop(reader);
        server.join().unwrap();
    }
}
