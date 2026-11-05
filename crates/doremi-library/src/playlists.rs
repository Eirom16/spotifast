//! App-owned playlists retain qualified metadata and ordered duplicate entries.
//! No provider resources or filesystem locators belong in this document.
use crate::collection::{self, Record};
use doremi_core::media::Media;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub const LIST_LIMIT: usize = 32;
pub const ENTRY_LIMIT: usize = 256;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    TooLarge,
    Invalid,
    UnsupportedSchema,
    Missing,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Playlist {
    pub id: u64,
    pub name: String,
    pub entries: Vec<Media>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Data {
    next: u64,
    lists: Vec<Playlist>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Playlists(Arc<Data>);
impl Default for Playlists {
    fn default() -> Self {
        Self(Arc::new(Data {
            next: 1,
            lists: vec![],
        }))
    }
}
impl Playlists {
    pub fn lists(&self) -> &[Playlist] {
        &self.0.lists
    }
    pub fn get(&self, id: u64) -> Option<&Playlist> {
        self.0.lists.iter().find(|list| list.id == id)
    }
    pub(crate) fn same_snapshot(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
    pub fn create(&mut self, name: &str) -> Result<u64, Error> {
        name_valid(name)?;
        if self.0.lists.len() >= LIST_LIMIT {
            return Err(Error::TooLarge);
        }
        let next = self.0.next.checked_add(1).ok_or(Error::TooLarge)?;
        let data = Arc::make_mut(&mut self.0);
        let id = data.next;
        data.next = next;
        data.lists.push(Playlist {
            id,
            name: name.trim().into(),
            entries: vec![],
        });
        Ok(id)
    }
    pub fn rename(&mut self, id: u64, name: &str) -> Result<(), Error> {
        name_valid(name)?;
        let index = self.index(id)?;
        Arc::make_mut(&mut self.0).lists[index].name = name.trim().into();
        Ok(())
    }
    pub fn delete(&mut self, id: u64) -> Result<(), Error> {
        let index = self.index(id)?;
        Arc::make_mut(&mut self.0).lists.remove(index);
        Ok(())
    }
    pub fn append(&mut self, id: u64, media: Media) -> Result<(), Error> {
        collection::validate(&media).map_err(|_| Error::Invalid)?;
        let index = self.index(id)?;
        if self
            .0
            .lists
            .iter()
            .map(|list| list.entries.len())
            .sum::<usize>()
            >= ENTRY_LIMIT
        {
            return Err(Error::TooLarge);
        }
        Arc::make_mut(&mut self.0).lists[index].entries.push(media);
        Ok(())
    }
    pub fn remove(&mut self, id: u64, index: usize) -> Result<(), Error> {
        let list = self.index(id)?;
        if index >= self.0.lists[list].entries.len() {
            return Err(Error::Missing);
        }
        Arc::make_mut(&mut self.0).lists[list].entries.remove(index);
        Ok(())
    }
    pub fn move_entry(&mut self, id: u64, from: usize, to: usize) -> Result<(), Error> {
        let list = self.index(id)?;
        let len = self.0.lists[list].entries.len();
        if from >= len || to >= len {
            return Err(Error::Missing);
        }
        let entries = &mut Arc::make_mut(&mut self.0).lists[list].entries;
        let entry = entries.remove(from);
        entries.insert(to, entry);
        Ok(())
    }
    fn index(&self, id: u64) -> Result<usize, Error> {
        self.0
            .lists
            .iter()
            .position(|list| list.id == id)
            .ok_or(Error::Missing)
    }
    pub fn encode(&self) -> Result<Vec<u8>, Error> {
        let document = Document {
            schema: 1,
            next: self.0.next,
            lists: self
                .0
                .lists
                .iter()
                .map(|list| ListRecord {
                    id: list.id,
                    name: list.name.clone(),
                    entries: list.entries.iter().map(Record::from).collect(),
                })
                .collect(),
        };
        let bytes = serde_json::to_vec_pretty(&document).map_err(|_| Error::Invalid)?;
        if bytes.len() > crate::MAX_BYTES {
            return Err(Error::TooLarge);
        }
        Ok(bytes)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() > crate::MAX_BYTES {
            return Err(Error::TooLarge);
        }
        let document: Document = serde_json::from_slice(bytes).map_err(|_| Error::Invalid)?;
        if document.schema != 1 {
            return Err(Error::UnsupportedSchema);
        }
        if document.lists.len() > LIST_LIMIT
            || document
                .lists
                .iter()
                .map(|list| list.entries.len())
                .sum::<usize>()
                > ENTRY_LIMIT
        {
            return Err(Error::TooLarge);
        }
        if document.next == 0 {
            return Err(Error::Invalid);
        }
        let mut lists = Vec::<Playlist>::new();
        for list in document.lists {
            name_valid(&list.name)?;
            if list.id == 0
                || list.id >= document.next
                || lists.iter().any(|other| other.id == list.id)
            {
                return Err(Error::Invalid);
            }
            let entries = list
                .entries
                .into_iter()
                .map(|entry| Media::try_from(entry).map_err(|_| Error::Invalid))
                .collect::<Result<_, _>>()?;
            lists.push(Playlist {
                id: list.id,
                name: list.name,
                entries,
            });
        }
        Ok(Self(Arc::new(Data {
            next: document.next,
            lists,
        })))
    }
}
fn name_valid(name: &str) -> Result<(), Error> {
    if name.trim().is_empty()
        || name.len() > 256
        || name.chars().count() > 64
        || name.chars().any(char::is_control)
    {
        Err(Error::Invalid)
    } else {
        Ok(())
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    schema: u32,
    next: u64,
    lists: Vec<ListRecord>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ListRecord {
    id: u64,
    name: String,
    entries: Vec<Record>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use doremi_core::media::{MediaId, ProviderId};
    fn media(provider: &str) -> Media {
        Media {
            id: MediaId::new(ProviderId::new(provider).unwrap(), "same-id").unwrap(),
            title: "Café · 音楽".into(),
            duration_ms: Some(239_518),
        }
    }
    #[test]
    fn editing_roundtrip_keeps_duplicates_namespaces_order_and_stable_list_ids() {
        let mut lists = Playlists::default();
        let id = lists.create("Piano para leer").unwrap();
        for provider in ["commons", "other", "commons"] {
            lists.append(id, media(provider)).unwrap();
        }
        let frozen = lists.clone();
        lists.move_entry(id, 1, 0).unwrap();
        lists.remove(id, 2).unwrap();
        lists.rename(id, "  Música de café  ").unwrap();
        assert_eq!(frozen.get(id).unwrap().entries.len(), 3);
        let mut restored = Playlists::decode(&lists.encode().unwrap()).unwrap();
        assert_eq!(restored, lists);
        assert_eq!(restored.get(id).unwrap().name, "Música de café");
        assert_eq!(
            restored.get(id).unwrap().entries[0].id.provider.as_str(),
            "other"
        );
        restored.delete(id).unwrap();
        assert!(restored.create("Otra lista").unwrap() > id);
    }
    #[test]
    fn invalid_future_or_conflicting_documents_are_rejected_without_replacement() {
        let mut lists = Playlists::default();
        let id = lists.create("Piano").unwrap();
        let original = lists.clone();
        assert_eq!(lists.rename(id, "\n"), Err(Error::Invalid));
        assert_eq!(lists.move_entry(id, 0, 0), Err(Error::Missing));
        assert_eq!(lists, original);
        let mut value: serde_json::Value =
            serde_json::from_slice(&lists.encode().unwrap()).unwrap();
        value["schema"] = 99.into();
        assert_eq!(
            Playlists::decode(&serde_json::to_vec(&value).unwrap()),
            Err(Error::UnsupportedSchema)
        );
        value["schema"] = 1.into();
        value["next"] = id.into();
        assert_eq!(
            Playlists::decode(&serde_json::to_vec(&value).unwrap()),
            Err(Error::Invalid)
        );
    }
    #[test]
    fn the_accepted_metadata_budget_remains_encodable_and_excess_is_transactional() {
        let mut lists = Playlists::default();
        let id = lists.create(&"\\".repeat(64)).unwrap();
        let mut item = media("commons");
        item.title = "\\".repeat(1024);
        item.id = MediaId::new(ProviderId::new("c".repeat(64)).unwrap(), "\\".repeat(256)).unwrap();
        for _ in 0..ENTRY_LIMIT {
            lists.append(id, item.clone()).unwrap();
        }
        assert!(lists.encode().unwrap().len() <= crate::MAX_BYTES);
        let before = lists.clone();
        assert_eq!(lists.append(id, item), Err(Error::TooLarge));
        assert_eq!(lists, before);
    }
}
