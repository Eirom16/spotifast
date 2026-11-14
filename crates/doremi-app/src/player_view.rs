//! Full desktop player. The persistent bar remains the single transport surface.
use super::*;
use doremi_ui::{
    self as theme, Icon,
    collection::{Hero, HeroSlot},
    feedback::{self, Kind},
};

impl DoremiApp {
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
        let source = match media.id.provider.as_str() {
            "youtube-music" => "YouTube Music",
            "wikimedia-commons" => "Commons · CC0",
            "doremi-demo" => "Demostración · sin audio",
            _ if demo_candidate(&media.id) => "Demostración · sin audio",
            _ => "Audio en este dispositivo",
        };
        theme::collection::hero(
            ui,
            &palette,
            Hero {
                kind: "REPRODUCTOR",
                title: &media.title,
                description: Some(source),
                round: false,
                cover_clickable: false,
            },
            |ui, slot| match slot {
                HeroSlot::Cover {
                    rect,
                    radius,
                    response,
                } => {
                    theme::cover_placeholder(ui, &palette, rect, radius, Icon::Music);
                    self.media_artwork(ui, &media.id, rect, radius.min(127.0) as u8);
                    self.record("player-art", response);
                }
                HeroSlot::Text { text, font, color } => {
                    let mut job = egui::text::LayoutJob::simple(
                        text.into(),
                        font,
                        color,
                        ui.available_width(),
                    );
                    job.wrap.max_rows = if text == media.title { 3 } else { 2 };
                    ui.add(egui::Label::new(job)).on_hover_text(text);
                }
                HeroSlot::Byline => {
                    if let Some(ms) = media.duration_ms {
                        let seconds = ms / 1000;
                        ui.weak(format!("{}:{:02} · {caption}", seconds / 60, seconds % 60));
                    } else {
                        ui.weak(caption);
                    }
                }
            },
        );
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::Vec2::splat(GAP);
            let saved = self.model.as_ref().unwrap().library.contains(&media.id);
            if self.choice_button(
                ui,
                "player-save",
                if saved {
                    "Quitar de biblioteca"
                } else {
                    "Guardar canción"
                },
                200.0,
                saved,
            ) {
                let model = self.model.as_mut().unwrap();
                if saved {
                    model.remove_saved(&media.id);
                } else {
                    model.save_media(media.clone());
                }
            }
            if self.choice_button(ui, "player-queue", "Ver cola", 140.0, false) {
                self.navigate(Page::Queue);
            }
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
                if !notice.is_empty() {
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
    fn desktop_player_uses_real_media_and_one_centered_transport_in_both_themes() {
        let _guard = crate::tests::UI_TEST_LOCK.lock().unwrap();
        let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/desktop-player-layout-test");
        let ctx = egui::Context::default();
        let mut app = DoremiApp::new_context(&ctx, directory.clone());
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
                assert!(((shuffle.rect[0] + repeat.rect[2]) / 2.0 - size[0] / 2.0).abs() <= 1.0);
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
