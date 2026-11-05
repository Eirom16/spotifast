//! Bounded CC0 audio ranges tied to live provider metadata and TLS identity.
//! The fixed recording also verifies each block against music-corpus.json.
//! No audio bytes, browser identity or persistent network cache are included.
use crate::{Info, PROVIDER, StreamGrant, http};
use doremi_core::{
    catalog::{AudioFormat, ProviderError, TemporaryResource, Tick},
    media::MediaId,
};
use reqwest::{
    Url,
    blocking::Client,
    header::{CONTENT_LENGTH, CONTENT_RANGE},
    redirect::Policy,
};
use sha1::{Digest, Sha1};
use std::{
    io::{self, Read, Seek, SeekFrom},
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

pub const ID: &str = "26896924";
pub const SIZE: u64 = 4_260_399;
pub const HASH: &str = "8ec78c0db83119e4461cfb85817b714b70210acc";
pub const TITLE: &str = "Chopin · Nocturno op. 9 n.º 2 — Membeth";
pub const FRAMES: u64 = 10_562_768;
pub const RATE: u32 = 44_100;
pub const CHUNK: usize = 131_072;
/// Total per-range IO budget, including connect/TLS and a bounded body.
pub const REQUEST_TIMEOUT_MS: u64 = 6_000;
static CLIENT: OnceLock<Result<Client, ProviderError>> = OnceLock::new();
static LAST_REQUEST: Mutex<Option<Instant>> = Mutex::new(None);
static COOLDOWN: Mutex<Option<Instant>> = Mutex::new(None);
static CLOSED: AtomicBool = AtomicBool::new(false);
pub fn candidate(id: &MediaId) -> bool {
    id.provider.as_str() == PROVIDER && id.local() == ID
}
pub fn validate(info: &Info) -> bool {
    candidate(&info.media.id) && info.byte_len == SIZE && info.sha1 == HASH
}

#[derive(Default)]
pub struct Metrics {
    pub failure: AtomicU8,
    pub bytes: AtomicU64,
    pub requests: AtomicU64,
    pub total_bytes: AtomicU64,
}
impl Metrics {
    pub fn message(&self) -> Option<&'static str> {
        match self.failure.load(Ordering::Acquire) {
            1 => Some("La fuente agotó el tiempo de espera. Puedes volver a seleccionar la pista."),
            2 => Some("No se pudo leer la fuente online. Revisa la conexión."),
            3 => Some("El proveedor rechazó el audio. No se intentará otro método de acceso."),
            4 => Some(
                "El proveedor limitó las peticiones. Espera antes de volver a seleccionar la pista.",
            ),
            5 => Some("La grabación ya no está disponible."),
            6 => Some(
                "Esta fuente no permite los rangos necesarios para reproducir y buscar posición.",
            ),
            7 | 8 => Some("El recurso recibido cambió o no coincide con la grabación verificada."),
            9 => Some("La autorización temporal caducó. Vuelve a seleccionar la pista."),
            _ => None,
        }
    }

    pub fn snapshot(&self) -> (u64, u64) {
        (
            self.bytes.load(Ordering::Acquire),
            self.requests.load(Ordering::Acquire),
        )
    }
}
/// Worker-owned only. HTTP is never read by the UI or audio callback.
pub struct Reader {
    client: Client,
    uri: String,
    hashes: Vec<String>,
    size: u64,
    object_hash: Option<String>,
    etag: Option<String>,
    position: u64,
    cached_at: u64,
    cached: Vec<u8>,
    cancelled: Box<dyn Fn() -> bool + Send + Sync>,
    pub metrics: Arc<Metrics>,
    expires: Instant,
    failed: bool,
    paced: bool,
}
impl Reader {
    fn pace(&self) -> io::Result<()> {
        loop {
            let now = Instant::now();
            if CLOSED.load(Ordering::Acquire)
                || COOLDOWN.lock().unwrap().is_some_and(|until| now < until)
            {
                self.metrics.failure.store(4, Ordering::Release);
                return Err(io::Error::other("Provider cooldown"));
            }
            if (self.cancelled)() || now >= self.expires {
                return Err(io::ErrorKind::ConnectionAborted.into());
            }
            let wait = {
                let mut last = LAST_REQUEST.lock().unwrap();
                if let Some(wait) = last
                    .and_then(|last| (last + Duration::from_secs(1)).checked_duration_since(now))
                {
                    wait
                } else {
                    *last = Some(now);
                    return Ok(());
                }
            };
            std::thread::sleep(wait.min(Duration::from_millis(20)));
        }
    }
    pub fn open(
        resource: TemporaryResource,
        now: Tick,
        cancelled: impl Fn() -> bool + Send + Sync + 'static,
        metrics: Arc<Metrics>,
    ) -> Result<Self, ProviderError> {
        Self::open_internal(resource, None, now, cancelled, metrics)
    }
    pub fn open_stream(
        resource: TemporaryResource,
        grant: StreamGrant,
        now: Tick,
        cancelled: impl Fn() -> bool + Send + Sync + 'static,
        metrics: Arc<Metrics>,
    ) -> Result<Self, ProviderError> {
        Self::open_internal(resource, Some(grant), now, cancelled, metrics)
    }
    fn open_internal(
        resource: TemporaryResource,
        grant: Option<StreamGrant>,
        now: Tick,
        cancelled: impl Fn() -> bool + Send + Sync + 'static,
        metrics: Arc<Metrics>,
    ) -> Result<Self, ProviderError> {
        if grant.as_ref().is_some_and(|grant| {
            grant.media != *resource.media()
                || grant.uri != resource.expose_uri()
                || Some(grant.bytes) != resource.info().byte_len
                || Instant::now() >= grant.expires
        }) || (grant.is_none()
            && (!candidate(resource.media()) || resource.info().byte_len != Some(SIZE)))
            || resource.info().format != AudioFormat::OggVorbis
            || !resource.info().seekable
            || resource.expose_headers().next().is_some()
            || now.0 >= resource.info().usable_until.0
        {
            return Err(ProviderError::AccessDenied);
        }
        let uri = Url::parse(resource.expose_uri()).map_err(|_| ProviderError::InvalidRequest)?;
        if !http::allowed(&uri) || uri.host_str() != Some("upload.wikimedia.org") {
            return Err(ProviderError::AccessDenied);
        }
        let size = resource
            .info()
            .byte_len
            .ok_or(ProviderError::InvalidResponse)?;
        let mut hashes = Vec::new();
        if candidate(resource.media()) {
            if size != SIZE || grant.as_ref().is_some_and(|grant| grant.hash != HASH) {
                return Err(ProviderError::Unavailable);
            }
            let data: serde_json::Value =
                serde_json::from_str(include_str!("../music-corpus.json"))
                    .map_err(|_| ProviderError::InvalidResponse)?;
            hashes = serde_json::from_value(data["chunks"].clone())
                .map_err(|_| ProviderError::InvalidResponse)?;
            if hashes.len() != (SIZE as usize).div_ceil(CHUNK) {
                return Err(ProviderError::InvalidResponse);
            }
        }
        // This constructor runs on the audio worker, outside any Tokio runtime.
        let client = CLIENT
            .get_or_init(|| {
                Client::builder()
                    .user_agent(http::AGENT)
                    .connect_timeout(Duration::from_secs(3))
                    .timeout(Duration::from_millis(REQUEST_TIMEOUT_MS))
                    .redirect(Policy::none())
                    .pool_max_idle_per_host(1)
                    .pool_idle_timeout(Duration::from_secs(15))
                    .build()
                    .map_err(http::error)
            })
            .clone()?;
        metrics.total_bytes.store(size, Ordering::Release);
        let resource_expiry =
            Instant::now() + Duration::from_millis(resource.info().usable_until.0 - now.0);
        let expires = grant
            .as_ref()
            .map_or(resource_expiry, |grant| resource_expiry.min(grant.expires));
        Ok(Self {
            client,
            uri: resource.expose_uri().to_owned(),
            hashes,
            size,
            object_hash: grant.map(|grant| grant.hash),
            etag: None,
            position: 0,
            cached_at: u64::MAX,
            cached: Vec::new(),
            cancelled: Box::new(cancelled),
            metrics,
            failed: false,
            paced: true,
            expires,
        })
    }
    fn block(&mut self) -> io::Result<()> {
        if Instant::now() >= self.expires {
            self.metrics.failure.store(9, Ordering::Release);
            return Err(io::Error::other("Audio lease expired"));
        }
        if (self.cancelled)() {
            return Err(io::ErrorKind::ConnectionAborted.into());
        }
        let at = self.position / CHUNK as u64 * CHUNK as u64;
        if self.cached_at == at {
            return Ok(());
        }
        let end = (at + CHUNK as u64).min(self.size) - 1;
        if self.paced {
            self.pace()?;
        }
        self.metrics.requests.fetch_add(1, Ordering::AcqRel);
        let response = self
            .client
            .get(&self.uri)
            .header("Range", format!("bytes={at}-{end}"))
            .header("Accept-Encoding", "identity")
            .send()
            .map_err(|error| {
                self.metrics
                    .failure
                    .store(if error.is_timeout() { 1 } else { 2 }, Ordering::Release);
                io::Error::other("HTTP range failed")
            })?;
        // A 200 is not a usable range: never consume a whole ignored-range body.
        if response.status().as_u16() != 206 {
            let status = response.status().as_u16();
            self.metrics.failure.store(
                match status {
                    401 | 403 => 3,
                    429 => 4,
                    404 | 410 => 5,
                    200 => 6,
                    _ => 2,
                },
                Ordering::Release,
            );
            if status == 429 {
                let duration = response
                    .headers()
                    .get("retry-after")
                    .and_then(|value| value.to_str().ok())
                    .and_then(|value| {
                        value
                            .parse::<u64>()
                            .ok()
                            .map(Duration::from_secs)
                            .or_else(|| {
                                httpdate::parse_http_date(value).ok().map(|date| {
                                    date.duration_since(std::time::SystemTime::now())
                                        .unwrap_or(Duration::ZERO)
                                })
                            })
                    })
                    .unwrap_or(Duration::from_secs(60))
                    .max(Duration::from_secs(1));
                if let Some(until) = Instant::now().checked_add(duration) {
                    *COOLDOWN.lock().unwrap() = Some(until);
                } else {
                    CLOSED.store(true, Ordering::Release);
                }
            }
            return Err(io::Error::other("HTTP range unavailable or denied"));
        }
        let expected = format!("bytes {at}-{end}/{}", self.size);
        if response
            .headers()
            .get(CONTENT_RANGE)
            .and_then(|v| v.to_str().ok())
            != Some(expected.as_str())
            || response
                .headers()
                .get(CONTENT_LENGTH)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok())
                != Some(end - at + 1)
        {
            self.metrics.failure.store(7, Ordering::Release);
            return Err(io::Error::other("Invalid HTTP range identity"));
        }
        if let Some(hash) = &self.object_hash {
            let identity = response
                .headers()
                .get("x-object-meta-sha1base36")
                .and_then(|v| v.to_str().ok());
            let etag = response
                .headers()
                .get("etag")
                .and_then(|v| v.to_str().ok())
                .filter(|value| {
                    !value.is_empty() && value.len() <= 256 && !value.starts_with("W/")
                });
            let mime = response
                .headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok());
            if !identity.is_some_and(|value| object_hash_matches(value, hash))
                || etag.is_none()
                || self
                    .etag
                    .as_deref()
                    .is_some_and(|previous| Some(previous) != etag)
                || !matches!(mime, Some("application/ogg" | "audio/ogg"))
            {
                self.metrics.failure.store(7, Ordering::Release);
                return Err(io::Error::other(
                    "Provider object identity changed or unavailable",
                ));
            }
            self.etag = etag.map(str::to_owned);
        }
        let mut bytes = Vec::with_capacity(CHUNK + 1);
        response.take(CHUNK as u64 + 1).read_to_end(&mut bytes)?;
        self.metrics
            .bytes
            .fetch_add(bytes.len() as u64, Ordering::AcqRel);
        if (self.cancelled)() {
            return Err(io::ErrorKind::ConnectionAborted.into());
        }
        if Instant::now() >= self.expires {
            self.metrics.failure.store(9, Ordering::Release);
            return Err(io::Error::other("Audio lease expired during IO"));
        }
        if bytes.len() as u64 != end - at + 1
            || (!self.hashes.is_empty()
                && format!("{:x}", Sha1::digest(&bytes)) != self.hashes[at as usize / CHUNK])
        {
            self.metrics.failure.store(8, Ordering::Release);
            return Err(io::Error::other("Authenticated audio block mismatch"));
        }
        self.cached = bytes;
        self.cached_at = at;
        Ok(())
    }
}

