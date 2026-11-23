//! Player presentation and favorite actions shared by the native hosts.
use super::*;
use doremi_ui::{
    self as theme, Icon,
    feedback::{self, Kind},
};

impl DoremiApp {
    fn toggle_player_saved(&mut self, media: &Media) {
        let model = self.model.as_mut().unwrap();
        if model.library.contains(&media.id) {
            model.remove_saved(&media.id);
        } else {
            model.save_media(media.clone());
        }
    }

    pub(super) fn player_favorite(&mut self, ui: &mut egui::Ui, palette: &theme::Palette) {
        let model = self.model.as_ref().unwrap();
        let Some(current) = model.queue.current() else {
            return;
        };
        if !self.playable_media(&current.media) {
            return;
        }
        let media = current.media.clone();
        let saved = model.library.contains(&media.id);
        let writable = model.write_library;
        let response = ui
            .add_enabled_ui(writable, |ui| {
                theme::icon_toggle(
                    ui,
                    palette,
                    if saved {
                        Icon::HeartFilled
                    } else {
                        Icon::Heart
                    },
                    [22.0, TOUCH],
                    saved,
                    if !writable {
                        "Biblioteca de solo lectura"
                    } else if saved {
                        "Quitar de biblioteca"
                    } else {
                        "Guardar canción"
                    },
                )
            })
            .inner;
        self.record("player-save", &response);
        if response.clicked() {
            self.toggle_player_saved(&media);
        }
    }

