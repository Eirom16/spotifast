use crate::http;
use doremi_core::{
    catalog::ProviderError,
    media::{Media, MediaId, ProviderId},
};
use reqwest::{Url, blocking::Client};
use serde_json::Value;

pub const PROVIDER: &str = "wikimedia-commons";
// Only these two author's CC0 sine recordings may be downloaded in this probe.
// The audited identities/hashes and primary permission links are in corpus.json.
pub fn audited(id: &str) -> Option<(u64, &'static str)> {
    match id {
        "21923157" => Some((19761, "a126f9b23840b957463cef725bb2ceb4c5f7bcc1")),
        "21923199" => Some((23824, "302d5a0ef283ad99b8ca616c0038cbfaf72621dd")),
        _ => None,
    }
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
    Ok(Info {
        media: Media {
            id: MediaId::new(
                ProviderId::new(PROVIDER).map_err(|_| ProviderError::InvalidResponse)?,
                id,
            )
            .map_err(|_| ProviderError::InvalidResponse)?,
            title: title.clone(),
            duration_ms: None,
        },
        title,
        uri,
        sha1,
        byte_len,
    })
}
fn query(client: &Client, selector: &str, value: &str, limit: u16) -> Result<Value, ProviderError> {
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
        .map_err(http::error)?;
    let json: Value = serde_json::from_slice(&http::bounded(response, http::JSON_LIMIT)?)
        .map_err(|_| ProviderError::InvalidResponse)?;
    if !json["error"].is_null() {
        return Err(ProviderError::InvalidResponse);
    }
    Ok(json)
}
pub fn search(client: &Client, query_text: &str, limit: u16) -> Result<Vec<Info>, ProviderError> {
    if limit == 0 || limit > 10 {
        return Err(ProviderError::InvalidRequest);
    }
    let text = format!("filetype:audio incategory:CC-Zero {query_text}");
    let json = query(client, "gsrsearch", &text, limit)?;
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
pub fn resolve(client: &Client, id: &MediaId) -> Result<Info, ProviderError> {
    if id.provider.as_str() != PROVIDER {
        return Err(ProviderError::InvalidRequest);
    }
    let (size, hash) = audited(id.local()).ok_or(ProviderError::AccessDenied)?;
    let json = query(client, "pageids", id.local(), 1)?;
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
}
