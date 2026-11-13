//! Opt-in live playback, not an offline test or a replacement player.
//! Uses two explicit YouTube identities, the existing session store (only with
//! --session), and the same reader, AAC decoder and output actor as the app.
//! Never prints credentials, transport URLs, response bodies or raw IO errors.
use doremi_core::{
    media::{Media, MediaId, ProviderId},
    queue::{Command, Effect, Order, Queue},
    requests::SessionId,
};
use doremi_player::{Engine, EngineEvent, ogg_stream::Vorbis};
use doremi_youtube::Client;
use std::time::{Duration, Instant};

fn wait(engine: &Engine, predicate: impl Fn(doremi_player::Metrics) -> bool) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(45);
    while Instant::now() < deadline {
        if let Some(EngineEvent::Failed(_, error)) = engine.poll() {
            return Err(format!("native playback: {error:?}"));
        }
        if engine.metrics().is_some_and(&predicate) {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    Err("native playback: deadline".into())
}

fn run() -> Result<(), String> {
    let mut args: Vec<_> = std::env::args().skip(1).collect();
    let session = args.first().is_some_and(|arg| arg == "--session");
    if session {
        args.remove(0);
    }
    if args.len() != 2 || args[0] == args[1] {
        return Err("Usage: stream_probe [--session] FIRST_VIDEO_ID SECOND_VIDEO_ID".into());
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| "runtime unavailable")?;
    let mut client = Client::new().map_err(|error| format!("client: {error:?}"))?;
    if session {
        #[cfg(any(target_os = "linux", target_os = "windows"))]
        {
            let stored = doremi_youtube::credentials::load()
                .map_err(|error| format!("session store: {error:?}"))?
                .ok_or("session store: no session")?;
            runtime
                .block_on(client.sign_in(stored))
                .map_err(|error| format!("session verification: {error:?}"))?;
        }
        #[cfg(not(any(target_os = "linux", target_os = "windows")))]
        return Err("session store: unsupported platform".into());
    }
    let mut engine = Engine::new().map_err(|error| format!("output: {error:?}"))?;
    engine.set_volume(0.08);
    let mut queue = Queue::new(SessionId(1));
    for (index, local) in args.iter().enumerate() {
        let media = MediaId::new(ProviderId::new(doremi_youtube::PROVIDER).unwrap(), local)
            .map_err(|_| "invalid media identity")?;
        let stream = runtime.block_on(client.resolve(&media)).map_err(|error| {
            // These diagnostics contain only fixed categories and counters.
            eprintln!("resolution stage={}", client.resolution_stage());
            if let Some(report) = client.player_diagnostic() {
                eprintln!("player: {report:?}");
            }
            format!("resolution: {error:?}")
        })?;
        let length = stream.length();
        let request = queue
            .command(Command::StartContext {
                media: vec![Media {
                    id: media,
                    title: "Explicit live probe selection".into(),
                    duration_ms: Some(stream.duration_ms),
                }],
                start: 0,
                order: Order::Sequential,
            })
            .map_err(|_| "queue rejected selection")?
            .into_iter()
            .find_map(|effect| match effect {
                Effect::Load { request, .. } => Some(request),
                _ => None,
            })
            .ok_or("queue did not request audio")?;
        engine
            .load_stream(
                request,
                Box::new(move |cancel| {
                    let reader = stream
                        .reader(move || cancel.cancelled())
                        .map_err(|_| doremi_player::EngineError::Input)?;
                    let source = Vorbis::aac_from_read_seek(reader, length)
                        .map_err(doremi_player::EngineError::Decode)?;
                    Ok(Box::new(source))
                }),
                0,
            )
            .map_err(|error| format!("load: {error:?}"))?;
        engine.set_playing(true);
        wait(&engine, |m| {
            m.output_rate > 0
                && m.frames >= u64::from(m.output_rate) * 7
                && m.nonzero_samples > 1000
        })?;
        engine.set_playing(false);
        std::thread::sleep(Duration::from_millis(300));
        let paused = engine.metrics().ok_or("missing output metrics")?.frames;
        std::thread::sleep(Duration::from_millis(700));
        if engine.metrics().ok_or("missing output metrics")?.frames != paused {
            return Err("output advanced while paused".into());
        }
        engine.set_playing(true);
        wait(&engine, |m| {
            m.output_rate > 0
                && m.frames >= u64::from(m.output_rate) * 15
                && m.nonzero_samples > 1000
        })?;
        let m = engine.metrics().ok_or("missing output metrics")?;
        println!(
            "track={} frames={} rate={} nonzero_samples={} pcm_bytes={} pause_resume=true",
            index + 1,
            m.frames,
            m.output_rate,
            m.nonzero_samples,
            m.pcm_bytes
        );
        engine.set_playing(false);
    }
    engine.cancel();
    println!("native_output=true track_change=true acoustic_capture=false");
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
