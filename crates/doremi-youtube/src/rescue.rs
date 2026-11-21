//! Complete encoded audio before playback; no HTTP on decoder/audio threads.
//! Primary guest request and 1 MiB query ranges adapted from Tubefast's
//! src/ytm.rs and src/player.rs, MIT, Yiğit Bozyaka, revision
//! c0813fc0619e670096fc51a9d944a9ca4e4b2e42. See LICENSE-TUBEFAST.
//! No visitor replacement, client refresh, account fallback or access retries.
use crate::{Client, Error, PROVIDER, id, stream};
use doremi_core::media::MediaId;
use reqwest::{Client as Http, Response, redirect::Policy};
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
    while let Some(chunk) = response.chunk().await.map_err(crate::network_error)? {
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
    let http = Http::builder()
        .redirect(Policy::custom(|attempt| {
            if attempt.previous().len() > 3 || stream::media_url(attempt.url().as_str()).is_err() {
                attempt.stop()
            } else {
                attempt.follow()
            }
        }))
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
        .user_agent(grant.user_agent)
        .build()
        .map_err(crate::network_error)?;
    let bytes = download(&http, &grant.url, grant.length).await?;
    client.resolution_stage = "rescue-ready";
    Ok(Audio {
        bytes,
        duration_ms: grant.duration_ms,
    })
}
async fn download(http: &Http, url: &str, length: u64) -> Result<Vec<u8>, Error> {
    if !(1..=MAX_BYTES).contains(&length) {
        return Err(Error::TooLarge);
    }
    let mut bytes = Vec::with_capacity(length as usize);
    while (bytes.len() as u64) < length {
        let from = bytes.len() as u64;
        let to = (from + CHUNK).min(length) - 1;
        let expected = (to - from + 1) as usize;
        let mut url = reqwest::Url::parse(url).map_err(|_| Error::InvalidResponse)?;
        url.query_pairs_mut()
            .append_pair("range", &format!("{from}-{to}"));
        let mut response = http
            .get(url)
            .header("Accept-Encoding", "identity")
            .send()
            .await
            .map_err(crate::network_error)?;
        status(&response)?;
        if response
            .content_length()
            .is_some_and(|n| n != expected as u64)
        {
            return Err(Error::InvalidResponse);
        }
        if let Some(value) = response.headers().get("content-range") {
            let wanted = format!("bytes {from}-{to}/{length}");
            if value.to_str().ok() != Some(wanted.as_str()) {
                return Err(Error::InvalidResponse);
            }
        }
        let end = bytes.len() + expected;
        while let Some(chunk) = response.chunk().await.map_err(crate::network_error)? {
            if chunk.len() > end.saturating_sub(bytes.len()) {
                return Err(Error::TooLarge);
            }
            bytes.extend_from_slice(&chunk);
        }
        if bytes.len() != end {
            return Err(Error::Transport);
        }
    }
    Ok(bytes)
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
            socket.read(&mut request).await.unwrap();
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
        let http = Http::new();
        let url = format!("http://{address}/media?sig=fixture");
        let bytes = download(&http, &url, CHUNK + 7).await.unwrap();
        assert_eq!(bytes, vec![42; CHUNK as usize + 7]);
        assert!(matches!(
            download(&http, &url, 10).await,
            Err(Error::AccessDenied)
        ));
        server.await.unwrap();
        assert!(matches!(
            download(&http, &url, MAX_BYTES + 1).await,
            Err(Error::TooLarge)
        ));
    }
}
