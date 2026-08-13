//! Speech to text.
//!
//! Whisper only listens to 16 kHz mono PCM, so a transcription is two steps:
//! ffmpeg reduces whatever was dropped to that, then whisper reads it. The
//! pipeline is expressed in `job.rs` rather than shelling out twice from here.

use std::path::{Path, PathBuf};

use tauri::AppHandle;

use crate::engines::{self, EngineId};

/// What a video or an audio file can become once speech is installed.
/// `srt` and `vtt` are subtitles; `txt` is the words on their own.
pub const TARGETS: &[&str] = &["txt", "srt", "vtt"];

pub fn is_target(extension: &str) -> bool {
    TARGETS.contains(
        &extension
            .trim_start_matches('.')
            .to_ascii_lowercase()
            .as_str(),
    )
}

/// Both halves have to be there: the binary can do nothing without weights.
pub fn available(app: &AppHandle) -> bool {
    engines::executable(app, EngineId::Whisper).is_some()
        && engines::executable(app, EngineId::WhisperModel).is_some()
}

pub struct SpeechPlan {
    /// Where ffmpeg should put the 16 kHz mono file whisper will read.
    pub wav: PathBuf,
    pub program: PathBuf,
    pub args: Vec<String>,
    /// Whisper names its own output; the runner moves it onto the real path.
    pub produced: PathBuf,
    pub cleanup: Vec<PathBuf>,
}

/// The audio filter chain whisper needs. Kept next to the invocation it
/// belongs to rather than in `presets.rs`, which is about user-facing quality.
pub fn wav_args() -> Vec<String> {
    ["-ar", "16000", "-ac", "1", "-c:a", "pcm_s16le", "-vn"]
        .iter()
        .map(|s| s.to_string())
        .collect()
}

pub fn plan(app: &AppHandle, output: &Path, job_id: &str) -> Result<SpeechPlan, String> {
    let whisper =
        engines::executable(app, EngineId::Whisper).ok_or("The speech module is not installed")?;
    let model = engines::executable(app, EngineId::WhisperModel)
        .ok_or("The speech model is missing — reinstall the speech module")?;

    let format = output
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_else(|| "txt".into());

    let scratch = std::env::temp_dir().join(format!("coldmill-{job_id}"));
    std::fs::create_dir_all(&scratch).map_err(|e| e.to_string())?;

    let wav = scratch.join("audio.wav");
    // Whisper appends the extension to whatever stem it is given.
    let stem = scratch.join("transcript");
    let produced = scratch.join(format!("transcript.{format}"));

    let mut args = vec![
        "-m".to_string(),
        model.to_string_lossy().into_owned(),
        "-f".to_string(),
        wav.to_string_lossy().into_owned(),
        "-of".to_string(),
        stem.to_string_lossy().into_owned(),
        // Detect the language rather than assuming English: the app is
        // translated into sixteen of them, so its users are not all English
        // speakers either.
        "-l".to_string(),
        "auto".to_string(),
        // Progress goes to stderr and would otherwise be mistaken for errors.
        "--no-prints".to_string(),
    ];
    args.push(match format.as_str() {
        "srt" => "--output-srt".into(),
        "vtt" => "--output-vtt".into(),
        _ => "--output-txt".into(),
    });

    Ok(SpeechPlan {
        wav,
        program: whisper,
        args,
        produced,
        cleanup: vec![scratch],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subtitles_and_plain_text_are_recognised() {
        assert!(is_target("srt"));
        assert!(is_target(".VTT"));
        assert!(is_target("txt"));
        assert!(!is_target("mp4"));
    }

    #[test]
    fn the_wav_is_what_whisper_expects() {
        let args = wav_args();
        assert!(args.windows(2).any(|w| w == ["-ar", "16000"]));
        assert!(args.windows(2).any(|w| w == ["-ac", "1"]));
    }
}
