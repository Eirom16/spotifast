//! Desktop surfaces adapted from Spotifast's entrypoint and media/tray actions
//! (Carmine Paolino, MIT). The neutral queue and output engine stay resident.
use super::*;
use fastframe_now_playing as media;
use fastframe_shell::{Closed, Headless, Held, Resident, Shell, Waker, window::Geometry};
use fastframe_tray::{Config, Event as TrayEvent, MenuItem, Tray};
use raw_window_handle::HasWindowHandle;

const SHOW: &str = "show";
const HIDE: &str = "hide";
const MINI: &str = "mini";
const TOGGLE: &str = "play-pause";
const NEXT: &str = "next";
const PREVIOUS: &str = "previous";
const QUIT: &str = "quit";
const ICON: &[u8] = include_bytes!("../../../assets/doremi/Doremi.png");

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Mode {
    #[default]
    Main,
    Mini,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Intent {
    #[default]
    None,
    Show,
    Mini,
    Hide,
    Quit,
}

struct Desktop {
    app: DoremiApp,
    tray: Option<Tray>,
    media: Option<media::NowPlaying>,
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    cover: desktop_cover::Cover,
    mode: Mode,
    intent: Intent,
    after_close: Closed,
    geometry: Geometry,
    seek_serial: u64,
    last_tray_controls: Option<(bool, bool, bool)>,
}

pub fn run_desktop(directory: PathBuf) -> eframe::Result {
    let ctx = egui::Context::default();
    let waker = Waker::default();
    let mut app = DoremiApp::new_context(&ctx, directory);
    app.internal_ui = std::env::var("DOREMI_INTERNAL_QA").as_deref() == Ok("1")
        && std::env::var_os("DOREMI_DATA_DIR").is_some();
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    if std::env::var_os("DOREMI_DATA_DIR").is_none() {
        let model = app.model.as_mut().unwrap();
        model.music.session.restore();
        model.youtube_page(doremi_youtube::Request::Home);
    }
    let notify = waker.clone();
    let media = app
        .model
        .as_ref()
        .unwrap()
        .prefs
        .desktop
        .media_controls
        .then(|| {
            media::NowPlaying::start(media::App::new("doremi", "Doremi"), move || notify.wake())
        });
    let notify = waker.clone();
    let tray = Tray::spawn(tray_config(), move || notify.wake());
    let desktop = Desktop {
        app,
        tray,
        media,
        #[cfg(any(target_os = "linux", target_os = "windows"))]
        cover: desktop_cover::Cover::default(),
        mode: Mode::Main,
        intent: Intent::None,
        after_close: Closed::Quit,
        geometry: Geometry::default(),
        seek_serial: 0,
        last_tray_controls: None,
    };
    let shell = Shell::new(desktop, &waker);
    #[cfg(target_os = "macos")]
    let shell = shell.idle(fastframe_tray::idle);
    let result = shell.run(|lease| {
        let viewport = lease.peek(Desktop::viewport);
        eframe::run_native(
            "Doremi",
            eframe::NativeOptions {
                viewport,
                persist_window: false,
                ..Default::default()
            },
            Box::new(move |cc| {
                let mut desktop = lease.take(&cc.egui_ctx);
                if let Some(tray) = &mut desktop.tray {
                    tray.attach();
                }
                doremi_ui::install(&cc.egui_ctx);
                let wayland = cc.window_handle().is_ok_and(|window| {
                    matches!(
                        window.as_raw(),
                        raw_window_handle::RawWindowHandle::Wayland(_)
                    )
                });
                Ok(Box::new(Window {
                    desktop,
                    wayland,
                    recovery_checked: false,
                }))
            }),
        )
    });
    DoremiApp::shutdown();
    result
}

impl Desktop {
    fn viewport(&self) -> egui::ViewportBuilder {
        let viewport = egui::ViewportBuilder::default()
            .with_app_id("doremi")
            .with_icon(eframe::icon_data::from_png_bytes(ICON).expect("Doremi icon"));
        match self.mode {
            Mode::Main => self.geometry.apply(
                viewport
                    .with_title("Doremi")
                    .with_inner_size([1240.0, 800.0])
                    .with_min_inner_size([360.0, 640.0]),
            ),
            Mode::Mini => {
                let settings = &self.app.model.as_ref().unwrap().prefs.desktop;
                Geometry {
                    position: settings.mini_position,
                    ..Geometry::default()
                }
                .apply(
                    viewport
                        .with_title("Doremi · Mini reproductor")
                        .with_inner_size([350.0, 148.0])
                        .with_resizable(false)
                        .with_maximize_button(false)
                        .with_window_level(if settings.mini_on_top {
                            egui::WindowLevel::AlwaysOnTop
                        } else {
                            egui::WindowLevel::Normal
                        }),
                )
            }
        }
    }

    fn tray_visible(&self) -> bool {
        self.tray.as_ref().is_some_and(Tray::is_shown)
    }

    fn request(&mut self, intent: Intent) {
        if self.intent != Intent::Quit {
            self.intent = intent;
        }
    }

    fn handle_commands(&mut self, window_open: bool) {
        use media::Command as MediaCommand;
        for command in self
            .media
            .as_ref()
            .map_or_else(Vec::new, media::NowPlaying::commands)
        {
            let model = self.app.model.as_mut().unwrap();
            let real = model.real_audio();
            match command {
                MediaCommand::Raise => self.request(Intent::Show),
                MediaCommand::Quit => self.request(Intent::Quit),
                MediaCommand::Stop => model.command(Command::Stop),
                MediaCommand::Play if real => model.command(Command::Play),
                MediaCommand::Pause if real => model.command(Command::Pause),
                MediaCommand::PlayPause if real => toggle_play(model),
                MediaCommand::Next => model.command(Command::Next),
                MediaCommand::Previous if real => model.command(Command::Previous),
                MediaCommand::SeekBy(offset) if real => {
                    let position = model.queue.position_ms().saturating_add_signed(offset);
                    if model
                        .queue
                        .current()
                        .and_then(|entry| entry.media.duration_ms)
                        .is_some_and(|duration| position > duration)
                    {
                        model.command(Command::Next);
                    } else {
                        model.command(Command::Seek(position));
                    }
                }
                MediaCommand::SetPosition { track_id, position }
                    if real
                        && model
                            .queue
                            .current()
                            .is_some_and(|entry| id(entry) == track_id) =>
                {
                    let position = u64::try_from(position.as_millis()).unwrap_or(u64::MAX);
                    if model
                        .queue
                        .current()
                        .and_then(|entry| entry.media.duration_ms)
                        .is_some_and(|duration| position <= duration)
                    {
                        model.command(Command::Seek(position));
                    }
                }
                MediaCommand::SetVolume(value) => model.set_volume(value as f32),
                MediaCommand::SetRepeat(repeat) => {
                    model.command(Command::SetRepeat(match repeat {
                        media::Repeat::Off => Repeat::Off,
                        media::Repeat::Track => Repeat::Track,
                        media::Repeat::Playlist => Repeat::Context,
                    }))
                }
                MediaCommand::SetShuffle(enabled) if enabled != model.queue.shuffled() => {
                    let queue = &model.queue;
                    let order = if enabled {
                        shuffled(
                            queue.context().len(),
                            queue.current().and_then(|entry| {
                                queue
                                    .context()
                                    .iter()
                                    .position(|candidate| candidate.id == entry.id)
                            }),
                        )
                    } else {
                        Order::Sequential
                    };
                    model.command(Command::SetOrder(order));
                }
                _ => {}
            }
        }
        if let Some(tray) = &self.tray {
            let tray_visible = tray.is_shown();
            for event in tray.events() {
                let model = self.app.model.as_mut().unwrap();
                match event {
                    TrayEvent::Show | TrayEvent::Menu(SHOW) => self.request(Intent::Show),
                    TrayEvent::Menu(MINI) => self.request(Intent::Mini),
                    TrayEvent::Toggle if !window_open => self.request(Intent::Show),
                    TrayEvent::Toggle | TrayEvent::Menu(HIDE) if tray_visible => {
                        self.request(Intent::Hide);
                    }
                    TrayEvent::Menu(TOGGLE) if model.real_audio() => toggle_play(model),
                    TrayEvent::Menu(NEXT) => model.command(Command::Next),
                    TrayEvent::Menu(PREVIOUS) if model.real_audio() => {
                        model.command(Command::Previous)
                    }
                    TrayEvent::Menu(QUIT) => self.request(Intent::Quit),
                    _ => {}
                }
            }
        }
    }

    fn tick(&mut self, ctx: &egui::Context) {
        #[cfg(any(target_os = "linux", target_os = "windows"))]
        if self.media.is_some() {
            let model = self.app.model.as_mut().unwrap();
            if let Some(entry) = model.queue.current() {
                let url = entry
                    .media
                    .music
                    .as_ref()
                    .and_then(|data| data.artwork.clone());
                model.artwork.request_thumbnail(&entry.media.id, url, ctx);
            }
        }
        self.app.poll_background(ctx);
        self.publish(ctx);
    }

    fn publish(&mut self, _ctx: &egui::Context) {
        let model = self.app.model.as_ref().unwrap();
        let queue = &model.queue;
        let track = queue.current().filter(|_| model.real_audio()).map(|entry| {
            let song = &entry.media;
            let metadata = song.music.as_ref();
            let mut artists: Vec<_> = metadata
                .iter()
                .flat_map(|music| &music.artists)
                .map(|artist| artist.name.clone())
                .collect();
            if artists.is_empty()
                && let Some(artist) = metadata.and_then(|music| music.byline.clone())
            {
                artists.push(artist);
            }
            #[cfg(any(target_os = "linux", target_os = "windows"))]
            let art_file = if self.media.is_some() {
                self.cover.update(
                    model.artwork.media_pixels(
                        &song.id,
                        metadata.and_then(|music| music.artwork.as_deref()),
                    ),
                    _ctx,
                )
            } else {
                None
            };
            media::Track {
                id: id(entry),
                title: song.title.clone(),
                artists,
                album: metadata
                    .and_then(|music| music.album.as_ref())
                    .map_or_else(String::new, |album| album.name.clone()),
                duration: song.duration_ms.map(Duration::from_millis),
                art_url: metadata.and_then(|music| music.artwork.clone()),
                #[cfg(any(target_os = "linux", target_os = "windows"))]
                art_file,
                ..Default::default()
            }
        });
        let has_track = track.is_some();
        let playing = has_track && queue.playback() == Playback::Playing;
        let next = queue.upcoming().next().is_some()
            || queue.repeat() == Repeat::Context && !queue.context().is_empty();
        let position = Duration::from_millis(queue.position_ms());
        if let Some(media) = &mut self.media {
            media.update(media::State {
                playback: if playing {
                    media::Playback::Playing
                } else if has_track
                    && queue.playback() != Playback::Stopped
                    && queue.playback() != Playback::Failed
                {
                    media::Playback::Paused
                } else {
                    media::Playback::Stopped
                },
                track,
                position,
                volume: Some(f64::from(model.prefs.volume)),
                shuffle: Some(queue.shuffled()),
                repeat: Some(match queue.repeat() {
                    Repeat::Off => media::Repeat::Off,
                    Repeat::Track => media::Repeat::Track,
                    Repeat::Context => media::Repeat::Playlist,
                }),
                controls: media::Controls {
                    play: has_track,
                    pause: has_track && queue.play_requested(),
                    next,
                    previous: has_track,
                    seek: has_track
                        && queue
                            .current()
                            .is_some_and(|entry| entry.media.duration_ms.is_some()),
                },
            });
            if self.seek_serial != model.seek_serial {
                self.seek_serial = model.seek_serial;
                media.seeked(position);
            }
        }
        if let Some(tray) = &mut self.tray
            && self.last_tray_controls != Some((playing, has_track, next))
        {
            self.last_tray_controls = Some((playing, has_track, next));
            tray.set_label(TOGGLE, if playing { "Pausar" } else { "Reproducir" });
            tray.set_enabled(TOGGLE, has_track);
            tray.set_enabled(PREVIOUS, has_track);
            tray.set_enabled(NEXT, next);
        }
    }
}

impl Resident for Desktop {
    fn closed(&self) -> Closed {
        self.after_close
    }

    fn window_gone(&mut self) {
        self.intent = Intent::None;
    }

    fn headless_frame(&mut self, ctx: &egui::Context) -> Headless {
        self.handle_commands(false);
        self.tick(ctx);
        self.app.persist(ctx);
        match std::mem::take(&mut self.intent) {
            Intent::Quit => Headless::Quit,
            intent @ (Intent::Show | Intent::Mini) => {
                self.mode = if intent == Intent::Mini {
                    Mode::Mini
                } else {
                    Mode::Main
                };
                Headless::Show
            }
            _ if !self.tray_visible() => {
                self.mode = Mode::Main;
                Headless::Show
            }
            _ => Headless::Wait,
        }
    }

    fn shutdown(&mut self) {
        self.tray = None;
        // The caller joins the retained audio and I/O workers after the loop.
    }
}

impl Desktop {
    fn clean_window(&mut self) {
        self.app.controls.clear();
        self.app.spectrum = None;
        self.app.visual_rms = None;
        if let Some(audio) = &self.app.model.as_ref().unwrap().audio {
            audio.stop_visual_samples();
        }
        #[cfg(any(target_os = "linux", target_os = "windows"))]
        {
            self.app.artwork_textures = artwork::Textures::default();
            self.app.lyrics = lyrics::Panel::new(&self.app.directory);
        }
    }
}

struct Window {
    desktop: Held<Desktop>,
    wayland: bool,
    recovery_checked: bool,
}

impl Drop for Window {
    fn drop(&mut self) {
        self.desktop.clean_window();
    }
}

impl eframe::App for Window {
    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        use egui::ViewportCommand as View;
        if !std::mem::replace(&mut self.recovery_checked, true) && !self.wayland {
            fastframe_shell::window::recover_offscreen(ctx, frame);
        }
        let desktop = &mut *self.desktop;
        desktop.handle_commands(true);
        desktop.tick(ctx);

        if desktop.mode == Mode::Main {
            desktop.geometry.remember(ctx);
        } else if !self.wayland {
            let model = desktop.app.model.as_mut().unwrap();
            let mut geometry = Geometry {
                position: model.prefs.desktop.mini_position,
                ..Geometry::default()
            };
            geometry.remember(ctx);
            if model.prefs.desktop.mini_position != geometry.position {
                model.prefs.desktop.mini_position = geometry.position;
                model.revision += 1;
            }
        }

        if !ctx.egui_wants_keyboard_input()
            && ctx.input_mut(|input| {
                input.consume_key(
                    egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                    egui::Key::M,
                )
            })
        {
            desktop.app.open_mini = true;
        }

        let closing = ctx.input(|input| input.viewport().close_requested());
        let intent = std::mem::take(&mut desktop.intent);
        let transition = match intent {
            Intent::Quit => Some(Closed::Quit),
            Intent::Hide if desktop.tray_visible() => Some(Closed::Hide),
            Intent::Show if desktop.mode == Mode::Mini => {
                desktop.mode = Mode::Main;
                Some(Closed::Reopen)
            }
            Intent::Mini if desktop.mode == Mode::Main => {
                desktop.mode = Mode::Mini;
                Some(Closed::Reopen)
            }
            Intent::Show | Intent::Mini => {
                ctx.send_viewport_cmd(View::Minimized(false));
                ctx.send_viewport_cmd(View::Focus);
                None
            }
            _ if desktop.mode == Mode::Mini && (closing || desktop.app.open_mini) => {
                desktop.mode = Mode::Main;
                Some(Closed::Reopen)
            }
            _ if desktop.app.open_mini => {
                desktop.mode = Mode::Mini;
                Some(Closed::Reopen)
            }
            _ if closing => Some(
                if desktop.tray_visible()
                    && desktop
                        .app
                        .model
                        .as_ref()
                        .unwrap()
                        .prefs
                        .desktop
                        .close_to_tray
                {
                    Closed::Hide
                } else {
                    Closed::Quit
                },
            ),
            _ => None,
        };
        if let Some(transition) = transition {
            desktop.app.open_mini = false;
            desktop.after_close = transition;
            ctx.send_viewport_cmd(View::Close);
        }
        if transition.is_some()
            || ctx.input(|input| {
                input.viewport().minimized == Some(true) || input.viewport().occluded == Some(true)
            })
        {
            desktop.app.persist(ctx);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if self.desktop.mode == Mode::Main {
            self.desktop.app.paint(ui);
        } else {
            self.desktop.app.compact_player(ui);
            self.desktop.app.persist(ui.ctx());
        }
    }
}

impl DoremiApp {
    fn compact_player(&mut self, ui: &mut egui::Ui) {
        use doremi_ui::{self as theme, Icon};
        self.controls.clear();
        let model = self.model.as_ref().unwrap();
        let palette = theme::palette(model.prefs.light, model.prefs.green);
        let current = model.queue.current().map(|entry| entry.media.clone());
        let title = current
            .as_ref()
            .map_or("Doremi", |song| song.title.as_str());
        let artist = current
            .as_ref()
            .map_or_else(String::new, |song| self.music_byline(song));
        let mut style = (*ui.ctx().global_style()).clone();
        theme::apply_to_style(&mut style, &palette);
        ui.ctx().set_global_style(style);
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(palette.window)
                    .inner_margin(egui::Margin::same(10)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let (rect, _) =
                        ui.allocate_exact_size(egui::Vec2::splat(52.0), egui::Sense::hover());
                    theme::cover_placeholder(ui, &palette, rect, 8.0, Icon::Music);
                    if let Some(song) = &current {
                        self.media_artwork(ui, song, rect, 8);
                    }
                    ui.vertical(|ui| {
                        ui.set_width((ui.available_width() - 4.0).max(80.0));
                        ui.add(egui::Label::new(egui::RichText::new(title).strong()).truncate());
                        ui.add(
                            egui::Label::new(egui::RichText::new(&artist).color(palette.secondary))
                                .truncate(),
                        );
                    });
                });
                self.progress(ui);
                ui.horizontal(|ui| {
                    self.transport_controls(ui, &palette, false);
                    if self.choice_button(ui, "restore-main", "Ventana principal", 136.0, false) {
                        self.open_mini = true;
                    }
                });
            });
        self.page_rect = [0.0, 0.0, 350.0, 148.0];
    }
}

fn id(entry: &doremi_core::queue::Entry) -> String {
    entry.id.as_u64().to_string()
}

fn toggle_play(model: &mut Model) {
    model.command(if model.queue.play_requested() {
        Command::Pause
    } else {
        Command::Play
    });
}

fn tray_config() -> Config {
    Config {
        id: "doremi",
        title: "Doremi".into(),
        icon: |size| {
            image::load_from_memory(ICON)
                .expect("Doremi icon")
                .resize_exact(
                    size as u32,
                    size as u32,
                    image::imageops::FilterType::Triangle,
                )
                .into_rgba8()
                .into_raw()
        },
        template_icon: None,
        themed_icon: true,
        menu: vec![
            MenuItem::action(SHOW, "Mostrar Doremi"),
            MenuItem::action(HIDE, "Ocultar Doremi"),
            MenuItem::action(MINI, "Abrir mini reproductor"),
            MenuItem::Separator,
            MenuItem::action(TOGGLE, "Reproducir").enabled(false),
            MenuItem::action(NEXT, "Siguiente").enabled(false),
            MenuItem::action(PREVIOUS, "Anterior").enabled(false),
            MenuItem::Separator,
            MenuItem::action(QUIT, "Salir de Doremi"),
        ],
    }
}
