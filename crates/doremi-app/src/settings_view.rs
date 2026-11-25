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
        self.equalizer_settings(ui);
        ui.add_space(20.0);
        ui.heading("Visualización");
        ui.horizontal_wrapped(|ui| {
            let model = self.model.as_mut().unwrap();
            for (mode, label) in [
                (visualizer::Mode::Off, "Desactivada"),
                (visualizer::Mode::Spectrum, "Espectro"),
                (visualizer::Mode::Scope, "Osciloscopio"),
            ] {
                if ui
                    .selectable_label(model.prefs.visualizer == mode, label)
                    .clicked()
                {
                    model.prefs.visualizer = mode;
                    model.revision += 1;
                }
            }
        });
        ui.add_space(20.0);
        #[cfg(not(target_os = "android"))]
        {
            ui.heading("Ventanas");
            let model = self.model.as_mut().unwrap();
            if ui
                .checkbox(
                    &mut model.prefs.desktop.close_to_tray,
                    "Mantener la reproducción al cerrar la ventana",
                )
                .changed()
            {
                model.revision += 1;
            }
            if ui
                .checkbox(
                    &mut model.prefs.desktop.mini_on_top,
                    "Mini reproductor siempre visible si el escritorio lo permite",
                )
                .changed()
            {
                model.revision += 1;
            }
            if self.choice_button(ui, "open-mini", "Abrir mini reproductor", 208.0, false) {
                self.open_mini = true;
            }
            ui.add_space(20.0);
        }
        #[cfg(any(target_os = "linux", target_os = "android", target_os = "windows"))]
        self.youtube_account(ui);
        ui.add_space(12.0);
        ui.weak("Tu biblioteca y tus preferencias se guardan en este dispositivo.");
    }
}

impl DoremiApp {
    fn equalizer_settings(&mut self, ui: &mut egui::Ui) {
        use doremi_player::equalizer::{BANDS, EqSettings, PRESETS, RANGE_DB};

        let model = self.model.as_mut().unwrap();
        let mut settings = model.prefs.equalizer;
        ui.heading("Ecualizador");
        ui.checkbox(&mut settings.on, "Activar ecualizador");
        let preset = PRESETS
            .iter()
            .find(|preset| preset.bands_db == settings.bands_db);
        egui::ComboBox::from_id_salt("equalizer-preset")
            .selected_text(preset.map_or("Personalizado", |preset| preset.name))
            .show_ui(ui, |ui| {
                for preset in PRESETS {
                    if ui
                        .selectable_label(settings.bands_db == preset.bands_db, preset.name)
                        .clicked()
                    {
                        settings.bands_db = preset.bands_db;
                        settings.preamp_db = 0.0;
                        settings.on = true;
                    }
                }
            });
        if ui.button("Restablecer ecualizador").clicked() {
            settings = EqSettings {
                on: settings.on,
                ..EqSettings::default()
            };
        }
        ui.add(
            egui::Slider::new(&mut settings.preamp_db, -RANGE_DB..=RANGE_DB)
                .step_by(0.2)
                .suffix(" dB")
                .text("Preamplificador"),
        );
        for (hz, band) in BANDS.into_iter().zip(&mut settings.bands_db) {
            ui.add(
                egui::Slider::new(band, -RANGE_DB..=RANGE_DB)
                    .step_by(0.2)
                    .suffix(" dB")
                    .text(format!("{hz:.0} Hz")),
            );
        }
        if settings != model.prefs.equalizer {
            model.prefs.equalizer = settings;
            if let Some(engine) = &mut model.audio {
                engine.set_equalizer(settings);
            }
            model.revision += 1;
        }
    }
}
