//! Process-owned music actor. No EGL, Activity, or JNI references. Only this
//! thread mutates the app; platform and GL callers use a bounded mailbox.
use super::*;
use std::sync::mpsc::{self, SyncSender};

/// A GL surface may retire while its CPU frame is in flight. egui requires
/// explicit acknowledgement before an unapplied texture delta is discarded.
/// Owning that cleanup here covers both a queued frame and a failed send.
pub(super) struct RenderedFrame {
    pub output: egui::FullOutput,
    pub primitives: Vec<egui::ClippedPrimitive>,
    pub data: String,
}
impl RenderedFrame {
    pub fn new(ctx: &egui::Context, output: egui::FullOutput) -> Self {
        let mut frame = Self {
            output,
            primitives: Vec::new(),
            data: String::new(),
        };
        frame.primitives =
            ctx.tessellate(frame.output.shapes.clone(), frame.output.pixels_per_point);
        frame
    }
}
impl Drop for RenderedFrame {
    fn drop(&mut self) {
        // On a rendered frame the painter has already consumed these. On a
        // retired surface its entire GL context is being discarded instead.
        self.output.textures_delta.clear();
    }
}

type Artwork = Option<(u64, Arc<egui::ColorImage>)>;
type Job = Box<dyn FnOnce(&mut DoremiApp, &mut egui::Context) + Send>;
pub(super) struct Runtime {
    commands: Option<SyncSender<Job>>,
    #[cfg(test)]
    thread: Option<std::thread::JoinHandle<()>>,
    snapshot: Arc<Mutex<String>>,
    pub(super) artwork: Arc<Mutex<Artwork>>,
}
impl Runtime {
    pub fn start(directory: PathBuf, normal: bool, internal: bool) -> Self {
        let (commands, receive) = mpsc::sync_channel::<Job>(64);
        let snapshot = Arc::new(Mutex::new("{}".into()));
        let published = snapshot.clone();
        let artwork = Arc::new(Mutex::new(None));
        let published_artwork = artwork.clone();
        let thread = std::thread::Builder::new().name("doremi-playback".into()).spawn(move || {
            let mut ctx = egui::Context::default();
            let profile = directory.parent().filter(|parent|parent.file_name().is_some_and(|name|name=="qa")).and_then(|_|directory.file_name()).map(|name|name.to_string_lossy().into_owned());
            let mut app = DoremiApp::new_context(&ctx, directory);
            app.internal_ui = internal;
            if normal {
                app.model.as_mut().unwrap().music.session.restore();
                app.model.as_mut().unwrap().youtube_page(doremi_youtube::Request::Home);
            }
            let mut epoch = 1_u64;
            let mut identity = String::new();
            loop {
                let active = matches!(app.model.as_ref().unwrap().queue.playback(),Playback::Playing|Playback::Loading);
                match receive.recv_timeout(Duration::from_millis(if active {100} else {500})) {
                    Ok(job) => job(&mut app, &mut ctx),
                    Err(mpsc::RecvTimeoutError::Timeout) => {},
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }
                // Completes downloads, Ready, position and EOF without a surface.
                app.poll_background(&ctx);
                let model = app.model.as_ref().unwrap();
                let stopped = model.queue.playback()==Playback::Stopped || !matches!(model.queue.playback(),Playback::Loading|Playback::Playing) && model.audio.as_ref().and_then(Engine::current_request).is_none();
                let next_identity = format!("{:?}:{}:{}", model.queue.current().map(|e|e.id), model.music.session.revision, stopped);
                if identity != next_identity {
                    identity = next_identity;
                    epoch += 1;
                }
                let current = model.queue.current();
                let image = current.and_then(|entry|model.artwork.media_pixels(&entry.media.id, entry.media.music.as_ref().and_then(|data| data.artwork.as_deref())));
                let artwork_version = image.as_ref().map(|(version,_)|*version);
                *published_artwork.lock().unwrap()=image;
                let (artist, album) = current.map(|entry| {
                    let (known_artist, known_album) = model.music.session_metadata(&entry.media.id);
                    let metadata = entry.media.music.as_ref();
                    (
                        metadata.and_then(|data| data.byline.clone()).or(known_artist),
                        metadata.and_then(|data| data.album.as_ref().map(|album| album.name.clone())).or(known_album),
                    )
                }).unwrap_or_default();
                let state = serde_json::json!({
                    "profile":profile,"internal_ui":internal,"album":album,
                    "artwork_version":artwork_version,
                    "epoch":epoch,"intent":model.transport_serial,"instance":app.instance,"state":format!("{:?}",model.queue.playback()),
                    "active":model.real_audio() && model.queue.play_requested() && matches!(model.queue.playback(),Playback::Playing|Playback::Loading),
                    "position":model.queue.position_ms(),"duration":current.and_then(|e|e.media.duration_ms),
                    "title":current.map(|e|&e.media.title),
                    "artist":artist,
                    "id":current.map(|e|e.media.id.local()),"occurrence":current.map(|e|e.id.as_u64()),
                    "notice":model.notice,
                    "stopped":stopped,
                });
                *published.lock().unwrap() = state.to_string();
                if active || app.pending || app.pending_write != 0 || app.saved_revision != model.revision {
                    app.persist(&ctx);
                }
            }
            drop(app);
            DoremiApp::shutdown();
        }).expect("music owner thread");
        #[cfg(not(test))]
        let _ = thread;
        Self {
            commands: Some(commands),
            artwork,
            snapshot,
            #[cfg(test)]
            thread: Some(thread),
        }
    }
    pub fn post(
        &self,
        job: impl FnOnce(&mut DoremiApp, &mut egui::Context) + Send + 'static,
    ) -> bool {
        self.commands
            .as_ref()
            .is_some_and(|sender| sender.try_send(Box::new(job)).is_ok())
    }
    pub fn control(&self, command: i32, position: i64, epoch: i64) -> bool {
        let snapshot = self.snapshot.clone();
        self.post(move |app, _| {
            // Reject transport actions from a retired notification/track. Focus
            // revocation and shutdown always apply, including while loading.
            let snapshot: serde_json::Value =
                serde_json::from_str(&snapshot.lock().unwrap()).unwrap_or_default();
            if epoch != 0 && snapshot["epoch"].as_i64() != Some(epoch) {
                return;
            }
            let model = app.model.as_mut().unwrap();
            match command {
                0 => {
                    model.enable_audio();
                    model.command(Command::Play);
                }
                1 => model.command(Command::Pause),
                2 => model.command(Command::Next),
                3 => model.command(Command::Previous),
                4 => {
                    model.command(Command::Cancel);
                    model.focus = false;
                    model.audio.take();
                }
                5 => model.command(Command::Seek(position.max(0) as u64)),
                6 => {
                    model.focus = true;
                    if let Some(engine) = &model.audio {
                        engine.set_playing(model.queue.playback() == Playback::Playing);
                    }
                }
                8 => {
                    // Abandoning a grant must close the output gate without
                    // overwriting a newer user intent queued by the UI.
                    model.focus = false;
                    if let Some(engine) = &model.audio {
                        engine.set_playing(false);
                    }
                }
                7 => {
                    model.focus = false;
                    model.command_with_import_policy(Command::Pause, false);
                }
                _ => {}
            }
        })
    }
    pub fn snapshot(&self) -> String {
        self.snapshot.lock().unwrap().clone()
    }
}

