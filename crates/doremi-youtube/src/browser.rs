//! Explicit sign-in in a dedicated external browser profile; adapted from
//! Tubefast auth.rs, MIT. The profile lives under Doremi's own data directory
//! and is wiped out at the start of every attempt, so no external browser or
//! browser state beyond that window is ever read. Only youtube.com cookies
//! inside that one window are observed.
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
fn programs() -> Vec<PathBuf> {
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
/// Google's sign-in risk engine rejects several Chromium builds ("this browser
/// or app may not be secure") observed in the field, while Firefox sign-ins are
/// the ones Doremi-Rust and Tubefast rely on through YouTube sessions. Firefox
/// cookies are stored unencrypted in the profile, so a disposable profile is
/// enough and reading another browser's state is never needed.
fn firefox() -> Vec<PathBuf> {
    if cfg!(windows) {
        [("ProgramFiles", r"Mozilla Firefox\firefox.exe")]
            .iter()
            .filter_map(|(root, path)| Some(PathBuf::from(std::env::var_os(root)?).join(path)))
            .collect()
    } else {
        ["firefox", "firefox-esr"]
            .iter()
            .map(PathBuf::from)
            .collect()
    }
}
const FIREFOX_PREFS: &[u8] = b"\
user_pref(\"browser.shell.checkDefaultBrowser\", false);\n\
user_pref(\"browser.sessionstore.resume_from_crash\", false);\n\
user_pref(\"browser.warnOnQuit\", false);\n\
user_pref(\"toolkit.legacyUserProfileCustomizations.stylesheets\", false);\n";
fn write_firefox_prefs(profile: &Path) -> Result<(), Error> {
    std::fs::write(profile.join("prefs.js"), FIREFOX_PREFS).map_err(|_| Error::Unavailable)
}
/// Reads youtube.com cookies out of the disposable profile's own store.
fn firefox_cookies(profile: &Path) -> Option<String> {
    let store = rusqlite::Connection::open_with_flags(
        profile.join("cookies.sqlite"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .ok()?;
    let read = |names: &mut Vec<(String, String, String)>| -> rusqlite::Result<()> {
        let mut statement = store.prepare("SELECT name, value, host FROM moz_cookies")?;
        let mut rows = statement.query([])?;
        while let Some(row) = rows.next()? {
            let name: String = row.get(0)?;
            let value: String = row.get(1)?;
            let host: String = row.get(2)?;
            names.push((name, value, host));
        }
        Ok(())
    };
    let mut rows = Vec::new();
    read(&mut rows).ok()?;
    let cookie = rows
        .iter()
        .filter(|(_, _, host)| host == "youtube.com" || host.ends_with(".youtube.com"))
        .filter(|(name, value, _)| !name.is_empty() && !value.is_empty())
        .map(|(name, value, _)| format!("{name}={value}"))
        .collect::<Vec<_>>()
        .join("; ");
    authorization(&cookie, 0).is_some().then_some(cookie)
}
fn firefox_sign_in(cancel: &AtomicBool, base: &Path) -> Result<String, Error> {
    let profile = tempfile::Builder::new()
        .prefix("doremi-signin-ff-")
        .tempdir_in(base)
        .map_err(|_| Error::Unavailable)?;
    write_firefox_prefs(profile.path())?;
    let mut child = firefox()
        .iter()
        .find_map(|program| {
            Command::new(program)
                .arg("--no-remote")
                .arg(format!("--profile={}", profile.path().display()))
                .arg("--new-window")
                .arg(LOGIN)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .ok()
        })
        .ok_or(Error::Unavailable)?;
    // The disposable profile holds only this login attempt's state, so the
    // window is allowed to stay open until the user closes it after signing
    // in. Then the cookies are flushed and read from that profile alone.
    let start = Instant::now();
    let mut accepted: Option<String> = None;
    while start.elapsed() < Duration::from_secs(600) {
        if cancel.load(Ordering::Acquire) {
            return Err(Error::Cancelled);
        }
        if let Some(cookie) = firefox_cookies(profile.path()) {
            if cookie.contains("LOGIN_INFO=") {
                return Ok(cookie);
            }
            // The session is there but the music service marker lag: give
            // the browser a few more polls, and fall back to whatever the
            // profile finally holds when the window closes.
            accepted.get_or_insert(cookie);
        } else if accepted.is_some() {
            accepted = None;
        }
        if child.try_wait().map_err(|_| Error::Unavailable)?.is_some() {
            if let Some(final_value) = firefox_cookies(profile.path()) {
                return Ok(final_value);
            }
            return Err(Error::Cancelled);
        }
        std::thread::sleep(POLL);
    }
    Err(Error::Timeout)
}
/// Interactive authorization in an external browser window. Firefox is tried
/// first when installed; Edge/Chrome fall back to the debug-port flow adapted
/// from Tubefast. Both never touch an existing browser profile.
pub fn consent(cancel: &AtomicBool, base: &Path) -> Result<String, Error> {
    let _ = std::fs::create_dir_all(base);
    if firefox()
        .iter()
        .any(|path| path.is_file() || which_available(&path.to_string_lossy()))
    {
        firefox_sign_in(cancel, base)
    } else {
        sign_in_chromium(cancel, base)
    }
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
    let child = programs()
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
