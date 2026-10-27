//! Synthetic fixture harness, not a YouTube adapter or a production HTTP client.
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::{Duration, Instant};

use doremi_core::catalog::*;
use doremi_core::media::{Media, MediaId, ProviderId};
use doremi_core::requests::SessionId;
use serde_json::{Value, json};

const MAX_BODY: usize = 1024 * 1024;
const MAX_HEADER: usize = 8192;
const CASES: &[&str] = &[
    "ok",
    "denied",
    "rate-limited",
    "malformed",
    "too-many",
    "unavailable",
    "range-ignored",
    "truncated",
    "oversized",
    "stalled",
];

struct Response {
    status: u16,
    headers: BTreeMap<String, String>,
    body: Vec<u8>,
}

fn http(
    addr: SocketAddr,
    path: &str,
    budget: Duration,
    range: bool,
) -> Result<Response, ProviderError> {
    let started = Instant::now();
    let mut stream =
        TcpStream::connect_timeout(&addr, budget).map_err(|_| ProviderError::Transport)?;
    stream
        .set_write_timeout(Some(budget))
        .map_err(|_| ProviderError::Transport)?;
    let extra = if range { "Range: bytes=0-127\r\n" } else { "" };
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n{extra}\r\n"
    )
    .map_err(|_| ProviderError::Transport)?;
    let mut bytes = Vec::new();
    let mut parsed: Option<(usize, u16, BTreeMap<String, String>, usize)> = None;
    loop {
        let remaining = budget
            .checked_sub(started.elapsed())
            .filter(|d| !d.is_zero())
            .ok_or(ProviderError::Timeout)?;
        stream
            .set_read_timeout(Some(remaining))
            .map_err(|_| ProviderError::Transport)?;
        let mut block = [0; 4096];
        let n = stream.read(&mut block).map_err(|e| {
            if matches!(
                e.kind(),
                std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
            ) {
                ProviderError::Timeout
            } else {
                ProviderError::Transport
            }
        })?;
        if n == 0 {
            return Err(ProviderError::InvalidResponse);
        }
        bytes.extend_from_slice(&block[..n]);
        if bytes.len() > MAX_BODY + MAX_HEADER {
            return Err(ProviderError::TooLarge);
        }
        if parsed.is_none() {
            if let Some(at) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                if at > MAX_HEADER {
                    return Err(ProviderError::TooLarge);
                }
                let text = std::str::from_utf8(&bytes[..at])
                    .map_err(|_| ProviderError::InvalidResponse)?;
                let mut lines = text.split("\r\n");
                let status = lines
                    .next()
                    .and_then(|line| line.split_whitespace().nth(1))
                    .and_then(|s| s.parse().ok())
                    .ok_or(ProviderError::InvalidResponse)?;
                let mut headers = BTreeMap::new();
                for line in lines {
                    let (key, value) =
                        line.split_once(':').ok_or(ProviderError::InvalidResponse)?;
                    if headers
                        .insert(key.to_ascii_lowercase(), value.trim().to_owned())
                        .is_some()
                    {
                        return Err(ProviderError::InvalidResponse);
                    }
                }
                if headers.contains_key("transfer-encoding") {
                    return Err(ProviderError::InvalidResponse);
                }
                let length: usize = headers
                    .get("content-length")
                    .and_then(|v| v.parse().ok())
                    .ok_or(ProviderError::InvalidResponse)?;
                if length > MAX_BODY {
                    return Err(ProviderError::TooLarge);
                }
                parsed = Some((at + 4, status, headers, length));
            } else if bytes.len() > MAX_HEADER {
                return Err(ProviderError::TooLarge);
            }
        }
        if let Some((at, _, _, length)) = &parsed
            && bytes.len() >= at + length
        {
            let (at, status, headers, length) = parsed.take().expect("parsed response");
            if bytes.len() != at + length {
                return Err(ProviderError::InvalidResponse);
            }
            return Ok(Response {
                status,
                headers,
                body: bytes[at..].to_vec(),
            });
        }
    }
}

fn status(response: &Response) -> Result<(), ProviderError> {
    match response.status {
        200 | 206 => Ok(()),
        403 => Err(ProviderError::AccessDenied),
        404 => Err(ProviderError::Unavailable),
        429 => Err(ProviderError::RateLimited {
            retry_after_ms: response
                .headers
                .get("retry-after")
                .and_then(|s| s.parse::<u64>().ok())
                .and_then(|n| n.checked_mul(1000)),
        }),
        500..=599 => Err(ProviderError::Transport),
        _ => Err(ProviderError::InvalidResponse),
    }
}

struct FakeHttpWorker {
    searches: BTreeMap<String, Vec<Media>>,
    resolutions: BTreeMap<String, TemporaryResource>,
    origin: Instant,
}

