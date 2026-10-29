use crate::{commons, http};
use doremi_core::catalog::ProviderError;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    io::Read,
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

const EXECUTABLE_SHA256: &str = "e51e69e89489f199530f13d32a4342da3ddb2cf34842f364603a066db05afbf3";
fn collect(mut source: impl Read, limit: usize) -> (Vec<u8>, bool) {
    let mut data = Vec::new();
    let mut oversized = false;
    let mut block = [0; 4096];
    while let Ok(n) = source.read(&mut block) {
        if n == 0 {
            break;
        }
        let keep = n.min(limit.saturating_sub(data.len()));
        data.extend_from_slice(&block[..keep]);
        oversized |= keep < n;
    }
    (data, oversized)
}
pub fn resolve(executable: &Path, info: &commons::Info) -> Result<String, ProviderError> {
    let file = std::fs::File::open(executable).map_err(|_| ProviderError::Unavailable)?;
    let mut bytes = Vec::new();
    file.take(10 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| ProviderError::Transport)?;
    if bytes.len() > 10 * 1024 * 1024 {
        return Err(ProviderError::TooLarge);
    }
    if format!("{:x}", Sha256::digest(&bytes)) != EXECUTABLE_SHA256 {
        return Err(ProviderError::AccessDenied);
    }
    let mut command = Command::new("python3");
    command
        .args(["-I", "-B"])
        .arg(executable)
        .args([
            "--ignore-config",
            "--no-plugin-dirs",
            "--no-cache-dir",
            "--no-cookies",
            "--no-cookies-from-browser",
            "--no-js-runtimes",
            "--no-remote-components",
            "--xff",
            "never",
            "--retries",
            "0",
            "--extractor-retries",
            "0",
            "--socket-timeout",
            "5",
            "--no-playlist",
            "--skip-download",
            "--dump-single-json",
            "--format",
            "source",
            "--user-agent",
            http::AGENT,
            "--use-extractors",
            "wikimedia.org",
        ])
        .arg(commons::page_url(info)?)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env_clear();
    for name in [
        "PATH",
        "LANG",
        "LC_ALL",
        "SYSTEMROOT",
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
    let mut child = command.spawn().map_err(|_| ProviderError::Unavailable)?;
    let output = child.stdout.take().ok_or(ProviderError::Transport)?;
    let error = child.stderr.take().ok_or(ProviderError::Transport)?;
    let stdout = std::thread::spawn(move || collect(output, http::JSON_LIMIT));
    let stderr = std::thread::spawn(move || collect(error, 16 * 1024));
    let started = Instant::now();
    let mut timed_out = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(ProviderError::Transport);
            }
        }
        if started.elapsed() > Duration::from_secs(8) {
            timed_out = true;
            let _ = child.kill();
            break child.wait().map_err(|_| ProviderError::Transport)?;
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let (data, oversized) = stdout.join().map_err(|_| ProviderError::Transport)?;
    let (error, error_large) = stderr.join().map_err(|_| ProviderError::Transport)?;
    if timed_out {
        return Err(ProviderError::Timeout);
    }
    if oversized || error_large {
        return Err(ProviderError::TooLarge);
    }
    if !status.success() {
        let text = String::from_utf8_lossy(&error);
        return Err(if text.contains("403") || text.contains("401") {
            ProviderError::AccessDenied
        } else if text.contains("429") {
            ProviderError::RateLimited {
                retry_after_ms: None,
            }
        } else if text.contains("404") {
            ProviderError::Unavailable
        } else if text.contains("No suitable extractor") {
            ProviderError::Unsupported
        } else {
            ProviderError::Transport
        });
    }
    let json: Value = serde_json::from_slice(&data).map_err(|_| ProviderError::InvalidResponse)?;
    let uri = json["url"].as_str().ok_or(ProviderError::InvalidResponse)?;
    // Search's explicitly selected page/hash must still identify this original.
    // A different format, track or title fallback cannot replace the selection.
    if uri != info.uri {
        return Err(ProviderError::InvalidResponse);
    }
    Ok(uri.into())
}
