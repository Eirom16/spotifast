//! Private host locators, separate from neutral library metadata and the queue.
//! This index never scans, opens, copies or deletes the user's media files.
use doremi_core::media::{MediaId, ProviderId};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

pub const PROVIDER: &str = "local-files";
pub const MAX_FILES: usize = 256;
pub const MAX_PATH_BYTES: usize = 4096;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidRecord,
    Conflict,
    TooLarge,
    Encoding,
    UnsupportedSchema,
}
#[derive(Clone)]
struct File {
    id: MediaId,
    path: PathBuf,
}
#[derive(Clone, Default)]
pub struct Index {
    files: Arc<Vec<File>>,
    weight: usize,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    id: String,
    path: PathBuf,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    schema: u32,
    files: Vec<Record>,
}

pub fn media_id(key: &str) -> Result<MediaId, Error> {
    if key.len() != 32
        || !key
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(Error::InvalidRecord);
    }
    MediaId::new(ProviderId::new(PROVIDER).unwrap(), key).map_err(|_| Error::InvalidRecord)
}
fn weight(id: &MediaId, path: &Path) -> Result<usize, Error> {
    if id.provider.as_str() != PROVIDER || media_id(id.local())? != *id || !path.is_absolute() {
        return Err(Error::InvalidRecord);
    }
    let text = path.to_str().ok_or(Error::InvalidRecord)?;
    if text.len() > MAX_PATH_BYTES
        || text.chars().any(char::is_control)
        || path.components().any(|part| {
            matches!(
                part,
                std::path::Component::CurDir | std::path::Component::ParentDir
            )
        })
    {
        return Err(Error::InvalidRecord);
    }
    // Pessimistic JSON budget, including string escaping. No encoding on the UI.
    Ok(96
        + text
            .bytes()
            .map(|b| if b == b'\\' || b == b'"' { 2 } else { 1 })
            .sum::<usize>())
}
impl Index {
    pub fn len(&self) -> usize {
        self.files.len()
    }
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }
    pub fn get(&self, id: &MediaId) -> Option<&Path> {
        self.files
            .iter()
            .find(|file| file.id == *id)
            .map(|file| file.path.as_path())
    }
    pub fn by_path(&self, path: &Path) -> Option<&MediaId> {
        self.files
            .iter()
            .find(|file| file.path == path)
            .map(|file| &file.id)
    }
    pub fn same_snapshot(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.files, &other.files)
    }
    pub fn insert(&mut self, id: MediaId, path: PathBuf) -> Result<bool, Error> {
        let additional = weight(&id, &path)?;
        if let Some(previous) = self
            .files
            .iter()
            .find(|file| file.id == id || file.path == path)
        {
            return if previous.id == id && previous.path == path {
                Ok(false)
            } else {
                Err(Error::Conflict)
            };
        }
        if self.len() == MAX_FILES || self.weight + additional + 64 > crate::MAX_BYTES {
            return Err(Error::TooLarge);
        }
        Arc::make_mut(&mut self.files).push(File { id, path });
        self.weight += additional;
        Ok(true)
    }
    pub fn encode(&self) -> Result<Vec<u8>, Error> {
        let files = self
            .files
            .iter()
            .map(|file| Record {
                id: file.id.local().into(),
                path: file.path.clone(),
            })
            .collect();
        let bytes =
            serde_json::to_vec(&Document { schema: 1, files }).map_err(|_| Error::Encoding)?;
        if bytes.len() > crate::MAX_BYTES {
            return Err(Error::TooLarge);
        }
        Ok(bytes)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() > crate::MAX_BYTES {
            return Err(Error::TooLarge);
        }
        let doc: Document = serde_json::from_slice(bytes).map_err(|_| Error::Encoding)?;
        if doc.schema != 1 {
            return Err(Error::UnsupportedSchema);
        }
        if doc.files.len() > MAX_FILES {
            return Err(Error::TooLarge);
        }
        let mut index = Self::default();
        for file in doc.files {
            if !index.insert(media_id(&file.id)?, file.path)? {
                return Err(Error::Conflict);
            }
        }
        Ok(index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn path(name: &str) -> PathBuf {
        std::env::temp_dir()
            .join("doremi-locator-codec-only")
            .join(name)
    }
    fn id(n: u128) -> MediaId {
        media_id(&format!("{n:032x}")).unwrap()
    }
    #[test]
    fn qualified_unicode_paths_roundtrip_and_snapshots_do_not_mutate() {
        let mut index = Index::default();
        let file = path("Café 🎵.wav");
        assert!(index.insert(id(1), file.clone()).unwrap());
        let saved = index.clone();
        assert!(!index.insert(id(1), file.clone()).unwrap());
        assert!(index.same_snapshot(&saved));
        assert!(index.insert(id(2), path("other.wav")).unwrap());
        assert_eq!(saved.len(), 1);
        assert!(!index.same_snapshot(&saved));
        let restored = Index::decode(&index.encode().unwrap()).unwrap();
        assert_eq!(restored.get(&id(1)), Some(file.as_path()));
        assert_eq!(restored.by_path(&file), Some(&id(1)));
        let foreign = MediaId::new(ProviderId::new("demo").unwrap(), id(1).local()).unwrap();
        assert!(restored.get(&foreign).is_none());
    }
    #[test]
    fn conflicts_and_incompatible_documents_do_not_replace_existing_locators() {
        let mut index = Index::default();
        index.insert(id(1), path("keep.wav")).unwrap();
        let saved = index.encode().unwrap();
        assert_eq!(index.insert(id(1), path("wrong.wav")), Err(Error::Conflict));
        assert_eq!(index.insert(id(2), path("keep.wav")), Err(Error::Conflict));
        assert_eq!(
            index.insert(id(2), PathBuf::from("relative.wav")),
            Err(Error::InvalidRecord)
        );
        assert_eq!(index.encode().unwrap(), saved);
        assert!(matches!(
            Index::decode(br#"{"schema":99,"files":[]}"#),
            Err(Error::UnsupportedSchema)
        ));
        assert!(matches!(
            Index::decode(br#"{"schema":1,"files":[],"unknown":true}"#),
            Err(Error::Encoding)
        ));
        let record = Record {
            id: id(1).local().into(),
            path: path("keep.wav"),
        };
        let record2 = Record {
            id: id(1).local().into(),
            path: path("keep.wav"),
        };
        assert!(matches!(
            Index::decode(
                &serde_json::to_vec(&Document {
                    schema: 1,
                    files: vec![record, record2]
                })
                .unwrap()
            ),
            Err(Error::Conflict)
        ));
    }
    #[test]
    fn escaped_path_budget_is_bounded_before_encoding_and_failure_is_transactional() {
        let mut index = Index::default();
        let mut accepted = 0;
        loop {
            let n = accepted + 1;
            let file = path(&format!("{n}-{}", "\\quoted\"".repeat(400)));
            match index.insert(id(n), file) {
                Ok(true) => accepted = n,
                Err(Error::TooLarge) => break,
                other => panic!("Unexpected: {other:?}"),
            }
        }
        assert!(accepted > 1 && accepted < MAX_FILES as u128);
        let snapshot = index.clone();
        let encoded = index.encode().unwrap();
        assert!(encoded.len() <= crate::MAX_BYTES);
        assert_eq!(
            index.insert(id(999), path(&"x".repeat(MAX_PATH_BYTES))),
            Err(Error::InvalidRecord)
        );
        assert!(snapshot.same_snapshot(&index));
        assert_eq!(Index::decode(&encoded).unwrap().len(), accepted as usize);
    }
}
