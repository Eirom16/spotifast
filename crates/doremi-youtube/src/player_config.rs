//! Public player initialization. Read a numeric protocol version from Google's
//! published player, without executing JavaScript or sending the user's session.
use crate::{Client, Error};
use reqwest::Client as Http;
use std::time::{Duration, Instant};

const ORIGIN: &str = "https://www.youtube.com";
const IFRAME_LIMIT: usize = 64 * 1024;
const CACHE_TTL: Duration = Duration::from_secs(6 * 3600);

pub(crate) struct PlayerConfig {
    pub(crate) timestamp: u32,
    pub(crate) loaded: Instant,
}

impl Client {
    pub(crate) async fn signature_timestamp(&mut self) -> Result<u32, Error> {
        if let Some(config) = &self.player_config
            && config.loaded.elapsed() < CACHE_TTL
        {
            return Ok(config.timestamp);
        }
        let timestamp = load(&self.http, ORIGIN).await?;
        self.player_config = Some(PlayerConfig {
            timestamp,
            loaded: Instant::now(),
        });
        Ok(timestamp)
    }
}

async fn load(http: &Http, origin: &str) -> Result<u32, Error> {
    // Only the eight hex digits are taken from the public response. Neither a
    // returned URL nor a redirect can select a host, path or query for this GET.
    let iframe = resource(http, &format!("{origin}/iframe_api"), IFRAME_LIMIT).await?;
    let version = player_version(&iframe).ok_or(Error::InvalidResponse)?;
    let script = resource(
        http,
        &format!("{origin}/s/player/{version}/player_ias.vflset/en_US/base.js"),
        crate::MAX_BYTES,
    )
    .await?;
    timestamp(&script).ok_or(Error::InvalidResponse)
}

