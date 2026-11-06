//! Bounded session worker. Browser, keyring and network never run in draw().
use super::*;
use doremi_core::catalog::ProviderError;
use doremi_youtube::{Account, Client};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver, SyncSender},
};
pub type SharedClient = Arc<tokio::sync::Mutex<Option<Result<Client, ProviderError>>>>;
#[derive(Clone, Copy)]
enum Intent {
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    Restore,
    Connect,
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    ImportLegacy,
    Disconnect,
    Expire,
}
struct Job {
    intent: Intent,
    ctx: egui::Context,
}
struct Completion {
    result: Result<Option<Account>, ProviderError>,
    durable: bool,
    focus: bool,
}
pub(super) struct State {
    pub client: SharedClient,
    pub account: Option<Account>,
    pub error: Option<ProviderError>,
    pub busy: bool,
    pub durable: bool,
    pub revision: u64,
    pending: Option<Intent>,
    commands: Option<SyncSender<Job>>,
    events: Option<Receiver<Completion>>,
    thread: Option<std::thread::JoinHandle<()>>,
    cancel: Arc<AtomicBool>,
}
impl State {
    pub fn new() -> Self {
        Self {
            client: Arc::new(tokio::sync::Mutex::new(None)),
            account: None,
            error: None,
            busy: false,
            durable: false,
            revision: 0,
            pending: None,
            commands: None,
            events: None,
            thread: None,
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    pub fn restore(&mut self) {
        self.pending = Some(Intent::Restore);
    }
    pub fn connect(&mut self) {
        self.pending = Some(Intent::Connect);
        self.error = None;
    }
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    pub fn import_legacy(&mut self) {
        self.pending = Some(Intent::ImportLegacy);
        self.error = None;
    }
    pub fn disconnect(&mut self) {
        self.cancel.store(true, Ordering::Release);
        self.pending = Some(Intent::Disconnect);
    }
    // Desktop calls only after the event loop; drawing and Android lifecycle
    // never wait for native browser/keyring cleanup.
    pub fn shutdown(&mut self) {
        self.cancel();
        self.commands.take();
        self.events.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Release);
    }
    pub fn poll(&mut self, ctx: &egui::Context) {
        if self.account.is_some()
            && self.client.try_lock().ok().is_some_and(
                |client| matches!(client.as_ref(),Some(Ok(client)) if !client.signed_in()),
            )
        {
            self.account = None;
            self.error = Some(ProviderError::AccessDenied);
            self.durable = false;
            self.pending = Some(Intent::Expire);
        }
        if let Some(events) = &self.events {
            while let Ok(completion) = events.try_recv() {
                if completion.focus {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                }
                self.revision = self
                    .revision
                    .checked_add(1)
                    .expect("session revision exhausted");
                self.busy = false;
                self.durable = completion.durable;
                match completion.result {
                    Ok(account) => {
                        self.account = account;
                        self.error = None;
                    }
                    Err(error) => {
                        self.error = Some(error);
                        self.account = None;
                    }
                }
            }
        }
        if self.busy {
            ctx.request_repaint_after(Duration::from_millis(100));
            return;
        }
        let Some(intent) = self.pending else { return };
        if self.commands.is_none() {
            let (send, receive) = mpsc::sync_channel::<Job>(1);
            let (complete, events) = mpsc::sync_channel(1);
            let client = self.client.clone();
            let cancel = self.cancel.clone();
            let thread = std::thread::Builder::new()
                .name("doremi-youtube-session".into())
                .spawn(move || {
                    let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
                        .max_blocking_threads(1)
                        .enable_all()
                        .build()
                    else {
                        return;
                    };
                    while let Ok(job) = receive.recv() {
                        let (result, durable) = run(job.intent, &client, &runtime, &cancel);
                        if complete
                            .send(Completion {
                                result,
                                durable,
                                focus: matches!(job.intent, Intent::Connect),
                            })
                            .is_err()
                        {
                            break;
                        }
                        job.ctx.request_repaint();
                    }
                    runtime.shutdown_timeout(Duration::from_millis(100));
                });
            match thread {
                Ok(thread) => self.thread = Some(thread),
                Err(_) => {
                    self.error = Some(ProviderError::Unavailable);
                    self.pending = None;
                    return;
                }
            }
            self.commands = Some(send);
            self.events = Some(events);
        }
        self.cancel.store(false, Ordering::Release);
        match self.commands.as_ref().unwrap().try_send(Job {
            intent,
            ctx: ctx.clone(),
        }) {
            Ok(()) => {
                self.pending = None;
                self.busy = true;
                ctx.request_repaint_after(Duration::from_millis(100));
            }
            Err(_) => {
                self.error = Some(ProviderError::Unavailable);
                self.pending = None;
            }
        }
    }
}
impl Drop for State {
    fn drop(&mut self) {
        self.cancel();
    }
}
#[cfg(any(target_os = "linux", target_os = "windows"))]
fn run(
    intent: Intent,
    client: &SharedClient,
    runtime: &tokio::runtime::Runtime,
    cancel: &AtomicBool,
) -> (Result<Option<Account>, ProviderError>, bool) {
    use doremi_youtube::{browser, credentials};
    if matches!(intent, Intent::Disconnect | Intent::Expire) {
        runtime.block_on(async {
            if let Some(Ok(client)) = &mut *client.lock().await {
                client.clear_session();
            }
        });
        let result = credentials::forget().and({
            if matches!(intent, Intent::Expire) {
                Err(ProviderError::AccessDenied)
            } else {
                Ok(None)
            }
        });
        return (result, false);
    }
    let cookie = match intent {
        Intent::Restore => match credentials::load() {
            Ok(Some(cookie)) => Ok(cookie),
            Ok(None) => return (Ok(None), false),
            Err(error) => return (Err(error), false),
        },
        Intent::Connect => browser::sign_in(cancel),
        Intent::ImportLegacy => credentials::import_doremi_rust(),
        Intent::Disconnect | Intent::Expire => unreachable!(),
    };
    let cookie = match cookie {
        Ok(cookie) => cookie,
        Err(error) => return (Err(error), false),
    };
    if cancel.load(Ordering::Acquire) {
        return (Err(ProviderError::Cancelled), false);
    }
    let result = runtime.block_on(async {
        match client.lock().await.get_or_insert_with(Client::new) {
            Ok(client) => client.sign_in(cookie.clone()).await.map(Some),
            Err(error) => Err(*error),
        }
    });
    if cancel.load(Ordering::Acquire) {
        runtime.block_on(async {
            if let Some(Ok(client)) = &mut *client.lock().await {
                client.clear_session();
            }
        });
        return (Err(ProviderError::Cancelled), false);
    }
    if result.is_ok() {
        let durable = credentials::store(&cookie).is_ok();
        (result, durable)
    } else {
        if result == Err(ProviderError::AccessDenied) {
            let _ = credentials::forget();
        }
        (result, false)
    }
}
#[cfg(target_os = "android")]
fn run(
    intent: Intent,
    _client: &SharedClient,
    _runtime: &tokio::runtime::Runtime,
    _cancel: &AtomicBool,
) -> (Result<Option<Account>, ProviderError>, bool) {
    // Custom Tabs cannot expose HttpOnly cookies to an app. Do not substitute
    // Google identity OAuth for access to the YouTube Music private library.
    if matches!(intent, Intent::Disconnect | Intent::Expire) {
        (Ok(None), false)
    } else {
        (Err(ProviderError::Unsupported), false)
    }
}
impl DoremiApp {
    pub(super) fn youtube_account(&mut self, ui: &mut egui::Ui) {
        ui.heading("Cuenta de YouTube Music");
        let state = &self.model.as_ref().unwrap().music.session;
        if state.busy {
            ui.spinner();
            ui.label("Conectando con YouTube Music…");
            if self.button(ui, "youtube-cancel-signin", "Cancelar conexión", 220.0) {
                self.model.as_ref().unwrap().music.session.cancel();
            }
            return;
        }
        if let Some(account) = &state.account {
            ui.label(format!("Conectado: {}", account.name));
            if !state.durable {
                ui.weak("Sesión temporal: no se pudo guardar en el almacén protegido del sistema.");
            }
            if self.button(ui, "youtube-signout", "Cerrar sesión", 220.0) {
                let model = self.model.as_mut().unwrap();
                model.command(Command::Cancel);
                model.music.cancel();
                model.music.session.disconnect();
            }
            for (name, label, request) in [
                (
                    "youtube-personal-home",
                    "Inicio personalizado",
                    doremi_youtube::Request::Home,
                ),
                (
                    "youtube-library",
                    "Playlists de YouTube Music",
                    doremi_youtube::Request::Library,
                ),
                (
                    "youtube-liked",
                    "Canciones favoritas",
                    doremi_youtube::Request::LikedSongs,
                ),
                (
                    "youtube-albums",
                    "Álbumes guardados",
                    doremi_youtube::Request::SavedAlbums,
                ),
                (
                    "youtube-artists",
                    "Artistas de la biblioteca",
                    doremi_youtube::Request::SavedArtists,
                ),
            ] {
                if self.button(ui, name, label, 280.0) {
                    self.model.as_mut().unwrap().music.youtube_page(request);
                    self.navigate(Page::Search);
                }
            }
        } else {
            ui.label("Busca y explora sin cuenta. Conecta una cuenta para consultar los datos musicales que YouTube Music permita.");
            #[cfg(any(target_os="linux",target_os="windows"))]
            ui.small("Autoriza en una ventana externa de Chrome, Chromium o Edge. Doremi recibe la sesión de esa ventana y la guarda en el almacén protegido del sistema.");
            #[cfg(target_os="android")]
            ui.small("La conexión de cuenta en Android aún no está disponible: el navegador externo no permite compartir la sesión musical con Doremi.");
            if self.button(ui, "youtube-signin", "Iniciar sesión", 220.0) {
                self.model.as_mut().unwrap().music.session.connect();
            }
            #[cfg(any(target_os = "linux", target_os = "windows"))]
            {
                ui.small("También puedes conectar una sesión ya autorizada y guardada en el llavero por Doremi-Rust. Solo se leerá cuando pulses el botón.");
                if self.button(
                    ui,
                    "youtube-import-session",
                    "Conectar cuenta de Doremi-Rust",
                    280.0,
                ) {
                    self.model.as_mut().unwrap().music.session.import_legacy();
                }
            }
        }
        if let Some(error) = self.model.as_ref().unwrap().music.session.error {
            ui.colored_label(ui.visuals().error_fg_color,match error {
                ProviderError::AccessDenied=>"YouTube Music rechazó la sesión. Vuelve a conectar la cuenta.",
                ProviderError::Cancelled=>"Conexión cancelada.",
                ProviderError::Unsupported=>"La conexión de cuenta todavía no está implementada en esta plataforma.",
                ProviderError::Timeout=>"La conexión agotó el tiempo de espera.",
                _=>"No se pudo conectar o restaurar la cuenta. Comprueba el navegador y el almacén de credenciales.",
            });
        }
    }
}
