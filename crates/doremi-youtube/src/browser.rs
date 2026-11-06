//! Explicit sign-in in a disposable external browser profile; adapted from
//! Tubefast auth.rs (MIT). Reads only youtube.com cookies from that profile.
use crate::authorization;
use doremi_core::catalog::ProviderError as Error;
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::TcpStream,
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};
const LOGIN: &str = "https://accounts.google.com/ServiceLogin?service=youtube&continue=https%3A%2F%2Fmusic.youtube.com%2F";
struct Browser {
    child: Child,
    _profile: tempfile::TempDir,
}
impl Drop for Browser {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
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
fn debugger(port: u16) -> Option<String> {
    let mut stream = TcpStream::connect_timeout(
        &format!("127.0.0.1:{port}").parse().ok()?,
        Duration::from_secs(1),
    )
    .ok()?;
    stream.set_read_timeout(Some(Duration::from_secs(1))).ok()?;
    stream
        .set_write_timeout(Some(Duration::from_secs(1)))
        .ok()?;
    write!(
        stream,
        "GET /json/version HTTP/1.0\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n"
    )
    .ok()?;
    let mut bytes = Vec::new();
    stream.take(16 * 1024).read_to_end(&mut bytes).ok()?;
    let start = bytes.windows(4).position(|s| s == b"\r\n\r\n")? + 4;
    let value: Value = serde_json::from_slice(&bytes[start..]).ok()?;
    let address = value["webSocketDebuggerUrl"].as_str()?;
    // Never follow a debugger URL to a non-loopback service.
    address
        .starts_with(&format!("ws://127.0.0.1:{port}/devtools/browser/"))
        .then(|| address.to_owned())
}
pub fn sign_in(cancel: &AtomicBool) -> Result<String, Error> {
    let profile = tempfile::Builder::new()
        .prefix("doremi-signin-")
        .tempdir()
        .map_err(|_| Error::Unavailable)?;
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
        _profile: profile,
    };
    let start = Instant::now();
    let mut calls = 0_u64;
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
        std::thread::sleep(Duration::from_millis(250));
        let Some(port) =
            std::fs::read_to_string(browser._profile.path().join("DevToolsActivePort"))
                .ok()
                .and_then(|v| v.lines().next()?.parse::<u16>().ok())
        else {
            continue;
        };
        let Some(address) = debugger(port) else {
            continue;
        };
        let stream = TcpStream::connect(("127.0.0.1", port)).map_err(|_| Error::Transport)?;
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .map_err(|_| Error::Transport)?;
        stream
            .set_write_timeout(Some(Duration::from_secs(2)))
            .map_err(|_| Error::Transport)?;
        let config = tungstenite::protocol::WebSocketConfig::default()
            .max_message_size(Some(128 * 1024))
            .max_frame_size(Some(128 * 1024));
        let (mut socket, _) =
            tungstenite::client::client_with_config(address.as_str(), stream, Some(config))
                .map_err(|_| Error::Transport)?;
        calls += 1;
        socket
            .send(tungstenite::Message::Text(
                json!({"id":calls,"method":"Storage.getCookies","params":{}})
                    .to_string()
                    .into(),
            ))
            .map_err(|_| Error::Transport)?;
        for _ in 0..32 {
            let reply = socket.read().map_err(|_| Error::Transport)?;
            if let Ok(value) = serde_json::from_slice::<Value>(&reply.into_data()) {
                if value["id"].as_u64() != Some(calls) {
                    continue;
                }
                if let Some(cookie) = cookie(&value)
                    .filter(|v| v.split(';').any(|p| p.trim().starts_with("LOGIN_INFO=")))
                {
                    let _ = socket.send(tungstenite::Message::Text(
                        json!({"id":calls+1,"method":"Browser.close","params":{}})
                            .to_string()
                            .into(),
                    ));
                    return Ok(cookie);
                }
                break;
            }
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
