//! The IPC surface. Deliberately small: inspect a file, convert a batch, cancel.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tauri::{AppHandle, Emitter, State};
use uuid::Uuid;

use crate::ffmpeg::{self, ConvertError};
use crate::model::{ConvertRequest, ErrorPayload, FileProbe, JobCreated, MediaKind, EVENT_ERROR};
use crate::queue::JobRegistry;
use crate::{detect, presets, probe};

/// Inspect a dropped file: what it really is, and how long it runs for.
#[tauri::command]
pub async fn probe_file(app: AppHandle, path: String) -> Result<FileProbe, String> {
    let file = Path::new(&path);
    let meta = std::fs::metadata(file).map_err(|e| format!("{path}: {e}"))?;
    if !meta.is_file() {
        return Err(format!("{path} is not a file"));
    }

    let detection = detect::detect(file);
    let mut result = FileProbe {
        path: path.clone(),
        file_name: file
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.clone()),
        size_bytes: meta.len(),
        kind: detection.kind,
        mime: detection.mime,
        extension: file
            .extension()
            .map(|e| e.to_string_lossy().to_ascii_lowercase()),
        duration_secs: None,
        width: None,
        height: None,
        reason: detection.reason,
    };

    if !result.kind.is_media() {
        return Ok(result);
    }

    // Magic bytes say it is media; ffprobe says whether ffmpeg can actually
    // open it — and gives us the duration the progress bar needs.
    match probe::inspect(&app, &path).await {
        Ok(info) => {
            result.kind = detect::refine_with_streams(result.kind, info.has_video, info.has_audio);
            result.width = info.width;
            result.height = info.height;
            if result.kind != MediaKind::Image {
                result.duration_secs = info.duration_secs;
            }
        }
        Err(message) => {
            result.kind = MediaKind::Unsupported;
            result.reason = Some(first_line(&message));
        }
    }

    Ok(result)
}

/// Target formats per media type, so the UI cannot offer what we cannot encode.
#[tauri::command]
pub fn supported_targets() -> HashMap<&'static str, &'static [&'static str]> {
    HashMap::from([
        ("image", presets::targets(MediaKind::Image)),
        ("audio", presets::targets(MediaKind::Audio)),
        ("video", presets::targets(MediaKind::Video)),
    ])
}

#[tauri::command]
pub fn max_concurrency(registry: State<'_, Arc<JobRegistry>>) -> usize {
    registry.concurrency()
}