async fn resource(http: &Http, url: &str, limit: usize) -> Result<String, Error> {
    // Deliberately use the public HTTP client directly, never authenticate().
    let mut response = http.get(url).send().await.map_err(crate::network_error)?;
    match response.status().as_u16() {
        200 => {}
        401 | 403 => return Err(Error::AccessDenied),
        429 => {
            return Err(Error::RateLimited {
                retry_after_ms: Some(30_000),
            });
        }
        _ => return Err(Error::Unavailable),
    }
    if response.content_length().is_some_and(|n| n > limit as u64) {
        return Err(Error::TooLarge);
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(crate::network_error)? {
        if chunk.len() > limit.saturating_sub(bytes.len()) {
            return Err(Error::TooLarge);
        }
        bytes.extend_from_slice(&chunk);
    }
    String::from_utf8(bytes).map_err(|_| Error::InvalidResponse)
}

fn player_version(source: &str) -> Option<String> {
    // The iframe API can quote slashes as \/ in its widget script path.
    let source = source.replace("\\/", "/");
    source
        .match_indices("/s/player/")
        .find_map(|(start, prefix)| {
            let tail = &source[start + prefix.len()..];
            let version = tail.get(..8)?;
            (version.bytes().all(|b| b.is_ascii_hexdigit())
                && tail.as_bytes().get(8) == Some(&b'/'))
            .then(|| version.to_owned())
        })
}

fn timestamp(source: &str) -> Option<u32> {
    for key in ["signatureTimestamp", "sts"] {
        for (start, _) in source.match_indices(key) {
            // Do not match substrings in unrelated identifiers.
            if source
                .as_bytes()
                .get(start.wrapping_sub(1))
                .is_some_and(|b| b.is_ascii_alphanumeric() || b"_$".contains(b))
            {
                continue;
            }
            let tail = source[start + key.len()..]
                .strip_prefix('"')
                .unwrap_or(&source[start + key.len()..]);
            let Some(tail) = tail.trim_start().strip_prefix(':') else {
                continue;
            };
            let tail = tail.trim_start();
            let Some(number) = tail.get(..5) else {
                continue;
            };
            if number.bytes().all(|b| b.is_ascii_digit())
                && tail
                    .as_bytes()
                    .get(5)
                    .is_none_or(|b| b.is_ascii_whitespace() || b",}]".contains(b))
                && let Ok(value @ 10000..=99999) = number.parse()
            {
                return Some(value);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol_tests::raw_server;

    #[test]
    fn parses_only_fixed_player_ids_and_numeric_protocol_versions() {
        assert_eq!(
            player_version(r#"https:\/\/www.youtube.com\/s\/player\/a5c2aafe\/www-widgetapi.js"#)
                .as_deref(),
            Some("a5c2aafe")
        );
        for invalid in [
            "/s/player/../base.js",
            "/s/player/a5c2aafe9/base.js",
            "/s/player/evil.com/base.js",
            "/s/player/a5c2aafe?url=other/",
        ] {
            assert!(player_version(invalid).is_none());
        }
        assert_eq!(
            timestamp(r#"x={signatureTimestamp:20760,other:1}"#),
            Some(20760)
        );
        assert_eq!(timestamp(r#"{"sts": 20761}"#), Some(20761));
        for invalid in [
            "signatureTimestamp:207600,",
            "not_signatureTimestamp:20760,",
            "signatureTimestamp:-20760,",
            "signatureTimestamp:20760evil,",
            "sts:NaN",
            "sts:00000,",
        ] {
            assert_eq!(timestamp(invalid), None);
        }
    }

    #[tokio::test]
    async fn public_bootstrap_never_sends_session_or_follows_a_returned_url() {
        let (base, server) = raw_server(vec![
            (
                200,
                r#"https://untrusted.invalid/s/player/a5c2aafe/widget.js"#.into(),
            ),
            (200, "const player={signatureTimestamp:20760};".into()),
        ])
        .await;
        let mut client = Client::new().unwrap();
        client.session = Some(crate::session::Session::parse("SAPISID=fixture-only").unwrap());
        assert_eq!(
            load(&client.http, base.trim_end_matches('/'))
                .await
                .unwrap(),
            20760
        );
        let requests = server.await.unwrap();
        assert!(requests[0].starts_with("GET /iframe_api "));
        assert!(requests[1].starts_with("GET /s/player/a5c2aafe/player_ias.vflset/en_US/base.js "));
        for request in requests {
            let headers = request.to_ascii_lowercase();
            assert!(!headers.contains("cookie:") && !headers.contains("authorization:"));
            assert!(!headers.contains("fixture-only") && !headers.contains("untrusted.invalid"));
        }
    }

    #[tokio::test]
    async fn bootstrap_refusals_invalid_data_and_size_limits_stop_the_request_chain() {
        for (status, body, expected) in [
            (403, String::new(), Error::AccessDenied),
            (302, String::new(), Error::Unavailable),
            (200, "no player config".into(), Error::InvalidResponse),
            (200, "x".repeat(IFRAME_LIMIT + 1), Error::TooLarge),
        ] {
            let (base, server) = raw_server(vec![(status, body)]).await;
            let client = Client::new().unwrap();
            assert_eq!(
                load(&client.http, base.trim_end_matches('/')).await,
                Err(expected)
            );
            assert_eq!(server.await.unwrap().len(), 1);
        }
        let (base, server) = raw_server(vec![
            (200, "/s/player/a5c2aafe/widget.js".into()),
            (200, "x".repeat(crate::MAX_BYTES + 1)),
        ])
        .await;
        assert_eq!(
            load(&Client::new().unwrap().http, base.trim_end_matches('/')).await,
            Err(Error::TooLarge)
        );
        assert_eq!(server.await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn cached_public_config_does_not_require_another_request_or_account() {
        let mut client = Client::new().unwrap();
        client.player_config = Some(PlayerConfig {
            timestamp: 20760,
            loaded: Instant::now(),
        });
        assert_eq!(client.signature_timestamp().await.unwrap(), 20760);
        client.clear_session();
        assert_eq!(client.signature_timestamp().await.unwrap(), 20760);
    }
}