#[cfg(test)]
impl Drop for Runtime {
    fn drop(&mut self) {
        self.commands.take();
        if let Some(thread) = self.thread.take() {
            thread.join().unwrap();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn replacing_the_surface_context_keeps_the_music_model_and_draws_again() {
        let _guard = crate::tests::UI_TEST_LOCK.lock().unwrap();
        let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/background-context-recreate");
        DoremiApp::shutdown();
        let mut ctx = egui::Context::default();
        let mut app = DoremiApp::new_context(&ctx, directory.clone());
        app.internal_ui = true;
        app.model
            .as_mut()
            .unwrap()
            .start_context(songs()[..3].to_vec(), 0);
        app.model.as_mut().unwrap().page = Page::Player;
        let instance = app.instance;
        for replacement in 0..3 {
            for frame_index in 0..3 {
                let output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(384.0, 658.0),
                        )),
                        ..Default::default()
                    },
                    |ui| app.draw(ui),
                );
                let mut frame = RenderedFrame::new(&ctx, output);
                frame.data = "retired surface".into();
                assert_eq!(frame.data, "retired surface");
                assert!(!frame.primitives.is_empty());
                let (send, receive) = mpsc::sync_channel(1);
                if frame_index == 0 {
                    assert!(!frame.output.textures_delta.is_empty());
                }
                if replacement % 2 == 0 {
                    // Renderer went away before the worker could send.
                    drop(receive);
                    assert!(send.send(frame).is_err());
                } else {
                    // Renderer went away after a frame had been queued.
                    assert!(send.send(frame).is_ok());
                    drop(receive);
                }
            }
            ctx = egui::Context::default();
            doremi_ui::install(&ctx);
            app.artwork_textures = artwork::Textures::default();
            assert_eq!(app.instance, instance);
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
    fn music_actor_serializes_transport_and_keeps_one_instance_without_frames() {
        let _guard = crate::tests::UI_TEST_LOCK.lock().unwrap();
        let directory =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/background-actor-test");
        DoremiApp::shutdown();
        let runtime = Runtime::start(directory.clone(), false, true);
        let (send, receive) = mpsc::sync_channel(1);
        assert!(runtime.post(move |app, _| {
            app.model
                .as_mut()
                .unwrap()
                .start_context(songs()[..3].to_vec(), 0);
            send.send(app.instance).unwrap();
        }));
        assert!(runtime.artwork.lock().unwrap().is_none());
        let instance = receive.recv_timeout(Duration::from_secs(5)).unwrap();
        for command in [
            Command::Pause,
            Command::Play,
            Command::Next,
            Command::Previous,
            Command::Pause,
        ] {
            assert!(runtime.post(move |app, _| app.model.as_mut().unwrap().command(command)));
        }
        let (send, receive) = mpsc::sync_channel(1);
        runtime.post(move |app, ctx| {
            // A new EGL context reconnects to exactly the same model.
            *ctx = egui::Context::default();
            assert_eq!(app.instance, instance);
            let model = app.model.as_ref().unwrap();
            assert_eq!(model.queue.playback(), Playback::Paused);
            assert_eq!(model.queue.current().unwrap().media.id.local(), "dawn");
            send.send(()).unwrap();
        });
        receive.recv_timeout(Duration::from_secs(5)).unwrap();
        std::thread::sleep(Duration::from_millis(150));
        let state: serde_json::Value = serde_json::from_str(&runtime.snapshot()).unwrap();
        assert_eq!(state["instance"], instance);
        assert_eq!(state["state"], "Paused");
        // A paused in-flight load stays paused when Ready eventually arrives.
        let (send, receive) = mpsc::sync_channel(1);
        runtime.post(move |app, _| {
            let model = app.model.as_mut().unwrap();
            let effects = model
                .queue
                .command(Command::StartContext {
                    media: songs()[..2].to_vec(),
                    start: 0,
                    order: Order::Sequential,
                })
                .unwrap();
            let request = effects
                .into_iter()
                .find_map(|effect| {
                    if let Effect::Load { request, .. } = effect {
                        Some(request)
                    } else {
                        None
                    }
                })
                .unwrap();
            send.send(request).unwrap();
        });
        let request = receive.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(runtime.control(1, 0, 0));
        let (send, receive) = mpsc::sync_channel(1);
        runtime.post(move |app, _| {
            app.model.as_mut().unwrap().event(Event::Ready(request));
            assert_eq!(
                app.model.as_ref().unwrap().queue.playback(),
                Playback::Paused
            );
            send.send(()).unwrap();
        });
        receive.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(runtime.control(0, 0, -999)); // a retired notification must do nothing
        // Negative epochs are not accepted as a wildcard; only zero is internal.
        let (send, receive) = mpsc::sync_channel(1);
        runtime.post(move |app, _| {
            send.send(app.model.as_ref().unwrap().queue.playback())
                .unwrap();
        });
        assert_eq!(
            receive.recv_timeout(Duration::from_secs(5)).unwrap(),
            Playback::Paused
        );
        assert!(runtime.control(8, 0, 0));
        let (send, receive) = mpsc::sync_channel(1);
        runtime.post(move |app, _| {
            let model = app.model.as_ref().unwrap();
            send.send((model.focus, model.queue.playback())).unwrap();
        });
        assert_eq!(
            receive.recv_timeout(Duration::from_secs(5)).unwrap(),
            (false, Playback::Paused)
        );
        assert!(runtime.control(4, 0, 0));
        let (send, receive) = mpsc::sync_channel(1);
        runtime.post(move |app, _| {
            send.send(app.model.as_ref().unwrap().audio.is_none())
                .unwrap();
        });
        assert!(receive.recv_timeout(Duration::from_secs(5)).unwrap());
        drop(runtime);
        let _ = std::fs::remove_dir_all(directory);
    }
}
