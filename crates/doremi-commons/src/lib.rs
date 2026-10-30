//! Explicit-ID, credential-free experiment. Download policy is an allowlist of
//! two audited CC0 tones, not a general claim of rights over search results.
pub mod http;
use doremi_core::{
    catalog::{AudioFormat, LeaseBasis, ProviderError, StreamInfo, TemporaryResource, Tick},
    media::{Media, MediaId, ProviderId},
};
pub use reqwest::Client;
use reqwest::Url;
use serde_json::Value;
use sha1::{Digest, Sha1};

pub const PROVIDER: &str = "wikimedia-commons";
// Only these two author's CC0 sine recordings may be downloaded in this probe.
// The audited identities/hashes and primary permission links are in corpus.json.
const AUDITED: [(&str, u64, &str); 2] = [
    (
        "21923157",
        19761,
        "a126f9b23840b957463cef725bb2ceb4c5f7bcc1",
    ),
    (
        "21923199",
        23824,
        "302d5a0ef283ad99b8ca616c0038cbfaf72621dd",
    ),
];
pub fn audited(id: &str) -> Option<(u64, &'static str)> {
    AUDITED.iter().find(|v| v.0 == id).map(|v| (v.1, v.2))
}
pub fn audited_ids() -> impl Iterator<Item = &'static str> {
    AUDITED.iter().map(|v| v.0)
}
/// Decoded duration of the two fixed, hash-audited recordings, verified live.
pub fn audited_duration(id: &str) -> Option<u64> {
    audited(id).map(|_| 10_000)
}
pub struct Info {
    pub media: Media,
    pub title: String,
    pub uri: String,
    pub byte_len: u64,
    pub sha1: String,
}
fn licensed(info: &Value) -> bool {
    let metadata = &info["extmetadata"];
    metadata["LicenseShortName"]["value"] == "CC0"
        && metadata["Credit"]["value"]
            .as_str()
            .is_some_and(|v| v.contains("Own work"))
        && metadata["LicenseUrl"]["value"]
            .as_str()
            .and_then(|v| Url::parse(v).ok())
            .is_some_and(|u| {
                u.host_str() == Some("creativecommons.org")
                    && u.path().starts_with("/publicdomain/zero/1.0/")
            })
}
fn parse(page: &Value) -> Result<Info, ProviderError> {
    let id = page["pageid"]
        .as_u64()
        .ok_or(ProviderError::InvalidResponse)?
        .to_string();
    let title = page["title"]
        .as_str()
        .filter(|v| v.starts_with("File:") && v.len() <= 4096)
        .ok_or(ProviderError::InvalidResponse)?
        .to_owned();
    let info = page
        .get("imageinfo")
        .or_else(|| page.get("videoinfo"))
        .and_then(|v| v.as_array())
        .and_then(|v| v.first())
        .ok_or(ProviderError::Unavailable)?;
    if !licensed(info) {
        return Err(ProviderError::AccessDenied);
    }
    let uri = info["url"]
        .as_str()
        .ok_or(ProviderError::InvalidResponse)?
        .to_owned();
    if !http::allowed(&Url::parse(&uri).map_err(|_| ProviderError::InvalidResponse)?) {
        return Err(ProviderError::AccessDenied);
    }
    let sha1 = info["sha1"]
        .as_str()
        .ok_or(ProviderError::InvalidResponse)?
        .to_owned();
    let byte_len = info["size"]
        .as_u64()
        .ok_or(ProviderError::InvalidResponse)?;
    let duration_ms = audited_duration(&id);
    Ok(Info {
        media: Media {
            id: MediaId::new(
                ProviderId::new(PROVIDER).map_err(|_| ProviderError::InvalidResponse)?,
                id,
            )
            .map_err(|_| ProviderError::InvalidResponse)?,
            title: title.clone(),
            duration_ms,
        },
        title,
        uri,
        sha1,
        byte_len,
    })
}
async fn query(
    client: &Client,
    selector: &str,
    value: &str,
    limit: u16,
) -> Result<Value, ProviderError> {
    let mut params = vec![
        ("action", "query"),
        ("format", "json"),
        ("formatversion", "2"),
        ("prop", "imageinfo"),
        ("iiprop", "url|size|sha1|extmetadata"),
        ("iiextmetadatafilter", "LicenseShortName|LicenseUrl|Credit"),
        ("iiextmetadatalanguage", "en"),
        (selector, value),
    ];
    let limit = limit.to_string();
    if selector == "gsrsearch" {
        params.extend([
            ("generator", "search"),
            ("gsrnamespace", "6"),
            ("gsrlimit", &limit),
        ]);
    }
    let response = client
        .get("https://commons.wikimedia.org/w/api.php")
        .query(&params)
        .send()
        .await
        .map_err(http::error)?;
    let json: Value = serde_json::from_slice(&http::bounded(response, http::JSON_LIMIT).await?)
        .map_err(|_| ProviderError::InvalidResponse)?;
    if !json["error"].is_null() {
        return Err(ProviderError::InvalidResponse);
    }
    Ok(json)
}
pub async fn search(
    client: &Client,
    query_text: &str,
    limit: u16,
) -> Result<Vec<Info>, ProviderError> {
    if limit == 0 || limit > 10 {
        return Err(ProviderError::InvalidRequest);
    }
    let text = format!("filetype:audio incategory:CC-Zero {query_text}");
    let json = query(client, "gsrsearch", &text, limit).await?;
    let Some(pages) = json["query"]["pages"].as_array() else {
        return Ok(vec![]);
    };
    if pages.len() > usize::from(limit) {
        return Err(ProviderError::TooLarge);
    }
    let mut found = Vec::new();
    for page in pages {
        match parse(page) {
            Ok(info) => found.push(info),
            Err(ProviderError::AccessDenied) => {}
            Err(error) => return Err(error),
        }
    }
    Ok(found)
}
pub async fn resolve(client: &Client, id: &MediaId) -> Result<Info, ProviderError> {
    if id.provider.as_str() != PROVIDER {
        return Err(ProviderError::InvalidRequest);
    }
    let (size, hash) = audited(id.local()).ok_or(ProviderError::AccessDenied)?;
    let json = query(client, "pageids", id.local(), 1).await?;
    let page = json["query"]["pages"]
        .as_array()
        .and_then(|v| v.first())
        .ok_or(ProviderError::Unavailable)?;
    let info = parse(page)?;
    if info.media.id != *id || info.byte_len != size || info.sha1 != hash {
        return Err(ProviderError::Unavailable);
    }
    Ok(info)
}
pub fn temporary(info: &Info, now: Tick) -> Result<TemporaryResource, ProviderError> {
    TemporaryResource::http(
        info.media.id.clone(),
        info.uri.clone(),
        vec![],
        StreamInfo {
            format: AudioFormat::OggVorbis,
            byte_len: Some(info.byte_len),
            seekable: false,
            usable_until: Tick(now.0.checked_add(30_000).ok_or(ProviderError::Exhausted)?),
            lease_basis: LeaseBasis::ConservativeHostLimit,
        },
    )
}
/// Authorization is independent of transport success and of UI filtering.
pub fn verify_audio(id: &MediaId, bytes: &[u8]) -> Result<(), ProviderError> {
    if id.provider.as_str() != PROVIDER {
        return Err(ProviderError::InvalidRequest);
    }
    let (size, hash) = audited(id.local()).ok_or(ProviderError::AccessDenied)?;
    if bytes.len() as u64 != size || format!("{:x}", Sha1::digest(bytes)) != hash {
        return Err(ProviderError::InvalidResponse);
    }
    Ok(())
}
/// Caller must also discard completions superseded/cancelled during this await.
/// Dropping this future aborts pending HTTP IO; no decoder/output runs here.
pub async fn download(
    client: &Client,
    resource: &TemporaryResource,
    now: Tick,
) -> Result<Vec<u8>, ProviderError> {
    if audited(resource.media().local()).is_none()
        || resource.media().provider.as_str() != PROVIDER
        || resource.expose_headers().next().is_some()
        || !http::allowed(
            &Url::parse(resource.expose_uri()).map_err(|_| ProviderError::InvalidRequest)?,
        )
    {
        return Err(ProviderError::AccessDenied);
    }
    if now.0 >= resource.info().usable_until.0 {
        return Err(ProviderError::Unavailable);
    }
    if resource.info().format != AudioFormat::OggVorbis {
        return Err(ProviderError::UnsupportedFormat);
    }
    if resource.info().byte_len != audited(resource.media().local()).map(|v| v.0) {
        return Err(ProviderError::InvalidResponse);
    }
    let response = client
        .get(resource.expose_uri())
        .send()
        .await
        .map_err(http::error)?;
    let bytes = http::bounded(response, http::AUDIO_LIMIT).await?;
    verify_audio(resource.media(), &bytes)?;
    Ok(bytes)
}
pub fn page_url(info: &Info) -> Result<String, ProviderError> {
    let mut uri = Url::parse("https://commons.wikimedia.org/wiki/")
        .map_err(|_| ProviderError::InvalidRequest)?;
    uri.path_segments_mut()
        .map_err(|_| ProviderError::InvalidRequest)?
        .pop_if_empty()
        .push(&info.title);
    Ok(uri.into())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn license_is_explicit_not_inferred_from_title_or_category() {
        let mut data = serde_json::json!({"extmetadata":{"LicenseShortName":{"value":"CC0"},"Credit":{"value":"Own work"},"LicenseUrl":{"value":"https://creativecommons.org/publicdomain/zero/1.0/deed.en"}}});
        assert!(licensed(&data));
        data["extmetadata"]["LicenseShortName"]["value"] = "CC-BY-SA".into();
        assert!(!licensed(&data));
        data["extmetadata"]["LicenseShortName"]["value"] = "CC0".into();
        data["extmetadata"]["Credit"]["value"] = "Unknown".into();
        assert!(!licensed(&data));
        assert!(audited("21923157").is_some());
        assert!(audited("anything-else").is_none());
    }
    #[test]
    fn successful_transport_cannot_authorize_unknown_or_modified_content() {
        let provider = ProviderId::new(PROVIDER).unwrap();
        let unauthorized = MediaId::new(provider.clone(), "unknown").unwrap();
        assert_eq!(
            verify_audio(&unauthorized, b"OggS"),
            Err(ProviderError::AccessDenied)
        );
        let known = MediaId::new(provider, "21923157").unwrap();
        assert_eq!(
            verify_audio(&known, &vec![0; 19761]),
            Err(ProviderError::InvalidResponse)
        );
        let foreign = MediaId::new(ProviderId::new("other").unwrap(), "21923157").unwrap();
        assert_eq!(
            verify_audio(&foreign, b"OggS"),
            Err(ProviderError::InvalidRequest)
        );
    }
}
