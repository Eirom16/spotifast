//! Explicit, bounded live experiment. No app integration or login is advertised.
mod commons;
mod extractor;
mod http;
use doremi_core::{
    catalog::*,
    media::{MediaId, ProviderId},
    requests::SessionId,
};
use serde_json::{Value, json};
use sha1::{Digest, Sha1};
use std::{io::Read, path::PathBuf, time::Instant};

fn tick(start: Instant) -> Tick {
    Tick(start.elapsed().as_millis() as u64)
}
fn experiment(
    query: &str,
    selected: &str,
    extractor: Option<&std::path::Path>,
) -> Result<Value, ProviderError> {
    let origin = Instant::now();
    let client = http::client()?;
    let mut catalog = Catalog::new(
        SessionId(1),
        Capabilities {
            provider: ProviderId::new(commons::PROVIDER)
                .map_err(|_| ProviderError::InvalidRequest)?,
            search: true,
            resolve: true,
            max_results: 10,
            formats: vec![AudioFormat::OggVorbis],
        },
    )?;
    let request = catalog
        .search(query.to_owned(), 5, tick(origin), 8000)?
        .into_iter()
        .find_map(|c| {
            if let ProviderCommand::Search(r) = c {
                Some(r)
            } else {
                None
            }
        })
        .ok_or(ProviderError::InvalidRequest)?;
    let at = Instant::now();
    let wire = commons::search(&client, &request.query, request.limit)?;
    let results = Ok(wire.iter().map(|v| v.media.clone()).collect());
    let search_ms = at.elapsed().as_secs_f64() * 1000.0;
    catalog.complete(
        ProviderEvent::Search {
            ticket: request.ticket,
            result: results,
        },
        tick(origin),
    )?;
    if let Status::Failed(error) = catalog.search_status() {
        return Err(error);
    }
    let found = catalog
        .results()
        .iter()
        .map(|m| json!({"id":m.id.local(),"title":m.title}))
        .collect::<Vec<_>>();
    let id = MediaId::new(
        ProviderId::new(commons::PROVIDER).map_err(|_| ProviderError::InvalidRequest)?,
        selected,
    )
    .map_err(|_| ProviderError::InvalidRequest)?;
    let request = catalog
        .resolve(id, tick(origin), 8000)?
        .into_iter()
        .find_map(|c| {
            if let ProviderCommand::Resolve(r) = c {
                Some(r)
            } else {
                None
            }
        })
        .ok_or(ProviderError::InvalidRequest)?;
    let at = Instant::now();
    let info = if let Some(executable) = extractor {
        let info = wire
            .into_iter()
            .find(|v| v.media.id == request.media)
            .ok_or(ProviderError::InvalidResponse)?;
        let (size, hash) =
            commons::audited(info.media.id.local()).ok_or(ProviderError::AccessDenied)?;
        if size != info.byte_len || hash != info.sha1 {
            return Err(ProviderError::Unavailable);
        }
        let uri = extractor::resolve(executable, &info)?;
        commons::Info { uri, ..info }
    } else {
        commons::resolve(&client, &request.media)?
    };
    let resolve_ms = at.elapsed().as_secs_f64() * 1000.0;
    let resource = TemporaryResource::http(
        info.media.id.clone(),
        info.uri,
        vec![],
        StreamInfo {
            format: AudioFormat::OggVorbis,
            byte_len: Some(info.byte_len),
            seekable: false,
            usable_until: Tick(tick(origin).0 + 30000),
            lease_basis: LeaseBasis::ConservativeHostLimit,
        },
    )?;
    catalog.complete(
        ProviderEvent::Resolved {
            ticket: request.ticket,
            result: Ok(resource),
        },
        tick(origin),
    )?;
    if let Status::Failed(error) = catalog.resolve_status() {
        return Err(error);
    }
    let resource = catalog.resource().ok_or(ProviderError::Unavailable)?;
    let at = Instant::now();
    let mut response = client
        .get(resource.expose_uri())
        .send()
        .map_err(http::error)?;
    http::status(&response)?;
    if response
        .content_length()
        .is_some_and(|v| v > http::AUDIO_LIMIT as u64)
    {
        return Err(ProviderError::TooLarge);
    }
    let mut first = [0; 1];
    response
        .read_exact(&mut first)
        .map_err(|_| ProviderError::InvalidResponse)?;
    let first_byte_ms = at.elapsed().as_secs_f64() * 1000.0;
    let mut bytes = http::bounded(response, http::AUDIO_LIMIT)?;
    bytes.insert(0, first[0]);
    let download_ms = at.elapsed().as_secs_f64() * 1000.0;
    if bytes.len() as u64 != info.byte_len || format!("{:x}", Sha1::digest(&bytes)) != info.sha1 {
        return Err(ProviderError::InvalidResponse);
    }
    let offset = 27 + usize::from(*bytes.get(26).ok_or(ProviderError::InvalidResponse)?);
    if bytes.get(..4) != Some(b"OggS") || bytes.get(offset..offset + 7) != Some(b"\x01vorbis") {
        return Err(ProviderError::UnsupportedFormat);
    }
    let output = PathBuf::from("target/provider-live");
    std::fs::create_dir_all(&output).map_err(|_| ProviderError::Transport)?;
    let payload = output.join(format!(
        "{}-{selected}.ogg",
        if extractor.is_some() {
            "yt-dlp"
        } else {
            "native"
        }
    ));
    std::fs::write(&payload, &bytes).map_err(|_| ProviderError::Transport)?;
    Ok(
        json!({"schema":1,"experiment":true,"provider":commons::PROVIDER,"resolver":if extractor.is_some(){"yt-dlp-2026.10.26"}else{"native-rust"},"search_component":"common-native-catalog","results":found,"selected_id":selected,"search_ms":search_ms,"resolve_ms":resolve_ms,"first_byte_ms":first_byte_ms,"download_ms":download_ms,"download_bytes":bytes.len(),"sha1":info.sha1,"license":"CC0-1.0","payload":payload,"first_pcm_ms":null,"audible_audio_ms":null,"audio_played":false}),
    )
}
fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args == ["--version"] {
        println!("Doremi Provider Live Probe {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    let (extractor, query, id) = match args.as_slice() {
        [query, id] => (None, query, id),
        [flag, path, query, id] if flag == "--yt-dlp" => {
            (Some(std::path::Path::new(path)), query, id)
        }
        _ => {
            eprintln!(
                "Usage: doremi-provider-live-probe [--yt-dlp VERIFIED_ZIPAPP] QUERY EXPLICIT_PAGE_ID"
            );
            std::process::exit(2);
        }
    };
    match experiment(query, id, extractor) {
        Ok(value) => println!("{value}"),
        Err(error) => {
            println!(
                "{}",
                json!({"schema":1,"status":"error","error":format!("{error:?}"),"audio_played":false})
            );
            std::process::exit(1);
        }
    }
}
