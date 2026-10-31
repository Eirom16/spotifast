//! Explicit file CLI: metadata reads precede any UI; decoding runs on the file worker.
use super::*;
pub fn run(path: PathBuf) -> Result<String, String> {
    if !std::fs::symlink_metadata(&path)
        .map_err(|_| "Selected WAV is unavailable")?
        .is_file()
    {
        return Err("Select a regular WAV file".into());
    }
    let file = std::fs::File::open(&path).map_err(|_| "Selected WAV is unavailable")?;
    let wav = doremi_player::wav_stream::Wav::open(file)
        .map_err(|error| format!("Selected WAV: {error:?}"))?;
    let info = wav.info();
    drop(wav);
    if info.duration_ms() < 4000 {
        return Err(
            "The automated seek/control probe requires a WAV of at least four seconds".into(),
        );
    }
    let id = MediaId::new(ProviderId::new("local-wav").unwrap(), "explicit-cli-file").unwrap();
    let media = Media {
        id: id.clone(),
        title: "Archivo WAV seleccionado".into(),
        duration_ms: Some(info.duration_ms()),
    };
    let mut model = Model::new(Preferences {
        volume: 0.08,
        ..Preferences::default()
    });
    model.wav_input = Some((id, path));
    let report = probe_controls(model, vec![media, Builtin::Chord.media()])?;
    if report["first"]["streaming"] != true
        || report["first"]["pcm_bytes"]
            .as_u64()
            .is_none_or(|bytes| bytes > 65536)
        || report["first"]["nonzero_samples"].as_u64().unwrap_or(0) == 0
    {
        return Err("Selected WAV did not reach bounded native output".into());
    }
    Ok(serde_json::json!({"source":"explicit local PCM16 WAV","source_rate":info.rate,"source_channels":info.channels,"duration_ms":info.duration_ms(),"controls":report,"acoustic_capture":false}).to_string())
}
