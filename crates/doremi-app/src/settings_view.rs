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
        ui.add_space(20.0);
        let equalizer = ui.collapsing("Ecualizador", |ui| self.equalizer_settings(ui));
        self.record("equalizer-settings", &equalizer.header_response);
        ui.add_space(20.0);
        ui.heading("Visualización");
        ui.horizontal_wrapped(|ui| {
            for (mode, key, label) in [
                (visualizer::Mode::Off, "visual-off", "Desactivada"),
                (visualizer::Mode::Spectrum, "visual-spectrum", "Espectro"),
                (visualizer::Mode::Scope, "visual-scope", "Osciloscopio"),
            ] {
                let selected = self.model.as_ref().unwrap().prefs.visualizer == mode;
                if self.choice_button(ui, key, label, 0.0, selected) && !selected {
                    let model = self.model.as_mut().unwrap();
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
            let tray = ui.checkbox(
                &mut model.prefs.desktop.close_to_tray,
                "Mantener la reproducción al cerrar la ventana",
            );
            let above = ui.checkbox(
                &mut model.prefs.desktop.mini_on_top,
                "Mini reproductor siempre visible si el escritorio lo permite",
            );
            let controls = ui.checkbox(
                &mut model.prefs.desktop.media_controls,
                "Controles multimedia del sistema (al reiniciar)",
            );
            if tray.changed() || above.changed() || controls.changed() {
                model.revision += 1;
            }
            self.record("close-to-tray", &tray);
            self.record("mini-on-top", &above);
            self.record("desktop-media-controls", &controls);
            if self.choice_button(ui, "open-mini", "Abrir mini reproductor", 208.0, false) {
                self.open_mini = true;
            }
            ui.weak("Ctrl+Mayús+M alterna entre la ventana principal y el mini reproductor.");
            ui.add_space(20.0);
        }
        ui.add_space(12.0);
        ui.weak("Tu biblioteca y tus preferencias se guardan en este dispositivo.");
    }
}

impl DoremiApp {
    fn equalizer_settings(&mut self, ui: &mut egui::Ui) {
        use doremi_player::equalizer::{BANDS, EqSettings, PRESETS, RANGE_DB};

        let model = self.model.as_ref().unwrap();
        let mut settings = model.prefs.equalizer;
        let rate = model
            .audio
            .as_ref()
            .and_then(Engine::metrics)
            .map(|metrics| metrics.output_rate)
            .filter(|rate| *rate != 0);
        let enabled = ui.checkbox(&mut settings.on, "Activar ecualizador");
        self.record("equalizer-enable", &enabled);
        let preset = PRESETS
            .iter()
            .find(|preset| preset.bands_db == settings.bands_db);
        let presets = egui::ComboBox::from_id_salt("equalizer-preset")
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
        self.record("equalizer-preset", &presets.response);
        let reset = ui.button("Restablecer ecualizador");
        self.record("equalizer-reset", &reset);
        if reset.clicked() {
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
        for (index, (hz, band)) in BANDS.into_iter().zip(&mut settings.bands_db).enumerate() {
            let supported =
                rate.is_none_or(|rate| index < doremi_player::equalizer::supported_bands(rate));
            let response = ui
                .add_enabled(
                    supported,
                    egui::Slider::new(band, -RANGE_DB..=RANGE_DB)
                        .step_by(0.2)
                        .suffix(" dB")
                        .text(format!("{hz:.0} Hz")),
                )
                .on_disabled_hover_text("Banda fuera del rango de la salida actual.");
            self.record(&format!("equalizer-band-{index}"), &response);
        }
        let model = self.model.as_mut().unwrap();
        if settings != model.prefs.equalizer {
            model.prefs.equalizer = settings;
            if let Some(engine) = &mut model.audio {
                engine.set_equalizer(settings);
            }
            model.revision += 1;
        }
    }
}
