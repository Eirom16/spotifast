//! Listening history shared by every native host; persistence stays on its writer.
use super::*;
use persistence::history::History;

impl Model {
    pub(super) fn load_history(&mut self, directory: &std::path::Path) {
        match persistence::read(directory, "history-v1.json") {
            Ok(None) => {}
            Ok(Some(bytes)) => {
                match History::decode(&bytes) {
                    Ok(history) => self.history = history,
                    Err(_) => {
                        self.write_history = false;
                        self.storage_warning.get_or_insert("Historial incompatible. Se conserva el archivo en modo de solo lectura.");
                    }
                }
            }
            Err(_) => {
                self.write_history = false;
                self.storage_warning
                    .get_or_insert("No se pudo leer el historial. El archivo se conserva.");
            }
        }
    }
    pub(super) fn note_history(&mut self) {
        if !self.write_history {
            return;
        }
        let Some(engine) = &self.audio else { return };
        let Some(request) = engine.current_request() else {
            return;
        };
        let Some(metrics) = engine.metrics() else {
            return;
        };
        if self
            .listening
            .observe(request, metrics.frames, metrics.output_rate)
            && let Some(media) = self.listening_media.clone()
        {
            let at = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .ok()
                .and_then(|elapsed| elapsed.as_millis().try_into().ok());
            if self.history.record(media, at).is_err() {
                self.notice = "No se pudo guardar esta escucha: metadatos incompatibles.".into();
            }
            self.revision += 1;
        }
    }
    fn clear_history(&mut self) {
        if self.write_history {
            self.history.clear();
            self.listening.suppress_current();
            self.history_clear_pending = false;
            self.revision += 1;
        }
    }
}
impl DoremiApp {
    pub(super) fn recent_shelf(&mut self, ui: &mut egui::Ui) {
        let recent: Vec<_> = self
            .model
            .as_ref()
            .unwrap()
            .history
            .recent(16)
            .into_iter()
            .filter(|media| self.media_visible(media))
            .collect();
        if recent.is_empty() {
            return;
        }
        let prefs = &self.model.as_ref().unwrap().prefs;
        let palette = doremi_ui::palette(prefs.light, prefs.green);
        ui.add_space(16.0);
        let title = ui.add(
            egui::Button::new(
                egui::RichText::new("Escuchado recientemente").font(doremi_ui::bold(20.0)),
            )
            .frame(false)
            .min_size(egui::vec2(
                0.0,
                if ui.ctx().content_rect().width() < 760.0 {
                    TOUCH
                } else {
                    32.0
                },
            )),
        );
        self.record("history-open", &title);
        if title.clicked() {
            self.navigate(Page::History);
        }
        ui.add_space(4.0);
        egui::ScrollArea::horizontal()
            .id_salt("home-recent-listens")
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = doremi_ui::cards::CARD_GAP / 2.0;
                    for media in &recent {
                        ui.push_id((&media.id, "recent-card"), |ui| {
                            let playing = self
                                .model
                                .as_ref()
                                .unwrap()
                                .queue
                                .current()
                                .is_some_and(|entry| entry.media.id == media.id)
                                && self.model.as_ref().unwrap().queue.playback()
                                    == Playback::Playing;
                            let response = doremi_ui::cards::card(
                                ui,
                                &palette,
                                doremi_ui::cards::Card {
                                    title: &media.title,
                                    subtitle: if !self.internal_ui {
                                        "Escuchado recientemente"
                                    } else if media.id.provider.as_str() == "wikimedia-commons" {
                                        "Commons · CC0"
                                    } else if media.id.provider.as_str() == "youtube-music" {
                                        "YouTube Music"
                                    } else {
                                        "Audio en este dispositivo"
                                    },
                                    cover: doremi_ui::cards::CardCover::square(playing),
                                    play_label: if playing { "Pausar" } else { "Reproducir" },
                                    play_size: if ui.ctx().content_rect().width() < 760.0 {
                                        TOUCH
                                    } else {
                                        44.0
                                    },
                                },
                                |ui, slot| match slot {
                                    doremi_ui::cards::Slot::Cover { rect, radius } => {
                                        doremi_ui::cards::shadow(ui, &palette, rect, radius);
                                        doremi_ui::cover_placeholder(
                                            ui,
                                            &palette,
                                            rect,
                                            radius,
                                            doremi_ui::Icon::Music,
                                        );
                                        self.media_artwork(
                                            ui,
                                            &media.id,
                                            rect,
                                            radius.min(127.0) as u8,
                                        );
                                    }
                                    doremi_ui::cards::Slot::Text {
                                        text,
                                        rect,
                                        font,
                                        color,
                                        rows,
                                    } => {
                                        let mut text = egui::text::LayoutJob::simple(
                                            text.into(),
                                            font,
                                            color,
                                            rect.width(),
                                        );
                                        text.wrap.max_rows = rows;
                                        let galley = ui.painter().layout_job(text);
                                        ui.painter().galley(rect.min, galley, color);
                                    }
                                },
                            );
                            self.record(
                                &format!("history-play:{}", media.id.local()),
                                &response.response,
                            );
                            if response.play && playing {
                                self.model.as_mut().unwrap().command(Command::Pause);
                            } else if response.play || response.clicked {
                                self.open_library_entry(std::slice::from_ref(media), 0);
                            }
                        });
                    }
                })
            });
        ui.add_space(12.0);
    }
    pub(super) fn history_page(&mut self, ui: &mut egui::Ui) {
        ui.heading("Historial");
        ui.weak(if self.internal_ui {
            "Escuchas en este dispositivo, de más reciente a más antigua. Se conservan las últimas 500."
        } else {
            "Tu música escuchada, de más reciente a más antigua."
        });
        let history = self.model.as_ref().unwrap().history.clone();
        let plays: Vec<_> = history
            .plays()
            .iter()
            .filter(|play| self.media_visible(&play.media))
            .collect();
        if !self.model.as_ref().unwrap().write_history {
            ui.weak("Solo lectura: el archivo anterior se conserva.");
        } else if !plays.is_empty() {
            if self.button(ui, "history-clear", "Vaciar historial", 190.0) {
                self.model.as_mut().unwrap().history_clear_pending = true;
            }
            if self.model.as_ref().unwrap().history_clear_pending {
                ui.label("¿Vaciar el historial? Tu biblioteca, playlists y cola se conservan.");
                if self.button(ui, "history-clear-confirm", "Confirmar vaciado", 210.0) {
                    self.model.as_mut().unwrap().clear_history();
                    return;
                }
                if self.button(ui, "history-clear-cancel", "Conservar historial", 210.0) {
                    self.model.as_mut().unwrap().history_clear_pending = false;
                }
            }
        }
        if plays.is_empty() {
            ui.label(if self.internal_ui {
                "Las escuchas aparecerán aquí después de 30 segundos, o la mitad de una pista corta."
            } else {
                "Tus escuchas aparecerán aquí cuando reproduzcas música."
            });
            return;
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()
            .map(|elapsed| elapsed.as_millis().min(u128::from(u64::MAX)) as u64);
        doremi_ui::virtual_rows(ui, plays.len(), 56.0, |ui, index| {
            let play = plays[index];
            let when = relative_time(play.played_at_ms, now);
            ui.push_id(("history-row", index), |ui| {
                self.rows_with_subtitle(ui, std::slice::from_ref(&play.media), Some(&when))
            });
        });
    }
}
fn relative_time(at: Option<u64>, now: Option<u64>) -> String {
    match now.zip(at).and_then(|(now, at)| now.checked_sub(at)) {
        None => "Sin fecha".into(),
        Some(0..60_000) => "Ahora".into(),
        Some(ms @ 60_000..3_600_000) => format!("Hace {} min", ms / 60_000),
        Some(ms @ 3_600_000..86_400_000) => format!("Hace {} h", ms / 3_600_000),
        Some(ms) => format!("Hace {} días", ms / 86_400_000),
    }
}
