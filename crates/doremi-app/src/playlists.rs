//! Own playlists compose the extracted collection header and shared track rows.
use super::*;
use doremi_ui::{
    self as theme, Icon,
    collection::{Hero, HeroSlot},
};
use persistence::playlists::{Error, Playlists};

impl Model {
    pub(super) fn load_playlists(&mut self, directory: &std::path::Path) {
        match persistence::read(directory, "playlists-v1.json") {
            Ok(None) => {}
            Ok(Some(bytes)) => match Playlists::decode(&bytes) {
                Ok(playlists) => self.playlists = playlists,
                Err(_) => {
                    self.write_playlists = false;
                    self.storage_warning = Some(
                        "Playlists incompatibles. El archivo se conserva en modo de solo lectura.",
                    );
                }
            },
            Err(_) => {
                self.write_playlists = false;
                self.storage_warning =
                    Some("No se pudieron leer las playlists. El archivo se conserva.");
            }
        }
    }
    fn playlist_feedback(&mut self, result: Result<(), Error>, success: &str) {
        self.playlist_notice = Some(match result {
            Ok(()) => success.into(),
            Err(Error::TooLarge) => {
                "Tus playlists admiten 32 listas y 256 canciones en total.".into()
            }
            Err(Error::Invalid) => {
                "Usa un nombre de 1 a 64 caracteres, sin saltos de línea.".into()
            }
            Err(_) => "No se pudo modificar esta playlist.".into(),
        });
        self.revision += 1;
    }
    fn playlist_writable(&mut self) -> bool {
        if self.write_playlists {
            return true;
        }
        self.playlist_notice =
            Some("Las playlists son de solo lectura; sus datos se conservan.".into());
        self.revision += 1;
        false
    }
    pub(super) fn add_to_playlist(&mut self, id: u64, media: Media) {
        if self.playlist_writable() {
            let result = self.playlists.append(id, media);
            self.playlist_feedback(result, "Añadida a la playlist. La cola no cambia.");
        }
    }
}
impl DoremiApp {
    pub(super) fn editor_text(&self) -> &str {
        let model = self.model.as_ref().unwrap();
        match self.editor {
            TextField::Search => &model.query,
            TextField::PlaylistName => &model.playlist_name,
            TextField::PlaylistRename => &model.playlist_edit_name,
        }
    }
    #[cfg(any(target_os = "android", test))]
    pub(super) fn receive_editor_text(&mut self, field: &str, text: &str) {
        if field != self.editor.key() {
            return;
        }
        let model = self.model.as_mut().unwrap();
        match (self.editor, model.page) {
            (TextField::Search, Page::Search) => {
                let value: String = text.chars().take(128).collect();
                if model.query != value {
                    model.query = value;
                    model.search();
                }
            }
            (TextField::PlaylistName, Page::Library) => {
                model.playlist_name = text.chars().take(64).collect();
                model.revision += 1;
            }
            (TextField::PlaylistRename, Page::Playlist) => {
                model.playlist_edit_name = text.chars().take(64).collect();
                model.revision += 1;
            }
            _ => {}
        }
    }
    pub(super) fn follow_editor(&self, ui: &egui::Ui, response: &egui::Response, field: TextField) {
        if ui.ctx().content_rect().width() < 760.0 {
            let height = ui.ctx().content_rect().height();
            let resized = ui.ctx().data_mut(|data| {
                let id = response.id.with("editor-viewport-height");
                let previous = data.get_temp::<f32>(id);
                data.insert_temp(id, height);
                previous.is_some_and(|previous| height < previous)
            });
            // Android's native editor may own window focus while its IME
            // shrinks this viewport. Keep its explicit field target visible.
            if response.gained_focus()
                || response.changed()
                || (resized && (response.has_focus() || self.editor == field))
            {
                response.scroll_to_me(Some(egui::Align::Center));
            }
        }
    }
    fn playlist_editor(&mut self, ui: &mut egui::Ui, field: TextField) {
        let model = self.model.as_mut().unwrap();
        let value = match field {
            TextField::PlaylistName => &mut model.playlist_name,
            _ => &mut model.playlist_edit_name,
        };
        let height = if ui.ctx().content_rect().width() < 760.0 {
            TOUCH
        } else {
            32.0
        };
        let response = ui.add_sized(
            [(ui.available_width() - 96.0).max(48.0), height],
            egui::TextEdit::singleline(value)
                .id(egui::Id::new(field.key()))
                .hint_text("Nombre de la playlist")
                .char_limit(64),
        );
        if response.has_focus() {
            self.editor = field;
        }
        if response.changed() {
            model.revision += 1;
        }
        self.follow_editor(ui, &response, field);
        self.record(field.key(), &response);
    }
    pub(super) fn open_playlist(&mut self, id: u64) {
        let model = self.model.as_mut().unwrap();
        if let Some(list) = model.playlists.get(id) {
            model.playlist_edit_name = list.name.clone();
            model.selected_playlist = Some(id);
            model.playlist_delete_confirmation = false;
            self.navigate(Page::Playlist);
        }
    }
    pub(super) fn playlist_library(&mut self, ui: &mut egui::Ui) {
        ui.heading("Playlists");
        if !self.model.as_ref().unwrap().write_playlists {
            ui.weak("Solo lectura: se conserva el archivo anterior.");
        } else {
            ui.horizontal(|ui| {
                self.playlist_editor(ui, TextField::PlaylistName);
                if self.button(ui, "playlist-create", "Crear", 80.0) {
                    let model = self.model.as_mut().unwrap();
                    match model.playlists.create(&model.playlist_name) {
                        Ok(id) => {
                            model.playlist_name.clear();
                            model.revision += 1;
                            self.open_playlist(id);
                        }
                        Err(error) => model.playlist_feedback(Err(error), ""),
                    }
                }
            });
        }
        let snapshot = self.model.as_ref().unwrap().playlists.clone();
        if snapshot.lists().is_empty() {
            ui.weak("Crea una playlist y añade canciones desde su menú de opciones.");
        }
        let prefs = &self.model.as_ref().unwrap().prefs;
        let palette = theme::palette(prefs.light, prefs.green);
        for list in snapshot.lists() {
            let row = theme::list_row(
                ui,
                &palette,
                ui.id().with(("playlist", list.id)),
                theme::ListRow {
                    title: &list.name,
                    subtitle: &format!("{} canciones · En este dispositivo", list.entries.len()),
                    round: false,
                    actions: 0,
                    action_label: "Abrir playlist",
                },
            );
            self.record(&format!("playlist-open:{}", list.id), &row.body);
            if row.body.clicked() {
                self.open_playlist(list.id);
            }
        }
        if let Some(notice) = &self.model.as_ref().unwrap().playlist_notice {
            ui.weak(notice);
        }
    }
    pub(super) fn playlist_page(&mut self, ui: &mut egui::Ui) {
        let model = self.model.as_ref().unwrap();
        let snapshot = model.playlists.clone();
        let Some(list) = model.selected_playlist.and_then(|id| snapshot.get(id)) else {
            ui.heading("Esta playlist ya no está disponible");
            if self.button(ui, "playlist-library", "Volver a Biblioteca", 220.0) {
                self.navigate(Page::Library);
            }
            return;
        };
        let palette = theme::palette(model.prefs.light, model.prefs.green);
        if self.button(ui, "playlist-library", "Biblioteca", 140.0) {
            self.navigate(Page::Library);
            return;
        }
        theme::collection::hero(
            ui,
            &palette,
            Hero {
                kind: "PLAYLIST",
                title: &list.name,
                description: None,
                round: false,
                cover_clickable: false,
            },
            |ui, slot| match slot {
                HeroSlot::Cover { rect, radius, .. } => {
                    theme::cover_placeholder(ui, &palette, rect, radius, Icon::ListMusic)
                }
                HeroSlot::Text { text, font, color } => {
                    ui.label(egui::RichText::new(text).font(font).color(color));
                }
                HeroSlot::Byline => {
                    ui.weak(format!(
                        "{} canciones · En este dispositivo",
                        list.entries.len()
                    ));
                }
            },
        );
        ui.add_space(12.0);
        let playable = list
            .entries
            .iter()
            .any(|media| media.id.provider.as_str() != "musicbrainz");
        ui.add_enabled_ui(playable, |ui| {
            if self.button(ui, "playlist-play", "Reproducir playlist", 220.0) {
                let rows: Vec<_> = list
                    .entries
                    .iter()
                    .filter(|media| media.id.provider.as_str() != "musicbrainz")
                    .cloned()
                    .collect();
                self.model.as_mut().unwrap().start_context(rows, 0);
                self.navigate(Page::Player);
            }
        });
        if !list.entries.is_empty() && !playable {
            ui.weak("Esta lista contiene metadatos sin audio. Puedes explorar sus grabaciones.");
        }
        let edit = ui.collapsing("Editar playlist", |ui| {
            ui.add_enabled_ui(self.model.as_ref().unwrap().write_playlists, |ui| {
                ui.horizontal(|ui| {
                    self.playlist_editor(ui, TextField::PlaylistRename);
                    if self.button(ui, "playlist-rename-save", "Guardar", 80.0) {
                        let model = self.model.as_mut().unwrap();
                        let result = model.playlists.rename(list.id, &model.playlist_edit_name);
                        model.playlist_feedback(result, "Nombre actualizado.");
                    }
                });
                if self.button(ui, "playlist-delete", "Eliminar playlist", 200.0) {
                    self.model.as_mut().unwrap().playlist_delete_confirmation = true;
                }
                if self.model.as_ref().unwrap().playlist_delete_confirmation {
                    ui.label("¿Eliminar esta playlist? Tus favoritos y la cola se conservan.");
                    if self.button(
                        ui,
                        "playlist-delete-confirm",
                        "Confirmar eliminación",
                        220.0,
                    ) {
                        let model = self.model.as_mut().unwrap();
                        let result = model.playlists.delete(list.id);
                        model.playlist_feedback(result, "Playlist eliminada.");
                        self.navigate(Page::Library);
                    }
                    if self.button(ui, "playlist-delete-cancel", "Conservar playlist", 220.0) {
                        self.model.as_mut().unwrap().playlist_delete_confirmation = false;
                    }
                }
            });
        });
        self.record("playlist-edit", &edit.header_response);
        if let Some(notice) = &self.model.as_ref().unwrap().playlist_notice {
            ui.weak(notice);
        }
        if list.entries.is_empty() {
            ui.label("Añade canciones desde el menú de un resultado o de tu biblioteca.");
        }
        self.rows(ui, &list.entries);
    }
    pub(super) fn row_playlist_actions(&mut self, ui: &mut egui::Ui, media: &Media, index: usize) {
        let snapshot = self.model.as_ref().unwrap().playlists.clone();
        if !snapshot.lists().is_empty() {
            let menu = ui.menu_button("Añadir a playlist", |ui| {
                egui::ScrollArea::vertical()
                    .max_height(240.0)
                    .show(ui, |ui| {
                        for list in snapshot.lists() {
                            if self.button(
                                ui,
                                &format!("playlist-add:{}:{}", list.id, media.id.local()),
                                &list.name,
                                190.0,
                            ) {
                                self.model
                                    .as_mut()
                                    .unwrap()
                                    .add_to_playlist(list.id, media.clone());
                                ui.close();
                            }
                        }
                    });
            });
            self.record(
                &format!("playlist-choose:{}", media.id.local()),
                &menu.response,
            );
        }
        let model = self.model.as_ref().unwrap();
        if model.page == Page::Playlist
            && let Some(id) = model.selected_playlist
        {
            ui.separator();
            let len = snapshot.get(id).map_or(0, |list| list.entries.len());
            let mut edit = None;
            if index > 0 && self.button(ui, &format!("playlist-up:{index}"), "Subir", 190.0) {
                edit = Some(Some(index - 1));
            }
            if index + 1 < len && self.button(ui, &format!("playlist-down:{index}"), "Bajar", 190.0)
            {
                edit = Some(Some(index + 1));
            }
            if self.button(
                ui,
                &format!("playlist-remove:{index}"),
                "Quitar de esta playlist",
                190.0,
            ) {
                edit = Some(None);
            }
            if let Some(to) = edit {
                let model = self.model.as_mut().unwrap();
                if model.playlist_writable() {
                    let result = if let Some(to) = to {
                        model.playlists.move_entry(id, index, to)
                    } else {
                        model.playlists.remove(id, index)
                    };
                    model.playlist_feedback(result, "Playlist actualizada. La cola no cambia.");
                }
                ui.close();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn editing_a_playlist_is_independent_of_queue_occurrences_and_favorites() {
        let mut model = Model::new(Preferences::default());
        let media = songs()[0].clone();
        model.start_context(vec![media.clone(), media.clone()], 1);
        model.save_media(media.clone());
        let queue = model.queue.checkpoint();
        let library = model.library.clone();
        let id = model.playlists.create("Piano").unwrap();
        model.add_to_playlist(id, media.clone());
        model.add_to_playlist(id, media);
        model.playlists.rename(id, "Música para leer").unwrap();
        model.playlists.move_entry(id, 0, 1).unwrap();
        model.playlists.remove(id, 0).unwrap();
        model.playlists.delete(id).unwrap();
        assert_eq!(model.queue.checkpoint(), queue);
        assert_eq!(model.library, library);
    }
    #[test]
    fn a_future_playlist_document_stays_intact_when_other_library_intents_run() {
        let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
            "../../target/future-playlists-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let bytes = br#"{"schema":99,"future":"preserve"}"#;
        std::fs::write(directory.join("playlists-v1.json"), bytes).unwrap();
        let mut model = Model::load(&directory);
        assert!(!model.write_playlists);
        model.add_to_playlist(1, songs()[0].clone());
        model.save_media(songs()[0].clone());
        assert!(model.playlists.lists().is_empty());
        assert_eq!(
            std::fs::read(directory.join("playlists-v1.json")).unwrap(),
            bytes
        );
        std::fs::remove_dir_all(directory).unwrap();
    }
}
