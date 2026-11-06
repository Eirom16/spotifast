//! Explicit read-only public catalog probe. No account or credentials are read.
use doremi_youtube::{Client, Request};
fn main() {
    let query = std::env::args().nth(1).unwrap_or_else(|| "Chopin".into());
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let result = runtime.block_on(async {
        let mut client = Client::new()?;
        client
            .fetch(&Request::Search {
                query,
                kind: doremi_core::media::EntityKind::Recording,
            })
            .await
    });
    match result {
        Ok(page) => {
            for item in page
                .sections
                .iter()
                .flat_map(|section| &section.items)
                .take(10)
            {
                println!(
                    "{} | {} | {} | {}",
                    item.key(),
                    item.title,
                    item.subtitle,
                    item.duration
                );
            }
        }
        Err(error) => {
            eprintln!("YouTube Music: {error:?}");
            std::process::exit(1);
        }
    }
}