impl Default for FakeHttpWorker {
    fn default() -> Self {
        Self {
            searches: BTreeMap::new(),
            resolutions: BTreeMap::new(),
            origin: Instant::now(),
        }
    }
}

impl FakeHttpWorker {
    fn search(
        &mut self,
        addr: SocketAddr,
        case: &str,
        budget: Duration,
    ) -> Result<(Vec<Media>, bool), ProviderError> {
        if let Some(items) = self.searches.get(case) {
            return Ok((items.clone(), true));
        }
        let response = http(addr, &format!("/search/{case}"), budget, false)?;
        status(&response)?;
        let json: Value =
            serde_json::from_slice(&response.body).map_err(|_| ProviderError::InvalidResponse)?;
        let rows = json["items"]
            .as_array()
            .ok_or(ProviderError::InvalidResponse)?;
        if rows.len() > usize::from(MAX_RESULTS) {
            return Err(ProviderError::TooLarge);
        }
        let mut items = Vec::new();
        for row in rows {
            let local = row["id"].as_str().ok_or(ProviderError::InvalidResponse)?;
            let title = row["title"]
                .as_str()
                .ok_or(ProviderError::InvalidResponse)?;
            if local.len() > 1024 || title.len() > 4096 {
                return Err(ProviderError::TooLarge);
            }
            items.push(Media {
                id: MediaId::new(ProviderId::new("fake-http").expect("namespace"), local)
                    .map_err(|_| ProviderError::InvalidResponse)?,
                title: title.into(),
                duration_ms: None,
            });
        }
        self.searches.insert(case.into(), items.clone());
        Ok((items, false))
    }

    fn resolve(
        &mut self,
        addr: SocketAddr,
        case: &str,
        media: MediaId,
        now: Tick,
        budget: Duration,
    ) -> Result<(TemporaryResource, bool), ProviderError> {
        if let Some(resource) = self
            .resolutions
            .get(case)
            .filter(|r| now < r.info().usable_until)
        {
            return Ok((resource.clone(), true));
        }
        let response = http(addr, &format!("/resolve/{case}"), budget, false)?;
        status(&response)?;
        let json: Value =
            serde_json::from_slice(&response.body).map_err(|_| ProviderError::InvalidResponse)?;
        if json["id"] != media.local() {
            return Err(ProviderError::InvalidResponse);
        }
        let resource = TemporaryResource::http(
            media,
            format!("http://{addr}/stream/{case}?fixture_token=do-not-log"),
            vec![("Cookie".into(), "fixture-private-cookie".into())],
            StreamInfo {
                format: AudioFormat::WavPcm16,
                byte_len: json["byte_len"].as_u64(),
                seekable: true,
                usable_until: Tick(
                    now.0
                        .checked_add(5000)
                        .ok_or(ProviderError::InvalidResponse)?,
                ),
                lease_basis: LeaseBasis::ConservativeHostLimit,
            },
        )?;
        self.resolutions.insert(case.into(), resource.clone());
        Ok((resource, false))
    }
}

fn millis(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}

struct SampleConfig<'a> {
    addr: SocketAddr,
    case: &'a str,
    warm: bool,
    commit: &'a str,
    dirty: bool,
    budget_ms: u64,
}

