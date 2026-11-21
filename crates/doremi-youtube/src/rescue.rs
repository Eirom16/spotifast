//! Complete encoded audio before playback; no HTTP on decoder/audio threads.
//! Primary guest request and 1 MiB query ranges adapted from Tubefast's
//! src/ytm.rs and src/player.rs, MIT, Yiğit Bozyaka, revision
//! c0813fc0619e670096fc51a9d944a9ca4e4b2e42. See LICENSE-TUBEFAST.
//! No visitor replacement, client refresh or account fallback. Only transport
//! failures resume the same grant; any HTTP access refusal is terminal.
use crate::{Client, Error, PROVIDER, id, stream};
use doremi_core::media::MediaId;
use reqwest::Response;
use serde_json::{Value, json};
use std::time::Duration;

const CHUNK: u64 = 1024 * 1024;
pub const MAX_BYTES: u64 = 64 * 1024 * 1024;
const PLAYER: &str = "https://www.youtube.com/youtubei/v1/player?prettyPrint=false";
const VISITOR: &str = "https://www.youtube.com/youtubei/v1/visitor_id?prettyPrint=false";
const UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 15_7_3) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/26.0 Safari/605.1.15";

/// Ephemeral in-memory cache, owned by one playback generation. Never serialized.
pub struct Audio {
    pub bytes: Vec<u8>,
    pub duration_ms: u64,
}
fn status(response: &Response) -> Result<(), Error> {
    match response.status().as_u16() {
        200 | 206 => Ok(()),
        401 | 403 => Err(Error::AccessDenied),
        429 => Err(Error::RateLimited {
            retry_after_ms: Some(30_000),
        }),
        _ => Err(Error::Unavailable),
    }
}
async fn json_response(mut response: Response) -> Result<Value, Error> {
    status(&response)?;
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(transport_error)? {
        if chunk.len() > crate::MAX_BYTES.saturating_sub(bytes.len()) {
            return Err(Error::TooLarge);
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|_| Error::InvalidResponse)
}
async fn primary(client: &mut Client, media: &MediaId) -> Result<stream::Stream, Error> {
    // A visitor issued once for this attempt, as in the pinned reference.
    client.resolution_stage = "rescue-visitor";
    let value = json_response(client.http.post(VISITOR).json(&json!({
        "context":{"client":{"clientName":"WEB","clientVersion":"2.20260707.00.00","hl":"en"}}
    })).send().await.map_err(crate::network_error)?).await?;
    let visitor = value["responseContext"]["visitorData"]
        .as_str()
        .filter(|v| !v.is_empty() && v.len() <= 16384)
        .ok_or(Error::InvalidResponse)?;
    client.resolution_stage = "rescue-player";
    let response = client
        .http
        .post(PLAYER)
        .header("User-Agent", UA)
        .header("X-YouTube-Client-Name", "101")
        .header("X-YouTube-Client-Version", "1.02")
        .header("X-Goog-Visitor-Id", visitor)
        .json(&json!({"context":{"client":{
            "clientName":"VISIONOS","clientVersion":"1.02","deviceMake":"Apple",
            "deviceModel":"RealityDevice17,1","osName":"visionOS","osVersion":"26.5.23O471",
            "hl":"en","visitorData":visitor}},"videoId":media.local()
        }))
        .send()
        .await
        .map_err(crate::network_error)?;
    client.player_diagnostic = Some(stream::PlayerDiagnostic::http(response.status().as_u16()));
    let value = json_response(response).await?;
    client.player_diagnostic = Some(stream::PlayerDiagnostic::response(200, &value));
    stream::parse(media, &value)
}
/// Tubefast's primary public playback client is independent of the musical
/// catalog session, including when that catalog is signed in. Credentials are
/// never sent to this client. Private/account-only media can be refused; no
/// account/client fallback follows a refusal.
/// Dropping this future cancels the current request and releases all cached bytes.
pub async fn prepare(client: &mut Client, media: &MediaId) -> Result<Audio, Error> {
    client.player_diagnostic = None;
    client.resolution_stage = "rescue-identity";
    if media.provider.as_str() != PROVIDER || id(media.local()).is_err() {
        return Err(Error::InvalidRequest);
    }
    let grant = primary(client, media).await?;
    client.resolution_stage = "rescue-download";
    let bytes = download(&grant.url, grant.length, grant.user_agent).await?;
    client.resolution_stage = "rescue-ready";
    Ok(Audio {
        bytes,
        duration_ms: grant.duration_ms,
    })
}
struct Cancel(std::sync::Arc<std::sync::atomic::AtomicBool>);
impl Drop for Cancel {
    fn drop(&mut self) {
        self.0.store(true, std::sync::atomic::Ordering::Release);
    }
}
async fn download(url: &str, length: u64, user_agent: &str) -> Result<Vec<u8>, Error> {
    if !(1..=MAX_BYTES).contains(&length) {
        return Err(Error::TooLarge);
    }
    let cancel = Cancel(std::sync::Arc::new(std::sync::atomic::AtomicBool::new(
        false,
    )));
    let stopped = cancel.0.clone();
    let url = url.to_owned();
    let user_agent = user_agent.to_owned();
    tokio::task::spawn_blocking(move || {
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(10))
            .timeout_read(Duration::from_secs(30))
            .timeout_write(Duration::from_secs(30))
            .redirects(0)
            .user_agent(&user_agent)
            .build();
        let mut bytes = Vec::with_capacity(length as usize);
        let mut retries = 0;
        let mut stalled = 0;
        while (bytes.len() as u64) < length {
            if stopped.load(std::sync::atomic::Ordering::Acquire) {
                return Err(Error::Cancelled);
            }
            let before = bytes.len();
            match append_range(&agent, &url, length, &mut bytes, &stopped) {
                Ok(()) => stalled = 0,
                Err(Error::Transport | Error::Timeout) if retries < 32 => {
                    stalled = if bytes.len() > before { 0 } else { stalled + 1 };
                    if stalled > 3 {
                        return Err(Error::Transport);
                    }
                    retries += 1;
                    transfer_event(bytes.len() as u64, length - 1, "resume-transport");
                }
                Err(error) => return Err(error),
            }
        }
        Ok(bytes)
    })
    .await
    .map_err(|_| Error::Transport)?
}
fn append_range(
    agent: &ureq::Agent,
    url: &str,
    length: u64,
    bytes: &mut Vec<u8>,
    cancel: &std::sync::atomic::AtomicBool,
) -> Result<(), Error> {
    use std::io::Read;
    let from = bytes.len() as u64;
    let to = (from + CHUNK).min(length) - 1;
    let expected = (to - from + 1) as usize;
    let mut url = reqwest::Url::parse(url).map_err(|_| Error::InvalidResponse)?;
    url.query_pairs_mut()
        .append_pair("range", &format!("{from}-{to}"));
    transfer_event(from, to, "request");
    let mut redirects = 0;
    let response = loop {
        if cancel.load(std::sync::atomic::Ordering::Acquire) {
            return Err(Error::Cancelled);
        }
        let response = match agent
            .get(url.as_str())
            .set("Accept-Encoding", "identity")
            .call()
        {
            Ok(response) => response,
            Err(ureq::Error::Status(401 | 403, _)) => return Err(Error::AccessDenied),
            Err(ureq::Error::Status(429, _)) => {
                return Err(Error::RateLimited {
                    retry_after_ms: Some(30_000),
                });
            }
            Err(ureq::Error::Status(_, _)) => return Err(Error::Unavailable),
            Err(ureq::Error::Transport(_)) => return Err(Error::Transport),
        };
        match response.status() {
            200 | 206 => break response,
            301..=303 | 307 | 308 if redirects < 3 => {
                let next = url
                    .join(response.header("location").ok_or(Error::InvalidResponse)?)
                    .map_err(|_| Error::InvalidResponse)?;
                url = stream::media_url(next.as_str())?;
                redirects += 1;
            }
            _ => return Err(Error::Unavailable),
        }
    };
    if response
        .header("content-length")
        .is_some_and(|n| n.parse::<usize>().ok() != Some(expected))
    {
        return Err(Error::InvalidResponse);
    }
    if let Some(range) = response.header("content-range")
        && range != format!("bytes {from}-{to}/{length}")
    {
        return Err(Error::InvalidResponse);
    }
    let end = bytes.len() + expected;
    let mut body = response.into_reader();
    let mut block = [0; 16 * 1024];
    while bytes.len() < end {
        if cancel.load(std::sync::atomic::Ordering::Acquire) {
            return Err(Error::Cancelled);
        }
        let budget = block.len().min(end - bytes.len());
        let read = body
            .read(&mut block[..budget])
            .map_err(|_| Error::Transport)?;
        if read == 0 {
            return Err(Error::Transport);
        }
        bytes.extend_from_slice(&block[..read]);
    }
    transfer_event(from, to, "complete");
    Ok(())
}

