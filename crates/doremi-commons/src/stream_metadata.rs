//! Provider-issued metadata for the supported progressive audio subset.
//! A grant cannot be constructed by a host or restored from a library document.
use crate::{Info, http};
use doremi_core::{catalog::ProviderError, media::MediaId};
use serde_json::Value;

pub const MAX_BYTES: u64 = 32 * 1024 * 1024;
#[derive(Clone)]
pub struct StreamGrant {
    pub(crate) media: MediaId,
    pub(crate) uri: String,
    pub(crate) bytes: u64,
    pub(crate) hash: String,
    pub(crate) expires: std::time::Instant,
    pub(crate) title: Option<String>,
    rate: u32,
    channels: usize,
    frames: u64,
}
impl StreamGrant {
    pub fn rate(&self) -> u32 {
        self.rate
    }
    pub fn channels(&self) -> usize {
        self.channels
    }
    pub fn frames(&self) -> u64 {
        self.frames
    }
    pub fn byte_len(&self) -> u64 {
        self.bytes
    }
    pub(crate) fn parse(info: &Info, metadata: &Value) -> Result<Self, ProviderError> {
        let invalid = ProviderError::UnsupportedFormat;
        if !(1..=MAX_BYTES).contains(&info.byte_len)
            || info.sha1.len() != 40
            || !info.sha1.bytes().all(|c| c.is_ascii_hexdigit())
        {
            return Err(invalid);
        }
        if field(metadata, "version")?.as_u64() != Some(2) {
            return Err(invalid);
        }
        let streams = field(metadata, "streams")?.as_array().ok_or(invalid)?;
        if streams.len() != 1 {
            return Err(invalid);
        }
        let stream = &streams[0]["value"];
        if field(stream, "type")? != "Vorbis"
            || field(stream, "group")?.as_u64() != Some(0)
            || field(stream, "size")?.as_u64() != Some(info.byte_len)
        {
            return Err(invalid);
        }
        let seconds = field(stream, "length")?.as_f64().ok_or(invalid)?;
        if !seconds.is_finite() || seconds <= 0.0 || seconds > 600.0 {
            return Err(invalid);
        }
        let header = field(stream, "header")?;
        if field(header, "vorbis_version")?.as_u64() != Some(0) {
            return Err(invalid);
        }
        let rate = field(header, "audio_sample_rate")?
            .as_u64()
            .ok_or(invalid)?;
        let channels = field(header, "audio_channels")?.as_u64().ok_or(invalid)?;
        if !(8000..=192000).contains(&rate) || !(1..=2).contains(&channels) {
            return Err(invalid);
        }
        let frames = (seconds * rate as f64).round() as u64;
        if frames == 0 || frames > rate * 600 {
            return Err(invalid);
        }
        let uri = reqwest::Url::parse(&info.uri).map_err(|_| ProviderError::InvalidResponse)?;
        if !http::allowed(&uri)
            || uri.host_str() != Some("upload.wikimedia.org")
            || uri.fragment().is_some()
        {
            return Err(ProviderError::AccessDenied);
        }
        let title = field(stream, "comments")
            .ok()
            .and_then(|comments| field(comments, "TITLE").ok())
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| {
                !value.is_empty() && value.len() <= 1024 && !value.chars().any(char::is_control)
            })
            .map(str::to_owned);
        Ok(Self {
            media: info.media.id.clone(),
            uri: info.uri.clone(),
            bytes: info.byte_len,
            hash: info.sha1.to_ascii_lowercase(),
            expires: std::time::Instant::now() + std::time::Duration::from_secs(600),
            title,
            rate: rate as u32,
            channels: channels as usize,
            frames,
        })
    }
}
fn field<'a>(data: &'a Value, name: &str) -> Result<&'a Value, ProviderError> {
    let values = data.as_array().ok_or(ProviderError::UnsupportedFormat)?;
    let mut matches = values.iter().filter(|item| item["name"] == name);
    let value = matches
        .next()
        .and_then(|item| item.get("value"))
        .ok_or(ProviderError::UnsupportedFormat)?;
    if matches.next().is_some() {
        return Err(ProviderError::InvalidResponse);
    }
    Ok(value)
}
