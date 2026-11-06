//! Explicit native browser authorization and real musical-library verification.
//! No credentials are accepted on the command line or printed to stdout.
#[cfg(any(target_os = "linux", target_os = "windows"))]
fn main() {
    use doremi_youtube::{Client, Request, browser, credentials};
    use std::sync::atomic::AtomicBool;
    let connect = std::env::args().any(|arg| arg == "--connect");
    let import = std::env::args().any(|arg| arg == "--import-doremi-rust");
    let cookie = if import {
        credentials::import_doremi_rust()
    } else if connect {
        browser::sign_in(&AtomicBool::new(false))
    } else {
        credentials::load()
            .and_then(|value| value.ok_or(doremi_core::catalog::ProviderError::AccessDenied))
    };
    let cookie = match cookie {
        Ok(cookie) => cookie,
        Err(error) => {
            eprintln!("Session: {error:?}");
            std::process::exit(1)
        }
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let result = runtime.block_on(async {
        let mut client = Client::new()?;
        let account = client.sign_in(cookie.clone()).await?;
        credentials::store(&cookie)?;
        println!("Verified YouTube Music account: {}", account.name);
        for (name, request) in [
            ("home", Request::Home),
            ("playlists", Request::Library),
            ("liked songs", Request::LikedSongs),
            ("saved albums", Request::SavedAlbums),
            ("saved artists", Request::SavedArtists),
        ] {
            match client.fetch(&request).await {
                Ok(page) => println!(
                    "{name}: {} returned items",
                    page.sections
                        .iter()
                        .map(|section| section.items.len())
                        .sum::<usize>()
                ),
                Err(error) => println!("{name}: {error:?}"),
            }
        }
        Ok::<_, doremi_core::catalog::ProviderError>(())
    });
    if let Err(error) = result {
        eprintln!("Session: {error:?}");
        std::process::exit(1);
    }
}
#[cfg(not(any(target_os = "linux", target_os = "windows")))]
fn main() {
    eprintln!("Native browser session probe is available on Linux and Windows.");
    std::process::exit(1);
}
