//! Process-owned music actor. No EGL, Activity, or JNI references. Only this
//! thread mutates the app; platform and GL callers use a bounded mailbox.
use super::*;
use std::sync::mpsc::{self, SyncSender};

type Job = Box<dyn FnOnce(&mut DoremiApp, &mut egui::Context) + Send>;
pub(super) struct Runtime {
    commands: Option<SyncSender<Job>>,
    #[cfg(test)]
    thread: Option<std::thread::JoinHandle<()>>,
    snapshot: Arc<Mutex<String>>,
    pub(super) artwork: Arc<Mutex<Option<(u64, Arc<egui::ColorImage>)>>>,
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
            let mut app = DoremiApp::new_context(&ctx, directory);
            app.internal_ui = internal;
            if normal {
                app.model.as_mut().unwrap().music.session.restore();
                app.model.as_mut().unwrap().youtube_page(doremi_youtube::Request::Home);
            }
            let mut epoch = 1_u64;
            let mut identity = String::new();
            let mut wants_play = false;
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
                let next_identity = format!("{:?}:{:?}", model.queue.current().map(|e|e.id), model.audio.as_ref().and_then(Engine::current_request));
                if identity != next_identity {
                    identity = next_identity;
                    epoch += 1;
                }
                match model.queue.playback() {
                    Playback::Playing => wants_play = true,
                    Playback::Paused | Playback::Stopped | Playback::Failed => wants_play = false,
                    Playback::Loading => {},
                }
                let current = model.queue.current();
                let image = current.and_then(|entry|model.artwork.media_pixels(&entry.media.id));
                let artwork_version = image.as_ref().map(|(version,_)|*version);
                *published_artwork.lock().unwrap()=image;
                let state = serde_json::json!({
                    "artwork_version":artwork_version,
                    "epoch":epoch,"intent":model.transport_serial,"instance":app.instance,"state":format!("{:?}",model.queue.playback()),
                    "active":model.real_audio() && (wants_play || model.queue.playback()==Playback::Loading),
                    "position":model.queue.position_ms(),"duration":current.and_then(|e|e.media.duration_ms),
                    "title":current.map(|e|&e.media.title),
                    "artist":current.and_then(|e|model.music.byline(&e.media.id)),
                    "id":current.map(|e|e.media.id.local()),"occurrence":current.map(|e|e.id.as_u64()),
                    "notice":model.notice,
                    "stopped":!matches!(model.queue.playback(),Playback::Loading|Playback::Playing) && model.audio.as_ref().and_then(Engine::current_request).is_none(),
                });
                *published.lock().unwrap() = state.to_string();
                app.persist(&ctx);
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
        drop(runtime);
        let _ = std::fs::remove_dir_all(directory);
    }
}