impl Read for Reader {
    fn read(&mut self, target: &mut [u8]) -> io::Result<usize> {
        if target.is_empty() || self.position >= self.size {
            return Ok(0);
        }
        if self.failed {
            return Err(io::ErrorKind::ConnectionAborted.into());
        }
        if let Err(error) = self.block() {
            self.failed = true;
            return Err(error);
        }
        let offset = (self.position - self.cached_at) as usize;
        let len = target.len().min(self.cached.len() - offset);
        target[..len].copy_from_slice(&self.cached[offset..offset + len]);
        self.position += len as u64;
        Ok(len)
    }
}
impl Seek for Reader {
    fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
        let position = match from {
            SeekFrom::Start(n) => i128::from(n),
            SeekFrom::End(n) => i128::from(self.size) + i128::from(n),
            SeekFrom::Current(n) => i128::from(self.position) + i128::from(n),
        };
        if !(0..=i128::from(self.size)).contains(&position) {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        self.position = position as u64;
        Ok(self.position)
    }
}
// Expose the normal Read/Seek interface without adding decoder dependencies here.
impl Reader {
    pub fn byte_len(&self) -> u64 {
        self.size
    }
}

// Swift publishes the provider's object SHA-1 as a base36 integer. This binds
// each TLS range response to live API metadata; it is not a per-block digest.
fn object_hash_matches(value: &str, expected: &str) -> bool {
    if value.is_empty()
        || value.len() > 31
        || !value
            .bytes()
            .all(|c| c.is_ascii_digit() || c.is_ascii_lowercase())
    {
        return false;
    }
    let mut bytes = [0u8; 20];
    for digit in value.bytes() {
        let mut carry = u16::from(if digit.is_ascii_digit() {
            digit - b'0'
        } else {
            digit - b'a' + 10
        });
        for byte in bytes.iter_mut().rev() {
            carry += u16::from(*byte) * 36;
            *byte = carry as u8;
            carry >>= 8;
        }
        if carry != 0 {
            return false;
        }
    }
    bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>()
        == expected
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{io::Write, net::TcpListener, thread};
    #[test]
    fn swift_object_identity_is_exact_and_overflow_safe() {
        assert!(object_hash_matches("goezs4jsrmspoawopcrx6jfk77w1ebw", HASH));
        for value in [
            "",
            "not-a-hash",
            "zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz",
            "0",
            "GOEZS4JSRMSPOAWOPCRX6JFK77W1EBW",
        ] {
            assert!(!object_hash_matches(value, HASH));
        }
    }
    fn reader(status: u16, range: &str, tamper: bool) -> (Reader, thread::JoinHandle<()>) {
        delayed_reader(status, range, tamper, Duration::ZERO)
    }
    fn delayed_reader(
        status: u16,
        range: &str,
        tamper: bool,
        delay: Duration,
    ) -> (Reader, thread::JoinHandle<()>) {
        response_reader(status, range, tamper, delay, "")
    }
    fn response_reader(
        status: u16,
        range: &str,
        tamper: bool,
        delay: Duration,
        extra_headers: &str,
    ) -> (Reader, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let uri = format!("http://{}/owned.ogg", listener.local_addr().unwrap());
        let range = range.to_owned();
        let extra_headers = extra_headers.to_owned();
        let mut body = vec![42; CHUNK];
        let hash = format!("{:x}", Sha1::digest(&body));
        if tamper {
            body[100] ^= 1;
        }
        let worker = thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(1)))
                .unwrap();
            socket
                .set_write_timeout(Some(Duration::from_secs(1)))
                .unwrap();
            let mut request = [0; 4096];
            let _ = socket.read(&mut request);
            thread::sleep(delay);
            let headers = format!(
                "HTTP/1.1 {status} Test\r\nContent-Length: {CHUNK}\r\nContent-Range: {range}\r\n{extra_headers}Connection: close\r\n\r\n"
            );
            let _ = socket.write_all(headers.as_bytes());
            let _ = socket.write_all(&body);
        });
        (
            Reader {
                client: Client::builder()
                    .timeout(Duration::from_secs(1))
                    .build()
                    .unwrap(),
                uri,
                hashes: vec![hash],
                size: SIZE,
                object_hash: None,
                etag: None,
                position: 0,
                cached_at: u64::MAX,
                cached: vec![],
                cancelled: Box::new(|| false),
                metrics: Arc::default(),
                expires: Instant::now() + Duration::from_secs(5),
                failed: false,
                paced: false,
            },
            worker,
        )
    }
    #[test]
    fn live_ranges_require_the_provider_identity_and_a_consistent_strong_etag() {
        for (sha, etag, pinned, success) in [
            ("goezs4jsrmspoawopcrx6jfk77w1ebw", "version1", None, true),
            ("0", "version1", None, false),
            ("", "version1", None, false),
            ("goezs4jsrmspoawopcrx6jfk77w1ebw", "", None, false),
            ("goezs4jsrmspoawopcrx6jfk77w1ebw", "W/version1", None, false),
            (
                "goezs4jsrmspoawopcrx6jfk77w1ebw",
                "version2",
                Some("version1"),
                false,
            ),
        ] {
            let headers = format!(
                "Content-Type: application/ogg\r\nX-Object-Meta-Sha1base36: {sha}\r\nETag: {etag}\r\n"
            );
            let (mut input, server) = response_reader(
                206,
                "bytes 0-131071/4260399",
                false,
                Duration::ZERO,
                &headers,
            );
            input.object_hash = Some(HASH.into());
            input.hashes.clear();
            input.etag = pinned.map(str::to_owned);
            let mut bytes = [0; 8];
            assert_eq!(input.read(&mut bytes).is_ok(), success);
            if success {
                assert_eq!(bytes, [42; 8]);
                assert_eq!(input.etag.as_deref(), Some("version1"));
            } else {
                assert_eq!(bytes, [0; 8]);
                assert_eq!(input.metrics.failure.load(Ordering::Acquire), 7);
                assert!(input.read(&mut bytes).is_err());
                assert_eq!(input.metrics.requests.load(Ordering::Acquire), 1);
            }
            server.join().unwrap();
        }
    }
    #[test]
    fn slow_authorized_response_fits_the_native_budget_and_a_deadline_latches_without_retry() {
        let (mut source, server) = delayed_reader(
            206,
            "bytes 0-131071/4260399",
            false,
            Duration::from_millis(2200),
        );
        source.client = Client::builder()
            .timeout(Duration::from_millis(REQUEST_TIMEOUT_MS))
            .build()
            .unwrap();
        let mut target = [0; 32];
        assert_eq!(source.read(&mut target).unwrap(), 32);
        assert_eq!(target, [42; 32]);
        server.join().unwrap();
        let (mut source, server) = delayed_reader(
            206,
            "bytes 0-131071/4260399",
            false,
            Duration::from_millis(250),
        );
        source.client = Client::builder()
            .timeout(Duration::from_millis(100))
            .build()
            .unwrap();
        assert!(source.read(&mut target).is_err());
        assert_eq!(source.metrics.failure.load(Ordering::Acquire), 1);
        assert!(source.read(&mut target).is_err());
        assert_eq!(source.metrics.requests.load(Ordering::Acquire), 1);
        assert_eq!(source.metrics.bytes.load(Ordering::Acquire), 0);
        server.join().unwrap();
    }
    #[test]
    fn a_lease_expiring_during_a_response_cannot_publish_audio() {
        let (mut source, server) = delayed_reader(
            206,
            "bytes 0-131071/4260399",
            false,
            Duration::from_millis(250),
        );
        source.expires = Instant::now() + Duration::from_millis(100);
        let mut target = [0; 32];
        assert!(source.read(&mut target).is_err());
        assert_eq!(target, [0; 32]);
        assert_eq!(source.metrics.failure.load(Ordering::Acquire), 9);
        assert!(source.read(&mut target).is_err());
        assert_eq!(source.metrics.requests.load(Ordering::Acquire), 1);
        server.join().unwrap();
    }
    #[test]
    fn ranges_are_verified_before_any_byte_reaches_the_decoder() {
        for (status, range, tamper, success) in [
            (206, "bytes 0-131071/4260399", false, true),
            (200, "bytes 0-131071/4260399", false, false),
            (403, "bytes 0-131071/4260399", false, false),
            (429, "bytes 0-131071/4260399", false, false),
            (206, "bytes 1-131072/4260399", false, false),
            (206, "bytes 0-131071/4260399", true, false),
        ] {
            let (mut input, worker) = reader(status, range, tamper);
            let mut output = [0; 12];
            let result = input.read(&mut output);
            assert_eq!(result.is_ok(), success);
            if success {
                assert_eq!(output, [42; 12]);
                assert_eq!(input.metrics.snapshot(), (CHUNK as u64, 1));
            } else {
                assert_eq!(output, [0; 12]);
                assert_eq!(input.cached_at, u64::MAX);
                assert!(input.read(&mut output).is_err());
                assert_eq!(
                    input.metrics.snapshot().1,
                    1,
                    "A failed range must never be retried by the decoder"
                );
            }
            worker.join().unwrap();
        }
    }
    #[test]
    fn cancellation_and_expiry_stop_before_another_request() {
        // No local listener is needed: neither branch may initiate network IO.
        let data = |cancelled, expires| Reader {
            client: Client::new(),
            uri: "http://127.0.0.1:1/".into(),
            hashes: vec![],
            size: SIZE,
            object_hash: None,
            etag: None,
            position: 0,
            cached_at: u64::MAX,
            cached: vec![],
            cancelled: Box::new(move || cancelled),
            metrics: Arc::default(),
            expires,
            failed: false,
            paced: false,
        };
        let mut cancelled = data(true, Instant::now() + Duration::from_secs(1));
        let mut expired = data(false, Instant::now());
        for input in [&mut cancelled, &mut expired] {
            assert!(input.read_exact(&mut [0; 100]).is_err());
            assert_eq!(input.metrics.snapshot(), (0, 0));
        }
    }
}
