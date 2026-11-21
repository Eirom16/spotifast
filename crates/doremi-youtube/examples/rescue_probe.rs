//! Explicit public identities; --saved-session opts into the existing account. Audio bytes stay in
//! process memory and only redacted timings/counts are printed.
fn main() {
    let saved = std::env::args().any(|arg| arg == "--saved-session");
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    let cookie = if saved {
        match doremi_youtube::credentials::load() {
            Ok(Some(cookie)) => Some(cookie),
            _ => {
                eprintln!("Existing native session unavailable");
                std::process::exit(1);
            }
        }
    } else {
        None
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        for id in ["qQzdAsjWGPg", "_d3SaQI_Ag4"] {
            let start = std::time::Instant::now();
            let mut client = doremi_youtube::Client::new().unwrap();
            #[cfg(any(target_os = "linux", target_os = "windows"))]
            if let Some(cookie) = &cookie
                && let Err(error) = client.sign_in(cookie.clone()).await {
                eprintln!("Existing session verification failed: {error:?}"); std::process::exit(1);
            }
            #[cfg(not(any(target_os = "linux", target_os = "windows")))]
            if saved { eprintln!("Use the platform session adapter"); std::process::exit(1); }
            let media = doremi_youtube::id(id).unwrap();
            match doremi_youtube::rescue::prepare(&mut client, &media).await {
                Ok(audio) => println!("{}", serde_json::json!({"id":id,"bytes":audio.bytes.len(),"duration_ms":audio.duration_ms,"elapsed_ms":start.elapsed().as_millis(),"result":"complete encoded audio; not playback"})),
                Err(error) => {
                    eprintln!("id={id} stage={} error={error:?}", client.resolution_stage());
                    std::process::exit(1);
                }
            }
        }
    });
}
