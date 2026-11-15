//! User-facing appearance preferences. Account behavior stays with its adapter.
use super::*;
use doremi_ui as theme;

impl DoremiApp {
    pub(super) fn settings_content(&mut self, ui: &mut egui::Ui) {
        let prefs = &self.model.as_ref().unwrap().prefs;
        let palette = theme::palette(prefs.light, prefs.green);
        ui.label(egui::RichText::new("Ajustes").font(theme::bold(28.0)));
        ui.add_space(8.0);
        egui::Frame::new()
            .fill(palette.surface)
            .corner_radius(12)
            .inner_margin(egui::Margin::same(16))
            .show(ui, |ui| {
                ui.set_width(ui.available_width().min(560.0));
                ui.label(egui::RichText::new("Tema").font(theme::semibold(14.0)));
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = GAP;
                    let width = (ui.available_width() - GAP) / 2.0;
                    for (name, label, value) in
                        [("dark", "Oscuro", false), ("light", "Claro", true)]
                    {
                        let selected = self.model.as_ref().unwrap().prefs.light == value;
                        if self.choice_button(ui, name, label, width, selected) && !selected {
                            let model = self.model.as_mut().unwrap();
                            model.prefs.light = value;
                            model.revision += 1;
                        }
                    }
                });
                ui.add_space(8.0);
                ui.label(egui::RichText::new("Acento").font(theme::semibold(14.0)));
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = GAP;
                    let width = (ui.available_width() - GAP) / 2.0;
                    for (name, label, value) in
                        [("purple", "Morado", false), ("green", "Verde", true)]
                    {
                        let selected = self.model.as_ref().unwrap().prefs.green == value;
                        if self.choice_button(ui, name, label, width, selected) && !selected {
                            let model = self.model.as_mut().unwrap();
                            model.prefs.green = value;
                            model.revision += 1;
                        }
                    }
                });
            });
        ui.add_space(20.0);
        #[cfg(any(target_os = "linux", target_os = "android", target_os = "windows"))]
        self.youtube_account(ui);
        ui.add_space(12.0);
        ui.weak("Tu biblioteca y tus preferencias se guardan en este dispositivo.");
    }
}
