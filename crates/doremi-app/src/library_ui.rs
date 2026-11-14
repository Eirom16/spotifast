//! Library presentation only. Filters never modify the saved collection or queue.
use super::*;
use doremi_ui::{
    self as theme,
    feedback::{self, Kind},
};

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum Filter {
    #[default]
    All,
    Songs,
    Albums,
    Artists,
    Playlists,
}

impl Filter {
    fn includes(self, media: &Media) -> bool {
        use doremi_core::media::EntityKind;
        let kind = presentation::metadata_kind(&media.id).unwrap_or(EntityKind::Recording);
        match self {
            Self::All => true,
            Self::Songs => kind == EntityKind::Recording,
            Self::Albums => kind == EntityKind::Release,
            Self::Artists => kind == EntityKind::Artist,
            Self::Playlists => kind == EntityKind::Playlist,
        }
    }
}

impl DoremiApp {
    pub(super) fn library_content(&mut self, ui: &mut egui::Ui) {
        let model = self.model.as_ref().unwrap();
        let library: Vec<_> = model
            .library
            .items()
            .iter()
            .filter(|media| self.media_visible(media))
            .cloned()
            .collect();
        let playlists = model
            .playlists
            .lists()
            .iter()
            .filter(|list| self.collection_visible(&list.entries))
            .count();
        let palette = theme::palette(model.prefs.light, model.prefs.green);
        ui.label(egui::RichText::new("Biblioteca").font(theme::bold(28.0)));
        ui.label(
            egui::RichText::new(format!(
                "{} {} · {} {} en este dispositivo",
                library.len(),
                if library.len() == 1 {
                    "elemento guardado"
                } else {
                    "elementos guardados"
                },
                playlists,
                if playlists == 1 {
                    "playlist"
                } else {
                    "playlists"
                },
            ))
            .color(palette.secondary),
        );
        if !model.write_library {
            feedback::panel(
                ui,
                &palette,
                Kind::Info,
                "Biblioteca de solo lectura",
                "Se conserva el archivo anterior. No se sobrescribirán sus datos.",
            );
        }
        ui.add_space(12.0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::Vec2::splat(GAP);
            for (filter, key, label) in [
                (Filter::All, "all", "Todo"),
                (Filter::Songs, "songs", "Canciones"),
                (Filter::Albums, "albums", "Álbumes"),
                (Filter::Artists, "artists", "Artistas"),
                (Filter::Playlists, "playlists", "Playlists"),
            ] {
                if self.choice_button(
                    ui,
                    &format!("library-filter:{key}"),
                    label,
                    0.0,
                    self.library_filter == filter,
                ) {
                    self.library_filter = filter;
                }
            }
        });
        ui.add_space(16.0);
        if matches!(self.library_filter, Filter::All | Filter::Playlists) {
            self.playlist_library(ui);
            ui.add_space(20.0);
        }
        let rows: Vec<_> = library
            .iter()
            .filter(|media| self.library_filter.includes(media))
            .cloned()
            .collect();
        let title = match self.library_filter {
            Filter::All => "Tu música guardada",
            Filter::Songs => "Canciones favoritas",
            Filter::Albums => "Álbumes guardados",
            Filter::Artists => "Artistas guardados",
            Filter::Playlists => "Playlists del catálogo",
        };
        ui.label(egui::RichText::new(title).font(theme::bold(20.0)));
        ui.add_space(4.0);
        if rows.is_empty() {
            feedback::panel(
                ui,
                &palette,
                Kind::Empty,
                "Tu colección empieza aquí",
                "Guarda música desde Buscar para encontrarla en esta sección. Tus playlists propias se gestionan por separado.",
            );
            if self.button(ui, "library-browse", "Buscar música", 200.0) {
                self.navigate(Page::Search);
            }
        } else {
            self.rows(ui, &rows);
        }
        ui.add_space(16.0);
    }
}

#[cfg(test)]
#[path = "library_ui_tests.rs"]
mod tests;
