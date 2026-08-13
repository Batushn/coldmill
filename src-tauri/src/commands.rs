//! The IPC surface: inspect a file, convert a batch, cancel, and manage the
//! optional modules.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};
use uuid::Uuid;

use crate::engines::{self, EngineId, EngineStatus, EVENT_ENGINE_DONE, EVENT_ENGINE_ERROR};
use crate::estimate::{self, Estimate, EstimateItem};
use crate::ffmpeg::ConvertError;
use crate::model::{
    ConvertRequest, ErrorPayload, FileProbe, JobCreated, MediaKind, Quality, EVENT_ERROR,
};
use crate::queue::JobRegistry;
use crate::settings::{self, Settings};
use crate::{detect, edit, job, mesh, probe, thumbs};

/// Inspect a dropped file: what it really is, how long it runs, and whether
/// the modules it needs are available.
#[tauri::command]
pub async fn probe_file(app: AppHandle, path: String) -> Result<FileProbe, String> {
    let file = Path::new(&path);
    let meta = std::fs::metadata(file).map_err(|e| format!("{path}: {e}"))?;
    if !meta.is_file() {
        return Err(format!("{path} is not a file"));
    }

    let detection = detect::detect(file, meta.len());
    let extension = file
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase());

    let mut result = FileProbe {
        path: path.clone(),
        file_name: file
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.clone()),
        size_bytes: meta.len(),
        kind: detection.kind,
        mime: detection.mime,
        extension: extension.clone(),
        duration_secs: None,
        width: None,
        height: None,
        fps: None,
        triangles: None,
        reason: detection.reason,
    };

    match result.kind {
        MediaKind::Image | MediaKind::Audio | MediaKind::Video => {
            // Magic bytes say it is media; ffprobe says whether ffmpeg can
            // actually open it — and gives us the duration progress needs.
            match probe::inspect(&app, &path).await {
                Ok(info) => {
                    result.kind =
                        detect::refine_with_streams(result.kind, info.has_video, info.has_audio);
                    result.width = info.width;
                    result.height = info.height;
                    result.fps = info.fps;
                    if result.kind != MediaKind::Image {
                        result.duration_secs = info.duration_secs;
                    }
                }
                Err(message) => {
                    result.kind = MediaKind::Unsupported;
                    result.reason = Some(first_line(&message));
                }
            }
        }
        MediaKind::Model => result.triangles = mesh::quick_triangle_count(file).map(|t| t as u64),
        _ => {}
    }

    // A file can be perfectly valid and still not convertible today: its
    // module may be off, or its engine missing.
    if result.kind.is_media() {
        let settings = settings::load(&app);
        if let Some(reason) = job::rejection(
            &app,
            &settings,
            result.kind,
            extension.as_deref().unwrap_or_default(),
        ) {
            result.kind = MediaKind::Unsupported;
            result.reason = Some(reason);
        }
    }

    Ok(result)
}

/// Target formats per media type. Recomputed on every call because installing
/// an engine changes the answer.
#[tauri::command]
pub fn supported_targets(app: AppHandle) -> HashMap<String, Vec<String>> {
    let settings = settings::load(&app);
    [
        MediaKind::Image,
        MediaKind::Audio,
        MediaKind::Video,
        MediaKind::Document,
        MediaKind::Model,
    ]
    .into_iter()
    .map(|kind| {
        let key = serde_json::to_value(kind)
            .ok()
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_default();
        (key, job::targets_for(&app, &settings, kind))
    })
    .collect()
}

#[tauri::command]
pub fn max_concurrency(registry: State<'_, Arc<JobRegistry>>) -> usize {
    registry.concurrency()
}

