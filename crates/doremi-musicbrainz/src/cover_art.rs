//! CAA front thumbnails for an exact release MBID. No artist/title matching.
//! The service is separate from MusicBrainz metadata and owns no image rights.
use super::{EntityKind, Error, MediaId, local_id};
use reqwest::{Client as Http, Url, redirect::Policy};
use std::{
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant, SystemTime},
};

pub const MAX_BYTES: usize = 512 * 1024;
pub const PIXELS: u32 = 250;
static NEXT: Mutex<Option<Instant>> = Mutex::new(None);
static COOLDOWN: Mutex<Option<Instant>> = Mutex::new(None);
static CLOSED: AtomicBool = AtomicBool::new(false);

pub struct Image {
    pub bytes: Vec<u8>,
    pub lifetime: Duration,
    pub reusable: bool,
}
pub struct Client {
    http: Http,
}
impl Client {
    /// Construct and poll on a worker; decoding is the host worker's job.
    pub fn new() -> Result<Self, Error> {
        Ok(Self {
            http: Http::builder()
                .user_agent("Doremi/0.1.0 (https://github.com/Eirom16/spotifast)")
                .connect_timeout(Duration::from_secs(3))
                .timeout(Duration::from_secs(8))
                .redirect(Policy::none())
                .pool_max_idle_per_host(1)
                .build()
                .map_err(super::network_error)?,
        })
    }
    pub async fn fetch(&self, id: &MediaId, cancelled: impl Fn() -> bool) -> Result<Image, Error> {
        let id = local_id(id, EntityKind::Release)?;
        let uri = Url::parse(&format!(
            "https://coverartarchive.org/release/{id}/front-{PIXELS}"
        ))
        .map_err(|_| Error::InvalidRequest)?;
        self.fetch_uri(uri, cancelled).await
    }
    async fn fetch_uri(&self, uri: Url, cancelled: impl Fn() -> bool) -> Result<Image, Error> {
        let stop = async {
            loop {
                if cancelled() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        };
        tokio::select! {
            biased;
            _ = stop => Err(Error::Cancelled),
            result = tokio::time::timeout(Duration::from_secs(12), self.fetch_inner(uri, &cancelled)) => result.map_err(|_| Error::Timeout)?,
        }
    }
    async fn fetch_inner(
        &self,
        mut uri: Url,
        cancelled: &impl Fn() -> bool,
    ) -> Result<Image, Error> {
        let deadline = Instant::now() + Duration::from_secs(12);
        for _ in 0..4 {
            self.wait(deadline, cancelled).await?;
            let budget = deadline
                .checked_duration_since(Instant::now())
                .ok_or(Error::Timeout)?;
            let started = Instant::now();
            let mut response = self
                .http
                .get(uri.clone())
                .timeout(budget.min(Duration::from_secs(8)))
                .header("Accept", "image/jpeg, image/png")
                .send()
                .await
                .map_err(super::network_error)?;
            if cancelled() {
                return Err(Error::Cancelled);
            }
            match response.status().as_u16() {
                301 | 302 | 303 | 307 | 308 => {
                    let next = response
                        .headers()
                        .get("location")
                        .and_then(|v| v.to_str().ok())
                        .and_then(|value| uri.join(value).ok())
                        .ok_or(Error::InvalidResponse)?;
                    if !allowed(&next) {
                        return Err(Error::AccessDenied);
                    }
                    uri = next;
                }
                200 => {
                    if !matches!(
                        response
                            .headers()
                            .get("content-type")
                            .and_then(|v| v.to_str().ok())
                            .and_then(|v| v.split(';').next())
                            .map(|v| v.trim().to_ascii_lowercase())
                            .as_deref(),
                        Some("image/jpeg" | "image/png")
                    ) {
                        return Err(Error::UnsupportedFormat);
                    }
                    if response
                        .content_length()
                        .is_some_and(|len| len > MAX_BYTES as u64)
                    {
                        return Err(Error::TooLarge);
                    }
                    let (lifetime, reusable) = cache_policy(
                        response
                            .headers()
                            .get("cache-control")
                            .and_then(|v| v.to_str().ok()),
                    );
                    let age = response
                        .headers()
                        .get("age")
                        .and_then(|value| value.to_str().ok())
                        .and_then(|value| value.parse::<u64>().ok())
                        .unwrap_or(0);
                    let lifetime = lifetime.saturating_sub(Duration::from_secs(age));
                    let mut bytes = Vec::new();
                    loop {
                        if cancelled() {
                            return Err(Error::Cancelled);
                        }
                        if Instant::now() >= deadline {
                            return Err(Error::Timeout);
                        }
                        let Some(chunk) = response.chunk().await.map_err(super::network_error)?
                        else {
                            break;
                        };
                        if chunk.len() > MAX_BYTES.saturating_sub(bytes.len()) {
                            return Err(Error::TooLarge);
                        }
                        bytes.extend_from_slice(&chunk);
                    }
                    if bytes.is_empty() {
                        return Err(Error::InvalidResponse);
                    }
                    if cancelled() {
                        return Err(Error::Cancelled);
                    }
                    if Instant::now() >= deadline {
                        return Err(Error::Timeout);
                    }
                    return Ok(Image {
                        bytes,
                        lifetime: lifetime.saturating_sub(started.elapsed()),
                        reusable,
                    });
                }
                401 | 403 => return Err(Error::AccessDenied),
                404 | 410 => return Err(Error::Unavailable),
                429 | 503 => {
                    let delay = retry_after(
                        response
                            .headers()
                            .get("retry-after")
                            .and_then(|v| v.to_str().ok()),
                    );
                    if let Some(until) = Instant::now().checked_add(delay) {
                        *COOLDOWN.lock().unwrap() = Some(until);
                    } else {
                        CLOSED.store(true, Ordering::Release);
                    }
                    return Err(Error::RateLimited {
                        retry_after_ms: delay.as_millis().try_into().ok(),
                    });
                }
                _ => return Err(Error::Transport),
            }
        }
        Err(Error::AccessDenied)
    }
    async fn wait(&self, deadline: Instant, cancelled: &impl Fn() -> bool) -> Result<(), Error> {
        loop {
            if cancelled() {
                return Err(Error::Cancelled);
            }
            if CLOSED.load(Ordering::Acquire) {
                return Err(Error::Exhausted);
            }
            let now = Instant::now();
            if now >= deadline {
                return Err(Error::Timeout);
            }
            if let Some(until) = *COOLDOWN.lock().unwrap()
                && until > now
            {
                return Err(Error::RateLimited {
                    retry_after_ms: (until - now).as_millis().try_into().ok(),
                });
            }
            let delay = {
                let mut next = NEXT.lock().unwrap();
                if let Some(due) = *next
                    && due > now
                {
                    Some(due - now)
                } else {
                    *next = Some(now + Duration::from_secs(1));
                    None
                }
            };
            let Some(delay) = delay else {
                return Ok(());
            };
            tokio::time::sleep(delay.min(Duration::from_millis(20))).await;
        }
    }
}
fn allowed(url: &Url) -> bool {
    url.scheme() == "https"
        && url.port_or_known_default() == Some(443)
        && url.username().is_empty()
        && url.password().is_none()
        && url.fragment().is_none()
        && url.host_str().is_some_and(|host| {
            host == "coverartarchive.org" || host == "archive.org" || host.ends_with(".archive.org")
        })
}
fn cache_policy(value: Option<&str>) -> (Duration, bool) {
    let mut seconds = 600;
    let mut reusable = true;
    for part in value
        .unwrap_or("")
        .split(',')
        .map(|value| value.trim().to_ascii_lowercase())
    {
        if matches!(part.split('=').next(), Some("no-store" | "no-cache")) {
            reusable = false;
        }
        if let Some(n) = part
            .strip_prefix("max-age=")
            .and_then(|v| v.trim_matches('"').parse::<u64>().ok())
        {
            seconds = n.min(600);
        }
    }
    (Duration::from_secs(seconds), reusable && seconds > 0)
}
fn retry_after(value: Option<&str>) -> Duration {
    if value.is_some_and(|value| {
        !value.is_empty()
            && value.bytes().all(|byte| byte.is_ascii_digit())
            && value.parse::<u64>().is_err()
    }) {
        return Duration::MAX;
    }
    value
        .and_then(|v| {
            v.parse::<u64>().ok().map(Duration::from_secs).or_else(|| {
                httpdate::parse_http_date(v).ok().map(|at| {
                    at.duration_since(SystemTime::now())
                        .unwrap_or(Duration::ZERO)
                })
            })
        })
        .unwrap_or(Duration::from_secs(60))
        .max(Duration::from_secs(1))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn provider_redirects_never_accept_other_authorities_or_cleartext() {
        for uri in [
            "http://archive.org/a",
            "https://archive.org.evil.test/a",
            "https://user@archive.org/a",
            "https://archive.org:444/a",
            "https://127.0.0.1/a",
            "https://example.org/a",
        ] {
            assert!(!allowed(&Url::parse(uri).unwrap()));
        }
        assert!(allowed(
            &Url::parse("https://dn710908.ca.archive.org/cover.jpg?opaque=kept").unwrap()
        ));
        assert!(!cache_policy(Some("public, no-store, max-age=600")).1);
        assert_eq!(cache_policy(Some("max-age=20")).0, Duration::from_secs(20));
        assert_eq!(retry_after(Some("120")), Duration::from_secs(120));
        assert_eq!(
            retry_after(Some("99999999999999999999999999999")),
            Duration::MAX
        );
    }
    #[test]
    fn transport_rejects_denial_redirects_and_large_bodies_without_retry() {
        fn runtime() -> tokio::runtime::Runtime {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
        }
        use std::{
            io::{Read, Write},
            net::TcpListener,
        };
        fn request(response: String) -> Result<Image, Error> {
            let server = TcpListener::bind("127.0.0.1:0").unwrap();
            let address = server.local_addr().unwrap();
            let thread = std::thread::spawn(move || {
                let (mut stream, _) = server.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut buffer = [0; 4096];
                let _ = stream.read(&mut buffer).unwrap();
                stream.write_all(response.as_bytes()).unwrap();
            });
            let client = Client::new().unwrap();
            let result = runtime().block_on(client.fetch_uri(
                Url::parse(&format!("http://{address}/front-250")).unwrap(),
                || false,
            ));
            thread.join().unwrap();
            result
        }
        assert!(matches!(
            request("HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\n\r\n".into()),
            Err(Error::AccessDenied)
        ));
        assert!(matches!(request("HTTP/1.1 307 Temporary Redirect\r\nLocation: http://example.org/cover.jpg\r\nContent-Length: 0\r\n\r\n".into()), Err(Error::AccessDenied)));
        assert!(matches!(
            request(format!(
                "HTTP/1.1 200 OK\r\nContent-Type: image/jpeg\r\nContent-Length: {}\r\n\r\n",
                MAX_BYTES + 1
            )),
            Err(Error::TooLarge)
        ));
        let image = request("HTTP/1.1 200 OK\r\nContent-Type: IMAGE/PNG; charset=binary\r\nCache-Control: no-store\r\nContent-Length: 3\r\n\r\nabc".into()).unwrap();
        assert_eq!(image.bytes, b"abc");
        assert!(!image.reusable);
        // Cancellation must interrupt a pending response, before its 8 s timeout.
        let server = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = server.local_addr().unwrap();
        let cancelled = std::sync::Arc::new(AtomicBool::new(false));
        let flag = cancelled.clone();
        let (release, hold) = std::sync::mpsc::channel::<()>();
        let thread = std::thread::spawn(move || {
            let (mut stream, _) = server.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut buffer = [0; 4096];
            let _ = stream.read(&mut buffer).unwrap();
            flag.store(true, Ordering::Release);
            let _ = hold.recv_timeout(Duration::from_secs(3));
        });
        let client = Client::new().unwrap();
        let began = Instant::now();
        let result = runtime().block_on(client.fetch_uri(
            Url::parse(&format!("http://{address}/waiting")).unwrap(),
            || cancelled.load(Ordering::Acquire),
        ));
        let _ = release.send(());
        thread.join().unwrap();
        assert!(matches!(result, Err(Error::Cancelled)));
        assert!(began.elapsed() < Duration::from_secs(2));

        assert!(matches!(
            request(
                "HTTP/1.1 503 Service Unavailable\r\nRetry-After: 60\r\nContent-Length: 0\r\n\r\n"
                    .into()
            ),
            Err(Error::RateLimited {
                retry_after_ms: Some(60000)
            })
        ));
        assert!(matches!(
            runtime().block_on(
                Client::new()
                    .unwrap()
                    .fetch_uri(Url::parse("http://127.0.0.1:1/").unwrap(), || false)
            ),
            Err(Error::RateLimited { .. })
        ));

        assert!(matches!(
            runtime().block_on(
                Client::new()
                    .unwrap()
                    .fetch_uri(Url::parse("http://127.0.0.1:1/").unwrap(), || true)
            ),
            Err(Error::Cancelled)
        ));
    }
}
