//! Credential-free Commons audio: explicit CC0/Own-work metadata, a bounded
//! Vorbis subset, and freshly resolved identities. No inferred playback rights.
pub mod http;
pub mod progressive;
mod stream_metadata;
use doremi_core::{
    catalog::{AudioFormat, LeaseBasis, ProviderError, StreamInfo, TemporaryResource, Tick},
    media::{Media, MediaId, ProviderId},
};
pub use reqwest::Client;
use reqwest::Url;
use serde_json::Value;
use sha1::{Digest, Sha1};
pub use stream_metadata::StreamGrant;

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
    pub stream: Option<StreamGrant>,
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
        .filter(|v| v.starts_with("File:") && v.len() <= 1024 && !v.chars().any(char::is_control))
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
    let music = id == progressive::ID;
    let duration_ms = if music {
        Some(progressive::FRAMES * 1000 / u64::from(progressive::RATE))
    } else {
        audited_duration(&id)
    };
    let mut result = Info {
        media: Media {
            id: MediaId::new(
                ProviderId::new(PROVIDER).map_err(|_| ProviderError::InvalidResponse)?,
                id,
            )
            .map_err(|_| ProviderError::InvalidResponse)?,
            title: if music {
                progressive::TITLE.to_owned()
            } else {
                title.clone()
            },
            duration_ms,
        },
        title: title.clone(),
        uri,
        sha1,
        byte_len,
        stream: None,
    };
    if audited(result.media.id.local()).is_none() {
        if !matches!(info["mime"].as_str(), Some("application/ogg" | "audio/ogg"))
            || info["mediatype"] != "AUDIO"
        {
            return Err(ProviderError::UnsupportedFormat);
        }
        let grant = StreamGrant::parse(&result, &info["metadata"])?;
        result.media.duration_ms = Some(grant.frames() * 1000 / u64::from(grant.rate()));
        if !music {
            let name = title.strip_prefix("File:").unwrap_or(&title);
            let fallback = name
                .strip_suffix(".ogg")
                .or_else(|| name.strip_suffix(".oga"))
                .unwrap_or(name)
                .replace('_', " ");
            result.media.title = grant.title.clone().unwrap_or(fallback);
        }
        result.stream = Some(grant);
    }
    Ok(result)
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
        (
            "iiprop",
            "url|size|sha1|mime|mediatype|metadata|extmetadata",
        ),
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
    let words = query_text.to_lowercase();
    let title = "chopin nocturno nocturne op opus 9 2 membeth es dur";
    let mut found = Vec::new();
    if !words.trim().is_empty() && words.split_whitespace().all(|word| title.contains(word)) {
        let id = MediaId::new(ProviderId::new(PROVIDER).unwrap(), progressive::ID).unwrap();
        found.push(resolve(client, &id).await?);
    }
    let remaining = limit - found.len() as u16;
    if remaining == 0 {
        return Ok(found);
    }
    let text =
        format!("filetype:audio filemime:ogg filesize:<32768 incategory:CC-Zero {query_text}");
    let json = query(client, "gsrsearch", &text, remaining).await?;
    let Some(pages) = json["query"]["pages"].as_array() else {
        return Ok(found);
    };
    if pages.len() > usize::from(remaining) {
        return Err(ProviderError::TooLarge);
    }
    let mut ordered: Vec<_> = pages.iter().collect();
    ordered.sort_by_key(|page| page["index"].as_u64().unwrap_or(u64::MAX));
    for page in ordered {
        match parse(page) {
            Ok(info) => {
                if !found.iter().any(|row| row.media.id == info.media.id) {
                    found.push(info)
                }
            }
            Err(
                ProviderError::AccessDenied
                | ProviderError::UnsupportedFormat
                | ProviderError::Unavailable,
            ) => {}
            Err(error) => return Err(error),
        }
    }
    Ok(found)
}
pub async fn resolve(client: &Client, id: &MediaId) -> Result<Info, ProviderError> {
    if id.provider.as_str() != PROVIDER || !candidate(id) {
        return Err(ProviderError::InvalidRequest);
    }
    let json = query(client, "pageids", id.local(), 1).await?;
    let page = json["query"]["pages"]
        .as_array()
        .and_then(|v| v.first())
        .ok_or(ProviderError::Unavailable)?;
    let info = parse(page)?;
    if info.media.id != *id {
        return Err(ProviderError::Unavailable);
    }
    if let Some((size, hash)) = audited(id.local())
        .or_else(|| progressive::candidate(id).then_some((progressive::SIZE, progressive::HASH)))
        && (info.byte_len != size || info.sha1 != hash)
    {
        return Err(ProviderError::Unavailable);
    }
    Ok(info)
}
/// Routing only: every load still requires fresh provider metadata and a grant.
pub fn candidate(id: &MediaId) -> bool {
    id.provider.as_str() == PROVIDER
        && id
            .local()
            .parse::<u64>()
            .is_ok_and(|value| value > 0 && value.to_string() == id.local())
}
pub fn temporary(info: &Info, now: Tick) -> Result<TemporaryResource, ProviderError> {
    TemporaryResource::http(
        info.media.id.clone(),
        info.uri.clone(),
        vec![],
        StreamInfo {
            format: AudioFormat::OggVorbis,
            byte_len: Some(info.byte_len),
            seekable: info.stream.is_some(),
            usable_until: Tick(
                now.0
                    .checked_add(if info.stream.is_some() {
                        600_000
                    } else {
                        30_000
                    })
                    .ok_or(ProviderError::Exhausted)?,
            ),
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
    fn audio_page() -> Value {
        serde_json::json!({"pageid":123,"title":"File:Café_piano.ogg","imageinfo":[{
            "url":"https://upload.wikimedia.org/wikipedia/commons/a/a0/owned.ogg?opaque=retained",
            "size":400000,"sha1":"1111111111111111111111111111111111111111","mime":"application/ogg","mediatype":"AUDIO",
            "extmetadata":{"LicenseShortName":{"value":"CC0"},"Credit":{"value":"Own work"},"LicenseUrl":{"value":"https://creativecommons.org/publicdomain/zero/1.0/deed.en"}},
            "metadata":[{"name":"version","value":2},{"name":"streams","value":[{"name":1,"value":[
                {"name":"type","value":"Vorbis"},{"name":"group","value":0},{"name":"size","value":400000},
                {"name":"length","value":30.25},{"name":"header","value":[{"name":"vorbis_version","value":0},{"name":"audio_channels","value":2},{"name":"audio_sample_rate","value":48000}]}]}]}]
        }]})
    }
    #[test]
    fn public_audio_requires_explicit_license_and_supported_provider_metadata() {
        let page = audio_page();
        let info = parse(&page).unwrap();
        let grant = info.stream.as_ref().unwrap();
        assert_eq!(
            (grant.rate(), grant.channels(), grant.frames()),
            (48000, 2, 1452000)
        );
        assert_eq!(info.media.title, "Café piano");
        assert_eq!(info.media.duration_ms, Some(30250));
        assert!(info.uri.ends_with("?opaque=retained"));
        let mut tagged = page.clone();
        tagged.pointer_mut("/imageinfo/0/metadata/1/value/0/value").unwrap().as_array_mut().unwrap()
            .push(serde_json::json!({"name":"comments","value":[{"name":"TITLE","value":"  Música del autor  "}]}));
        assert_eq!(parse(&tagged).unwrap().media.title, "Música del autor");
        for (pointer, value) in [
            (
                "/imageinfo/0/extmetadata/LicenseShortName/value",
                Value::from("CC-BY"),
            ),
            (
                "/imageinfo/0/extmetadata/Credit/value",
                Value::from("unknown"),
            ),
            ("/imageinfo/0/mime", Value::from("audio/mpeg")),
            (
                "/imageinfo/0/metadata/1/value/0/value/0/value",
                Value::from("Opus"),
            ),
            (
                "/imageinfo/0/metadata/1/value/0/value/3/value",
                Value::from(601),
            ),
            (
                "/imageinfo/0/metadata/1/value/0/value/4/value/1/value",
                Value::from(6),
            ),
            ("/imageinfo/0/size", Value::from(40_000_000)),
            ("/imageinfo/0/sha1", Value::from("bad")),
            (
                "/imageinfo/0/url",
                Value::from("https://example.org/audio.ogg"),
            ),
        ] {
            let mut invalid = page.clone();
            *invalid.pointer_mut(pointer).unwrap() = value;
            assert!(parse(&invalid).is_err(), "Accepted {pointer}");
        }
        let mut chained = page.clone();
        let streams = chained
            .pointer_mut("/imageinfo/0/metadata/1/value")
            .unwrap()
            .as_array_mut()
            .unwrap();
        streams.push(streams[0].clone());
        assert!(parse(&chained).is_err());
    }
    #[test]
    fn a_stream_grant_cannot_authorize_a_different_id_uri_or_size() {
        let info = parse(&audio_page()).unwrap();
        for (media, uri, len) in [
            (
                MediaId::new(ProviderId::new(PROVIDER).unwrap(), "124").unwrap(),
                info.uri.clone(),
                info.byte_len,
            ),
            (
                info.media.id.clone(),
                "https://upload.wikimedia.org/other.ogg".into(),
                info.byte_len,
            ),
            (info.media.id.clone(), info.uri.clone(), info.byte_len + 1),
        ] {
            let resource = TemporaryResource::http(
                media,
                uri,
                vec![],
                StreamInfo {
                    format: AudioFormat::OggVorbis,
                    byte_len: Some(len),
                    seekable: true,
                    usable_until: Tick(1000),
                    lease_basis: LeaseBasis::ConservativeHostLimit,
                },
            )
            .unwrap();
            assert!(
                progressive::Reader::open_stream(
                    resource,
                    info.stream.clone().unwrap(),
                    Tick(0),
                    || false,
                    Default::default()
                )
                .is_err()
            );
        }
        let resource = temporary(&info, Tick(0)).unwrap();
        let mut expired = info.stream.unwrap();
        expired.expires = std::time::Instant::now() - std::time::Duration::from_secs(1);
        assert!(
            progressive::Reader::open_stream(
                resource,
                expired,
                Tick(0),
                || false,
                Default::default()
            )
            .is_err()
        );
    }
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
