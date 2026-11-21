//! Explicit corpus, fresh model, no profile or credentials read or written.
//! Real output callback, whole songs twice, followed by control checks.
use super::*;
use serde_json::{Value, json};
fn pump(model: &mut Model, ctx: &egui::Context) -> Result<(), String> {
    model.poll_online(ctx);
    model.poll_audio();
    if model.queue.playback() == Playback::Failed {
        return Err(model.notice.clone());
    }
    Ok(())
}
fn wait(
    model: &mut Model,
    ctx: &egui::Context,
    seconds: u64,
    mut predicate: impl FnMut(&Model) -> bool,
) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(seconds);
    while Instant::now() < deadline {
        pump(model, ctx)?;
        if predicate(model) {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    Err(format!("Rescue probe timeout: {}", model.notice))
}
fn metrics(m: doremi_player::Metrics) -> Value {
    json!({"position_ms":m.position_ms,"frames":m.frames,"rate":m.output_rate,
        "callbacks":m.callbacks,"nonzero_samples":m.nonzero_samples,"pcm_bytes":m.pcm_bytes,
        "buffer_underruns":m.buffer_underruns,"reported_xruns":m.reported_xruns})
}
pub fn run() -> Result<String, String> {
    let rows: Vec<_> = [("qQzdAsjWGPg", 276253), ("_d3SaQI_Ag4", 284427)]
        .into_iter()
        .map(|(id, duration)| Media {
            id: doremi_youtube::id(id).unwrap(),
            title: id.into(),
            duration_ms: Some(duration),
        })
        .collect();
    let ctx = egui::Context::default();
    let mut model = Model::new(Preferences {
        volume: 0.08,
        ..Preferences::default()
    });
    model.enable_audio();
    let mut complete = Vec::new();
    for round in 1..=2 {
        for row in &rows {
            model.start_context(vec![row.clone()], 0);
            wait(&mut model, &ctx, 600, |m| {
                m.queue.playback() == Playback::Playing
            })?;
            let start = Instant::now();
            let mut last = doremi_player::Metrics::default();
            wait(&mut model, &ctx, 400, |m| {
                if let Some(current) = m.audio.as_ref().and_then(Engine::metrics)
                    && current.frames >= last.frames
                {
                    last = current;
                }
                m.queue.playback() == Playback::Stopped
            })
            .map_err(|error| format!("{} round {round}: {error}", row.id.local()))?;
            // Ended is the only way a nonfailed playing single-item context stops.
            // Wall time independently rejects seek-to-end masquerading as playback.
            let elapsed = start.elapsed().as_millis() as u64;
            if last.output_rate == 0
                || last.nonzero_samples == 0
                || last.frames * 1000 / u64::from(last.output_rate) + 1500
                    < row.duration_ms.unwrap()
                || elapsed + 1500 < row.duration_ms.unwrap()
            {
                return Err("Premature natural EOF".into());
            }
            let value = json!({"id":row.id.local(),"round":round,"eof":true,"wall_ms":elapsed,"native":metrics(last)});
            println!("{value}");
            complete.push(value);
        }
    }
    model.start_context(rows.clone(), 0);
    wait(&mut model, &ctx, 600, |m| m.queue.position_ms() > 500)?;
    model.command(Command::Pause);
    wait(&mut model, &ctx, 2, |m| {
        m.queue.playback() == Playback::Paused
    })?;
    std::thread::sleep(Duration::from_millis(150));
    pump(&mut model, &ctx)?;
    let paused = model.queue.position_ms();
    std::thread::sleep(Duration::from_millis(500));
    pump(&mut model, &ctx)?;
    if model.queue.position_ms() != paused {
        return Err("Pause clock advanced".into());
    }
    model.command(Command::Seek(60000));
    wait(&mut model, &ctx, 3, |m| {
        m.queue.playback() == Playback::Paused && m.queue.position_ms() == 60000
    })?;
    model.command(Command::Play);
    wait(&mut model, &ctx, 5, |m| m.queue.position_ms() > 60500)?;
    let seek = metrics(model.audio.as_ref().unwrap().metrics().unwrap());
    model.command(Command::Next);
    wait(&mut model, &ctx, 600, |m| {
        m.queue.current().is_some_and(|e| e.media.id == rows[1].id) && m.queue.position_ms() > 250
    })?;
    model.command(Command::Previous);
    wait(&mut model, &ctx, 600, |m| {
        m.queue.current().is_some_and(|e| e.media.id == rows[0].id) && m.queue.position_ms() > 250
    })?;
    model.command(Command::Cancel);
    for _ in 0..50 {
        pump(&mut model, &ctx)?;
        std::thread::sleep(Duration::from_millis(20));
    }
    if model.audio.as_ref().unwrap().metrics().is_some()
        || model.queue.playback() != Playback::Paused
    {
        return Err("Cancelled audio survived".into());
    }
    Ok(json!({"complete":complete,"seek":seek,"controls":"pause/resume/seek/next/previous/cancel passed","acoustic_capture":false,"credentials":"not read; isolated guest model"}).to_string())
}

#[cfg(test)]
mod tests {
    #[test]
    #[ignore = "explicit maintainer-authorized real network and native audio, about 20 minutes"]
    fn authorized_corpus_twice_to_native_eof() {
        println!("{}", super::run().expect("whole-song rescue acceptance"));
    }
}
