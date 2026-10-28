#[cfg(not(target_os = "android"))]
fn main() -> eframe::Result {
    let directory = std::env::var_os("DOREMI_DATA_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::env::var_os("HOME")
                .map(std::path::PathBuf::from)
                .unwrap_or_default()
                .join(".local/share/doremi/native-dev")
        });
    let result = eframe::run_native(
        "Doremi Native Dev",
        eframe::NativeOptions {
            viewport: eframe::egui::ViewportBuilder::default().with_inner_size([440.0, 800.0]),
            ..Default::default()
        },
        Box::new(move |cc| Ok(Box::new(doremi_app::DoremiApp::new(cc, directory)))),
    );
    doremi_app::DoremiApp::shutdown();
    result
}

#[cfg(target_os = "android")]
fn main() {}
