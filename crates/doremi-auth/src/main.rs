//! A browser engine exists only in this disposable authentication process.
#[cfg(any(target_os = "linux", target_os = "windows"))]
fn main() {
    match std::env::args().nth(1).as_deref() {
        Some("--version") => println!("Doremi Auth {}-dev", env!("CARGO_PKG_VERSION")),
        Some("--auth") => {
            use doremi_youtube::embedded::{read_helper_request, send_helper_result};
            let Ok(request) = read_helper_request() else {
                std::process::exit(2)
            };
            let cancelled = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            let watched = cancelled.clone();
            if std::thread::Builder::new()
                .name("doremi-auth-owner".into())
                .spawn(move || {
                    use std::io::Read;
                    let _ = std::io::stdin().read(&mut [0]);
                    watched.store(true, std::sync::atomic::Ordering::Release);
                })
                .is_err()
            {
                std::process::exit(1);
            }
            // Native resources leave scope before any secret crosses the IPC.
            let result = consent(&request.profile, &cancelled);
            if send_helper_result(&request, result).is_err() {
                std::process::exit(1);
            }
        }
        _ => {
            eprintln!("Doremi authentication helper; launch sign-in from Doremi.");
            std::process::exit(2);
        }
    }
}
#[cfg(not(any(target_os = "linux", target_os = "windows")))]
fn main() {}

#[cfg(any(target_os = "linux", target_os = "windows"))]
fn consent(
    profile: &std::path::Path,
    cancelled: &std::sync::atomic::AtomicBool,
) -> Result<String, doremi_core::catalog::ProviderError> {
    use doremi_core::catalog::ProviderError as Error;
    use doremi_youtube::embedded::{LOGIN_URL, allowed_auth_url, provider_denied, session_cookie};
    use std::{
        cell::Cell,
        rc::Rc,
        time::{Duration, Instant},
    };
    use tao::{
        event::{Event, WindowEvent},
        event_loop::{ControlFlow, EventLoop},
        platform::run_return::EventLoopExtRunReturn,
        window::WindowBuilder,
    };
    use wry::{WebContext, WebViewBuilder};
    let mut event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title("Doremi · Iniciar sesión en YouTube Music")
        .with_inner_size(tao::dpi::LogicalSize::new(900.0, 700.0))
        .build(&event_loop)
        .map_err(|_| Error::Unavailable)?;
    let mut context = WebContext::new(Some(profile.to_owned()));
    let denied = Rc::new(Cell::new(false));
    let navigation = denied.clone();
    let builder = WebViewBuilder::new_with_web_context(&mut context)
        .with_incognito(true)
        .with_devtools(false)
        .with_autoplay(false)
        .with_url(LOGIN_URL)
        .with_navigation_handler(move |url| {
            if provider_denied(&url) {
                navigation.set(true);
                return false;
            }
            allowed_auth_url(&url)
        })
        .with_new_window_req_handler(|_, _| wry::NewWindowResponse::Deny)
        .with_download_started_handler(|_, _| false);
    #[cfg(target_os = "windows")]
    let webview = builder.build(&window).map_err(|_| Error::Unavailable)?;
    #[cfg(target_os = "linux")]
    let webview = {
        use tao::platform::unix::WindowExtUnix;
        use wry::WebViewBuilderExtUnix;
        builder
            .build_gtk(window.default_vbox().ok_or(Error::Unavailable)?)
            .map_err(|_| Error::Unavailable)?
    };
    let deadline = Instant::now() + Duration::from_secs(600);
    let mut result = Err(Error::Cancelled);
    event_loop.run_return(|event, _, control| {
        *control = ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(250));
        if let Event::WindowEvent {
            event: WindowEvent::CloseRequested,
            ..
        } = event
        {
            result = Err(Error::Cancelled);
            *control = ControlFlow::Exit;
            return;
        }
        if let Event::MainEventsCleared = event {
            if cancelled.load(std::sync::atomic::Ordering::Acquire) {
                result = Err(Error::Cancelled);
                *control = ControlFlow::Exit;
                return;
            }
            if denied.get() {
                result = Err(Error::AccessDenied);
                *control = ControlFlow::Exit;
                return;
            }
            if Instant::now() >= deadline {
                result = Err(Error::Timeout);
                *control = ControlFlow::Exit;
                return;
            }
            // Read only the service's cookies even if Google's confirmation
            // page has not navigated away. Account verification stays in Rust.
            if let Ok(cookies) = webview.cookies_for_url(doremi_youtube::ORIGIN)
                && let Some(cookie) = session_cookie(
                    cookies
                        .into_iter()
                        .map(|cookie| (cookie.name().to_owned(), cookie.value().to_owned())),
                )
            {
                result = Ok(cookie);
                *control = ControlFlow::Exit;
            }
        }
    });
    drop(webview);
    drop(window);
    drop(event_loop);
    drop(context);
    result
}
