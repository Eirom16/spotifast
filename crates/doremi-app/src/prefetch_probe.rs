//! Explicit authorized corpus, native output, no persisted profile or credentials.
//! A/B uses the same download/cache/decoder with only automatic prefetch disabled.
use super::*;
use serde_json::{Value, json};

fn pump(m: &mut Model, ctx: &egui::Context) -> Result<(), String> {
    m.poll_online(ctx);
    m.poll_audio();
    if m.queue.playback() == Playback::Failed {
        return Err(m.notice.clone());
    }
    Ok(())
}
fn wait(
    m: &mut Model,
    ctx: &egui::Context,
    mut done: impl FnMut(&Model) -> bool,
) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(600);
    while Instant::now() < deadline {
        pump(m, ctx)?;
        if done(m) {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    Err(format!("Timed out: {}", m.notice))
}
fn output(m: &Model) -> bool {
    m.queue.playback() == Playback::Playing
        && m.audio
            .as_ref()
            .and_then(Engine::metrics)
            .is_some_and(|s| s.nonzero_samples > 0)
}
fn memory() -> Value {
    let text = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    json!(
        text.lines()
            .filter(|line| line.starts_with("VmRSS:") || line.starts_with("VmHWM:"))
            .collect::<Vec<_>>()
    )
}
pub fn run(prefetch: bool) -> Result<String, String> {
    let ctx = egui::Context::default();
    let rows: Vec<_> = [("qQzdAsjWGPg", 276253), ("_d3SaQI_Ag4", 284427)]
        .into_iter()
        .map(|(id, ms)| Media {
            id: doremi_youtube::id(id).unwrap(),
            title: id.into(),
            duration_ms: Some(ms),
            music: None,
        })
        .collect();
    let mut m = Model::new(Preferences {
        volume: 0.08,
        ..Preferences::default()
    });
    m.online.prefetch_enabled = prefetch;
    m.enable_audio();
    let before = memory();
    let at = Instant::now();
    m.start_context(rows.clone(), 0);
    wait(&mut m, &ctx, output)?;
    let first_ms = at.elapsed().as_millis();
    let first_memory = memory();
    println!(
        "{}",
        json!({"stage":"first_pcm","elapsed_ms":first_ms,"memory":first_memory})
    );
    let play_at = Instant::now();
    let mut last = doremi_player::Metrics::default();
    let mut ready = Value::Null;
    wait(&mut m, &ctx, |m| {
        if m.queue.current().is_some_and(|e| e.media.id == rows[0].id) {
            if let Some(s) = m.audio.as_ref().and_then(Engine::metrics) {
                last = s;
            }
            let state = m.source_diagnostics();
            if ready.is_null() && state["prefetch"]["ready"] == true {
                ready = json!({"source":state,"memory":memory(),"since_play_ms":play_at.elapsed().as_millis()});
                println!("{}", json!({"stage":"prefetch_ready","data":ready}));
            }
        }
        m.queue.current().is_some_and(|e| e.media.id == rows[1].id) && output(m)
    })?;
    if play_at.elapsed().as_millis() + 1500 < 276253
        || last.frames * 1000 / u64::from(last.output_rate.max(1)) + 1500 < 276253
    {
        return Err("Premature natural EOF".into());
    }
    let transition = m.source_diagnostics();
    println!(
        "{}",
        json!({"stage":"natural_transition","source":transition})
    );
    m.command(Command::Previous);
    wait(&mut m, &ctx, output)?;
    if prefetch {
        wait(&mut m, &ctx, |m| {
            m.source_diagnostics()["prefetch"]["ready"] == true
        })?;
    }
    let at = Instant::now();
    m.command(Command::Next);
    wait(&mut m, &ctx, output)?;
    let next_ms = at.elapsed().as_millis();
    let final_source = m.source_diagnostics();
    let final_memory = memory();
    m.command(Command::Cancel);
    if m.online.snapshot()["prefetch"] != Value::Null {
        return Err("Orphaned prefetch after Cancel".into());
    }
    drop(m); // Never open a second output engine concurrently.
    let mut cold = Model::new(Preferences {
        volume: 0.08,
        ..Preferences::default()
    });
    cold.online.prefetch_enabled = prefetch;
    cold.enable_audio();
    cold.start_context(rows, 0);
    wait(&mut cold, &ctx, output)?;
    let at = Instant::now();
    cold.command(Command::Next); // adopts an in-flight prefetch, or normal cold load
    wait(&mut cold, &ctx, output)?;
    let next_cold_ms = at.elapsed().as_millis();
    let cold_source = cold.source_diagnostics();
    cold.command(Command::Cancel);
    Ok(json!({"mode":if prefetch {"prefetch"}else{"baseline-disabled"},"first_pcm_ms":first_ms,
        "next_cold_or_promoted_ms":next_cold_ms,"cold_source":cold_source,"natural_eof":true,"transition":transition,"prefetch_ready":ready,"next_cached_ms":next_ms,
        "first_track":{"frames":last.frames,"underruns":last.buffer_underruns,"xruns":last.reported_xruns},
        "final_source":final_source,"memory_before":before,"memory_first":first_memory,"memory_final":final_memory,
        "acoustic_capture":false}).to_string())
}
