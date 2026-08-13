//! Text to speech.
//!
//! Piper reads its text from standard input and writes a WAV, so this is the
//! one engine that is not driven purely by arguments. It runs on a blocking
//! thread with its input piped in and closed — without the close it would sit
//! waiting for more words forever.
//!
//! Only plain text goes in. Voicing a `.docx` is two steps on purpose: convert
//! it to `.txt` first, which this app already does. Chaining the two silently
//! would hide which half went wrong when a document reads badly.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use tauri::AppHandle;

use crate::engines::{self, EngineId};

/// Extensions we are willing to read aloud.
pub const SOURCES: &[&str] = &["txt", "md", "markdown"];

/// What a spoken document can be saved as. Anything other than wav is handed
/// to ffmpeg afterwards.
pub const TARGETS: &[&str] = &["mp3", "wav", "m4a", "opus", "ogg", "flac"];

pub fn is_source(extension: &str) -> bool {
    SOURCES.contains(
        &extension
            .trim_start_matches('.')
            .to_ascii_lowercase()
            .as_str(),
    )
}

pub fn is_target(extension: &str) -> bool {
    TARGETS.contains(
        &extension
            .trim_start_matches('.')
            .to_ascii_lowercase()
            .as_str(),
    )
}

/// The binary and the voice both have to be present; one without the other
/// does nothing.
pub fn available(app: &AppHandle) -> bool {
    engines::executable(app, EngineId::Piper).is_some()
        && engines::executable(app, EngineId::PiperVoice).is_some()
}

pub struct TtsJob {
    pub program: PathBuf,
    pub voice: PathBuf,
    pub input: PathBuf,
    /// Piper always writes WAV; a different target is converted afterwards.
    pub wav: PathBuf,
    pub scratch: PathBuf,
}

pub fn job(app: &AppHandle, input: &Path, job_id: &str) -> Result<TtsJob, String> {
    let program =
        engines::executable(app, EngineId::Piper).ok_or("The speech module is not installed")?;
    let voice = engines::executable(app, EngineId::PiperVoice)
        .ok_or("The voice is missing — reinstall the speech module")?;

    let scratch = std::env::temp_dir().join(format!("coldmill-{job_id}"));
    std::fs::create_dir_all(&scratch).map_err(|e| e.to_string())?;

    Ok(TtsJob {
        program,
        voice,
        input: input.to_path_buf(),
        wav: scratch.join("speech.wav"),
        scratch,
    })
}

/// Speaks the file. Blocking: the caller puts it on its own thread.
pub fn speak(job: &TtsJob) -> Result<(), String> {
    let text =
        std::fs::read_to_string(&job.input).map_err(|e| format!("could not read the text: {e}"))?;
    let text = strip_markdown(&text);
    if text.trim().is_empty() {
        return Err("there are no words in this file to read".into());
    }

    let mut child = Command::new(&job.program)
        .arg("--model")
        .arg(&job.voice)
        .arg("--output_file")
        .arg(&job.wav)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("could not start Piper: {e}"))?;

    {
        // Dropped at the end of this block, which is what tells Piper the text
        // has finished.
        let mut stdin = child.stdin.take().ok_or("Piper refused its input")?;
        stdin
            .write_all(text.as_bytes())
            .map_err(|e| format!("could not send the text to Piper: {e}"))?;
    }

    let output = child
        .wait_with_output()
        .map_err(|e| format!("Piper did not finish: {e}"))?;

    if !output.status.success() {
        let message = String::from_utf8_lossy(&output.stderr);
        let tail: Vec<&str> = message.lines().rev().take(6).collect();
        return Err(tail.into_iter().rev().collect::<Vec<_>>().join("\n"));
    }
    if !job.wav.is_file() {
        return Err("Piper reported success but wrote nothing".into());
    }
    Ok(())
}

/// Markdown read aloud is full of asterisks and hashes. This is deliberately
/// shallow — enough to stop the punctuation being spoken, not a parser.
fn strip_markdown(text: &str) -> String {
    text.lines()
        .map(|line| {
            let line = line.trim();
            let line = line.trim_start_matches('#').trim_start();
            line.replace("**", "")
                .replace("__", "")
                .replace(['`', '*'], "")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text_goes_in_and_audio_comes_out() {
        assert!(is_source("md"));
        assert!(is_source(".TXT"));
        assert!(!is_source("docx"));
        assert!(is_target("mp3"));
        assert!(!is_target("png"));
    }

    #[test]
    fn markdown_punctuation_is_not_read_aloud() {
        let spoken = strip_markdown("# Title\n\nSome **bold** and `code` here");
        assert_eq!(spoken, "Title\n\nSome bold and code here");
    }
}