fn transport_error(error: reqwest::Error) -> Error {
    let category = if error.is_timeout() {
        "timeout"
    } else if error.is_connect() {
        "connect"
    } else if error.is_body() {
        "body"
    } else {
        "request"
    };
    #[cfg(target_os = "android")]
    log::warn!("YTM buffered transport category={category}");
    #[cfg(not(target_os = "android"))]
    eprintln!("YTM buffered transport category={category}");
    crate::network_error(error)
}

fn transfer_event(from: u64, to: u64, stage: &'static str) {
    #[cfg(target_os = "android")]
    log::info!("YTM buffered range from={from} to={to} stage={stage}");
    #[cfg(not(target_os = "android"))]
    eprintln!("YTM buffered range from={from} to={to} stage={stage}");
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };
    #[tokio::test]
    async fn full_cache_checks_ranges_and_never_retries_a_refusal() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            for (from, size) in [(0, CHUNK as usize), (CHUNK, 7)] {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut request = vec![0; 8192];
                let read = socket.read(&mut request).await.unwrap();
                let request = String::from_utf8_lossy(&request[..read]);
                assert!(request.contains(&format!("range={from}-{}", from + size as u64 - 1)));
                socket
                    .write_all(
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {size}\r\nConnection: close\r\n\r\n"
                        )
                        .as_bytes(),
                    )
                    .await
                    .unwrap();
                socket.write_all(&vec![42; size]).await.unwrap();
            }
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = [0; 8192];
            let received = socket.read(&mut request).await.unwrap();
            assert!(received > 0);
            socket
                .write_all(
                    b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                )
                .await
                .unwrap();
            assert!(
                tokio::time::timeout(Duration::from_millis(150), listener.accept())
                    .await
                    .is_err()
            );
        });
        let url = format!("http://{address}/media?sig=fixture");
        let bytes = download(&url, CHUNK + 7, UA).await.unwrap();
        assert_eq!(bytes, vec![42; CHUNK as usize + 7]);
        assert!(matches!(
            download(&url, 10, UA).await,
            Err(Error::AccessDenied)
        ));
        server.await.unwrap();
        assert!(matches!(
            download(&url, MAX_BYTES + 1, UA).await,
            Err(Error::TooLarge)
        ));
    }
    #[tokio::test]
    async fn broken_body_resumes_the_same_grant_at_the_received_offset() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            // Repeated short bodies still make progress. Three interruptions
            // across an entire file must not discard the already cached bytes.
            for (range, size, body) in [
                ("0-9", 10, "ab"),
                ("2-9", 8, "cd"),
                ("4-9", 6, "ef"),
                ("6-9", 4, "gh"),
                ("8-9", 2, "ij"),
            ] {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut request = [0; 8192];
                let read = socket.read(&mut request).await.unwrap();
                assert!(
                    String::from_utf8_lossy(&request[..read]).contains(&format!("range={range}"))
                );
                socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {size}\r\nConnection: close\r\n\r\n{body}").as_bytes()).await.unwrap();
            }
        });
        assert_eq!(
            download(&format!("http://{address}/media?sig=same-grant"), 10, UA)
                .await
                .unwrap(),
            b"abcdefghij"
        );
        server.await.unwrap();
    }
}
