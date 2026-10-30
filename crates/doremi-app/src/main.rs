#[cfg(not(target_os = "android"))]
fn main() -> eframe::Result {
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    match arguments.as_slice() {
        [] => {}
        [argument] if argument == "--version" => {
            println!("Doremi Native Dev {}-dev", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        [argument] if argument == "--audio-probe" => {
            match doremi_app::audio_probe() {
                Ok(report) => println!("{report}"),
                Err(error) => {
                    eprintln!("{error}");
                    std::process::exit(1);
                }
            }
            return Ok(());
        }
        #[cfg(target_os = "linux")]
        [argument] if argument == "--online-audio-probe" => {
            match doremi_app::online_audio_probe() {
                Ok(report) => println!("{report}"),
                Err(error) => {
                    eprintln!("{error}");
                    std::process::exit(1);
                }
            }
            return Ok(());
        }
        [argument] if argument == "--help" => {
            println!(
                "Doremi Native Dev: --version, --help, --audio-probe. Linux: experimental Commons CC0 search in the UI and --online-audio-probe (two audited tones). Android remains offline."
            );
            return Ok(());
        }
        _ => {
            eprintln!("Unknown Doremi argument. Use --help.");
            std::process::exit(2);
        }
    }

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
