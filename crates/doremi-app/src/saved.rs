//! Shared saved-library intents. Neither platform owns another favorites list.
use super::*;
use persistence::collection::{Collection, Error};

pub(super) fn legacy(prefs: &Preferences) -> Result<Collection, Error> {
    let known = songs();
    let mut library = Collection::default();
    for id in &prefs.library {
        let media = match known.iter().find(|media| media.id.local() == id) {
            Some(media) => media.clone(),
            None => Media {
                id: MediaId::new(ProviderId::new("demo").unwrap(), id)
                    .map_err(|_| Error::InvalidMedia)?,
                title: format!("Guardada: {id}"),
                duration_ms: None,
            },
        };
        library.save(media)?;
    }
    Ok(library)
}
impl Model {
    pub fn load_library(&mut self, directory: &std::path::Path) {
        let legacy = legacy(&self.prefs);
        self.library = legacy.clone().unwrap_or_default();
        self.write_library = self.write_prefs && legacy.is_ok();
        match persistence::read(directory, "library-v1.json") {
            // Once present, this file is the sole library authority. The old
            // preferences list remains a rollback archive, never an input that
            // could silently reintroduce a removed favorite.
            Ok(Some(bytes)) => match Collection::decode(&bytes) {
                Ok(library) => {
                    self.library = library;
                    self.write_library = true;
                }
                Err(_) => {
                    self.write_library = false;
                    self.storage_warning = Some(
                        "Biblioteca incompatible. Se conserva el archivo; los favoritos son de solo lectura.",
                    );
                }
            },
            Ok(None) => {
                if !self.write_library {
                    self.storage_warning.get_or_insert(
                        "No se pudieron migrar los favoritos. Los datos anteriores se conservan.",
                    );
                }
            }
            Err(_) => {
                self.write_library = false;
                self.storage_warning = Some(
                    "No se pudo leer la biblioteca. El archivo se conserva sin modificaciones.",
                );
            }
        }
    }
    pub fn save_media(&mut self, media: Media) {
        if !self.write_library {
            self.notice =
                "La biblioteca está en modo de solo lectura; sus datos se conservan.".into();
        } else {
            self.notice = match self.library.save(media) {
                Ok(true) => "Añadida a tu biblioteca de Doremi.".into(),
                Ok(false) => "Ya está en tu biblioteca de Doremi.".into(),
                Err(Error::TooLarge) => {
                    "La biblioteca admite 256 pistas con metadatos acotados.".into()
                }
                Err(_) => "Estos metadatos no se pueden guardar.".into(),
            };
        }
        // Also allow an explicit repeated save to retry a failed disk write.
        self.revision += 1;
    }
    pub fn remove_saved(&mut self, id: &MediaId) {
        if !self.write_library {
            self.notice =
                "La biblioteca está en modo de solo lectura; sus datos se conservan.".into();
        } else if self.library.remove(id) {
            self.notice = "Quitada de la biblioteca. La cola no cambia.".into();
        }
        self.revision += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn directory(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target")
            .join(format!("{name}-{}", std::process::id()))
    }
    #[test]
    fn legacy_favorites_seed_once_and_removal_does_not_reimport_the_archived_list() {
        let directory = directory("saved-migration");
        std::fs::create_dir_all(&directory).unwrap();
        let original = br#"{"schema":1,"library":["dawn","unknown-old-id"]}"#;
        std::fs::write(directory.join("preferences-v1.json"), original).unwrap();
        let mut model = Model::load(&directory);
        assert!(model.write_library);
        assert_eq!(model.library.items().len(), 2);
        let dawn = songs()[0].id.clone();
        model.remove_saved(&dawn);
        std::fs::write(
            directory.join("library-v1.json"),
            model.library.encode().unwrap(),
        )
        .unwrap();
        let restored = Model::load(&directory);
        assert!(!restored.library.contains(&dawn));
        assert_eq!(restored.library.items().len(), 1);
        assert_eq!(
            std::fs::read(directory.join("preferences-v1.json")).unwrap(),
            original
        );
        std::fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn future_library_is_preserved_and_cannot_be_modified_through_ui_intents() {
        let directory = directory("saved-future");
        std::fs::create_dir_all(&directory).unwrap();
        let future = br#"{"schema":99,"items":[],"future":"preserve"}"#;
        std::fs::write(directory.join("library-v1.json"), future).unwrap();
        let mut model = Model::load(&directory);
        assert!(!model.write_library);
        model.save_media(songs()[0].clone());
        assert!(model.library.items().is_empty());
        assert_eq!(
            std::fs::read(directory.join("library-v1.json")).unwrap(),
            future
        );
        std::fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn saving_and_removing_favorites_never_changes_duplicate_queue_occurrences() {
        let mut model = Model::new(Preferences::default());
        let media = songs()[0].clone();
        model.command(Command::Append(vec![media.clone(), media.clone()]));
        let before = model.queue.checkpoint();
        model.save_media(media.clone());
        model.save_media(media.clone());
        assert_eq!(model.library.items().len(), 1);
        model.remove_saved(&media.id);
        assert_eq!(model.queue.checkpoint(), before);
    }
    #[test]
    fn another_namespace_cannot_be_played_as_the_same_named_demo() {
        let mut model = Model::new(Preferences::default());
        let media = Media {
            id: MediaId::new(ProviderId::new("unavailable-provider").unwrap(), "dawn").unwrap(),
            title: "Unavailable saved track".into(),
            duration_ms: None,
        };
        model.save_media(media.clone());
        model.start_context(vec![media], 0);
        assert_eq!(model.queue.playback(), Playback::Failed);
        assert!(model.audio.is_none() && model.prepared.is_empty());
        assert!(
            !model.source_diagnostics()["worker_initialized"]
                .as_bool()
                .unwrap_or(false)
        );
    }
}
