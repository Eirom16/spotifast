//! Bounded desktop yt-dlp adapter. The catalog and native audio engine are unchanged.
//! Session input and the ephemeral media grant use private pipes, never argv/files.
use crate::{Error, session::Session, stream::Stream};
use doremi_core::media::MediaId;
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Write},
    process::{Child, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

const BRIDGE: &str = include_str!("yt_dlp_bridge.py");
const OUTPUT_LIMIT: usize = 64 * 1024;
static WORK: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(1);

#[derive(Serialize)]
struct Request<'a> {
    schema: u8,
    id: &'a str,
    session: Option<&'a Session>,
}

// No Debug/Serialize: this reply contains a signed transport URL.
#[derive(Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
enum Reply {
    Ok {
        id: String,
        url: String,
        length: u64,
        duration_ms: u64,
        user_agent: String,
    },
    AccessDenied,
    RateLimited,
    Timeout,
    Unsupported,
    UnsupportedFormat,
    InvalidRequest,
    TooLarge,
    Unavailable,
}

fn decode(media: &MediaId, data: &[u8]) -> Result<Stream, Error> {
    match serde_json::from_slice(data).map_err(|_| Error::InvalidResponse)? {
        Reply::Ok {
            id,
            url,
            length,
            duration_ms,
            user_agent,
        } if id == media.local() => Stream::extracted(url, length, duration_ms, user_agent),
        Reply::Ok { .. } => Err(Error::InvalidResponse),
        Reply::AccessDenied => Err(Error::AccessDenied),
        Reply::RateLimited => Err(Error::RateLimited {
            retry_after_ms: None,
        }),
        Reply::Timeout => Err(Error::Timeout),
        Reply::Unsupported => Err(Error::Unsupported),
        Reply::UnsupportedFormat => Err(Error::UnsupportedFormat),
        Reply::InvalidRequest => Err(Error::InvalidRequest),
        Reply::TooLarge => Err(Error::TooLarge),
        Reply::Unavailable => Err(Error::Unavailable),
    }
}

struct Stop(Arc<AtomicBool>);
impl Drop for Stop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}

pub(super) async fn resolve(media: &MediaId, session: Option<&Session>) -> Result<Stream, Error> {
    let input = serde_json::to_vec(&Request {
        schema: 1,
        id: media.local(),
        session,
    })
    .map_err(|_| Error::InvalidRequest)?;
    if input.len() > 128 * 1024 {
        return Err(Error::TooLarge);
    }
    let permit = WORK.acquire().await.map_err(|_| Error::Unavailable)?;
    let stop = Stop(Arc::new(AtomicBool::new(false)));
    let cancelled = stop.0.clone();
    let data = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let mut command = Command::new(if cfg!(windows) { "python" } else { "python3" });
        command.args(["-I", "-B", "-c", BRIDGE]).env_clear();
        for name in [
            "PATH",
            "HOME",
            "USERPROFILE",
            "SYSTEMROOT",
            "WINDIR",
            "TEMP",
            "TMP",
            "LANG",
            "LC_ALL",
            "SSL_CERT_FILE",
            "SSL_CERT_DIR",
            "HTTPS_PROXY",
            "HTTP_PROXY",
            "ALL_PROXY",
            "NO_PROXY",
            "https_proxy",
            "http_proxy",
            "all_proxy",
            "no_proxy",
        ] {
            if let Some(value) = std::env::var_os(name) {
                command.env(name, value);
            }
        }
        run(command, input, cancelled, crate::stream::RESOLVE_TIMEOUT)
    })
    .await
    .map_err(|_| Error::Unavailable)??;
    decode(media, &data)
}

