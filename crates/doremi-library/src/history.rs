//! App-owned listening records. Never contains transport URLs or credentials.
use crate::collection::{Error, Record, validate};
use doremi_core::media::Media;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub const PLAY_LIMIT: usize = 500;
pub const MAX_BYTES: usize = 2 * 1024 * 1024;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Play {
    pub media: Media,
    pub played_at_ms: Option<u64>,
}
#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub struct History {
    plays: Arc<Vec<Play>>,
}
impl History {
    pub fn plays(&self) -> &[Play] {
        &self.plays
    }
    pub(crate) fn same_snapshot(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.plays, &other.plays)
    }
    pub fn record(&mut self, media: Media, played_at_ms: Option<u64>) -> Result<(), Error> {
        validate(&media)?;
        let plays = Arc::make_mut(&mut self.plays);
        plays.insert(
            0,
            Play {
                media,
                played_at_ms,
            },
        );
        plays.truncate(PLAY_LIMIT);
        Ok(())
    }
    pub fn clear(&mut self) -> bool {
        if self.plays.is_empty() {
            return false;
        }
        self.plays = Arc::new(Vec::new());
        true
    }
    /// Home shows unique recent songs; the history retains each actual listen.
    pub fn recent(&self, limit: usize) -> Vec<Media> {
        let mut seen = std::collections::HashSet::new();
        self.plays
            .iter()
            .filter(|play| seen.insert(&play.media.id))
            .take(limit)
            .map(|play| play.media.clone())
            .collect()
    }
    pub fn encode(&self) -> Result<Vec<u8>, Error> {
        let document = Document {
            schema: 1,
            plays: self
                .plays
                .iter()
                .map(|play| StoredPlay {
                    media: Record::from(&play.media),
                    played_at_ms: play.played_at_ms,
                })
                .collect(),
        };
        let bytes = serde_json::to_vec_pretty(&document).map_err(|_| Error::Encoding)?;
        if bytes.len() > MAX_BYTES {
            return Err(Error::TooLarge);
        }
        Ok(bytes)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() > MAX_BYTES {
            return Err(Error::TooLarge);
        }
        let document: Document = serde_json::from_slice(bytes).map_err(|_| Error::Encoding)?;
        if document.schema != 1 {
            return Err(Error::UnsupportedSchema);
        }
        if document.plays.len() > PLAY_LIMIT {
            return Err(Error::TooLarge);
        }
        let plays = document
            .plays
            .into_iter()
            .map(|play| {
                Ok(Play {
                    media: Media::try_from(play.media)?,
                    played_at_ms: play.played_at_ms,
                })
            })
            .collect::<Result<Vec<_>, Error>>()?;
        Ok(Self {
            plays: Arc::new(plays),
        })
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    schema: u32,
    plays: Vec<StoredPlay>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredPlay {
    media: Record,
    played_at_ms: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use doremi_core::media::{MediaId, ProviderId};
    fn media(provider: &str, id: &str) -> Media {
        Media {
            id: MediaId::new(ProviderId::new(provider).unwrap(), id).unwrap(),
            title: "Café · 音楽".into(),
            duration_ms: Some(90_000),
        }
    }
    #[test]
    fn separate_listens_survive_round_trip_and_recent_identity_is_provider_qualified() {
        let mut history = History::default();
        let a = media("test", "same");
        let b = media("other", "same");
        history.record(a.clone(), Some(1)).unwrap();
        history.record(b.clone(), Some(2)).unwrap();
        history.record(a.clone(), Some(3)).unwrap();
        assert_eq!(history.plays().len(), 3);
        assert_eq!(history.recent(16), vec![a, b]);
        assert_eq!(
            History::decode(&history.encode().unwrap()).unwrap(),
            history
        );
        let old = history.clone();
        assert!(history.clear());
        assert_eq!(old.plays().len(), 3);
        assert!(!history.clear());
        assert_eq!(
            History::decode(br#"{"schema":2,"plays":[]}"#),
            Err(Error::UnsupportedSchema)
        );
        assert!(
            History::decode(br#"{"schema":1,"plays":[],"stream_url":"https://invalid.test"}"#)
                .is_err()
        );
    }
    #[test]
    fn the_latest_five_hundred_listens_fit_the_document_bound_with_maximal_metadata() {
        let mut history = History::default();
        let mut item = media("test", &"\\".repeat(256));
        item.title = "\\\"".repeat(512);
        for at in 0..PLAY_LIMIT + 2 {
            history.record(item.clone(), Some(at as u64)).unwrap();
        }
        assert_eq!(history.plays().len(), PLAY_LIMIT);
        assert_eq!(history.plays()[0].played_at_ms, Some(501));
        let bytes = history.encode().unwrap();
        assert!(bytes.len() <= MAX_BYTES);
        assert_eq!(History::decode(&bytes).unwrap(), history);
    }
}