fn sample(config: &SampleConfig<'_>, iteration: usize, worker: &mut FakeHttpWorker) -> Value {
    let SampleConfig {
        addr,
        case,
        warm,
        commit,
        dirty,
        budget_ms,
    } = *config;
    if !warm {
        *worker = FakeHttpWorker::default();
    }
    let started = Instant::now();
    let mut search_ms = None;
    let mut resolve_ms = None;
    let mut transport_ms = None;
    let mut search_hit = false;
    let mut resolve_hit = false;
    let budget = Duration::from_millis(budget_ms);
    let result = (|| -> Result<(), ProviderError> {
        let caps = Capabilities {
            provider: ProviderId::new("fake-http").expect("namespace"),
            search: true,
            resolve: true,
            max_results: 20,
            formats: vec![AudioFormat::WavPcm16],
        };
        let mut catalog = Catalog::new(SessionId(iteration as u64 + 1), caps)?;
        let request = catalog
            .search(
                "authorized synthetic fixture".into(),
                2,
                Tick(worker.origin.elapsed().as_millis() as u64),
                budget_ms,
            )?
            .into_iter()
            .find_map(|c| match c {
                ProviderCommand::Search(r) => Some(r),
                _ => None,
            })
            .expect("search");
        let at = Instant::now();
        let found = worker.search(addr, case, budget).map(|(items, hit)| {
            search_hit = hit;
            items
        });
        search_ms = Some(millis(at));
        let now = Tick(worker.origin.elapsed().as_millis() as u64);
        catalog.complete(
            ProviderEvent::Search {
                ticket: request.ticket,
                result: found,
            },
            now,
        )?;
        if let Status::Failed(error) = catalog.search_status() {
            return Err(error);
        }
        // Explicit known fixture ID, never "first result" or a title equivalence.
        let id = MediaId::new(ProviderId::new("fake-http").expect("namespace"), "tone")
            .expect("fixture ID");
        let request = catalog
            .resolve(id, now, budget_ms)?
            .into_iter()
            .find_map(|c| match c {
                ProviderCommand::Resolve(r) => Some(r),
                _ => None,
            })
            .expect("resolution");
        let at = Instant::now();
        let resolved =
            worker
                .resolve(addr, case, request.media, now, budget)
                .map(|(resource, hit)| {
                    resolve_hit = hit;
                    resource
                });
        resolve_ms = Some(millis(at));
        let now = Tick(worker.origin.elapsed().as_millis() as u64);
        catalog.complete(
            ProviderEvent::Resolve {
                ticket: request.ticket,
                result: resolved,
            },
            now,
        )?;
        if let Status::Failed(error) = catalog.resolve_status() {
            return Err(error);
        }
        let resource = catalog.resource().ok_or(ProviderError::InvalidResponse)?;
        let base = format!("http://{addr}");
        let path = resource
            .expose_uri()
            .strip_prefix(&base)
            .filter(|p| p.starts_with("/stream/"))
            .ok_or(ProviderError::InvalidResponse)?;
        let at = Instant::now();
        let transferred = (|| {
            let response = http(addr, path, budget, true)?;
            status(&response)?;
            if response.status != 206
                || response.body.len() != 128
                || response
                    .headers
                    .get("content-range")
                    .is_none_or(|h| !h.starts_with("bytes 0-127/"))
                || !response.body.starts_with(b"RIFF")
            {
                return Err(ProviderError::InvalidResponse);
            }
            Ok(())
        })();
        transport_ms = Some(millis(at));
        transferred
    })();
    let successful = result.is_ok();
    let error = result.err().map(|e| format!("{e:?}"));
    json!({
        "schema": 1, "sample_kind": "synthetic-http-fixture", "provider": "fake-http",
        "source_commit": commit, "source_dirty": dirty, "harness_version": env!("CARGO_PKG_VERSION"),
        "case_id": case, "iteration": iteration,
        "temperature": if warm { "adapter-warm" } else { "adapter-cold" },
        "temperature_scope": "fake_adapter_cache", "startup_included": false,
        "search_cache_hit": search_hit, "resolve_cache_hit": resolve_hit,
        "search_ms": search_ms, "resolve_ms": resolve_ms, "transport_ms": transport_ms,
        "total_ms": millis(started), "first_pcm_ms": null, "audible_ms": null, "cancel_ms": null,
        "peak_rss_bytes": null, "cpu_user_ms": null, "cpu_system_ms": null,
        "outcome": if successful { "ok" } else { "error" },
        "error": error,
    })
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(2);
    }
}

fn run() -> Result<(), &'static str> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let value = |key| {
        args.iter()
            .position(|a| a == key)
            .and_then(|i| args.get(i + 1))
    };
    let addr: SocketAddr = value("--fixture")
        .ok_or("--fixture is required")?
        .parse()
        .map_err(|_| "invalid fixture address")?;
    if !addr.ip().is_loopback() {
        return Err("only loopback fixtures are allowed");
    }
    let case = value("--case").ok_or("--case is required")?;
    if !CASES.contains(&case.as_str()) {
        return Err("unknown fixture case");
    }
    let commit = value("--commit")
        .filter(|s| s.len() == 40 && s.bytes().all(|b| b.is_ascii_hexdigit()))
        .ok_or("a full --commit SHA is required")?;
    let mode = value("--mode").ok_or("--mode cold|warm is required")?;
    if !["cold", "warm"].contains(&mode.as_str()) {
        return Err("invalid mode");
    }
    let iterations: usize = value("--iterations")
        .and_then(|s| s.parse().ok())
        .filter(|n| (1..=1000).contains(n))
        .ok_or("--iterations must be 1..1000")?;
    let budget_ms: u64 = value("--budget-ms")
        .and_then(|s| s.parse().ok())
        .filter(|n| (1..=MAX_DEADLINE_MS).contains(n))
        .ok_or("invalid --budget-ms")?;
    let mut worker = FakeHttpWorker::default();
    let config = SampleConfig {
        addr,
        case,
        warm: mode == "warm",
        commit,
        dirty: args.iter().any(|a| a == "--dirty"),
        budget_ms,
    };
    for iteration in 0..iterations {
        println!("{}", sample(&config, iteration, &mut worker));
    }
    Ok(())
}
