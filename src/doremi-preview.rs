//! Doremi's explicitly offline visual preview, not the legacy launcher.
use clap::Parser;
use spotifast::{app::App, backend::Waker, model::Page, preview};
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[derive(Parser)]
#[command(name = "Doremi Preview", version = preview::VERSION,
    about = "Offline visual demo. Simulated catalogue; no music playback or Spotify login.")]
struct Cli {
    /// Separate Preview profile. Never imports Spotifast files.
    #[arg(long)]
    data_dir: Option<PathBuf>,
    /// Initial fixture page: home, search, albums, settings, playlist:pl1, artist:art0, album:alb0, queue.
    #[arg(long, default_value = "home")]
    page: String,
    /// Save an actual rendered OpenGL framebuffer and exit (visual QA).
    #[arg(long)]
    capture: Option<PathBuf>,
    #[arg(long, default_value_t = 1240)]
    width: u32,
    #[arg(long, default_value_t = 800)]
    height: u32,
}

struct Preview {
    app: App,
    capture: Option<PathBuf>,
    due: Instant,
    asked: bool,
    capture_writer: Option<std::thread::JoinHandle<()>>,
}

struct CaptureMarker;

fn captured_image(event: &egui::Event) -> Option<std::sync::Arc<egui::ColorImage>> {
    match event {
        egui::Event::Screenshot {
            viewport_id,
            user_data,
            image,
        } if *viewport_id == egui::ViewportId::ROOT
            && user_data
                .data
                .as_ref()
                .is_some_and(|data| data.is::<CaptureMarker>()) =>
        {
            Some(image.clone())
        }
        _ => None,
    }
}

impl eframe::App for Preview {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.app.background_frame(ctx);
        if self.capture.is_some() {
            ctx.request_repaint_after(Duration::from_millis(16));
            if !self.asked && Instant::now() >= self.due {
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::new(
                    CaptureMarker,
                )));
                self.asked = true;
            }
            let image = self
                .asked
                .then(|| ctx.input(|input| input.events.iter().find_map(captured_image)))
                .flatten();
            if let Some(image) = image {
                let pixels: Vec<u8> = image
                    .pixels
                    .iter()
                    .flat_map(|pixel| pixel.to_srgba_unmultiplied())
                    .collect();
                let path = self.capture.take().unwrap();
                let state = serde_json::json!({
                    "page": self.app.page().encode(),
                    "preferences": self.app.preview_preferences(),
                    "query": self.app.search.committed,
                    "tracks": self.app.search.results.get().and_then(|r| r.tracks.as_ref()).map(|p| p.items.len()),
                    "width": image.width(), "height": image.height(),
                    "source": "actual-egui-OpenGL-framebuffer",
                    "audio": false,
                    "controls": preview::controls(ctx),
                });
                self.capture_writer = Some(std::thread::spawn(move || {
                    image::RgbaImage::from_raw(image.width() as u32, image.height() as u32, pixels)
                        .expect("framebuffer dimensions")
                        .save(&path)
                        .expect("write real screenshot");
                    std::fs::write(
                        path.with_extension("json"),
                        serde_json::to_vec_pretty(&state).unwrap(),
                    )
                    .expect("write capture state");
                }));
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.app.frame_ui(ui);
    }

    fn persist_egui_memory(&self) -> bool {
        false
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.app.shutdown();
        if let Some(writer) = self.capture_writer.take() {
            writer.join().expect("capture worker");
        }
    }
}

fn main() -> eframe::Result<()> {
    let cli = Cli::parse();
    let page = Page::decode(&cli.page).unwrap_or_else(|| {
        eprintln!("Unknown fixture page: {}", cli.page);
        std::process::exit(2);
    });
    let base = cli
        .data_dir
        .or_else(|| std::env::var_os("DOREMI_PREVIEW_DATA").map(PathBuf::from))
        .unwrap_or_else(|| {
            std::env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .or_else(|| {
                    std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config"))
                })
                .unwrap_or_else(|| PathBuf::from("."))
                .join("doremi-preview")
        });
    let dirs = preview::dirs(&base);
    dirs.ensure().expect("create separate Preview folders");
    let preferences = preview::Preferences::load(&dirs.config.join("preview.json"));
    let waker = Waker::default();
    let mut app = preview::create(&waker, dirs.clone(), preferences);
    app.open(page);
    // Replace the native app icon only; keep the original content geometry.
    // Decode before the event loop, never in an immediate-mode frame.
    let icon = image::load_from_memory(include_bytes!("../assets/doremi/Doremi.png"))
        .expect("original Doremi icon")
        .into_rgba8();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Doremi Preview 0.1")
            .with_app_id("io.github.Eirom16.Doremi.Preview")
            .with_inner_size([cli.width as f32, cli.height as f32])
            .with_min_inner_size([760.0, 520.0])
            .with_icon(egui::IconData {
                width: icon.width(),
                height: icon.height(),
                rgba: icon.into_raw(),
            }),
        persistence_path: Some(dirs.cache.join("preview-window.ron")),
        persist_window: false,
        ..Default::default()
    };
    eframe::run_native(
        "Doremi Preview",
        options,
        Box::new(move |cc| {
            waker.attach(&cc.egui_ctx);
            app.attach(&cc.egui_ctx);
            Ok(Box::new(Preview {
                app,
                capture: cli.capture,
                due: Instant::now() + Duration::from_secs(3),
                asked: false,
                capture_writer: None,
            }))
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn capture_does_not_consume_the_theme_animation_screenshot() {
        let image = std::sync::Arc::new(egui::ColorImage::from_rgb([1, 1], &[0, 0, 0]));
        let mut event = egui::Event::Screenshot {
            viewport_id: egui::ViewportId::ROOT,
            user_data: egui::UserData::new("theme transition"),
            image: image.clone(),
        };
        assert!(captured_image(&event).is_none());
        if let egui::Event::Screenshot { user_data, .. } = &mut event {
            *user_data = egui::UserData::new(CaptureMarker);
        }
        assert!(std::sync::Arc::ptr_eq(
            &captured_image(&event).unwrap(),
            &image
        ));
    }
}
