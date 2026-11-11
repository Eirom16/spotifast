//! Disposable desktop authentication host. A private pipe bootstraps a
//! nonce-bound loopback channel; no browser redirect or HTTP cookie forwarding.
//! Only the helper links a web engine. It exits before credentials are returned
//! to the shared client for musical account verification and protected storage.
use crate::authorization;
use doremi_core::catalog::ProviderError as Error;
use serde::{Deserialize, Serialize};
use std::{
    io::{IsTerminal, Read, Write},
    net::{Ipv4Addr, SocketAddrV4, TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};
const MAX_FRAME: usize = 128 * 1024;
// Start with the service so it owns Google's sign-in and return parameters.
// This is the entry point used by Doremi-Rust's working cookie-capture flow.
pub const LOGIN_URL: &str = "https://music.youtube.com/";

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HelperRequest {
    port: u16,
    nonce: [u8; 32],
    pub profile: PathBuf,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Reply {
    status: u8,
    cookie: Option<String>,
}
struct Running {
    child: Child,
    profile: Option<tempfile::TempDir>,
    #[cfg(windows)]
    job: Option<WindowsJob>,
}
#[cfg(windows)]
struct WindowsJob(windows_sys::Win32::Foundation::HANDLE);
#[cfg(windows)]
impl WindowsJob {
    fn new(child: &Child) -> Result<Self, Error> {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::System::JobObjects::*;
        // The process is still waiting for its private stdin handshake here.
        // It cannot create any browser children before job ownership is set.
        unsafe {
            let job = Self(CreateJobObjectW(std::ptr::null(), std::ptr::null()));
            if job.0.is_null() {
                return Err(Error::Unavailable);
            }
            let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            if SetInformationJobObject(
                job.0,
                JobObjectExtendedLimitInformation,
                (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                std::mem::size_of_val(&limits) as u32,
            ) == 0
                || AssignProcessToJobObject(job.0, child.as_raw_handle().cast()) == 0
            {
                return Err(Error::Unavailable);
            }
            Ok(job)
        }
    }
}
#[cfg(windows)]
impl Drop for WindowsJob {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                windows_sys::Win32::Foundation::CloseHandle(self.0);
            }
        }
    }
}
impl Drop for Running {
    fn drop(&mut self) {
        // EOF asks the helper to drop its native WebView cooperatively first.
        let cooperative = self.child.stdin.take().is_some();
        let grace = Instant::now() + Duration::from_secs(2);
        let mut exited = false;
        while cooperative && Instant::now() < grace {
            if self.child.try_wait().ok().flatten().is_some() {
                exited = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        if !exited {
            #[cfg(target_os = "linux")]
            unsafe {
                libc::kill(-(self.child.id() as i32), libc::SIGKILL);
            }
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
        #[cfg(windows)]
        {
            self.job.take();
        } // Retire browser descendants before deleting the profile.
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
fn helper_path() -> Result<PathBuf, Error> {
    let executable = std::env::current_exe().map_err(|_| Error::Unavailable)?;
    let mut directory = executable.parent().ok_or(Error::Unavailable)?;
    // Explicit cargo examples live one directory below their sibling binaries.
    if directory.file_name().is_some_and(|name| name == "examples") {
        directory = directory.parent().ok_or(Error::Unavailable)?;
    }
    let path = directory.join(if cfg!(windows) {
        "doremi-auth.exe"
    } else {
        "doremi-auth"
    });
    if path.is_file() {
        Ok(path)
    } else {
        Err(Error::Unsupported)
    }
}
fn check(cancel: &AtomicBool, deadline: Instant) -> Result<(), Error> {
    if cancel.load(Ordering::Acquire) {
        Err(Error::Cancelled)
    } else if Instant::now() >= deadline {
        Err(Error::Timeout)
    } else {
        Ok(())
    }
}
fn bounded_read(
    stream: &mut TcpStream,
    bytes: &mut [u8],
    cancel: &AtomicBool,
    deadline: Instant,
) -> Result<(), Error> {
    let mut offset = 0;
    while offset < bytes.len() {
        check(cancel, deadline)?;
        match stream.read(&mut bytes[offset..]) {
            Ok(0) => return Err(Error::InvalidResponse),
            Ok(count) => offset += count,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(20))
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(_) => return Err(Error::Transport),
        }
    }
    Ok(())
}
fn receive(mut stream: TcpStream, nonce: &[u8; 32], cancel: &AtomicBool) -> Result<String, Error> {
    stream
        .set_nonblocking(true)
        .map_err(|_| Error::Unavailable)?;
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut actual = [0; 32];
    bounded_read(&mut stream, &mut actual, cancel, deadline)?;
    if actual != *nonce {
        return Err(Error::InvalidRequest);
    }
    let mut length = [0; 4];
    bounded_read(&mut stream, &mut length, cancel, deadline)?;
    let length = u32::from_be_bytes(length) as usize;
    if length > MAX_FRAME {
        return Err(Error::TooLarge);
    }
    let mut bytes = vec![0; length];
    bounded_read(&mut stream, &mut bytes, cancel, deadline)?;
    let reply: Reply = serde_json::from_slice(&bytes).map_err(|_| Error::InvalidResponse)?;
    if reply.status != 0 && reply.cookie.is_some() {
        return Err(Error::InvalidResponse);
    }
    match reply.status {
        0 => {
            let cookie = reply.cookie.ok_or(Error::InvalidResponse)?;
            if cookie.len() > 64 * 1024 {
                return Err(Error::TooLarge);
            }
            authorization(&cookie, 0).ok_or(Error::InvalidResponse)?;
            Ok(cookie)
        }
        1 => Err(Error::Cancelled),
        2 => Err(Error::Timeout),
        3 => Err(Error::AccessDenied),
        4 => Err(Error::Unavailable),
        _ => Err(Error::InvalidResponse),
    }
}
/// Called only by the bounded session worker following an explicit UI action.
pub fn consent_embedded(cancel: &AtomicBool, base: &Path) -> Result<String, Error> {
    check(cancel, Instant::now() + Duration::from_secs(600))?;
    let helper = helper_path()?;
    std::fs::create_dir_all(base).map_err(|_| Error::Unavailable)?;
    let profile = tempfile::Builder::new()
        .prefix("doremi-auth-")
        .tempdir_in(base)
        .map_err(|_| Error::Unavailable)?;
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).map_err(|_| Error::Unavailable)?;
    listener
        .set_nonblocking(true)
        .map_err(|_| Error::Unavailable)?;
    let mut nonce = [0; 32];
    getrandom::fill(&mut nonce).map_err(|_| Error::Unavailable)?;
    let request = HelperRequest {
        port: listener
            .local_addr()
            .map_err(|_| Error::Unavailable)?
            .port(),
        nonce,
        profile: profile
            .path()
            .canonicalize()
            .map_err(|_| Error::Unavailable)?,
    };
    let input = serde_json::to_vec(&request).map_err(|_| Error::InvalidRequest)?;
    if input.len() > 4096 {
        return Err(Error::TooLarge);
    }
    let mut command = Command::new(helper);
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let child = command
        .arg("--auth")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| Error::Unavailable)?;
    let mut running = Running {
        child,
        profile: Some(profile),
        #[cfg(windows)]
        job: None,
    };
    #[cfg(windows)]
    {
        running.job = Some(WindowsJob::new(&running.child)?);
    }
    running
        .child
        .stdin
        .as_mut()
        .ok_or(Error::Unavailable)?
        .write_all(&(input.len() as u32).to_be_bytes())
        .and_then(|_| running.child.stdin.as_mut().unwrap().write_all(&input))
        .map_err(|_| Error::Unavailable)?;
    let deadline = Instant::now() + Duration::from_secs(600);
    loop {
        check(cancel, deadline)?;
        match listener.accept() {
            Ok((stream, _)) => {
                let result = receive(stream, &nonce, cancel);
                // An unrelated local connection cannot inject a session. Reject
                // it without opening another browser or changing access method.
                if result == Err(Error::InvalidRequest) {
                    continue;
                }
                return result;
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(_) => return Err(Error::Unavailable),
        }
        if running
            .child
            .try_wait()
            .map_err(|_| Error::Unavailable)?
            .is_some()
        {
            return Err(Error::Unavailable);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}
/// Internal pipe protocol, never a URL, CLI cookie argument or log payload.
pub fn read_helper_request() -> Result<HelperRequest, Error> {
    if std::io::stdin().is_terminal() {
        return Err(Error::InvalidRequest);
    }
    let mut input = std::io::stdin().lock();
    let mut length = [0; 4];
    input
        .read_exact(&mut length)
        .map_err(|_| Error::InvalidRequest)?;
    let length = u32::from_be_bytes(length) as usize;
    if length > 4096 {
        return Err(Error::TooLarge);
    }
    let mut bytes = vec![0; length];
    input
        .read_exact(&mut bytes)
        .map_err(|_| Error::InvalidRequest)?;
    let request: HelperRequest =
        serde_json::from_slice(&bytes).map_err(|_| Error::InvalidRequest)?;
    if request.port == 0 || !request.profile.is_absolute() || !request.profile.is_dir() {
        return Err(Error::InvalidRequest);
    }
    Ok(request)
}
/// Call only after destroying the WebView and its native event loop/window.
pub fn send_helper_result(
    request: &HelperRequest,
    result: Result<String, Error>,
) -> Result<(), Error> {
    let reply = match result {
        Ok(cookie) if cookie.len() <= 64 * 1024 => Reply {
            status: 0,
            cookie: Some(cookie),
        },
        Ok(_) => Reply {
            status: 4,
            cookie: None,
        },
        Err(error) => Reply {
            status: match error {
                Error::Cancelled => 1,
                Error::Timeout => 2,
                Error::AccessDenied => 3,
                _ => 4,
            },
            cookie: None,
        },
    };
    let bytes = serde_json::to_vec(&reply).map_err(|_| Error::InvalidResponse)?;
    if bytes.len() > MAX_FRAME {
        return Err(Error::TooLarge);
    }
    let mut stream = TcpStream::connect_timeout(
        &SocketAddrV4::new(Ipv4Addr::LOCALHOST, request.port).into(),
        Duration::from_secs(2),
    )
    .map_err(|_| Error::Transport)?;
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .map_err(|_| Error::Transport)?;
    stream
        .write_all(&request.nonce)
        .and_then(|_| stream.write_all(&(bytes.len() as u32).to_be_bytes()))
        .and_then(|_| stream.write_all(&bytes))
        .map_err(|_| Error::Transport)
}
pub fn allowed_auth_url(value: &str) -> bool {
    let Ok(uri) = reqwest::Url::parse(value) else {
        return false;
    };
    uri.scheme() == "https"
        && uri.port().is_none()
        && uri.username().is_empty()
        && uri.password().is_none()
        && matches!(
            uri.host_str(),
            Some(
                "accounts.google.com"
                    | "consent.google.com"
                    | "myaccount.google.com"
                    | "music.youtube.com"
                    | "www.youtube.com"
                    | "youtube.com"
                    | "accounts.youtube.com"
                    | "consent.youtube.com"
            )
        )
}
pub fn provider_denied(value: &str) -> bool {
    let Ok(uri) = reqwest::Url::parse(value) else {
        return false;
    };
    uri.host_str() == Some("accounts.google.com")
        && (uri.path().contains("/signin/rejected")
            || uri.query_pairs().any(|(key, value)| {
                key == "error" && matches!(value.as_ref(), "disallowed_useragent" | "access_denied")
            }))
}
pub fn session_cookie(pairs: impl IntoIterator<Item = (String, String)>) -> Option<String> {
    let pairs: Vec<_> = pairs
        .into_iter()
        .filter(|(key, value)| {
            !key.contains([';', '\r', '\n', '=']) && !value.contains([';', '\r', '\n'])
        })
        .collect();
    if !pairs
        .iter()
        .any(|(key, value)| key == "LOGIN_INFO" && !value.is_empty())
    {
        return None;
    }
    let cookie = pairs
        .into_iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join("; ");
    (cookie.len() <= 64 * 1024 && authorization(&cookie, 0).is_some()).then_some(cookie)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ORIGIN;
    fn connected(bytes: Vec<u8>) -> (TcpStream, std::thread::JoinHandle<()>) {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let writer = std::thread::spawn(move || {
            let mut stream = TcpStream::connect(address).unwrap();
            let _ = stream.write_all(&bytes);
        });
        (listener.accept().unwrap().0, writer)
    }
    fn frame(nonce: [u8; 32], status: u8, cookie: Option<&str>) -> Vec<u8> {
        let bytes = serde_json::to_vec(&Reply {
            status,
            cookie: cookie.map(str::to_owned),
        })
        .unwrap();
        let mut frame = nonce.to_vec();
        frame.extend((bytes.len() as u32).to_be_bytes());
        frame.extend(bytes);
        frame
    }
    #[test]
    fn a_helper_result_requires_the_private_nonce_and_a_complete_bounded_frame() {
        let cancel = AtomicBool::new(false);
        let nonce = [17; 32];
        let fixture = "SAPISID=synthetic-ipc-only; LOGIN_INFO=synthetic";
        let (stream, writer) = connected(frame(nonce, 0, Some(fixture)));
        assert_eq!(receive(stream, &nonce, &cancel).unwrap(), fixture);
        writer.join().unwrap();
        let (stream, writer) = connected(frame([18; 32], 0, Some(fixture)));
        assert_eq!(receive(stream, &nonce, &cancel), Err(Error::InvalidRequest));
        writer.join().unwrap();
        let (stream, writer) = connected(frame(nonce, 3, None));
        assert_eq!(receive(stream, &nonce, &cancel), Err(Error::AccessDenied));
        writer.join().unwrap();
        let mut oversized = nonce.to_vec();
        oversized.extend(((MAX_FRAME + 1) as u32).to_be_bytes());
        let (stream, writer) = connected(oversized);
        assert_eq!(receive(stream, &nonce, &cancel), Err(Error::TooLarge));
        writer.join().unwrap();
        let mut partial = frame(nonce, 0, Some(fixture));
        partial.pop();
        let (stream, writer) = connected(partial);
        assert_eq!(
            receive(stream, &nonce, &cancel),
            Err(Error::InvalidResponse)
        );
        writer.join().unwrap();
        let (stream, writer) = connected(frame(nonce, 0, Some(fixture)));
        cancel.store(true, Ordering::Release);
        assert_eq!(receive(stream, &nonce, &cancel), Err(Error::Cancelled));
        writer.join().unwrap();
    }
    #[test]
    fn native_cookie_capture_and_navigation_do_not_grant_an_account_or_ignore_a_refusal() {
        assert!(session_cookie([("SAPISID".into(), "anonymous".into())]).is_none());
        assert!(session_cookie([("LOGIN_INFO".into(), "marker-only".into())]).is_none());
        assert!(
            session_cookie([
                ("LOGIN_INFO".into(), "fixture".into()),
                ("SAPISID".into(), "fixture".into())
            ])
            .is_some()
        );
        for url in [LOGIN_URL, ORIGIN, "https://consent.youtube.com/"] {
            assert!(allowed_auth_url(url));
        }
        for url in [
            "http://accounts.google.com/",
            "https://accounts.google.com.attacker.invalid/",
            "https://accounts.google.com:444/",
            "https://user@accounts.google.com/",
            "file:///tmp/",
            "javascript:alert(1)",
        ] {
            assert!(!allowed_auth_url(url));
        }
        assert!(provider_denied(
            "https://accounts.google.com/v3/signin/rejected?error=disallowed_useragent"
        ));
        assert!(!provider_denied(ORIGIN));
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn ending_an_attempt_reaps_its_helper_and_removes_only_its_private_profile() {
        let base = tempfile::tempdir().unwrap();
        let profile = tempfile::Builder::new()
            .prefix("doremi-auth-fixture-")
            .tempdir_in(base.path())
            .unwrap();
        let path = profile.path().to_owned();
        std::fs::write(path.join("fixture"), b"synthetic-only").unwrap();
        let child = Command::new("sleep").arg("30").spawn().unwrap();
        let pid = child.id();
        drop(Running {
            child,
            profile: Some(profile),
        });
        assert!(!path.exists());
        assert!(!PathBuf::from(format!("/proc/{pid}")).exists());
        assert!(base.path().exists());
    }
}
