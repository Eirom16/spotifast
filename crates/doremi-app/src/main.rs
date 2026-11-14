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
        [argument, path] if argument == "--wav-audio-probe" => {
            match doremi_app::wav_audio_probe(std::path::PathBuf::from(path)) {
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
                "Doremi Native Dev: --version, --help, --audio-probe, --wav-audio-probe PATH. UI: YouTube Music catalog and native media resolution, optional MusicBrainz metadata, selected Commons CC0 music with progressive native playback and test tones, local library and native player. MusicBrainz supplies no audio. Linux also supports --online-audio-probe. Development build; no stable release."
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
            #[cfg(windows)]
            let base = std::env::var_os("LOCALAPPDATA").map(std::path::PathBuf::from);
            #[cfg(not(windows))]
            let base = std::env::var_os("HOME")
                .map(|home| std::path::PathBuf::from(home).join(".local/share"));
            base.unwrap_or_else(std::env::temp_dir)
                .join("doremi/native-dev")
        });
    let result = eframe::run_native(
        "Doremi",
        eframe::NativeOptions {
            viewport: eframe::egui::ViewportBuilder::default()
                .with_inner_size([1240.0, 800.0])
                .with_min_inner_size([360.0, 640.0])
                .with_icon(
                    eframe::icon_data::from_png_bytes(include_bytes!(
                        "../../../assets/doremi/Doremi.png"
                    ))
                    .expect("original Doremi icon"),
                ),
            ..Default::default()
        },
        Box::new(move |cc| Ok(Box::new(doremi_app::DoremiApp::new(cc, directory)))),
    );
    doremi_app::DoremiApp::shutdown();
    result
}

#[cfg(target_os = "android")]
fn main() {}
