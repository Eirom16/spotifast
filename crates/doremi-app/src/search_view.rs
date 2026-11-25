//! Feedback and rows for the neutral offline/Commons search surfaces.
use super::*;
use doremi_core::catalog::{ProviderError, Status};
use doremi_ui::{
    self as theme,
    feedback::{self, Kind},
};

pub(super) fn provider_error_message(error: ProviderError) -> &'static str {
    match error {
        ProviderError::RateLimited { .. } => {
            "El servicio está ocupado o ha limitado las consultas. Espera antes de reintentar."
        }
        ProviderError::Timeout => "La consulta tardó demasiado. Puedes volver a intentarlo.",
        ProviderError::AccessDenied => "El proveedor rechazó la consulta.",
        ProviderError::Unavailable => "El contenido no está disponible en el catálogo.",
        ProviderError::TooLarge => "La respuesta supera la capacidad de carga.",
        ProviderError::Cancelled => "Consulta cancelada.",
        ProviderError::Transport => {
            "No se pudo conectar con el catálogo. Revisa la conexión y vuelve a intentarlo."
        }
        _ => "La respuesta del catálogo no pudo utilizarse. Vuelve a intentarlo más tarde.",
    }
}

impl DoremiApp {
    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "android"))]
    pub(super) fn suggestion_keys(&mut self, ctx: &egui::Context) {
        if !ctx.memory(|memory| memory.has_focus(egui::Id::new("catalog-query"))) {
            return;
        }
        let model = self.model.as_mut().unwrap();
        if !model.music.selected || !model.music.youtube {
            return;
        }
        let suggestions = &mut model.music.suggestions;
        let count = suggestions.values().len();
        let pressed = |key| ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, key));
        if suggestions.active() && pressed(egui::Key::Escape) {
            suggestions.clear();
        } else if count > 0 && pressed(egui::Key::ArrowDown) {
            suggestions.selected =
                Some(suggestions.selected.map_or(0, |index| (index + 1) % count));
        } else if count > 0 && pressed(egui::Key::ArrowUp) {
            suggestions.selected = Some(
                suggestions
                    .selected
                    .map_or(count - 1, |index| (index + count - 1) % count),
            );
        } else if pressed(egui::Key::Enter) {
            if let Some(value) = suggestions
                .selected
                .and_then(|index| suggestions.values().get(index))
            {
                model.query = value.clone();
            }
            model.music.search_now(&model.query);
            ctx.memory_mut(|memory| memory.surrender_focus(egui::Id::new("catalog-query")));
        }
    }

    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "android"))]
    pub(super) fn search_suggestions(&mut self, ui: &mut egui::Ui, field: &egui::Response) {
        let model = self.model.as_ref().unwrap();
        let suggestions = &model.music.suggestions;
        if !model.music.selected || !model.music.youtube || !suggestions.active() {
            return;
        }
        let selected = suggestions.selected;
        let values = suggestions.values().to_vec();
        let unavailable = suggestions.unavailable;
        let mut choice = None;
        let mut region = field.rect;
        if !values.is_empty() {
            let frame = egui::Frame::group(ui.style()).show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("search-suggestions")
                    .max_height((ui.ctx().content_rect().height() * 0.30).clamp(44.0, 224.0))
                    .show(ui, |ui| {
                        for (index, value) in values.iter().enumerate() {
                            let button = ui.add_sized(
                                [ui.available_width(), TOUCH],
                                egui::Button::new(value)
                                    .selected(selected == Some(index))
                                    .truncate(),
                            );
                            self.record(&format!("search-suggestion:{index}"), &button);
                            if button.clicked() {
                                choice = Some(value.clone());
                            }
                        }
                    });
            });
            region = region.union(frame.response.rect);
        } else if unavailable {
            ui.weak("Sugerencias no disponibles. Puedes continuar la búsqueda.");
        }
        let model = self.model.as_mut().unwrap();
        if let Some(choice) = choice {
            model.query = choice;
            model.music.search_now(&model.query);
            ui.ctx()
                .memory_mut(|memory| memory.surrender_focus(field.id));
        } else if ui.input(|input| {
            input.pointer.any_click()
                && input
                    .pointer
                    .interact_pos()
                    .is_some_and(|pos| !region.contains(pos))
        }) {
            model.music.suggestions.clear();
        }
    }
    #[cfg(target_os = "android")]
    pub(super) fn submit_android_search(&mut self, ctx: &egui::Context) {
        let model = self.model.as_mut().unwrap();
        if model.page == Page::Search
            && self.editor == TextField::Search
            && model.music.selected
            && model.music.youtube
        {
            model.music.search_now(&model.query);
            ctx.memory_mut(|memory| memory.surrender_focus(egui::Id::new("catalog-query")));
        }
    }
    pub(super) fn neutral_search_results(&mut self, ui: &mut egui::Ui) {
        let model = self.model.as_ref().unwrap();
        let palette = theme::palette(model.prefs.light, model.prefs.green);
        let results = model.catalog.results().to_vec();
        if let Some((error, message)) = model.search_feedback() {
            let detail = match (error, model.catalog.search_status()) {
                (true, Status::Failed(error)) => provider_error_message(error),
                _ => message.as_str(),
            };
            feedback::panel(
                ui,
                &palette,
                if error { Kind::Error } else { Kind::Loading },
                if error {
                    "No se pudo completar la búsqueda"
                } else {
                    "Buscando música"
                },
                detail,
            );
        } else if model.query.trim().is_empty() {
            feedback::panel(
                ui,
                &palette,
                Kind::Info,
                "Encuentra tu próxima canción",
                "Escribe un nombre o artista. La fuente seleccionada indica de dónde proceden los resultados.",
            );
        } else if results.is_empty() {
            feedback::panel(
                ui,
                &palette,
                Kind::Empty,
                "No hay resultados para esta consulta",
                "Prueba otro nombre o selecciona otra fuente. No se ha sustituido el catálogo automáticamente.",
            );
        } else {
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new(format!(
                    "{} {} {}",
                    results.len(),
                    if results.len() == 1 {
                        "resultado"
                    } else {
                        "resultados"
                    },
                    if model.online_search_selected() {
                        "CC0 de Commons"
                    } else {
                        "de demostración"
                    },
                ))
                .font(theme::semibold(14.0))
                .color(palette.secondary),
            );
            ui.add_space(4.0);
        }
        self.rows(ui, &results);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_query_empty_result_and_demo_match_are_distinct_without_audio() {
        let _guard = crate::tests::UI_TEST_LOCK.lock().unwrap();
        let directory =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/neutral-search-ui-test");
        let ctx = egui::Context::default();
        let mut app = DoremiApp::new_context(&ctx, directory.clone());
        app.internal_ui = true;
        #[cfg(any(target_os = "linux", target_os = "windows", target_os = "android"))]
        app.model.as_mut().unwrap().set_online_source(false);
        for light in [false, true] {
            app.model.as_mut().unwrap().prefs.light = light;
            for (query, expected, count) in [
                ("", "Encuentra tu próxima canción", 0),
                (
                    "No existe esta canción en la demostración",
                    "No hay resultados",
                    0,
                ),
                ("Lluvia", "1 resultado de demostración", 1),
            ] {
                let model = app.model.as_mut().unwrap();
                model.query = query.into();
                model.search();
                app.controls.clear();
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(320.0, 640.0),
                        )),
                        ..Default::default()
                    },
                    |ui| app.neutral_search_results(ui),
                );
                output.textures_delta.clear();
                let text = output
                    .shapes
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        egui::Shape::Text(text) => Some(text.galley.text()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                assert!(
                    text.contains(expected),
                    "Query {query:?}, status {:?}, notice {:?}: {text}",
                    app.model.as_ref().unwrap().catalog.search_status(),
                    app.model.as_ref().unwrap().notice,
                );
                assert_eq!(
                    app.controls
                        .iter()
                        .filter(|control| control.name.starts_with("select:"))
                        .count(),
                    count
                );
                assert!(app.model.as_ref().unwrap().queue.current().is_none());
            }
        }
        drop(app);
        DoremiApp::shutdown();
        let _ = std::fs::remove_dir_all(directory);
    }
}
