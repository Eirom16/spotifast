//! Explicit sign-in for YouTube Music authentication.
//!
//! IMPORTANTE: Este módulo implementa el navegador embebido EXCLUSIVAMENTE para
//! autenticación OAuth de YouTube Music, conforme a la decisión documentada en
//! docs/_decisions/2025-embedded-browser-auth.md
//!
//! RESTRICCIÓN ESTRICTA: La ventana embebida solo se usa durante el proceso de
//! inicio de sesión y se destruye inmediatamente después de obtener las credenciales.
//! NUNCA debe usarse para navegación, reproducción o visualización de contenido.
//!
//! Desktop currently uses a disposable Chromium window or the system browser.
//! The loopback prototype in `embedded` is not a WebView and cannot receive
//! youtube.com cookies at a localhost origin. Never use it as a login provider.
use crate::authorization;
use doremi_core::catalog::ProviderError as Error;
use serde_json::{Value, json};
use std::{
    net::TcpStream,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};
const LOGIN: &str = "https://accounts.google.com/ServiceLogin?service=youtube&continue=https%3A%2F%2Fmusic.youtube.com%2F";
const POLL: Duration = Duration::from_secs(1);
/// Tubefast accepts a session that stays visible across several polls even
/// when Google's LOGIN_INFO marker is not exposed through this extension API.
const SETTLED_POLLS: u32 = 4;
struct Browser {
    child: Child,
    profile: Option<tempfile::TempDir>,
}
impl Drop for Browser {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(profile) = self.profile.take() {
            let path = profile.keep();
            for _ in 0..15 {
                if std::fs::remove_dir_all(&path).is_ok() || !path.exists() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(200));
            }
        }
    }
}
/// Each attempt gets a private wiped profile inside Doremi's data directory.
fn profile(base: &Path) -> Result<tempfile::TempDir, Error> {
    let _ = std::fs::create_dir_all(base);
    tempfile::Builder::new()
        .prefix("doremi-signin-")
        .tempdir_in(base)
        .map_err(|_| Error::Unavailable)
}
fn chromium_programs() -> Vec<PathBuf> {
    if cfg!(windows) {
        [
            (
                "ProgramFiles(x86)",
                r"Microsoft\Edge\Application\msedge.exe",
            ),
            ("ProgramFiles", r"Microsoft\Edge\Application\msedge.exe"),
            ("ProgramFiles", r"Google\Chrome\Application\chrome.exe"),
            ("LocalAppData", r"Google\Chrome\Application\chrome.exe"),
        ]
        .iter()
        .filter_map(|(root, path)| Some(PathBuf::from(std::env::var_os(root)?).join(path)))
        .collect()
    } else {
        [
            "google-chrome",
            "google-chrome-stable",
            "/opt/google/chrome/chrome",
            "chromium",
            "chromium-browser",
            "microsoft-edge",
        ]
        .iter()
        .map(PathBuf::from)
        .collect()
    }
}
/// Firefox fallback - cookies are stored unencrypted in the profile.
/// Try to launch the system default browser.
/// Returns Some(child) if launched successfully, None otherwise.
fn launch_default_browser(url: &str) -> Option<Child> {
    if cfg!(windows) {
        Command::new("cmd")
            .args(["/C", "start", "", url])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .ok()
    } else if cfg!(target_os = "linux") {
        // Try xdg-open first
        Command::new("xdg-open")
            .arg(url)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .ok()
            .or_else(|| {
                // Fallback to common browsers if xdg-open fails
                chromium_programs().iter().find_map(|program| {
                    Command::new(program)
                        .arg(url)
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        .spawn()
                        .ok()
                })
            })
    } else {
        None
    }
}
/// Interactive authorization for YouTube Music.
/// Dedicated Chromium supports capture from its own disposable profile.
/// The system browser requires the explicit clipboard fallback; no existing
/// browser profile is inspected.
pub fn consent(cancel: &AtomicBool, base: &Path) -> Result<String, Error> {
    let _ = std::fs::create_dir_all(base);

    if cancel.load(Ordering::Acquire) {
        return Err(Error::Cancelled);
    }
    // Second, try Chromium-based browsers with debug port (allows automatic cookie reading)
    if chromium_programs()
        .iter()
        .any(|path| path.is_file() || which_available(&path.to_string_lossy()))
    {
        return sign_in_chromium(cancel, base);
    }

    // Third, try system default browser
    // Note: This will work for any browser but cannot auto-read cookies
    // The UI will need to handle the manual cookie paste fallback
    launch_default_browser(LOGIN);

    // Wait for user to complete login in browser
    std::thread::sleep(Duration::from_secs(2));

    if cancel.load(Ordering::Acquire) {
        return Err(Error::Cancelled);
    }

    // Return Unsupported to trigger manual paste in UI
    Err(Error::Unsupported)
}
fn which_available(program: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|dir| dir.join(program).is_file()))
        .unwrap_or(false)
}

