use super::*;

#[test]
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "android"))]
fn category_filters_preserve_collection_queue_and_distinct_touch_targets() {
    let _guard = crate::tests::UI_TEST_LOCK.lock().unwrap();
    let directory =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/library-category-ui-test");
    let ctx = egui::Context::default();
    let mut app = DoremiApp::new_context(&ctx, directory.clone());
    let media: Vec<_> = [
        ("abcdefghijk", "Canción de prueba"),
        ("UC_fixture_artist", "Artista de prueba"),
        ("MPRE_fixture_album", "Álbum de prueba"),
        ("VL_fixture_playlist", "Playlist de prueba"),
    ]
    .into_iter()
    .map(|(id, title)| Media {
        id: doremi_youtube::id(id).unwrap(),
        title: title.into(),
        duration_ms: None,
    })
    .collect();
    for item in &media {
        app.model.as_mut().unwrap().save_media(item.clone());
    }
    let saved = app.model.as_ref().unwrap().library.items().to_vec();
    for light in [false, true] {
        app.model.as_mut().unwrap().prefs.light = light;
        for width in [320.0, 384.0, 682.0, 1100.0] {
            for (filter, expected) in [
                (Filter::All, 4),
                (Filter::Songs, 1),
                (Filter::Albums, 1),
                (Filter::Artists, 1),
                (Filter::Playlists, 1),
            ] {
                app.library_filter = filter;
                app.controls.clear();
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(width, 900.0),
                        )),
                        ..Default::default()
                    },
                    |ui| app.library_content(ui),
                );
                output.textures_delta.clear();
                let tabs: Vec<_> = app
                    .controls
                    .iter()
                    .filter(|c| c.name.starts_with("library-filter:"))
                    .collect();
                assert_eq!(tabs.len(), 5);
                for (index, tab) in tabs.iter().enumerate() {
                    let rect = egui::Rect::from_min_max(
                        egui::pos2(tab.rect[0], tab.rect[1]),
                        egui::pos2(tab.rect[2], tab.rect[3]),
                    );
                    assert!(
                        rect.left() >= 0.0 && rect.right() <= width,
                        "{width}: {rect:?}"
                    );
                    assert!(rect.height() >= if width < 760.0 { TOUCH } else { 36.0 });
                    for other in &tabs[index + 1..] {
                        let other = egui::Rect::from_min_max(
                            egui::pos2(other.rect[0], other.rect[1]),
                            egui::pos2(other.rect[2], other.rect[3]),
                        );
                        assert!(!rect.intersects(other));
                    }
                }
                assert_eq!(
                    app.controls
                        .iter()
                        .filter(|c| c.name.starts_with("select:"))
                        .count(),
                    expected
                );
                assert_eq!(app.model.as_ref().unwrap().library.items(), saved);
                assert!(app.model.as_ref().unwrap().queue.current().is_none());
                assert!(!app.model.as_ref().unwrap().music.waiting);
            }
        }
    }
    drop(app);
    DoremiApp::shutdown();
    let _ = std::fs::remove_dir_all(directory);
}
