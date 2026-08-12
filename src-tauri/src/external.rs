//! Runs a non-ffmpeg engine (pandoc, LibreOffice, Blender).
//!
//! None of them stream machine-readable progress, so a job here is a spinner
//! rather than a percentage. What they do share with ffmpeg is the process
//! handle going into the registry, which is what makes cancel work.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tauri::{AppHandle, Emitter};
use tauri_plugin_shell::process::CommandEvent;
use tauri_plugin_shell::ShellExt;

use crate::ffmpeg::ConvertError;
use crate::model::{ProgressPayload, EVENT_PROGRESS};
use crate::queue::JobRegistry;

const STDERR_TAIL: usize = 12;

pub struct ExternalJob {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
    /// Where the tool insists on writing, when it will not take our path.
    pub produced: Option<PathBuf>,
    /// Scratch directories to remove once the job ends, whatever the outcome.
    pub cleanup: Vec<PathBuf>,
}

pub async fn run(
    app: &AppHandle,
    registry: &Arc<JobRegistry>,
    job_id: &str,
    output: &Path,
    job: ExternalJob,
) -> Result<(), ConvertError> {
    let result = execute(app, registry, job_id, output, &job).await;
    for directory in &job.cleanup {
        let _ = std::fs::remove_dir_all(directory);
    }
    result
}

async fn execute(
    app: &AppHandle,
    registry: &Arc<JobRegistry>,
    job_id: &str,
    output: &Path,
    job: &ExternalJob,
) -> Result<(), ConvertError> {
    let mut command = app
        .shell()
        .command(job.program.to_string_lossy().into_owned())
        .args(job.args.clone());
    if let Some(cwd) = &job.cwd {
        command = command.current_dir(cwd.clone());
    }

    let (mut events, child) = command
        .spawn()
        .map_err(|e| ConvertError::Failed(format!("could not start {}: {e}", label(job))))?;

    if let Some(child) = registry.attach_child(job_id, child) {
        let _ = child.kill();
        return Err(ConvertError::Cancelled);
    }

    // No progress stream: show an indeterminate bar for the whole run.
    let _ = app.emit(
        EVENT_PROGRESS,
        ProgressPayload {
            job_id: job_id.to_string(),
            fraction: None,
            out_bytes: None,
            speed: None,
            estimated_bytes: None,
        },
    );

    let mut tail: VecDeque<String> = VecDeque::with_capacity(STDERR_TAIL);
    let mut exit_code: Option<i32> = None;

    while let Some(event) = events.recv().await {
        match event {
            // Several of these engines report failures on stdout.
            CommandEvent::Stdout(line) | CommandEvent::Stderr(line) => {
                let line = String::from_utf8_lossy(&line).trim().to_string();
                if !line.is_empty() {
                    if tail.len() == STDERR_TAIL {
                        tail.pop_front();
                    }
                    tail.push_back(line);
                }
            }
            CommandEvent::Terminated(payload) => {
                exit_code = payload.code;
                break;
            }
            _ => {}
        }
    }

    if registry.is_cancelled(job_id) {
        return Err(ConvertError::Cancelled);
    }
    if exit_code != Some(0) {
        return Err(ConvertError::Failed(message(&tail, exit_code, job)));
    }

    // LibreOffice names its own output; move it where the user expects.
    if let Some(produced) = &job.produced {
        if !produced.is_file() {
            return Err(ConvertError::Failed(message(&tail, exit_code, job)));
        }
        move_file(produced, output).map_err(ConvertError::Failed)?;
    }

    if !output.is_file() {
        return Err(ConvertError::Failed(format!(
            "{} reported success but wrote no file",
            label(job)
        )));
    }
    Ok(())
}

fn label(job: &ExternalJob) -> String {
    job.program
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "the converter".into())
}

fn message(tail: &VecDeque<String>, code: Option<i32>, job: &ExternalJob) -> String {
    if tail.is_empty() {
        match code {
            Some(code) => format!("{} exited with code {code}", label(job)),
            None => format!("{} was terminated", label(job)),
        }
    } else {
        tail.iter().cloned().collect::<Vec<_>>().join("\n")
    }
}

/// Rename first; fall back to copy when the scratch directory is on another
/// volume, which it usually is on Windows.
fn move_file(from: &Path, to: &Path) -> Result<(), String> {
    if std::fs::rename(from, to).is_ok() {
        return Ok(());
    }
    std::fs::copy(from, to).map_err(|e| format!("could not save the result: {e}"))?;
    let _ = std::fs::remove_file(from);
    Ok(())
}
