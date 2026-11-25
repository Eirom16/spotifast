//! Optional display metadata shared by the persisted collection and queue records.
use doremi_core::media::{EntityKind, MediaId, MusicMetadata, MusicReference, ProviderId};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Kind {
    Recording,
    Artist,
    Release,
    Playlist,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Reference {
    provider: String,
    id: String,
    name: String,
}
impl From<&MusicReference> for Reference {
    fn from(value: &MusicReference) -> Self {
        Self {
            provider: value.id.provider.as_str().into(),
            id: value.id.local().into(),
            name: value.name.clone(),
        }
    }
}
impl Reference {
    fn decode(self) -> Result<MusicReference, ()> {
        if !valid(&self.name, 1024) || !valid(&self.id, 256) || !valid(&self.provider, 64) {
            return Err(());
        }
        let provider = ProviderId::new(self.provider).map_err(|_| ())?;
        Ok(MusicReference {
            id: MediaId::new(provider, self.id).map_err(|_| ())?,
            name: self.name,
        })
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Record {
    kind: Kind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    byline: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    artists: Vec<Reference>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    album: Option<Reference>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    artwork: Option<String>,
}
impl From<&MusicMetadata> for Record {
    fn from(data: &MusicMetadata) -> Self {
        Self {
            kind: match data.kind {
                EntityKind::Recording => Kind::Recording,
                EntityKind::Artist => Kind::Artist,
                EntityKind::Release => Kind::Release,
                EntityKind::Playlist => Kind::Playlist,
            },
            byline: data.byline.clone(),
            artists: data.artists.iter().map(Reference::from).collect(),
            album: data.album.as_ref().map(Reference::from),
            artwork: data.artwork.clone(),
        }
    }
}
impl Record {
    pub(crate) fn decode(self) -> Result<Arc<MusicMetadata>, ()> {
        if self.artists.len() > 32
            || self
                .byline
                .as_ref()
                .is_some_and(|value| !valid(value, 1024))
            || self
                .artwork
                .as_ref()
                .is_some_and(|value| !valid(value, 1024) || !value.starts_with("https://"))
        {
            return Err(());
        }
        Ok(Arc::new(MusicMetadata {
            kind: match self.kind {
                Kind::Recording => EntityKind::Recording,
                Kind::Artist => EntityKind::Artist,
                Kind::Release => EntityKind::Release,
                Kind::Playlist => EntityKind::Playlist,
            },
            byline: self.byline,
            artists: self
                .artists
                .into_iter()
                .map(Reference::decode)
                .collect::<Result<_, _>>()?,
            album: self.album.map(Reference::decode).transpose()?,
            artwork: self.artwork,
        }))
    }
}
fn valid(value: &str, max: usize) -> bool {
    !value.trim().is_empty() && value.len() <= max && !value.chars().any(char::is_control)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{collection::Collection, history::History, playlists::Playlists};
    use doremi_core::{
        media::Media,
        queue::{Command, Order, Queue},
    };

    #[test]
    fn credits_album_and_artwork_survive_every_user_document() {
        let id = |local| MediaId::new(ProviderId::new("youtube-music").unwrap(), local).unwrap();
        let artist = MusicReference {
            id: id("UCfixture"),
            name: "Invitada Ñ".into(),
        };
        let album = MusicReference {
            id: id("MPREfixture"),
            name: "Éditions".into(),
        };
        let track = Media {
            id: id("abcdefghijk"),
            title: "Canción con créditos".into(),
            duration_ms: Some(123_000),
            music: Some(Arc::new(MusicMetadata {
                kind: EntityKind::Recording,
                byline: Some("Artista & Invitada Ñ".into()),
                artists: vec![artist],
                album: Some(album),
                artwork: Some("https://lh3.googleusercontent.com/fixture=w226-h226".into()),
            })),
        };
        let mut collection = Collection::decode(br#"{"schema":1,"items":[{"provider":"demo","id":"old","title":"Old","duration_ms":null}]}"#).unwrap();
        collection.save(track.clone()).unwrap();
        let stored = collection.encode().unwrap();
        assert!(String::from_utf8_lossy(&stored).contains(""schema": 2"));
        assert_eq!(Collection::decode(&stored).unwrap(), collection);

        let mut history = History::default();
        history.record(track.clone(), Some(1)).unwrap();
        assert_eq!(
            History::decode(&history.encode().unwrap()).unwrap(),
            history
        );

        let mut playlists = Playlists::default();
        let list = playlists.create("Local").unwrap();
        playlists.append(list, track.clone()).unwrap();
        assert_eq!(
            Playlists::decode(&playlists.encode().unwrap()).unwrap(),
            playlists
        );

        let mut queue = Queue::default();
        queue
            .command(Command::StartContext {
                media: vec![track.clone()],
                start: 0,
                order: Order::Sequential,
            })
            .unwrap();
        let restored = crate::decode(&crate::encode(&queue).unwrap()).unwrap();
        assert_eq!(restored.current.unwrap().entry.media, track);
    }
}
