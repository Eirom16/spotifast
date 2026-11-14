//! Queue presentation only: occurrence identity and edits stay in doremi-core.
use super::*;
use doremi_ui::{self as theme, Icon};
use egui::{Align, Layout};

impl DoremiApp {
    pub(super) fn queue_page(&mut self, ui: &mut egui::Ui) {
        let model = self.model.as_ref().unwrap();
        let palette = theme::palette(model.prefs.light, model.prefs.green);
        let current = model
            .queue
            .current()
            .filter(|entry| self.media_visible(&entry.media))
            .cloned();
        let caption = super::presentation::playback_caption(model);
        let entries = model
            .queue
            .upcoming()
            .filter(|entry| self.media_visible(&entry.media))
            .cloned()
            .collect::<Vec<_>>();
        let manual = model
            .queue
            .manual()
            .iter()
            .map(|entry| entry.id)
            .collect::<Vec<_>>();
        ui.heading("Cola");
        ui.add_enabled_ui(!manual.is_empty(), |ui| {
            if self.button(ui, "clear", "Vaciar cola manual", 210.0) {
                self.model.as_mut().unwrap().command(Command::ClearManual);
            }
        });
        if let Some(entry) = current {
            ui.add_space(GAP);
            ui.strong("Ahora");
            let row = theme::list_row(
                ui,
                &palette,
                ui.id().with("queue-current"),
                theme::ListRow {
                    title: &entry.media.title,
                    subtitle: caption,
                    round: false,
                    actions: 0,
                    action_label: "Abrir reproductor",
                },
            );
            self.media_artwork(ui, &entry.media.id, row.cover_rect(), 4);
            self.record("queue-current", &row.body);
            if row.body.clicked() {
                self.navigate(Page::Player);
            }
        }
        ui.add_space(GAP);
        if entries.is_empty() {
            ui.strong("No hay canciones pendientes");
            ui.weak("Busca música o añádela desde tu biblioteca.");
            if self.button(ui, "queue-search", "Buscar música", 200.0) {
                self.navigate(Page::Search);
            }
            return;
        }
        ui.strong(format!("A continuación · {}", entries.len()));
        for entry in entries {
            let index = manual.iter().position(|id| *id == entry.id);
            let placement = if index.is_some() {
                "Añadida por ti"
            } else {
                "Continuación"
            };
            let subtitle = if demo_candidate(&entry.media.id) {
                format!("{placement} · demo sin audio")
            } else if let Some(ms) = entry.media.duration_ms {
                let seconds = ms / 1000;
                format!("{}:{:02} · {placement}", seconds / 60, seconds % 60)
            } else {
                placement.to_owned()
            };
            ui.push_id(entry.id.as_u64(), |ui| {
                let row = theme::static_list_row(
                    ui,
                    &palette,
                    ui.id().with("queue-entry"),
                    theme::ListRow {
                        title: &entry.media.title,
                        subtitle: &subtitle,
                        round: false,
                        actions: usize::from(index.is_some()),
                        action_label: "",
                    },
                );
                self.media_artwork(ui, &entry.media.id, row.cover_rect(), 4);
                self.record(&format!("queue-entry:{}", entry.id.as_u64()), &row.body);
                if let Some(index) = index {
                    let mut child = ui.new_child(
                        egui::UiBuilder::new()
                            .max_rect(row.action_rect(0))
                            .layout(Layout::left_to_right(Align::Center)),
                    );
                    let more = theme::icon_button_sized(
                        &mut child,
                        Icon::Ellipsis,
                        [22.0, TOUCH],
                        palette.secondary,
                        palette.text,
                        "Opciones de esta aparición en la cola",
                    );
                    self.record(&format!("queue-more:{}", entry.id.as_u64()), &more);
                    egui::Popup::menu(&more).show(|ui| {
                        if index > 0 && self.button(ui, &format!("up:{index}"), "Subir", 190.0) {
                            self.model.as_mut().unwrap().command(Command::Move {
                                entry: entry.id,
                                gap: index - 1,
                            });
                            ui.close();
                        }
                        if index + 1 < manual.len()
                            && self.button(ui, &format!("down:{index}"), "Bajar", 190.0)
                        {
                            // Move consumes a gap in the list before removal.
                            self.model.as_mut().unwrap().command(Command::Move {
                                entry: entry.id,
                                gap: index + 2,
                            });
                            ui.close();
                        }
                        if self.button(ui, &format!("remove:{index}"), "Quitar de la cola", 190.0) {
                            self.model
                                .as_mut()
                                .unwrap()
                                .command(Command::Remove(entry.id));
                            ui.close();
                        }
                    });
                }
            });
        }
    }
}
