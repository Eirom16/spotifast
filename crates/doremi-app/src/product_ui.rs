//! Product/QA presentation boundary. This never migrates or deletes stored data.
use super::*;

impl DoremiApp {
    pub(super) fn media_visible(&self, media: &Media) -> bool {
        self.internal_ui || matches!(media.id.provider.as_str(), "youtube-music" | "local-files")
    }

    pub(super) fn current_visible(&self) -> bool {
        self.model.as_ref().is_some_and(|model| {
            model
                .queue
                .current()
                .is_some_and(|entry| self.media_visible(&entry.media))
        })
    }

    pub(super) fn collection_visible(&self, entries: &[Media]) -> bool {
        self.internal_ui
            || entries.is_empty()
            || entries.iter().any(|media| self.media_visible(media))
    }

    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "android"))]
    pub(super) fn ensure_product_search(&mut self) {
        if self.internal_ui {
            return;
        }
        let model = self.model.as_mut().unwrap();
        if !model.music.selected || !model.music.youtube {
            model.set_online_source(false);
            model.music.selected = true;
            model.music.youtube = true;
            model.search();
            model.revision += 1;
        }
    }

    pub(super) fn search_active(&self) -> bool {
        let model = self.model.as_ref().unwrap();
        #[cfg(any(target_os = "linux", target_os = "windows", target_os = "android"))]
        if model.music.selected {
            return model.music.waiting;
        }
        model.search_feedback().is_some_and(|(error, _)| !error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(
        app: &mut DoremiApp,
        ctx: &egui::Context,
        size: [f32; 2],
        events: Vec<egui::Event>,
    ) -> String {
        let prefs = &app.model.as_ref().unwrap().prefs;
        let palette = doremi_ui::palette(prefs.light, prefs.green);
        let mut style = (*ctx.global_style()).clone();
        doremi_ui::apply_to_style(&mut style, &palette);
        if size[0] < 760.0 {
            style.spacing.interact_size.y = TOUCH;
        }
        ctx.set_global_style(style);
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size.into())),
                events,
                ..Default::default()
            },
            |ui| {
                app.controls.clear();
                // Draw only: fixtures must not start network workers or restore accounts.
                app.shell(ui, &palette);
            },
        );
        output.textures_delta.clear();
        output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) => Some(text.galley.text()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn normal_pages_never_expose_internal_controls_or_retained_qa_media() {
        let _guard = crate::tests::UI_TEST_LOCK.lock().unwrap();
        let directory =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/product-surface-test");
        let ctx = egui::Context::default();
        let mut app = DoremiApp::new_context(&ctx, directory.clone());
        assert!(
            !app.internal_ui,
            "The public context defaults to the product surface"
        );
        let samples = vec![
            songs()[0].clone(),
            Media {
                id: MediaId::new(
                    ProviderId::new("musicbrainz").unwrap(),
                    "artist:b7ffd2af-418f-4be2-bdd1-22f8b48613da",
                )
                .unwrap(),
                title: "MusicBrainz fixture".into(),
                duration_ms: None,
            },
            Media {
                id: MediaId::new(ProviderId::new("wikimedia-commons").unwrap(), "26896924")
                    .unwrap(),
                title: "Commons fixture".into(),
                duration_ms: None,
            },
        ];
        for media in &samples {
            app.model
                .as_mut()
                .unwrap()
                .library
                .save(media.clone())
                .unwrap();
            app.model
                .as_mut()
                .unwrap()
                .history
                .record(media.clone(), None)
                .unwrap();
        }
        app.model
            .as_mut()
            .unwrap()
            .start_context(samples.clone(), 0);
        let occurrence = app.model.as_ref().unwrap().queue.current().unwrap().id;
        for light in [false, true] {
            app.model.as_mut().unwrap().prefs.light = light;
            for size in [
                [320.0, 640.0],
                [384.0, 760.0],
                [682.0, 360.0],
                [1100.0, 800.0],
            ] {
                for page in [
                    Page::Home,
                    Page::Search,
                    Page::Library,
                    Page::Queue,
                    Page::Player,
                    Page::Settings,
                    Page::History,
                ] {
                    app.model.as_mut().unwrap().page = page;
                    let mut text = String::new();
                    for _ in 0..3 {
                        text = render(&mut app, &ctx, size, vec![]);
                    }
                    for control in &app.controls {
                        assert!(
                            !control.name.starts_with("source-"),
                            "{page:?}: {}",
                            control.name
                        );
                        assert!(
                            ![
                                "demo-examples",
                                "local-audio",
                                "long-audio",
                                "session-cookie",
                                "youtube-cookie-signin",
                                "youtube-import-session",
                                "cancel-search"
                            ]
                            .contains(&control.name.as_str()),
                            "{page:?}: {}",
                            control.name
                        );
                    }
                    let lower = text.to_lowercase();
                    for forbidden in [
                        "musicbrainz",
                        "commons",
                        "demostración",
                        "cc0",
                        "sapIsid",
                        "f12",
                        "fuente:",
                    ] {
                        assert!(
                            !lower.contains(&forbidden.to_lowercase()),
                            "{page:?}: {text}"
                        );
                    }
                    if page != Page::Settings {
                        assert!(!lower.contains("youtube music"), "{page:?}: {text}");
                    }
                    assert_eq!(app.model.as_ref().unwrap().library.items(), samples);
                    assert_eq!(
                        app.model.as_ref().unwrap().queue.current().unwrap().id,
                        occurrence
                    );
                }
            }
        }
        app.internal_ui = true;
        app.model.as_mut().unwrap().page = Page::Home;
        render(&mut app, &ctx, [384.0, 760.0], vec![]);
        assert!(
            app.controls
                .iter()
                .any(|control| control.name == "demo-examples"),
            "QA tools are retained, not deleted"
        );
        drop(app);
        DoremiApp::shutdown();
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "android"))]
    fn product_search_has_four_content_filters_and_only_cancellable_work_shows_cancel() {
        let _guard = crate::tests::UI_TEST_LOCK.lock().unwrap();
        let directory =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/product-search-test");
        let ctx = egui::Context::default();
        let mut app = DoremiApp::new_context(&ctx, directory.clone());
        app.model.as_mut().unwrap().page = Page::Search;
        for size in [
            [320.0, 640.0],
            [384.0, 760.0],
            [682.0, 360.0],
            [1100.0, 800.0],
        ] {
            for _ in 0..3 {
                render(&mut app, &ctx, size, vec![]);
            }
            let filters: Vec<_> = app
                .controls
                .iter()
                .filter(|control| control.name.starts_with("catalog-kind:"))
                .collect();
            assert_eq!(filters.len(), 4);
            for control in &filters {
                let [left, top, right, bottom] = control.rect;
                assert!(
                    left >= 0.0 && right <= size[0] && top >= 0.0 && bottom <= size[1],
                    "{}: {:?}",
                    control.name,
                    control.rect
                );
                if size[0] < 760.0 {
                    assert!(bottom - top >= TOUCH);
                }
            }
            if size[0] < 760.0 {
                let query = app
                    .controls
                    .iter()
                    .find(|control| control.name == "query")
                    .unwrap();
                assert!(query.rect[1] <= 24.0);
                assert!(filters.iter().all(|filter| filter.rect[1] >= query.rect[3]));
            }
            assert!(
                !app.controls
                    .iter()
                    .any(|control| control.name == "cancel-search")
            );
            app.model.as_mut().unwrap().music.waiting = true;
            render(&mut app, &ctx, size, vec![]);
            let cancel = app
                .controls
                .iter()
                .find(|control| control.name == "cancel-search")
                .unwrap()
                .rect;
            assert!(cancel[0] >= 0.0 && cancel[2] <= size[0]);
            let pointer = egui::pos2((cancel[0] + cancel[2]) / 2.0, (cancel[1] + cancel[3]) / 2.0);
            for pressed in [true, false] {
                render(
                    &mut app,
                    &ctx,
                    size,
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
            assert!(!app.model.as_ref().unwrap().music.waiting);
            assert!(
                app.model.as_ref().unwrap().music.selected
                    && app.model.as_ref().unwrap().music.youtube
            );
            render(&mut app, &ctx, size, vec![]);
            assert!(
                !app.controls
                    .iter()
                    .any(|control| control.name == "cancel-search")
            );
        }
        drop(app);
        DoremiApp::shutdown();
        let _ = std::fs::remove_dir_all(directory);
    }
}