fn cookie(reply: &Value) -> Option<String> {
    let pairs: Vec<_> = reply["result"]["cookies"]
        .as_array()?
        .iter()
        .filter(|v| {
            v["domain"]
                .as_str()
                .is_some_and(|host| host == "youtube.com" || host.ends_with(".youtube.com"))
        })
        .filter_map(|v| Some(format!("{}={}", v["name"].as_str()?, v["value"].as_str()?)))
        .collect();
    let cookie = pairs.join("; ");
    authorization(&cookie, 0).is_some().then_some(cookie)
}
fn debugger(path: &Path) -> Option<u16> {
    // Chrome writes the chosen port into our private profile; 0 is not a port.
    std::fs::read_to_string(path.join("DevToolsActivePort"))
        .ok()?
        .lines()
        .next()?
        .trim()
        .parse::<u16>()
        .ok()
        .filter(|port| *port != 0)
}
/// One control connection reused across polls, the way Tubefast keeps it.
/// Recreating the connection per poll is wasteful.
struct Control {
    socket: tungstenite::WebSocket<TcpStream>,
    calls: u64,
}
impl Control {
    fn open(port: u16) -> Option<Self> {
        let stream = TcpStream::connect_timeout(
            &format!("127.0.0.1:{port}").parse().ok()?,
            Duration::from_secs(1),
        )
        .ok()?;
        stream.set_read_timeout(Some(Duration::from_secs(2))).ok()?;
        stream
            .set_write_timeout(Some(Duration::from_secs(2)))
            .ok()?;
        let (socket, _) = tungstenite::client::client_with_config(
            format!("ws://127.0.0.1:{port}/devtools/browser/doremi").as_str(),
            stream,
            Some(
                tungstenite::protocol::WebSocketConfig::default()
                    .max_message_size(Some(128 * 1024))
                    .max_frame_size(Some(128 * 1024)),
            ),
        )
        .ok()?;
        Some(Self { socket, calls: 0 })
    }
    fn cookies(&mut self) -> Option<Value> {
        self.calls += 1;
        let id = self.calls;
        self.socket
            .send(tungstenite::Message::Text(
                json!({"id":id,"method":"Storage.getCookies","params":{}})
                    .to_string()
                    .into(),
            ))
            .ok()?;
        for _ in 0..32 {
            let reply = self.socket.read().ok()?;
            let value = serde_json::from_slice::<Value>(&reply.into_data()).ok()?;
            if value["id"].as_u64() == Some(id) {
                // Never follow a debugger URL to a non-loopback service.
                return Some(value);
            }
        }
        None
    }
    fn close(&mut self) {
        self.calls += 1;
        let _ = self.socket.send(tungstenite::Message::Text(
            json!({"id":self.calls,"method":"Browser.close","params":{}})
                .to_string()
                .into(),
        ));
        let _ = self.socket.flush();
    }
}
pub fn sign_in_chromium(cancel: &AtomicBool, base: &Path) -> Result<String, Error> {
    let profile = profile(base)?;
    // Chrome selects its own port and writes it into our private profile.
    let child = chromium_programs()
        .iter()
        .find_map(|program| {
            Command::new(program)
                .arg(format!("--user-data-dir={}", profile.path().display()))
                .args([
                    "--remote-debugging-port=0",
                    "--remote-debugging-address=127.0.0.1",
                    "--no-first-run",
                    "--no-default-browser-check",
                    "--window-size=520,760",
                ])
                .arg(format!("--app={LOGIN}"))
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .ok()
        })
        .ok_or(Error::Unavailable)?;
    let mut browser = Browser {
        child,
        profile: Some(profile),
    };
    let start = Instant::now();
    let mut control: Option<Control> = None;
    let mut settled = 0_u32;
    while start.elapsed() < Duration::from_secs(600) {
        if cancel.load(Ordering::Acquire) {
            return Err(Error::Cancelled);
        }
        if browser
            .child
            .try_wait()
            .map_err(|_| Error::Unavailable)?
            .is_some()
        {
            return Err(Error::Cancelled);
        }
        std::thread::sleep(POLL);
        let Some(port) = debugger(browser.profile.as_ref().unwrap().path()) else {
            // Port file races with startup; closing a stale socket restarts it.
            control = None;
            continue;
        };
        if control.is_none() {
            control = Control::open(port);
        }
        let Some(reply) = control.as_mut().and_then(Control::cookies) else {
            control = None;
            settled = 0;
            continue;
        };
        match cookie(&reply) {
            Some(next) if next.contains("LOGIN_INFO=") => {
                if let Some(watch) = control.as_mut() {
                    watch.close();
                }
                return Ok(next);
            }
            Some(next) => {
                settled += 1;
                if settled == SETTLED_POLLS {
                    if let Some(watch) = control.as_mut() {
                        watch.close();
                    }
                    return Ok(next);
                }
            }
            None => settled = 0,
        }
    }
    Err(Error::Timeout)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reads_only_youtube_cookies() {
        let v = json!({"result":{"cookies":[{"domain":".youtube.com","name":"SAPISID","value":"fake"},{"domain":"accounts.google.com","name":"SID","value":"unrelated"},{"domain":"notyoutube.com","name":"SID","value":"wrong"}]}});
        assert_eq!(cookie(&v).as_deref(), Some("SAPISID=fake"));
    }
}