    pub(super) fn desktop_player(&mut self, ui: &mut egui::Ui) {
        let model = self.model.as_ref().unwrap();
        let Some(current) = model.queue.current() else {
            self.empty_player(ui);
            return;
        };
        let media = current.media.clone();
        let palette = theme::palette(model.prefs.light, model.prefs.green);
        let caption = presentation::playback_caption(model);
        let playback = model.queue.playback();
        let real_audio = model.real_audio();
        let notice = model.notice.clone();
        if self.choice_button(ui, "player-back", "Volver", 100.0, false) {
            self.go_back();
            return;
        }
        ui.add_space(20.0);
        let width = ui.available_width();
        let height = (self.page_rect[3] - self.page_rect[1] - 100.0).max(160.0);
        let art = (width * 0.44).min(height - 40.0).clamp(140.0, 380.0);
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 32.0;
            ui.vertical(|ui| {
                ui.set_width(art);
                let (rect, response) =
                    ui.allocate_exact_size(egui::Vec2::splat(art), egui::Sense::hover());
                theme::cards::shadow(ui, &palette, rect, 16.0);
                theme::cover_placeholder(ui, &palette, rect, 16.0, Icon::Music);
                self.media_artwork(ui, &media.id, rect, 16);
                self.record("player-art", &response);
            });
            ui.vertical(|ui| {
                ui.set_width(ui.available_width().max(0.0));
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new("AHORA SUENA")
                        .font(theme::semibold(11.0))
                        .color(palette.accent),
                );
                ui.add_space(12.0);
                let mut text = egui::text::LayoutJob::simple(
                    media.title.clone(),
                    theme::bold(if width > 800.0 { 36.0 } else { 28.0 }),
                    palette.text,
                    ui.available_width(),
                );
                text.wrap.max_rows = 2;
                ui.add(egui::Label::new(text)).on_hover_text(&media.title);
                let byline = self.music_byline(&media);
                if !byline.is_empty() {
                    ui.label(
                        egui::RichText::new(byline)
                            .font(theme::regular(16.0))
                            .color(palette.secondary),
                    );
                }
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new(caption)
                        .font(theme::regular(12.0))
                        .color(palette.secondary),
                );
                ui.add_space(16.0);
                self.progress(ui);
                ui.add_space(12.0);
                let large_controls = ui.available_width() >= 320.0;
                self.transport(ui, &palette, large_controls);
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    self.player_favorite(ui, &palette);
                    if self.icon_control(
                        ui,
                        &palette,
                        "player-queue",
                        Icon::ListMusic,
                        "Ver cola",
                        true,
                    ) {
                        self.navigate(Page::Queue);
                    }
                    if self.icon_control(
                        ui,
                        &palette,
                        "stop",
                        Icon::Square,
                        "Detener / cancelar descarga",
                        true,
                    ) {
                        self.model.as_mut().unwrap().command(Command::Cancel);
                    }
                });
                ui.horizontal(|ui| {
                    let slider_width = (ui.available_width() - 48.0).max(48.0);
                    self.volume_sized(ui, &palette, slider_width);
                });
            });
        });
        ui.add_space(16.0);
        match playback {
            Playback::Failed => feedback::panel(
                ui,
                &palette,
                Kind::Error,
                "No se pudo reproducir esta selección",
                &notice,
            ),
            Playback::Loading => feedback::panel(
                ui,
                &palette,
                Kind::Loading,
                caption,
                "Puedes seguir explorando mientras se prepara el audio.",
            ),
            _ if !real_audio => feedback::panel(
                ui,
                &palette,
                Kind::Info,
                "Demostración sin audio",
                "Esta selección muestra la interfaz y no reproduce música.",
            ),
            _ => {
                if self.internal_ui && !notice.is_empty() {
                    ui.add(
                        egui::Label::new(egui::RichText::new(notice).color(palette.secondary))
                            .wrap(),
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn player_favorite_toggles_saved_media_without_touching_queue_and_respects_read_only() {
        let _guard = crate::tests::UI_TEST_LOCK.lock().unwrap();
        let directory =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/player-favorite-ui-test");
        let ctx = egui::Context::default();
        let mut app = DoremiApp::new_context(&ctx, directory.clone());
        app.internal_ui = true;
        let media = songs()[0].clone();
        app.model
            .as_mut()
            .unwrap()
            .start_context(vec![media.clone()], 0);
        let occurrence = app.model.as_ref().unwrap().queue.current().unwrap().id;
        let palette = theme::Palette::dark();
        let render = |app: &mut DoremiApp, events| {
            app.controls.clear();
            let mut output = ctx.run_ui(
                egui::RawInput {
                    events,
                    ..Default::default()
                },
                |ui| app.player_favorite(ui, &palette),
            );
            output.textures_delta.clear();
            let rect = app
                .controls
                .iter()
                .find(|control| control.name == "player-save")
                .unwrap()
                .rect;
            egui::pos2((rect[0] + rect[2]) / 2.0, (rect[1] + rect[3]) / 2.0)
        };
        for (writable, expected) in [(true, true), (true, false), (false, false)] {
            app.model.as_mut().unwrap().write_library = writable;
            let pointer = render(&mut app, vec![]);
            for pressed in [true, false] {
                render(
                    &mut app,
                    vec![
                        egui::Event::PointerMoved(pointer),
                        egui::Event::PointerButton {
                            pos: pointer,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                );
            }
            assert_eq!(
                app.model.as_ref().unwrap().library.contains(&media.id),
                expected
            );
            assert_eq!(
                app.model.as_ref().unwrap().queue.current().unwrap().id,
                occurrence
            );
            assert_eq!(
                app.model.as_ref().unwrap().queue.playback(),
                Playback::Playing
            );
        }
        drop(app);
        DoremiApp::shutdown();
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn desktop_player_uses_real_media_and_one_centered_transport_in_both_themes() {
        let _guard = crate::tests::UI_TEST_LOCK.lock().unwrap();
        let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/desktop-player-layout-test");
        let ctx = egui::Context::default();
        let mut app = DoremiApp::new_context(&ctx, directory.clone());
        app.internal_ui = true;
        let mut media = songs()[0].clone();
        media.title = "Canción con un título muy largo para comprobar la disposición ".repeat(8);
        app.model.as_mut().unwrap().command(Command::StartContext {
            media: vec![media.clone()],
            start: 0,
            order: Order::Sequential,
        });
        app.navigate(Page::Player);
        for light in [false, true] {
            app.model.as_mut().unwrap().prefs.light = light;
            for size in [[760.0, 540.0], [1100.0, 800.0], [1440.0, 900.0]] {
                for _ in 0..3 {
                    let mut output = ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(
                                egui::Pos2::ZERO,
                                size.into(),
                            )),
                            ..Default::default()
                        },
                        |ui| app.draw(ui),
                    );
                    output.textures_delta.clear();
                }
                for name in [
                    "shuffle",
                    "previous",
                    "next",
                    "repeat",
                    "seek",
                    "volume",
                    "player-art",
                    "player-save",
                    "player-queue",
                    "player-back",
                ] {
                    let controls: Vec<_> = app
                        .controls
                        .iter()
                        .filter(|control| control.name == name)
                        .collect();
                    assert_eq!(controls.len(), 1, "{name}");
                    let [x1, y1, x2, y2] = controls[0].rect;
                    assert!(
                        0.0 <= x1 && x2 <= size[0] && 0.0 <= y1 && y2 <= size[1],
                        "{name} {:?} at {size:?}",
                        controls[0].rect
                    );
                }
                let shuffle = app
                    .controls
                    .iter()
                    .find(|control| control.name == "shuffle")
                    .unwrap();
                let repeat = app
                    .controls
                    .iter()
                    .find(|control| control.name == "repeat")
                    .unwrap();
                let cover = app
                    .controls
                    .iter()
                    .find(|c| c.name == "player-art")
                    .unwrap();
                let center = (cover.rect[2] + 32.0 + app.page_rect[2]) / 2.0;
                assert!(
                    ((shuffle.rect[0] + repeat.rect[2]) / 2.0 - center).abs() <= 1.0,
                    "Transport must be centered in the content column beside the cover"
                );
                assert_eq!(
                    app.model.as_ref().unwrap().queue.current().unwrap().media,
                    media
                );
                assert!(
                    !app.model.as_ref().unwrap().real_audio(),
                    "Demo metadata cannot become real audio"
                );
            }
        }
        app.go_back();
        assert_eq!(app.model.as_ref().unwrap().page, Page::Home);
        drop(app);
        DoremiApp::shutdown();
        let _ = std::fs::remove_dir_all(directory);
    }
}
