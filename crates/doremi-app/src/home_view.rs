//! Music-first Home, composed from the inherited shelf cards and library intents.
use super::*;
use doremi_ui::{self as theme, Icon, cards};
use egui::{RichText, Vec2};

impl DoremiApp {
    pub(super) fn home_header(&mut self, ui: &mut egui::Ui) {
        let prefs = &self.model.as_ref().unwrap().prefs;
        let palette = theme::palette(prefs.light, prefs.green);
        let compact = ui.ctx().content_rect().width() < 760.0;
        ui.add_space(if compact { 4.0 } else { 12.0 });
        ui.label(
            RichText::new("HECHO PARA ESCUCHAR")
                .font(theme::semibold(11.0))
                .color(palette.accent),
        );
        ui.add_space(4.0);
        ui.label(
            RichText::new("Tu música, a tu ritmo").font(theme::bold(if compact {
                28.0
            } else {
                38.0
            })),
        );
        ui.add_space(12.0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = Vec2::splat(8.0);
            let response = ui.add_sized(
                [180.0, 48.0],
                egui::Button::new(RichText::new("Buscar música").color(palette.on_accent))
                    .fill(palette.accent)
                    .corner_radius(24),
            );
            self.record("browse-catalog", &response);
            if response.clicked() {
                self.navigate(Page::Search);
            }
            #[cfg(any(target_os = "linux", target_os = "android", target_os = "windows"))]
            ui.add_enabled_ui(
                self.model.as_ref().unwrap().music.can_refresh_home(),
                |ui| {
                    if self.choice_button(
                        ui,
                        "youtube-home",
                        if self.model.as_ref().unwrap().music.home_root() {
                            "Actualizar inicio"
                        } else {
                            "Explorar música"
                        },
                        180.0,
                        false,
                    ) {
                        self.model.as_mut().unwrap().refresh_music_home();
                    }
                },
            );
        });
        ui.add_space(16.0);
    }

    pub(super) fn music_byline(&self, media: &Media) -> String {
        #[cfg(any(target_os = "linux", target_os = "android", target_os = "windows"))]
        if let Some(text) = self.model.as_ref().unwrap().music.byline(&media.id) {
            return text;
        }
        if demo_candidate(&media.id) {
            return "Demostración · sin audio".into();
        }
        if media.id.provider.as_str() == "wikimedia-commons" {
            return "Commons · CC0".into();
        }
        String::new()
    }

    pub(super) fn music_card(
        &mut self,
        ui: &mut egui::Ui,
        media: &Media,
        subtitle: &str,
        width: f32,
        icon: Icon,
    ) -> cards::CardResponse {
        let prefs = &self.model.as_ref().unwrap().prefs;
        let palette = theme::palette(prefs.light, prefs.green);
        let playing = self
            .model
            .as_ref()
            .unwrap()
            .queue
            .current()
            .is_some_and(|entry| entry.media.id == media.id)
            && self.model.as_ref().unwrap().queue.playback() == Playback::Playing;
        cards::card_sized(
            ui,
            &palette,
            cards::Card {
                title: &media.title,
                subtitle,
                cover: cards::CardCover {
                    playable: self.playable_media(media),
                    ..cards::CardCover::square(playing)
                },
                play_label: if playing { "Pausar" } else { "Reproducir" },
                play_size: 48.0,
            },
            width,
            |ui, slot| match slot {
                cards::Slot::Cover { rect, radius } => {
                    cards::shadow(ui, &palette, rect, radius);
                    theme::cover_placeholder(ui, &palette, rect, radius, icon);
                    self.media_artwork(ui, &media.id, rect, radius as u8);
                }
                cards::Slot::Text {
                    text,
                    rect,
                    font,
                    color,
                    rows,
                } => {
                    let mut job =
                        egui::text::LayoutJob::simple(text.into(), font, color, rect.width());
                    job.wrap.max_rows = rows;
                    let galley = ui.painter().layout_job(job);
                    ui.painter().galley(rect.min, galley, color);
                }
            },
        )
    }

    pub(super) fn home_library(&mut self, ui: &mut egui::Ui) {
        let rows: Vec<_> = self
            .model
            .as_ref()
            .unwrap()
            .library
            .items()
            .iter()
            .filter(|m| self.media_visible(m))
            .cloned()
            .collect();
        let lists = self.model.as_ref().unwrap().playlists.clone();
        if !rows.is_empty() {
            ui.add_space(16.0);
            ui.label(RichText::new("Tus favoritos").font(theme::bold(22.0)));
            ui.add_space(8.0);
            egui::ScrollArea::horizontal()
                .id_salt("home-favorites")
                .show(ui, |ui| {
                    ui.horizontal_top(|ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;
                        for (i, media) in rows.iter().take(16).enumerate() {
                            ui.push_id(("home-saved", &media.id), |ui| {
                                let subtitle = self.music_byline(media);
                                let card_width = if ui.ctx().content_rect().width() < 760.0 {
                                    148.0
                                } else {
                                    172.0
                                };
                                let card =
                                    self.music_card(ui, media, &subtitle, card_width, Icon::Music);
                                self.record(
                                    &format!("home-select:{}", media.id.local()),
                                    &card.response,
                                );
                                if card.clicked || card.play {
                                    self.open_library_entry(&rows, i);
                                }
                            });
                        }
                    });
                });
        }
        if !lists.lists().is_empty() {
            ui.add_space(20.0);
            ui.label(RichText::new("Tus playlists").font(theme::bold(22.0)));
            ui.add_space(8.0);
            egui::ScrollArea::horizontal()
                .id_salt("home-playlists")
                .show(ui, |ui| {
                    ui.horizontal_top(|ui| {
                        for list in lists
                            .lists()
                            .iter()
                            .filter(|l| self.collection_visible(&l.entries))
                            .collect::<Vec<_>>()
                        {
                            ui.push_id(("home-list", list.id), |ui| {
                                let cover = list
                                    .entries
                                    .first()
                                    .cloned()
                                    .unwrap_or_else(|| songs()[0].clone());
                                let media = Media {
                                    title: list.name.clone(),
                                    ..cover
                                };
                                let card = self.music_card(
                                    ui,
                                    &media,
                                    &format!("{} canciones · Playlist", list.entries.len()),
                                    156.0,
                                    Icon::ListMusic,
                                );
                                self.record(&format!("home-playlist:{}", list.id), &card.response);
                                if card.clicked || card.play {
                                    self.open_playlist(list.id);
                                }
                            });
                        }
                    });
                });
        }
        if rows.is_empty() && lists.lists().is_empty() {
            let prefs = &self.model.as_ref().unwrap().prefs;
            let palette = theme::palette(prefs.light, prefs.green);
            theme::feedback::panel(
                ui,
                &palette,
                theme::feedback::Kind::Info,
                "Dale espacio a tus favoritos",
                "Guarda canciones, álbumes y artistas, o crea una playlist en Biblioteca. Tu música aparecerá aquí.",
            );
            if self.choice_button(ui, "home-library", "Abrir biblioteca", 180.0, false) {
                self.navigate(Page::Library);
            }
        }
    }
}
