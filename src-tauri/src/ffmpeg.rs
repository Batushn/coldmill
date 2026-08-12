//! Runs one ffmpeg process and turns its `-progress` stream into Tauri events.

use std::collections::VecDeque;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use tauri::{AppHandle, Emitter};
use tauri_plugin_shell::process::CommandEvent;
use tauri_plugin_shell::ShellExt;

use crate::model::{DonePayload, ProgressPayload, EVENT_DONE, EVENT_PROGRESS};
use crate::queue::JobRegistry;

/// How many stderr lines to keep for the error tooltip.
const STDERR_TAIL: usize = 12;

pub enum ConvertError {
    Cancelled,
    Failed(String),
}

/// Everything ffmpeg needs besides the encoder settings themselves.
fn base_args(input: &Path, output: &Path, encode: Vec<String>) -> Vec<String> {
    let mut args: Vec<String> = [
        "-hide_banner",
        "-nostdin",
        "-loglevel",
        "error",
        // Overwrite: collisions are already resolved when the path is built.
        "-y",
        // Machine readable progress on stdout, human noise off.
        "-progress",
        "pipe:1",
        "-nostats",
        "-i",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();

    args.push(input.to_string_lossy().into_owned());
    args.extend(encode);
    args.push(output.to_string_lossy().into_owned());
    args
}

pub async fn run(
    app: &AppHandle,
    registry: &Arc<JobRegistry>,
    job_id: &str,
    input: &Path,
    output: &Path,
    encode: Vec<String>,
    total_secs: Option<f64>,
) -> Result<(), ConvertError> {
    let started = Instant::now();
    let args = base_args(input, output, encode);

    let command = app
        .shell()
        .sidecar("ffmpeg")
        .map_err(|e| ConvertError::Failed(format!("ffmpeg sidecar missing: {e}")))?
        .args(args);

    let (mut events, child) = command
        .spawn()
        .map_err(|e| ConvertError::Failed(format!("could not start ffmpeg: {e}")))?;

    // Cancel may have landed between register() and spawn().
    if let Some(child) = registry.attach_child(job_id, child) {
        let _ = child.kill();
        return Err(ConvertError::Cancelled);
    }

    let mut stderr_tail: VecDeque<String> = VecDeque::with_capacity(STDERR_TAIL);
    let mut frame = ProgressFrame::default();
    let mut exit_code: Option<i32> = None;

    while let Some(event) = events.recv().await {
        match event {
            CommandEvent::Stdout(line) => {
                let line = String::from_utf8_lossy(&line);
                if let Some(done) = frame.absorb(line.trim()) {
                    let _ = app.emit(
                        EVENT_PROGRESS,
                        ProgressPayload {
                            job_id: job_id.to_string(),
                            fraction: frame.fraction(total_secs),
                            out_bytes: frame.out_bytes,
                            speed: frame.speed.clone(),
                        },
                    );
                    if done {
                        break;
                    }
                }
            }
            CommandEvent::Stderr(line) => {
                let line = String::from_utf8_lossy(&line).trim().to_string();
                if !line.is_empty() {
                    if stderr_tail.len() == STDERR_TAIL {
                        stderr_tail.pop_front();
                    }
                    stderr_tail.push_back(line);
                }
            }
            CommandEvent::Terminated(payload) => {
                exit_code = payload.code;
                break;
            }
            _ => {}
        }
    }

    // `Terminated` may arrive after the final progress block; drain the rest so
    // we always learn the exit code.
    if exit_code.is_none() {
        while let Some(event) = events.recv().await {
            match event {
                CommandEvent::Stderr(line) => {
                    let line = String::from_utf8_lossy(&line).trim().to_string();
                    if !line.is_empty() {
                        if stderr_tail.len() == STDERR_TAIL {
                            stderr_tail.pop_front();
                        }
                        stderr_tail.push_back(line);
                    }
                }
                CommandEvent::Terminated(payload) => {
                    exit_code = payload.code;
                    break;
                }
                _ => {}
            }
        }
    }

    if registry.is_cancelled(job_id) {
        return Err(ConvertError::Cancelled);
    }

    match exit_code {
        Some(0) => {
            let output_bytes = std::fs::metadata(output).map(|m| m.len()).unwrap_or(0);
            let _ = app.emit(
                EVENT_DONE,
                DonePayload {
                    job_id: job_id.to_string(),
                    output_path: output.to_string_lossy().into_owned(),
                    output_bytes,
                    elapsed_ms: started.elapsed().as_millis(),
                },
            );
            Ok(())
        }
        other => {
            let message = if stderr_tail.is_empty() {
                match other {
                    Some(code) => format!("ffmpeg exited with code {code}"),
                    None => "ffmpeg was terminated".to_string(),
                }
            } else {
                stderr_tail.into_iter().collect::<Vec<_>>().join("\n")
            };
            Err(ConvertError::Failed(message))
        }
    }
}

/// Accumulates one `-progress` block. ffmpeg writes a run of `key=value` lines
/// terminated by `progress=continue` (or `progress=end` for the last one).
#[derive(Default)]
struct ProgressFrame {
    out_time_us: Option<u64>,
    out_bytes: Option<u64>,
    speed: Option<String>,
}

impl ProgressFrame {
    /// Returns `Some(is_final)` when the block is complete.
    fn absorb(&mut self, line: &str) -> Option<bool> {
        let (key, value) = line.split_once('=')?;
        let value = value.trim();
        match key.trim() {
            "out_time_us" | "out_time_ms" => {
                // out_time_ms is microseconds too — a long-standing ffmpeg
                // misnomer — so only trust out_time_us when both appear.
                if key.trim() == "out_time_us" || self.out_time_us.is_none() {
                    self.out_time_us = value.parse().ok();
                }
            }
            "out_time" if self.out_time_us.is_none() => {
                self.out_time_us = parse_timestamp(value).map(|s| (s * 1_000_000.0) as u64);
            }
            "total_size" => self.out_bytes = value.parse().ok(),
            "speed" => {
                self.speed = (value != "N/A" && !value.is_empty()).then(|| value.to_string())
            }
            "progress" => return Some(value == "end"),
            _ => {}
        }
        None
    }

    fn fraction(&self, total_secs: Option<f64>) -> Option<f64> {
        let total = total_secs?;
        let elapsed = self.out_time_us? as f64 / 1_000_000.0;
        (total > 0.0).then(|| (elapsed / total).clamp(0.0, 1.0))
    }
}

/// `HH:MM:SS.microseconds` -> seconds.
fn parse_timestamp(value: &str) -> Option<f64> {
    let mut secs = 0.0;
    for part in value.split(':') {
        secs = secs * 60.0 + part.parse::<f64>().ok()?;
    }
    Some(secs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_progress_block() {
        let mut frame = ProgressFrame::default();
        assert_eq!(frame.absorb("bitrate=1234kbits/s"), None);
        assert_eq!(frame.absorb("total_size=2048"), None);
        assert_eq!(frame.absorb("out_time_us=5000000"), None);
        assert_eq!(frame.absorb("speed=2.4x"), None);
        assert_eq!(frame.absorb("progress=continue"), Some(false));

        assert_eq!(frame.out_bytes, Some(2048));
        assert_eq!(frame.speed.as_deref(), Some("2.4x"));
        assert_eq!(frame.fraction(Some(10.0)), Some(0.5));
        assert_eq!(frame.fraction(None), None);
    }

    #[test]
    fn end_marker_is_flagged() {
        let mut frame = ProgressFrame::default();
        assert_eq!(frame.absorb("progress=end"), Some(true));
    }

    #[test]
    fn falls_back_to_the_formatted_timestamp() {
        let mut frame = ProgressFrame::default();
        frame.absorb("out_time=00:00:30.500000");
        assert_eq!(frame.out_time_us, Some(30_500_000));
    }

    #[test]
    fn fraction_never_exceeds_one() {
        let mut frame = ProgressFrame::default();
        frame.absorb("out_time_us=99000000");
        assert_eq!(frame.fraction(Some(10.0)), Some(1.0));
    }
}