struct Running {
    child: Child,
    retired: bool,
    #[cfg(windows)]
    job: Option<crate::embedded::WindowsJob>,
}
impl Running {
    fn retire(&mut self) {
        if self.retired {
            return;
        }
        self.retired = true;
        #[cfg(target_os = "linux")]
        unsafe {
            libc::kill(-(self.child.id() as i32), libc::SIGKILL);
        }
        #[cfg(windows)]
        self.job.take();
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
impl Drop for Running {
    fn drop(&mut self) {
        self.retire();
    }
}

fn collect(mut pipe: impl Read, limit: usize, overflow: Arc<AtomicBool>) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut block = [0; 4096];
    while let Ok(n) = pipe.read(&mut block) {
        if n == 0 {
            break;
        }
        let keep = n.min(limit.saturating_sub(bytes.len()));
        bytes.extend_from_slice(&block[..keep]);
        if keep != n {
            overflow.store(true, Ordering::Release);
        }
    }
    bytes
}

fn run(
    mut command: Command,
    input: Vec<u8>,
    cancelled: Arc<AtomicBool>,
    timeout: Duration,
) -> Result<Vec<u8>, Error> {
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| Error::Unsupported)?;
    let mut running = Running {
        child,
        retired: false,
        #[cfg(windows)]
        job: None,
    };
    #[cfg(windows)]
    {
        // Python reads its handshake before importing yt-dlp/spawning Deno.
        running.job = Some(crate::embedded::WindowsJob::new(&running.child)?);
    }
    let mut stdin = running.child.stdin.take().ok_or(Error::Transport)?;
    let stdout = running.child.stdout.take().ok_or(Error::Transport)?;
    let stderr = running.child.stderr.take().ok_or(Error::Transport)?;
    let overflow = Arc::new(AtomicBool::new(false));
    let signal = overflow.clone();
    let output = std::thread::spawn(move || collect(stdout, OUTPUT_LIMIT, signal));
    let signal = overflow.clone();
    let errors = std::thread::spawn(move || collect(stderr, 16 * 1024, signal));
    let writer = std::thread::spawn(move || stdin.write_all(&input));
    let started = Instant::now();
    let result = loop {
        if cancelled.load(Ordering::Acquire) {
            break Err(Error::Cancelled);
        }
        if overflow.load(Ordering::Acquire) {
            break Err(Error::TooLarge);
        }
        if started.elapsed() >= timeout {
            break Err(Error::Timeout);
        }
        match running.child.try_wait() {
            Ok(Some(status)) => break Ok(status.success()),
            Ok(None) => std::thread::sleep(Duration::from_millis(10)),
            Err(_) => break Err(Error::Transport),
        }
    };
    // Retire descendants before joining pipe readers, even on an early exit.
    running.retire();
    let written = writer.join().map_err(|_| Error::Transport)?;
    let bytes = output.join().map_err(|_| Error::Transport)?;
    let _redacted = errors.join().map_err(|_| Error::Transport)?;
    let success = result?;
    if overflow.load(Ordering::Acquire) {
        return Err(Error::TooLarge);
    }
    if !success || written.is_err() {
        return Err(Error::Unavailable);
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn command(code: &str) -> Command {
        let mut command = Command::new(if cfg!(windows) { "python" } else { "python3" });
        command.args(["-I", "-B", "-c", code]);
        command
    }

    #[test]
    fn grants_require_matching_identity_safe_headers_size_and_a_live_first_party_lease() {
        let media = crate::id("abcdefghijk").unwrap();
        let expires = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 600;
        let good = json!({
            "status":"ok","id":media.local(),
            "url":format!("https://rr.googlevideo.com/videoplayback?expire={expires}&sig=PRIVATE_FIXTURE"),
            "length":100000,"duration_ms":180000,"user_agent":"Fixture/1"
        });
        assert!(decode(&media, &serde_json::to_vec(&good).unwrap()).is_ok());
        for (field, value) in [
            ("id", json!("other-track")),
            (
                "url",
                json!("https://untrusted.invalid/?secret=PRIVATE_FIXTURE"),
            ),
            (
                "url",
                json!("https://rr.googlevideo.com/videoplayback?expire=1"),
            ),
            ("length", json!(0)),
            ("length", json!(512 * 1024 * 1024_u64 + 1)),
            ("duration_ms", json!(6 * 3600 * 1000_u64 + 1)),
            ("user_agent", json!("Fixture\r\nCookie: PRIVATE_FIXTURE")),
        ] {
            let mut bad = good.clone();
            bad[field] = value;
            let error = decode(&media, &serde_json::to_vec(&bad).unwrap())
                .err()
                .unwrap();
            assert!(!format!("{error:?}").contains("PRIVATE_FIXTURE"));
        }
        let mut unknown = good;
        unknown["unexpected"] = json!("PRIVATE_FIXTURE");
        assert!(matches!(
            decode(&media, &serde_json::to_vec(&unknown).unwrap()),
            Err(Error::InvalidResponse)
        ));
    }

    #[test]
    fn a_refusal_is_typed_without_exposing_private_stdin_or_stderr() {
        let code = r#"import json,sys
request=json.load(sys.stdin)
assert request["secret"] not in "\0".join(sys.argv)
sys.stderr.write(request["secret"])
print('{"status":"access_denied"}')
"#;
        let input = serde_json::to_vec(&json!({"secret":"PRIVATE_FIXTURE"})).unwrap();
        let bytes = run(
            command(code),
            input,
            Arc::new(AtomicBool::new(false)),
            Duration::from_secs(3),
        )
        .unwrap();
        assert!(!String::from_utf8_lossy(&bytes).contains("PRIVATE_FIXTURE"));
        assert!(matches!(
            decode(&crate::id("abcdefghijk").unwrap(), &bytes),
            Err(Error::AccessDenied)
        ));
    }

    #[test]
    fn stdout_and_stderr_overflow_are_bounded_and_retire_the_child() {
        for pipe in ["stdout", "stderr"] {
            let code = format!(
                "import sys,time; sys.{pipe}.write('x'*200000); sys.{pipe}.flush(); time.sleep(10)"
            );
            assert!(matches!(
                run(
                    command(&code),
                    vec![0; 128 * 1024],
                    Arc::new(AtomicBool::new(false)),
                    Duration::from_secs(3)
                ),
                Err(Error::TooLarge)
            ));
        }
    }

    #[test]
    fn timeout_and_cancellation_close_descendant_pipes_before_returning() {
        let code = r#"import subprocess,sys,time
subprocess.Popen([sys.executable,"-I","-c","import time;time.sleep(10)"])
time.sleep(10)
"#;
        let started = Instant::now();
        assert!(matches!(
            run(
                command(code),
                vec![],
                Arc::new(AtomicBool::new(false)),
                Duration::from_millis(500)
            ),
            Err(Error::Timeout)
        ));
        assert!(started.elapsed() < Duration::from_secs(3));
        let signal = Arc::new(AtomicBool::new(false));
        let cancel = signal.clone();
        let started = Instant::now();
        let worker =
            std::thread::spawn(move || run(command(code), vec![], cancel, Duration::from_secs(3)));
        std::thread::sleep(Duration::from_millis(500));
        drop(Stop(signal));
        assert!(matches!(worker.join().unwrap(), Err(Error::Cancelled)));
        assert!(started.elapsed() < Duration::from_secs(3));
    }
}
