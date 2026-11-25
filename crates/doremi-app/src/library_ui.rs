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
        let kind = presentation::metadata_kind(media).unwrap_or(EntityKind::Recording);
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
        // Creation sits above the filters so the field keeps a stable position
        // at the top of the page, like the search field, even with the IME open.
        self.playlist_creation(ui);
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
            .into_iter()
            .filter(|media| self.library_filter.includes(media))
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
            self.collection_controls(ui, TextField::LibraryFilter);
            let view = self.library_tracks.indices(&rows);
            if view.is_empty() {
                ui.weak("Ningún elemento coincide con el filtro.");
            } else {
                self.indexed_rows(ui, &rows, view, None);
            }
        }
        ui.add_space(16.0);
    }
}

#[cfg(test)]
#[path = "library_ui_tests.rs"]
mod tests;

#[derive(Default)]
pub(super) struct TrackView {
    pub filter: String,
    sort: Sort,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Sort {
    #[default]
    Original,
    Title,
    Artist,
    Album,
    Duration,
}

impl Sort {
    fn label(self) -> &'static str {
        match self {
            Self::Original => "Orden original",
            Self::Title => "Título",
            Self::Artist => "Artista",
            Self::Album => "Álbum",
            Self::Duration => "Duración",
        }
    }
}

fn artist(media: &Media) -> &str {
    let Some(music) = media.music.as_ref() else {
        return "";
    };
    music
        .byline
        .as_deref()
        .or_else(|| music.artists.first().map(|artist| artist.name.as_str()))
        .unwrap_or_default()
}

fn album(media: &Media) -> &str {
    media
        .music
        .as_ref()
        .and_then(|music| music.album.as_ref())
        .map_or("", |album| album.name.as_str())
}

impl TrackView {
    /// Like Spotifast's collection table, keep stable source positions for
    /// edits while playback follows the filtered and sorted view.
    pub fn indices(&self, rows: &[Media]) -> Vec<usize> {
        let needle = self.filter.trim().to_lowercase();
        let mut visible: Vec<_> = rows
            .iter()
            .enumerate()
            .filter_map(|(index, media)| {
                (needle.is_empty()
                    || format!("{} {} {}", media.title, artist(media), album(media))
                        .to_lowercase()
                        .contains(&needle))
                .then_some(index)
            })
            .collect();
        match self.sort {
            Sort::Original => {}
            Sort::Title => visible.sort_by_cached_key(|&index| rows[index].title.to_lowercase()),
            Sort::Artist => {
                visible.sort_by_cached_key(|&index| artist(&rows[index]).to_lowercase())
            }
            Sort::Album => visible.sort_by_cached_key(|&index| album(&rows[index]).to_lowercase()),
            Sort::Duration => {
                visible.sort_by_key(|&index| rows[index].duration_ms.unwrap_or(u64::MAX))
            }
        }
        visible
    }
}

impl DoremiApp {
    pub(super) fn collection_controls(&mut self, ui: &mut egui::Ui, field: TextField) {
        let state = if field == TextField::PlaylistFilter {
            &mut self.playlist_tracks
        } else {
            &mut self.library_tracks
        };
        let response = ui.add_sized(
            [ui.available_width().min(420.0), TOUCH],
            egui::TextEdit::singleline(&mut state.filter)
                .id(egui::Id::new(field.key()))
                .hint_text(if field == TextField::PlaylistFilter {
                    "Buscar en la playlist"
                } else {
                    "Buscar en tu biblioteca"
                })
                .char_limit(128),
        );
        if response.has_focus() {
            self.editor = field;
        }
        let dropdown = egui::ComboBox::from_id_salt((field.key(), "sort"))
            .selected_text(state.sort.label())
            .show_ui(ui, |ui| {
                for sort in [
                    Sort::Original,
                    Sort::Title,
                    Sort::Artist,
                    Sort::Album,
                    Sort::Duration,
                ] {
                    ui.selectable_value(&mut state.sort, sort, sort.label());
                }
            });
        self.follow_editor(ui, &response, field);
        self.record(field.key(), &response);
        self.record(&format!("{}-sort", field.key()), &dropdown.response);
    }
}

#[cfg(test)]
mod view_tests {
    use super::*;

    #[test]
    fn filtering_and_sorting_preserve_source_positions_for_duplicate_occurrences() {
        let mut rows = songs()[..3].to_vec();
        rows[0].title = "Zebra".into();
        rows[1].title = "Alpha".into();
        rows[2].title = "No match".into();
        rows.push(rows[0].clone());
        let state = TrackView {
            filter: "a".into(),
            sort: Sort::Title,
        };
        assert_eq!(state.indices(&rows), [1, 2, 0, 3]);
        let state = TrackView {
            filter: "ZEB".into(),
            sort: Sort::Title,
        };
        assert_eq!(state.indices(&rows), [0, 3]);
    }
}