#[tauri::command]
pub fn estimate_output(items: Vec<EstimateItem>, quality: Quality) -> Vec<Estimate> {
    items
        .iter()
        .map(|item| Estimate {
            path: item.path.clone(),
            bytes: estimate::estimate(item, quality),
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Modules
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupState {
    pub settings: Settings,
    pub engines: Vec<EngineStatus>,
    /// Path to the system LibreOffice, when there is one.
    pub libreoffice: Option<String>,
}

#[tauri::command]
pub fn setup_state(app: AppHandle) -> SetupState {
    SetupState {
        settings: settings::load(&app),
        engines: EngineId::ALL
            .iter()
            .map(|id| engines::status(&app, *id))
            .collect(),
        libreoffice: engines::find_libreoffice().map(|p| p.to_string_lossy().into_owned()),
    }
}

/// Installs whatever the chosen modules need and removes what they no longer
/// do, then remembers the choice. Progress arrives as `engine:*` events.
#[tauri::command]
pub async fn apply_setup(app: AppHandle, settings: Settings) -> Result<SetupState, String> {
    let mut wanted: Vec<EngineId> = Vec::new();
    if settings.documents {
        wanted.push(EngineId::Pandoc);
        wanted.push(EngineId::Typst);
    }
    if settings.models && settings.blender {
        wanted.push(EngineId::Blender);
    }
    if settings.speech {
        wanted.push(EngineId::Whisper);
        wanted.push(EngineId::WhisperModel);
    }
    if settings.ocr {
        wanted.push(EngineId::OcrDetection);
        wanted.push(EngineId::OcrRecognition);
    }
    if settings.tts {
        wanted.push(EngineId::Piper);
        wanted.push(EngineId::PiperVoice);
    }
    if settings.extra_images {
        wanted.push(EngineId::ImageMagick);
    }

    for id in EngineId::ALL {
        if wanted.contains(id) {
            if let Err(message) = engines::install(&app, *id).await {
                let _ = app.emit(EVENT_ENGINE_ERROR, EngineEvent::new(*id, Some(&message)));
                return Err(message);
            }
            let _ = app.emit(EVENT_ENGINE_DONE, EngineEvent::new(*id, None));
        } else {
            // Freeing 400 MB when someone turns Blender back off is the least
            // we can do.
            engines::remove(&app, *id)?;
        }
    }

    let mut settings = settings;
    settings.setup_done = true;
    settings::save(&app, &settings)?;
    Ok(setup_state(app))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct EngineEvent {
    engine_id: EngineId,
    label: &'static str,
    message: Option<String>,
}

impl EngineEvent {
    fn new(id: EngineId, message: Option<&str>) -> Self {
        Self {
            engine_id: id,
            label: id.label(),
            message: message.map(str::to_string),
        }
    }
}

// ---------------------------------------------------------------------------
// Conversion
// ---------------------------------------------------------------------------

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
            return Err(format!("{} is not a supported file", item.path));
        }

        let input = PathBuf::from(&item.path);
        let dir = output_dir
            .clone()
            .or_else(|| input.parent().map(Path::to_path_buf))
            .ok_or_else(|| format!("Cannot determine an output folder for {}", item.path))?;

        // A split turns one file into several, each needing its own name.
        let segments = edit::segments(&item.edit, item.duration_secs);
        let outputs: Vec<PathBuf> = (0..segments.len())
            .map(|index| {
                let suffix = (segments.len() > 1).then_some(index + 1);
                unique_output(&input, &dir, &item.target_format, suffix, &mut taken)
            })
            .collect();
        let output = outputs[0].clone();
        let job_id = Uuid::new_v4().to_string();

        // Built here rather than in the worker so a bad request fails loudly
        // and immediately, before anything is queued.
        let plan = job::build(
            &app,
            job::BuildRequest {
                kind: item.kind,
                input: &input,
                outputs: &outputs,
                target: &item.target_format,
                quality,
                edit: &item.edit,
                segments: &segments,
                job_id: &job_id,
            },
        )?;

        registry.register(&job_id);
        created.push(JobCreated {
            job_id: job_id.clone(),
            path: item.path.clone(),
            output_path: output.to_string_lossy().into_owned(),
            outputs: outputs
                .iter()
                .map(|path| path.to_string_lossy().into_owned())
                .collect(),
        });
        planned.push((job_id, input, outputs, plan));
    }

    for (job_id, input, outputs, plan) in planned {
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

            if let Err(err) = job::run(&app, &registry, &job_id, &input, &outputs, plan).await {
                // Half-written files are worse than none: they look converted.
                for leftover in &outputs {
                    let _ = std::fs::remove_file(leftover);
                }
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
    part: Option<usize>,
    taken: &mut HashSet<PathBuf>,
) -> PathBuf {
    let extension = extension
        .trim()
        .trim_start_matches('.')
        .to_ascii_lowercase();
    let mut stem = input
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "output".to_string());
    // Pieces of a split are numbered rather than left to the "(1)" collision
    // suffix, which would read as an accident instead of an intent.
    if let Some(part) = part {
        stem = format!("{stem}-{part}");
    }

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
        let a = unique_output(Path::new("/in/clip.mov"), dir, "mp4", None, &mut taken);
        let b = unique_output(Path::new("/other/clip.avi"), dir, ".MP4", None, &mut taken);
        assert_eq!(a, dir.join("clip.mp4"));
        assert_eq!(b, dir.join("clip (1).mp4"));
    }

    #[test]
    fn split_pieces_are_numbered() {
        let mut taken = HashSet::new();
        let input = Path::new("/in/clip.mov");
        let dir = Path::new("/out");
        assert_eq!(
            unique_output(input, dir, "mp4", Some(1), &mut taken),
            dir.join("clip-1.mp4")
        );
        assert_eq!(
            unique_output(input, dir, "mp4", Some(2), &mut taken),
            dir.join("clip-2.mp4")
        );
    }

    #[test]
    fn never_overwrites_the_source() {
        let mut taken = HashSet::new();
        let input = Path::new("/in/song.mp3");
        let out = unique_output(input, Path::new("/in"), "mp3", None, &mut taken);
        assert_ne!(out, input);
        assert_eq!(out, Path::new("/in/song (1).mp3"));
    }
}

// ---------------------------------------------------------------------------
// Previews
// ---------------------------------------------------------------------------

/// One small frame (or a waveform) for a queue row. Returns `None` rather than
/// an error when there is nothing to draw: a missing preview is a cosmetic
/// shortfall, never a reason to fail a file.
#[tauri::command]
pub async fn thumbnail(
    app: AppHandle,
    path: String,
    kind: MediaKind,
    duration_secs: Option<f64>,
) -> Result<Option<String>, String> {
    Ok(thumbs::poster(&app, &path, kind, duration_secs)
        .await
        .unwrap_or(None))
}

/// The hover-scrub filmstrip. Built on demand, because it costs a full decode
/// pass and most files never get hovered.
#[tauri::command]
pub async fn scrub_strip(
    app: AppHandle,
    path: String,
    duration_secs: f64,
) -> Result<Option<thumbs::ScrubStrip>, String> {
    Ok(thumbs::strip(&app, &path, duration_secs)
        .await
        .unwrap_or(None))
}
