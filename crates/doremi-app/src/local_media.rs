//! The model owns file identities and saved metadata. Workers return only facts.
use super::*;
#[cfg(target_os = "linux")]
use crate::file_input::{Error, Metadata, Source, Worker};
use persistence::local_files::Index;
pub struct State {
    pub index: Index,
    pub writable: bool,
    #[cfg(target_os = "linux")]
    worker: Option<Worker>,
    #[cfg(target_os = "linux")]
    pending: Option<u64>,
    #[cfg(target_os = "linux")]
    directory: PathBuf,
}
impl State {
    pub fn new() -> Self {
        Self {
            index: Index::default(),
            writable: true,
            #[cfg(target_os = "linux")]
            worker: None,
            #[cfg(target_os = "linux")]
            pending: None,
            #[cfg(target_os = "linux")]
            directory: PathBuf::new(),
        }
    }
    pub fn pending(&self) -> bool {
        #[cfg(target_os = "linux")]
        {
            self.pending.is_some()
        }
        #[cfg(not(target_os = "linux"))]
        {
            false
        }
    }
    pub fn snapshot(&self) -> serde_json::Value {
        #[cfg(target_os = "linux")]
        let (phase, pid) = self
            .pending
            .and_then(|id| self.worker.as_ref().map(|worker| worker.phase(id)))
            .unwrap_or(("idle", 0));
        #[cfg(not(target_os = "linux"))]
        let (phase, pid) = ("idle", 0);
        serde_json::json!({"files":self.index.len(),"writable":self.writable,"pending":self.pending(),"phase":phase,"picker_pid":pid})
    }
}
impl Model {
    pub fn load_local_files(&mut self, directory: &std::path::Path) {
        #[cfg(target_os = "linux")]
        {
            self.local_files.directory = directory.join("samples");
        }
        match persistence::read(directory, "local-files-v1.json") {
            Ok(Some(bytes)) => match Index::decode(&bytes) {
                Ok(index) => self.local_files.index = index,
                Err(_) => {
                    self.local_files.writable = false;
                    self.storage_warning =
                        Some("Índice de archivos incompatible. Se conserva sin cambios.");
                }
            },
            Ok(None) => {}
            Err(_) => {
                self.local_files.writable = false;
                self.storage_warning =
                    Some("No se pudo leer el índice de archivos. Se conserva sin cambios.");
            }
        }
    }
    pub fn cancel_local_import(&mut self) {
        #[cfg(target_os = "linux")]
        if self.local_files.pending.take().is_some() {
            if let Some(worker) = &self.local_files.worker {
                worker.cancel();
            }
            self.notice = "Importación cancelada.".into();
            self.revision += 1;
        }
    }
    #[cfg(target_os = "linux")]
    pub fn open_local(&mut self, path: Option<PathBuf>, ctx: &egui::Context) {
        if !self.local_files.writable || !self.write_library {
            self.notice =
                "Importación no disponible: los datos existentes son de solo lectura.".into();
            self.revision += 1;
            return;
        }
        self.cancel_local_import();
        if self.local_files.worker.is_none() {
            self.local_files.worker = Worker::new().ok();
        }
        let source = path
            .map(Source::Path)
            .unwrap_or_else(|| Source::Picker(self.local_files.directory.clone()));
        match self
            .local_files
            .worker
            .as_ref()
            .ok_or(Error::Busy)
            .and_then(|worker| worker.start(source, ctx))
        {
            Ok(id) => {
                self.local_files.pending = Some(id);
                self.notice = "Selecciona un WAV para importarlo a tu biblioteca.".into();
            }
            Err(_) => self.notice = "No se pudo iniciar la importación. Inténtalo de nuevo.".into(),
        }
        self.revision += 1;
    }
    #[cfg(target_os = "linux")]
    pub fn poll_local(&mut self, ctx: &egui::Context) {
        let event = self
            .local_files
            .worker
            .as_ref()
            .and_then(|worker| worker.poll(ctx));
        let Some(event) = event else {
            return;
        };
        if self.local_files.pending != Some(event.id) {
            return;
        }
        self.local_files.pending = None;
        match event.result {
            Ok(meta) => self.accept_local(meta),
            Err(error) => {
                self.notice = match error {
                    Error::Cancelled => "Importación cancelada.",
                    Error::Deadline => "La importación agotó su tiempo de espera.",
                    Error::Format => "Este archivo no es WAV PCM16 mono o estéreo compatible.",
                    Error::Name => "La ruta del archivo no se puede guardar de forma segura.",
                    Error::Input => "No se pudo leer el archivo seleccionado.",
                    Error::Picker => "Selector no disponible. Puedes arrastrar un WAV a Doremi.",
                    Error::Busy => "La importación está ocupada. Inténtalo de nuevo.",
                }
                .into();
                self.revision += 1;
            }
        }
    }
    #[cfg(target_os = "linux")]
    fn accept_local(&mut self, meta: Metadata) {
        let id = self
            .local_files
            .index
            .by_path(&meta.path)
            .cloned()
            .unwrap_or_else(|| {
                persistence::local_files::media_id(&format!("{:032x}", rand::random::<u128>()))
                    .unwrap()
            });
        let media = Media {
            id: id.clone(),
            title: meta.title,
            duration_ms: Some(meta.duration_ms),
        };
        let mut files = self.local_files.index.clone();
        let mut library = self.library.clone();
        if files.insert(id, meta.path.clone()).is_err() || library.save(media.clone()).is_err() {
            self.notice="La biblioteca o el índice alcanzaron su límite. Los datos anteriores se conservan.".into();
            self.revision += 1;
            return;
        }
        self.local_files.index = files;
        self.library = library;
        if let Some(parent) = meta.path.parent() {
            self.local_files.directory = parent.to_path_buf();
        }
        self.start_context(vec![media], 0);
        self.page = Page::Player;
        self.revision += 1;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn an_incompatible_locator_index_is_preserved_and_imports_cannot_replace_it() {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target")
            .join(format!("local-index-future-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let original = br#"{"schema":99,"files":[]}"#;
        std::fs::write(dir.join("local-files-v1.json"), original).unwrap();
        let mut model = Model::load(&dir);
        assert!(!model.local_files.writable);
        model.cancel_local_import();
        #[cfg(target_os = "linux")]
        model.open_local(None, &egui::Context::default());
        assert!(!model.local_files.pending());
        assert_eq!(
            std::fs::read(dir.join("local-files-v1.json")).unwrap(),
            original
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}
