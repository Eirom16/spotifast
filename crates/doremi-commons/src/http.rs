use doremi_core::catalog::ProviderError;
use reqwest::{Client, Response, Url, redirect::Policy};
use std::time::Duration;

pub const AGENT: &str = "DoremiResearchBot/0.1 (https://github.com/Eirom16/spotifast)";
pub const JSON_LIMIT: usize = 1024 * 1024;
pub const AUDIO_LIMIT: usize = 256 * 1024;

pub fn allowed(url: &Url) -> bool {
    url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url.port_or_known_default() == Some(443)
        && matches!(
            url.host_str(),
            Some("commons.wikimedia.org" | "upload.wikimedia.org")
        )
}
pub fn client() -> Result<Client, ProviderError> {
    Client::builder()
        .user_agent(AGENT)
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(8))
        .redirect(Policy::custom(|attempt| {
            if attempt.previous().len() < 3 && allowed(attempt.url()) {
                attempt.follow()
            } else {
                attempt.stop()
            }
        }))
        .build()
        .map_err(error)
}
pub fn error(error: reqwest::Error) -> ProviderError {
    if error.is_timeout() {
        ProviderError::Timeout
    } else {
        ProviderError::Transport
    }
}
pub fn status(response: &Response) -> Result<(), ProviderError> {
    match response.status().as_u16() {
        200 | 206 => Ok(()),
        401 | 403 => Err(ProviderError::AccessDenied),
        404 | 410 => Err(ProviderError::Unavailable),
        429 => Err(ProviderError::RateLimited {
            retry_after_ms: response
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok())
                .and_then(|v| v.checked_mul(1000)),
        }),
        300..=399 => Err(ProviderError::AccessDenied),
        _ => Err(ProviderError::Transport),
    }
}
pub async fn bounded(response: Response, limit: usize) -> Result<Vec<u8>, ProviderError> {
    status(&response)?;
    if response.content_length().is_some_and(|n| n > limit as u64) {
        return Err(ProviderError::TooLarge);
    }
    bounded_remaining(response, limit).await
}
/// Used after reading the first chunk; Content-Length describes the original
/// response, not its remaining body. The caller already checked that header.
pub async fn bounded_remaining(
    mut response: Response,
    limit: usize,
) -> Result<Vec<u8>, ProviderError> {
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(error)? {
        if chunk.len() > limit.saturating_sub(bytes.len()) {
            return Err(ProviderError::TooLarge);
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn transport_rejects_other_hosts_ports_credentials_and_protocols() {
        for uri in [
            "http://commons.wikimedia.org/",
            "https://commons.wikimedia.org:444/",
            "https://a:secret@commons.wikimedia.org/",
            "https://commons.wikimedia.org.evil.test/",
            "https://127.0.0.1/",
        ] {
            assert!(!allowed(&Url::parse(uri).unwrap()));
        }
        assert!(allowed(
            &Url::parse("https://upload.wikimedia.org/wikipedia/commons/a/a0/tone.ogg").unwrap()
        ));
    }
}
