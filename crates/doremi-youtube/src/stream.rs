//! Single-attempt native media resolution and bounded HTTP range input.
//! Uses Tubefast's pinned primary player client, without its client rotation,
//! visitor retries, signature deciphering or content-check overrides.
use crate::{Client, Error, PROVIDER, authorization, id};
use doremi_core::media::MediaId;
use reqwest::{Url, redirect::Policy};
use serde_json::{Value, json};
use std::{
    io::{self, Read, Seek, SeekFrom},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
const PLAYER: &str = "https://www.youtube.com/youtubei/v1/player?prettyPrint=false";
const UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 15_7_3) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/26.0 Safari/605.1.15";
const CHUNK: usize = 64 * 1024;
/// Ephemeral, non-serializable grant. Its transport URL is never a media ID.
pub struct Stream {
    url: String,
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
            .redirect(Policy::none())
            .connect_timeout(Duration::from_secs(4))
            .timeout(Duration::from_secs(6))
            .user_agent(UA)
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
}
impl Client {
    pub async fn resolve(&mut self, media: &MediaId) -> Result<Stream, Error> {
        if media.provider.as_str() != PROVIDER
            || media.local().len() != 11
            || id(media.local()).is_err()
        {
            return Err(Error::InvalidRequest);
        }
        let body = json!({"context":{"client":{"clientName":"VISIONOS","clientVersion":"1.02","deviceMake":"Apple","deviceModel":"RealityDevice17,1","osName":"visionOS","osVersion":"26.5.23O471","hl":"en"}},"videoId":media.local()});
        let mut request = self
            .http
            .post(PLAYER)
            .header("User-Agent", UA)
            .header("X-YouTube-Client-Name", "101")
            .header("X-YouTube-Client-Version", "1.02")
            .header("Origin", crate::ORIGIN)
            .json(&body);
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
                .header("X-Origin", crate::ORIGIN)
                .header("X-Goog-AuthUser", "0");
        }
        let mut response = request.send().await.map_err(crate::network_error)?;
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
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(crate::network_error)? {
            if chunk.len() > crate::MAX_BYTES.saturating_sub(bytes.len()) {
                return Err(Error::TooLarge);
            }
            bytes.extend_from_slice(&chunk);
        }
        parse(
            media,
            &serde_json::from_slice(&bytes).map_err(|_| Error::InvalidResponse)?,
        )
    }
}
fn parse(media: &MediaId, value: &Value) -> Result<Stream, Error> {
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
    let formats = value["streamingData"]["adaptiveFormats"]
        .as_array()
        .ok_or(Error::UnsupportedFormat)?;
    let format = formats
        .iter()
        .filter(|f| {
            f["mimeType"]
                .as_str()
                .is_some_and(|mime| mime.starts_with("audio/mp4;") && mime.contains("mp4a.40.2"))
        })
        .find(|f| {
            f["url"].is_string()
                && f["signatureCipher"].is_null()
                && f["drmFamilies"].as_array().is_none_or(|v| v.is_empty())
        })
        .ok_or(Error::UnsupportedFormat)?;
    let uri = Url::parse(format["url"].as_str().ok_or(Error::InvalidResponse)?)
        .map_err(|_| Error::InvalidResponse)?;
    let host = uri.host_str().ok_or(Error::InvalidResponse)?;
    if uri.scheme() != "https"
        || uri.port().is_some()
        || !uri.username().is_empty()
        || uri.password().is_some()
        || !(host == "googlevideo.com" || host.ends_with(".googlevideo.com"))
    {
        return Err(Error::AccessDenied);
    }
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
            .get(&self.grant.url)
            .header("Range", format!("bytes={start}-{end}"))
            .header("Accept-Encoding", "identity")
            .send()
            .map_err(|_| io::Error::other("YouTube audio transport failed"))?;
        self.check()?;
        match response.status().as_u16() {
            206 => {}
            401 | 403 => return Err(io::ErrorKind::PermissionDenied.into()),
            _ => return Err(io::Error::other("YouTube audio range unavailable")),
        }
        let expected = format!("bytes {start}-{end}/{}", self.grant.length);
        if response
            .headers()
            .get("content-range")
            .and_then(|v| v.to_str().ok())
            != Some(expected.as_str())
        {
            return Err(io::Error::other("YouTube audio range changed"));
        }
        let size = (end - start + 1) as usize;
        self.bytes.clear();
        response
            .by_ref()
            .take(size as u64 + 1)
            .read_to_end(&mut self.bytes)
            .map_err(|_| io::Error::other("YouTube audio read failed"))?;
        self.check()?;
        if self.bytes.len() != size {
            self.bytes.clear();
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        self.cached_at = start;
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
    fn response() -> Value {
        json!({"playabilityStatus":{"status":"OK"},"videoDetails":{"videoId":"abcdefghijk","lengthSeconds":"180"},"streamingData":{"expiresInSeconds":"600","adaptiveFormats":[{"mimeType":"audio/mp4; codecs=\"mp4a.40.2\"","contentLength":"100000","url":"https://r1.googlevideo.com/videoplayback?expire=123&token=fixture"}]}})
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
    fn ranges_start_playback_without_downloading_the_whole_track_and_seek_is_cancelable() {
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
                assert!(
                    String::from_utf8(request)
                        .unwrap()
                        .to_ascii_lowercase()
                        .contains(&format!("range: bytes={start}-{end}"))
                );
                use std::io::Write;
                write!(socket,"HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {start}-{end}/{length}\r\nConnection: close\r\n\r\n",end-start+1).unwrap();
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
