//! App-owned saved metadata. Queue occurrences and transient resources are separate.
use doremi_core::media::{Media, MediaId, ProviderId};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub const ITEM_LIMIT: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    TooLarge,
    InvalidMedia,
    DuplicateIdentity,
    Encoding,
    UnsupportedSchema,
}

/// One saved entry per qualified media identity, in insertion order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Collection {
    items: Arc<Vec<Media>>,
}
impl Collection {
    pub fn items(&self) -> &[Media] {
        &self.items
    }
    pub fn contains(&self, id: &MediaId) -> bool {
        self.items.iter().any(|media| media.id == *id)
    }
    pub(crate) fn same_snapshot(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.items, &other.items)
    }
    /// Refresh metadata for an existing ID without creating another favorite.
    /// This does not deduplicate queue occurrences.
    pub fn save(&mut self, media: Media) -> Result<bool, Error> {
        validate(&media)?;
        if let Some(index) = self.items.iter().position(|item| item.id == media.id) {
            if self.items[index] == media {
                return Ok(false);
            }
            Arc::make_mut(&mut self.items)[index] = media;
        } else {
            if self.items.len() == ITEM_LIMIT {
                return Err(Error::TooLarge);
            }
            Arc::make_mut(&mut self.items).push(media);
        }
        Ok(true)
    }
    pub fn remove(&mut self, id: &MediaId) -> bool {
        if let Some(index) = self.items.iter().position(|item| item.id == *id) {
            Arc::make_mut(&mut self.items).remove(index);
            true
        } else {
            false
        }
    }
    pub fn encode(&self) -> Result<Vec<u8>, Error> {
        let document = Document {
            schema: 1,
            items: self.items.iter().map(Record::from).collect(),
        };
        let bytes = serde_json::to_vec_pretty(&document).map_err(|_| Error::Encoding)?;
        if bytes.len() > super::MAX_BYTES {
            return Err(Error::TooLarge);
        }
        Ok(bytes)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() > super::MAX_BYTES {
            return Err(Error::TooLarge);
        }
        let document: Document = serde_json::from_slice(bytes).map_err(|_| Error::Encoding)?;
        if document.schema != 1 {
            return Err(Error::UnsupportedSchema);
        }
        if document.items.len() > ITEM_LIMIT {
            return Err(Error::TooLarge);
        }
        let mut collection = Self::default();
        for record in document.items {
            let media = Media::try_from(record)?;
            if collection.contains(&media.id) {
                return Err(Error::DuplicateIdentity);
            }
            collection.save(media)?;
        }
        Ok(collection)
    }
}
fn validate(media: &Media) -> Result<(), Error> {
    // Even JSON's worst-case quote/backslash escaping stays below MAX_BYTES
    // at ITEM_LIMIT. Reject control characters rather than expanding them 6x.
    if media.id.provider.as_str().len() > 64
        || media.id.local().len() > 256
        || media.title.len() > 1024
    {
        return Err(Error::TooLarge);
    }
    if media.title.trim().is_empty()
        || media.title.chars().any(char::is_control)
        || media.id.local().chars().any(char::is_control)
    {
        return Err(Error::InvalidMedia);
    }
    Ok(())
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    schema: u32,
    items: Vec<Record>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    provider: String,
    id: String,
    title: String,
    duration_ms: Option<u64>,
}
impl From<&Media> for Record {
    fn from(media: &Media) -> Self {
        Self {
            provider: media.id.provider.as_str().into(),
            id: media.id.local().into(),
            title: media.title.clone(),
            duration_ms: media.duration_ms,
        }
    }
}
impl TryFrom<Record> for Media {
    type Error = Error;
    fn try_from(record: Record) -> Result<Self, Error> {
        let provider = ProviderId::new(record.provider).map_err(|_| Error::InvalidMedia)?;
        let id = MediaId::new(provider, record.id).map_err(|_| Error::InvalidMedia)?;
        let media = Self {
            id,
            title: record.title,
            duration_ms: record.duration_ms,
        };
        validate(&media)?;
        Ok(media)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn media(provider: &str, id: &str) -> Media {
        Media {
            id: MediaId::new(ProviderId::new(provider).unwrap(), id).unwrap(),
            title: "Café · 音楽".into(),
            duration_ms: Some(10_000),
        }
    }
    #[test]
    fn qualified_ids_refresh_in_place_and_survive_round_trip_without_resources() {
        let mut library = Collection::default();
        let a = media("demo", "same-id");
        let mut b = media("wikimedia-commons", "same-id");
        assert!(library.save(a.clone()).unwrap());
        assert!(library.save(b.clone()).unwrap());
        assert!(!library.save(b.clone()).unwrap());
        b.title = "Título actualizado".into();
        assert!(library.save(b.clone()).unwrap());
        assert_eq!(library.items(), &[a.clone(), b.clone()]);
        let bytes = library.encode().unwrap();
        assert_eq!(Collection::decode(&bytes).unwrap(), library);
        let document: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let fields = document["items"][0].as_object().unwrap();
        assert_eq!(fields.len(), 4);
        assert!(fields.contains_key("provider") && fields.contains_key("id"));
        assert!(library.remove(&a.id));
        assert_eq!(library.items(), &[b]);
    }
    #[test]
    fn invalid_or_future_documents_are_rejected_without_silent_deduplication() {
        let item = r#"{"provider":"demo","id":"x","title":"Track","duration_ms":null}"#;
        let duplicate = format!(r#"{{"schema":1,"items":[{item},{item}]}}"#);
        assert_eq!(
            Collection::decode(duplicate.as_bytes()),
            Err(Error::DuplicateIdentity)
        );
        assert_eq!(
            Collection::decode(br#"{"schema":2,"items":[]}"#),
            Err(Error::UnsupportedSchema)
        );
        assert_eq!(
            Collection::decode(
                br#"{"schema":1,"items":[],"stream_url":"https://invalid.test/ephemeral"}"#
            ),
            Err(Error::Encoding)
        );
        assert!(Collection::decode(br#"{"schema":1,"items":[{"provider":"UPPER","id":"x","title":"x","duration_ms":null}]}"#).is_err());
    }
    #[test]
    fn full_collection_rejects_an_addition_but_allows_metadata_refresh() {
        let mut library = Collection::default();
        for i in 0..ITEM_LIMIT {
            let mut item = media("demo", &i.to_string());
            item.title = "\\\"".repeat(512);
            library.save(item).unwrap();
        }
        let before = library.clone();
        assert_eq!(
            library.save(media("demo", "overflow")),
            Err(Error::TooLarge)
        );
        assert_eq!(library, before);
        assert!(library.save(media("demo", "0")).unwrap());
        assert_ne!(library.items()[0], before.items()[0]);
        assert_eq!(
            Collection::decode(&library.encode().unwrap()).unwrap(),
            library
        );
    }
}
