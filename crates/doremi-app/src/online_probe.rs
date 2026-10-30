//! Isolated Linux CLI experiment, never called from a frame or audio callback.
//! Same Model, Catalog, Queue, native actor and controls as the offline host.
use super::*;
use doremi_commons as commons;
use doremi_core::catalog::{AudioFormat, ProviderError, Status};
use serde_json::{Value, json};

fn protocol(error: ProviderError) -> String {
    format!("Commons experiment: {error:?}")
}
fn tick(start: Instant) -> Tick {
    Tick(start.elapsed().as_millis() as u64)
}
async fn prepare() -> Result<(Model, Vec<Media>, Value), String> {
    let start = Instant::now();
    let client = commons::http::client().map_err(protocol)?;
    let mut model = Model::new(Preferences {
        volume: 0.08,
        ..Preferences::default()
    });
    // This fresh instance has issued no demo tickets. One catalog owner.
    model.catalog = Catalog::new(
        SessionId(1),
        Capabilities {
            provider: ProviderId::new(commons::PROVIDER).unwrap(),
            search: true,
            resolve: true,
            max_results: 10,
            formats: vec![AudioFormat::OggVorbis],
        },
    )
    .map_err(protocol)?;
    let request = model
        .catalog
        .search("sine".into(), 10, tick(start), 8000)
        .map_err(protocol)?
        .into_iter()
        .find_map(|effect| match effect {
            ProviderCommand::Search(request) => Some(request),
            _ => None,
        })
        .ok_or("No search ticket")?;
    let at = Instant::now();
    let found = commons::search(&client, &request.query, request.limit)
        .await
        .map_err(protocol)?;
    let search_ms = at.elapsed().as_secs_f64() * 1000.0;
    model
        .catalog
        .complete(
            ProviderEvent::Search {
                ticket: request.ticket,
                result: Ok(found
                    .into_iter()
                    .filter(|info| commons::audited(info.media.id.local()).is_some())
                    .map(|info| info.media)
                    .collect()),
            },
            tick(start),
        )
        .map_err(protocol)?;
    if let Status::Failed(error) = model.catalog.search_status() {
        return Err(protocol(error));
    }
    let mut rows = Vec::new();
    let mut evidence = Vec::new();
    // Explicitly selected audited identities, never a title/format fallback.
    for local in commons::audited_ids() {
        let mut media = model
            .catalog
            .results()
            .iter()
            .find(|media| media.id.local() == local)
            .cloned()
            .ok_or("Audited CC0 selection missing from real search results")?;
        let request = model
            .catalog
            .resolve(media.id.clone(), tick(start), 8000)
            .map_err(protocol)?
            .into_iter()
            .find_map(|effect| match effect {
                ProviderCommand::Resolve(request) => Some(request),
                _ => None,
            })
            .ok_or("No resolve ticket")?;
        let at = Instant::now();
        let info = commons::resolve(&client, &request.media)
            .await
            .map_err(protocol)?;
        let resource = commons::temporary(&info, tick(start)).map_err(protocol)?;
        let resolve_ms = at.elapsed().as_secs_f64() * 1000.0;
        model
            .catalog
            .complete(
                ProviderEvent::Resolve {
                    ticket: request.ticket,
                    result: Ok(resource),
                },
                tick(start),
            )
            .map_err(protocol)?;
        if let Status::Failed(error) = model.catalog.resolve_status() {
            return Err(protocol(error));
        }
        let resource = model
            .catalog
            .resource()
            .ok_or("Resolved resource expired or mismatched")?;
        let at = Instant::now();
        let bytes = commons::download(&client, resource, tick(start))
            .await
            .map_err(protocol)?;
        let download_ms = at.elapsed().as_secs_f64() * 1000.0;
        let at = Instant::now();
        let pcm = doremi_player::decode_ogg(&bytes)
            .map_err(|error| format!("Bounded Vorbis decoder: {error:?}"))?;
        let prepared_pcm_ms = at.elapsed().as_secs_f64() * 1000.0;
        media.duration_ms = Some(pcm.duration_ms());
        evidence.push(json!({"id":media.id.local(),"license":"CC0-1.0","verified_encoded_bytes":bytes.len(),"pcm_bytes":pcm.byte_len(),"sample_rate":pcm.sample_rate(),"duration_ms":pcm.duration_ms(),"resolve_ms":resolve_ms,"download_ms":download_ms,"prepared_pcm_ms":prepared_pcm_ms}));
        model.prepared.push((media.id.clone(), Arc::new(pcm)));
        rows.push(media);
    }
    // Drop all transport URIs before native playback. Only bounded PCM/IDs remain.
    model.catalog.cancel();
    Ok((
        model,
        rows,
        json!({"source":commons::PROVIDER,"search_ms":search_ms,"buffering":"two complete audited resources before playback; not streaming","prefetch_ms":start.elapsed().as_secs_f64()*1000.0,"resources":evidence}),
    ))
}

pub fn run() -> Result<String, String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| "Could not start isolated provider runtime")?;
    let (model, rows, network) = runtime.block_on(prepare())?;
    drop(runtime);
    let controls = super::probe_controls(model, rows)?;
    Ok(json!({"schema":1,"experiment":true,"interface":"Linux CLI; GUI not exercised by this probe","network":network,"controls":controls,"acoustic_capture":false,"audible_audio_ms":null}).to_string())
}
