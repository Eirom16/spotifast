//! Bounded session worker. Browser, keyring and network never run in draw().
use super::*;
use doremi_core::catalog::ProviderError;
use doremi_youtube::{Account, Client};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver, SyncSender},
};
pub type SharedClient = Arc<tokio::sync::Mutex<Option<Result<Client, ProviderError>>>>;
// Renderer recreation must not reuse a retired Android document request.
#[cfg(target_os = "android")]
static PICKER_GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
#[derive(Clone)]
enum Intent {
    Restore,
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    Connect(PathBuf),
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    ImportLegacy,
    #[cfg(target_os = "android")]
    ImportDocument(String),
    Paste(String),
    Disconnect,
    Expire,
}
impl Intent {
    fn focus(&self) -> bool {
        match self {
            #[cfg(any(target_os = "linux", target_os = "windows"))]
            Self::Connect(_) | Self::ImportLegacy => true,
            #[cfg(target_os = "android")]
            Self::ImportDocument(_) => true,
            Self::Paste(_) => true,
            _ => false,
        }
    }
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
    /// Clipboard paste buffer for the documented cookie fallback path. Never
    /// persisted; cookies must not enter snapshots or logs.
    pub pasted: String,
    pending: Option<Intent>,
    restored: bool,
    #[cfg(target_os = "android")]
    picker: Option<(u64, Instant)>,
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
            pasted: String::new(),
            pending: None,
            restored: false,
            #[cfg(target_os = "android")]
            picker: None,
            commands: None,
            events: None,
            thread: None,
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }
    pub fn restore(&mut self) {
        if !self.restored {
            self.restored = true;
            self.pending = Some(Intent::Restore);
        }
    }
    pub fn connect(&mut self, profile: PathBuf) {
        self.restored = true;
        self.error = None;
        #[cfg(any(target_os = "linux", target_os = "windows"))]
        {
            self.pending = Some(Intent::Connect(profile));
        }
        #[cfg(target_os = "android")]
        {
            let _ = profile;
            let Ok(previous) =
                PICKER_GENERATION.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                    value.checked_add(1)
                })
            else {
                self.error = Some(ProviderError::Exhausted);
                return;
            };
            let generation = previous + 1;
            self.picker = Some((generation, Instant::now() + Duration::from_secs(120)));
        }
    }
    #[cfg(target_os = "android")]
    pub fn picker_request(&self) -> u64 {
        self.picker.map_or(0, |(generation, _)| generation)
    }
    #[cfg(target_os = "android")]
    pub fn selected_document(&mut self, generation: u64, uri: String) {
        let Some((active, deadline)) = self.picker else {
            return;
        };
        if generation != active {
            return;
        }
        self.picker = None;
        if Instant::now() >= deadline {
            self.error = Some(ProviderError::Timeout);
            return;
        }
        if uri.is_empty() {
            self.error = Some(ProviderError::Cancelled);
            return;
        }
        if !uri.starts_with("content://") || uri.len() > 4096 || uri.chars().any(char::is_control) {
            self.error = Some(ProviderError::InvalidRequest);
            return;
        }
        self.pending = Some(Intent::ImportDocument(uri));
    }
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    pub fn import_legacy(&mut self) {
        self.restored = true;
        self.pending = Some(Intent::ImportLegacy);
        self.error = None;
    }
    /// The explicit clipboard fallback for platforms where the separate
    /// browser window cannot hand over the session itself.
    pub fn paste(&mut self) {
        self.restored = true;
        self.pending = Some(Intent::Paste(self.pasted.clone()));
        self.error = None;
    }
    pub fn disconnect(&mut self) {
        self.cancel();
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
    pub fn cancel(&mut self) {
        self.cancel.store(true, Ordering::Release);
        self.pending = None;
        #[cfg(target_os = "android")]
        {
            self.picker = None;
        }
    }
    pub fn poll(&mut self, ctx: &egui::Context) {
        #[cfg(target_os = "android")]
        if let Some((_, deadline)) = self.picker {
            if Instant::now() >= deadline {
                self.picker = None;
                self.error = Some(ProviderError::Timeout);
            } else {
                ctx.request_repaint_after(Duration::from_millis(100));
            }
        }
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
        let Some(intent) = self.pending.clone() else {
            return;
        };
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
                        let focus = job.intent.focus();
                        let (result, durable) = run(job.intent, &client, &runtime, &cancel);
                        if complete
                            .send(Completion {
                                result,
                                durable,
                                focus,
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
fn run(
    intent: Intent,
    client: &SharedClient,
    runtime: &tokio::runtime::Runtime,
    cancel: &AtomicBool,
) -> (Result<Option<Account>, ProviderError>, bool) {
    #[cfg(target_os = "android")]
    use super::android_native::youtube_credentials as credentials;
    #[cfg(any(target_os = "linux", target_os = "windows"))]
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
    if cancel.load(Ordering::Acquire) {
        return (Err(ProviderError::Cancelled), false);
    }
    let cookie = match intent {
        Intent::Restore => match credentials::load() {
            Ok(Some(cookie)) => Ok(cookie),
            Ok(None) => return (Ok(None), false),
            Err(error) => return (Err(error), false),
        },
        #[cfg(any(target_os = "linux", target_os = "windows"))]
        Intent::Connect(profile) => browser::consent(cancel, &profile),
        #[cfg(any(target_os = "linux", target_os = "windows"))]
        Intent::ImportLegacy => credentials::import_doremi_rust(),
        #[cfg(target_os = "android")]
        Intent::ImportDocument(uri) => {
            credentials::read_document(&uri).and_then(|bytes| doremi_youtube::legacy_cookie(&bytes))
        }
        Intent::Paste(text) => doremi_youtube::paste_cookie(&text),
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
        if cancel.load(Ordering::Acquire) {
            if durable {
                let _ = credentials::forget();
            }
            runtime.block_on(async {
                if let Some(Ok(client)) = &mut *client.lock().await {
                    client.clear_session();
                }
            });
            return (Err(ProviderError::Cancelled), false);
        }
        (result, durable)
    } else {
        if result == Err(ProviderError::AccessDenied) {
            let _ = credentials::forget();
        }
        (result, false)
    }
}
impl DoremiApp {
    pub(super) fn youtube_account(&mut self, ui: &mut egui::Ui) {
        ui.heading("Cuenta de YouTube Music");
        let state = &self.model.as_ref().unwrap().music.session;
        #[cfg(target_os = "android")]
        if state.picker.is_some() {
            ui.label("Selecciona el archivo de la sesión autorizada en el selector del sistema.");
            if self.button(ui, "youtube-cancel-signin", "Cancelar conexión", 220.0) {
                self.model.as_mut().unwrap().music.session.cancel();
            }
            return;
        }
        if state.busy {
            ui.spinner();
            ui.label("Conectando con YouTube Music…");
            if self.button(ui, "youtube-cancel-signin", "Cancelar conexión", 220.0) {
                self.model.as_mut().unwrap().music.session.cancel();
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
            ui.small("Autoriza en una ventana externa de Chrome/Edge/Chromium o tu navegador por defecto. Doremi intenta leer la sesión automáticamente (Chrome/Edge/Chromium) o te pedirá pegar la cookie manualmente. La sesión se guarda en el almacén protegido del sistema.");
            #[cfg(target_os="android")]
            ui.small("Conecta una sesión ya autorizada seleccionando su archivo de Doremi-Rust. Doremi verificará la cuenta y la guardará cifrada en este dispositivo. La autorización directa con Google todavía no está disponible.");
            if self.button(
                ui,
                "youtube-signin",
                if cfg!(target_os = "android") {
                    "Importar sesión autorizada"
                } else {
                    "Iniciar sesión"
                },
                260.0,
            ) {
                let model = self.model.as_mut().unwrap();
                #[cfg(any(target_os = "linux", target_os = "windows"))]
                {
                    let base = self
                        .writer
                        .as_ref()
                        .map(|writer| writer.directory().to_owned())
                        .unwrap_or_else(std::env::temp_dir);
                    model.music.session.connect(base.join("signin-profile"));
                }
                #[cfg(target_os = "android")]
                {
                    // There is no desktop browser window on Android; connect
                    // switches to the system document picker instead.
                    model.music.session.connect(PathBuf::new());
                }
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
            ui.add_space(12.0);
            ui.small("Alternativa: copia la cabecera Cookie de music.youtube.com con la sesión ya iniciada (F12, Red, copiar Cabeceras de solicitud) y pégala aquí.");
            let pasted = ui.add_sized(
                [ui.available_width(), 56.0],
                egui::TextEdit::multiline(&mut self.model.as_mut().unwrap().music.session.pasted)
                    .id(egui::Id::new(TextField::SessionCookie.key()))
                    .hint_text("SID=…; SAPISID=…; LOGIN_INFO=…")
                    .font(egui::TextStyle::Monospace)
                    .char_limit(16384),
            );
            if pasted.has_focus() {
                self.editor = TextField::SessionCookie;
            }
            self.follow_editor(ui, &pasted, TextField::SessionCookie);
            self.record("session-cookie", &pasted);
            if !self.model.as_ref().unwrap().music.session.pasted.is_empty()
                && self.button(
                    ui,
                    "youtube-cookie-signin",
                    "Conectar con esa cookie",
                    260.0,
                )
            {
                self.model.as_mut().unwrap().music.session.paste();
            }
        }
        if let Some(error) = self.model.as_ref().unwrap().music.session.error {
            ui.colored_label(ui.visuals().error_fg_color,match error {
                ProviderError::AccessDenied=>"YouTube Music rechazó la sesión. Vuelve a conectar la cuenta.",
                ProviderError::Cancelled=>"Conexión cancelada.",
                ProviderError::Unsupported=>"Navegador no compatible para lectura automática de cookies. Usa Chrome/Edge/Chromium o pega la cookie manualmente.",
                ProviderError::InvalidRequest|ProviderError::InvalidResponse=>"No se pudo validar el archivo o la sesión de YouTube Music.",
                ProviderError::TooLarge=>"El archivo de sesión supera el tamaño admitido.",
                ProviderError::Timeout=>"La conexión agotó el tiempo de espera.",
                _=>"No se pudo conectar o restaurar la cuenta. Comprueba el navegador y el almacén de credenciales.",
            });
            if self.model.as_ref().unwrap().music.session.account.is_none()
                && matches!(
                    error,
                    ProviderError::Unavailable
                        | ProviderError::InvalidResponse
                        | ProviderError::AccessDenied
                )
                && self.button(
                    ui,
                    "youtube-forget-session",
                    "Eliminar sesión guardada",
                    260.0,
                )
            {
                self.model.as_mut().unwrap().music.session.disconnect();
            }
        }
    }
}