/// Queues every item and returns immediately. Progress arrives as events.
#[tauri::command]
pub async fn convert_files(
    app: AppHandle,
    registry: State<'_, Arc<JobRegistry>>,
    request: ConvertRequest,
) -> Result<Vec<JobCreated>, String> {
    let registry = Arc::clone(&registry);
    let quality = request.quality;

    let output_dir = match &request.output_dir {
        Some(dir) => {
            let dir = PathBuf::from(dir);
            std::fs::create_dir_all(&dir)
                .map_err(|e| format!("Cannot use output folder {}: {e}", dir.display()))?;
            Some(dir)
        }
        None => None,
    };

    // Reserve names up front so two jobs in the same batch cannot collide.
    let mut taken: HashSet<PathBuf> = HashSet::new();
    let mut created = Vec::with_capacity(request.items.len());
    let mut planned = Vec::with_capacity(request.items.len());

    for item in request.items {
        if !item.kind.is_media() {
            return Err(format!("{} is not a supported media file", item.path));
        }
        let encode = presets::encode_args(item.kind, &item.target_format, quality)?;

        let input = PathBuf::from(&item.path);
        let dir = output_dir
            .clone()
            .or_else(|| input.parent().map(Path::to_path_buf))
            .ok_or_else(|| format!("Cannot determine an output folder for {}", item.path))?;
        let output = unique_output(&input, &dir, &item.target_format, &mut taken);

        let job_id = Uuid::new_v4().to_string();
        registry.register(&job_id);

        created.push(JobCreated {
            job_id: job_id.clone(),
            path: item.path.clone(),
            output_path: output.to_string_lossy().into_owned(),
        });
        planned.push((job_id, input, output, encode, item.duration_secs, item.kind));
    }

    for (job_id, input, output, encode, duration_secs, kind) in planned {
        let app = app.clone();
        let registry = Arc::clone(&registry);

        tauri::async_runtime::spawn(async move {
            // Wait for a slot. Cancelling a queued job is why `register`
            // happens before the task is spawned.
            let _permit = registry.acquire().await;

            if registry.is_cancelled(&job_id) {
                emit_cancelled(&app, &job_id);
                registry.finish(&job_id);
                return;
            }

            // Stills have no timeline; everything else needs a total to divide
            // ffmpeg's out_time by.
            let total_secs = match kind {
                MediaKind::Image => None,
                _ => match duration_secs {
                    Some(secs) => Some(secs),
                    None => probe::inspect(&app, &input.to_string_lossy())
                        .await
                        .ok()
                        .and_then(|info| info.duration_secs),
                },
            };

            let outcome = ffmpeg::run(
                &app, &registry, &job_id, &input, &output, encode, total_secs,
            )
            .await;

            if let Err(err) = outcome {
                // A half-written file is worse than none: it looks converted.
                let _ = std::fs::remove_file(&output);
                match err {
                    ConvertError::Cancelled => emit_cancelled(&app, &job_id),
                    ConvertError::Failed(message) => {
                        let _ = app.emit(
                            EVENT_ERROR,
                            ErrorPayload {
                                job_id: job_id.clone(),
                                message,
                                cancelled: false,
                            },
                        );
                    }
                }
            }

            registry.finish(&job_id);
        });
    }

    Ok(created)
}

#[tauri::command]
pub fn cancel_job(registry: State<'_, Arc<JobRegistry>>, job_id: String) -> bool {
    registry.cancel(&job_id)
}

#[tauri::command]
pub fn cancel_all(registry: State<'_, Arc<JobRegistry>>) -> Vec<String> {
    registry.cancel_all()
}

fn emit_cancelled(app: &AppHandle, job_id: &str) {
    let _ = app.emit(
        EVENT_ERROR,
        ErrorPayload {
            job_id: job_id.to_string(),
            message: "Cancelled".into(),
            cancelled: true,
        },
    );
}

/// `photo.png` -> `photo.jpg`, or `photo (1).jpg` when that name is in use.
/// Never returns the input path itself.
fn unique_output(
    input: &Path,
    dir: &Path,
    extension: &str,
    taken: &mut HashSet<PathBuf>,
) -> PathBuf {
    let extension = extension
        .trim()
        .trim_start_matches('.')
        .to_ascii_lowercase();
    let stem = input
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "output".to_string());

    let mut candidate = dir.join(format!("{stem}.{extension}"));
    let mut counter = 1;
    while candidate == input || candidate.exists() || taken.contains(&candidate) {
        candidate = dir.join(format!("{stem} ({counter}).{extension}"));
        counter += 1;
    }
    taken.insert(candidate.clone());
    candidate
}

fn first_line(message: &str) -> String {
    message
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("Unreadable media file")
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_names_do_not_repeat_within_a_batch() {
        let mut taken = HashSet::new();
        let dir = Path::new("/out");
        let a = unique_output(Path::new("/in/clip.mov"), dir, "mp4", &mut taken);
        let b = unique_output(Path::new("/other/clip.avi"), dir, ".MP4", &mut taken);
        assert_eq!(a, dir.join("clip.mp4"));
        assert_eq!(b, dir.join("clip (1).mp4"));
    }

    #[test]
    fn never_overwrites_the_source() {
        let mut taken = HashSet::new();
        let input = Path::new("/in/song.mp3");
        let out = unique_output(input, Path::new("/in"), "mp3", &mut taken);
        assert_ne!(out, input);
        assert_eq!(out, Path::new("/in/song (1).mp3"));
    }
}
