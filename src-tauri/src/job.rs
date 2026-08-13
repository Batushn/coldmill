//! Picks the backend for a job and reports the result the same way for all of
//! them. Everything that knows *which* engine handles *what* lives here.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use tauri::{AppHandle, Emitter};

use crate::edit::{self, EditSpec, Segment};
use crate::external::ExternalJob;
use crate::ffmpeg::ConvertError;
use crate::model::{DonePayload, MediaKind, ModuleId, Quality, EVENT_DONE};
use crate::queue::JobRegistry;
use crate::settings::Settings;
use crate::{document, external, ffmpeg, mesh, ocr, presets, speech, tts};

pub enum Plan {
    /// One entry per piece of output. A plain conversion has exactly one; a
    /// split has one per segment.
    Ffmpeg {
        runs: Vec<ffmpeg::Run>,
    },
    External(ExternalJob),
    /// An engine that needs ffmpeg to prepare its input first — transcription
    /// reads 16 kHz mono PCM and nothing else.
    Pipeline {
        pre: ffmpeg::Run,
        external: ExternalJob,
    },
    /// Converted in-process by `mesh.rs`, no engine needed.
    Mesh,
    /// Read in-process by the built-in OCR engine.
    Ocr,
    /// Spoken by Piper, then converted if the target is not WAV.
    Speak {
        job: tts::TtsJob,
        post: Option<ffmpeg::Run>,
    },
}

/// Target formats currently possible for a kind. Depends on which engines are
/// installed, so the UI refetches this after the setup screen changes.
pub fn targets_for(app: &AppHandle, settings: &Settings, kind: MediaKind) -> Vec<String> {
    let Some(module) = kind.module().filter(|m| settings.enabled(*m)) else {
        return Vec::new();
    };
    match module {
        ModuleId::Media => {
            let mut targets: Vec<String> = presets::targets(kind)
                .iter()
                .map(|s| s.to_string())
                .collect();
            // Anything with speech in it can also come out as words.
            if settings.speech
                && matches!(kind, MediaKind::Audio | MediaKind::Video)
                && speech::available(app)
            {
                targets.extend(speech::TARGETS.iter().map(|s| s.to_string()));
            }
            // And a picture of words can come out as the words.
            if settings.ocr && kind == MediaKind::Image && ocr::available(app) {
                targets.extend(ocr::TARGETS.iter().map(|s| s.to_string()));
            }
            targets
        }
        ModuleId::Documents => {
            let mut targets = document::targets(app);
            // A plain-text document can also be read aloud.
            if settings.tts && tts::available(app) {
                targets.extend(tts::TARGETS.iter().map(|s| s.to_string()));
            }
            targets
        }
        ModuleId::Models => mesh::targets(app),
    }
}

/// Why a file cannot be queued right now — a missing module, a missing engine,
/// or a format nothing here handles.
pub fn rejection(
    app: &AppHandle,
    settings: &Settings,
    kind: MediaKind,
    extension: &str,
) -> Option<String> {
    let Some(module) = kind.module() else {
        return Some("Unsupported file".into());
    };
    if !settings.enabled(module) {
        return Some(match module {
            ModuleId::Documents => "Turn on the document module in setup to convert this".into(),
            ModuleId::Models => "Turn on the 3D module in setup to convert this".into(),
            ModuleId::Media => "The media module is unavailable".into(),
        });
    }
    match module {
        ModuleId::Media => None,
        ModuleId::Documents => document::rejection(app, extension),
        ModuleId::Models => mesh::rejection(app, extension),
    }
}

pub struct BuildRequest<'a> {
    pub kind: MediaKind,
    pub input: &'a Path,
    /// One path per segment; never empty. Only the first is used by the
    /// backends that cannot split.
    pub outputs: &'a [PathBuf],
    pub target: &'a str,
    pub quality: Quality,
    pub edit: &'a EditSpec,
    pub segments: &'a [Segment],
    pub job_id: &'a str,
}

pub fn build(app: &AppHandle, request: BuildRequest) -> Result<Plan, String> {
    let BuildRequest {
        kind,
        input,
        outputs,
        target,
        quality,
        edit,
        segments,
        job_id,
    } = request;
    let primary = outputs.first().ok_or("no output path")?;

    match kind {
        // A transcript is not an encode: it goes out through whisper instead.
        MediaKind::Audio | MediaKind::Video if speech::is_target(target) => {
            let plan = speech::plan(app, primary, job_id)?;
            Ok(Plan::Pipeline {
                pre: ffmpeg::Run {
                    pre_input: edit::pre_input_args(&segments[0]),
                    encode: {
                        let mut args = speech::wav_args();
                        args.extend(edit::output_args(edit, &segments[0]));
                        args
                    },
                    output: plan.wav,
                    total_secs: segments[0].duration,
                },
                external: ExternalJob {
                    program: plan.program,
                    args: plan.args,
                    cwd: None,
                    produced: Some(plan.produced),
                    cleanup: plan.cleanup,
                },
            })
        }
        // A picture of words, read rather than re-encoded.
        MediaKind::Image if ocr::is_target(target) => match ocr::tesseract_plan(input, job_id) {
            Some(plan) => Ok(Plan::External(ExternalJob {
                program: plan.program,
                args: plan.args,
                cwd: None,
                produced: Some(plan.produced),
                cleanup: plan.cleanup,
            })),
            None => Ok(Plan::Ocr),
        },
        MediaKind::Image | MediaKind::Audio | MediaKind::Video => {
            let preset = presets::encode_args(kind, target, quality)?;

            let runs = segments
                .iter()
                .zip(outputs)
                .map(|(segment, output)| {
                    // Re-framing is folded into the preset filter; trimming and
                    // muting are plain options appended after it.
                    let mut encode = edit::apply_orientation(preset.clone(), edit.orientation);
                    encode.extend(edit::output_args(edit, segment));

                    ffmpeg::Run {
                        pre_input: edit::pre_input_args(segment),
                        encode,
                        output: output.clone(),
                        total_secs: segment.duration,
                    }
                })
                .collect();

            Ok(Plan::Ffmpeg { runs })
        }
        // Words on a page, read out loud.
        MediaKind::Document
            if tts::is_target(target)
                && input
                    .extension()
                    .map(|e| tts::is_source(&e.to_string_lossy()))
                    .unwrap_or(false) =>
        {
            let job = tts::job(app, input, job_id)?;
            let post = (!target.eq_ignore_ascii_case("wav")).then(|| ffmpeg::Run {
                pre_input: Vec::new(),
                encode: presets::encode_args(MediaKind::Audio, target, quality).unwrap_or_default(),
                output: primary.clone(),
                total_secs: None,
            });
            Ok(Plan::Speak { job, post })
        }
        MediaKind::Document => {
            let plan = document::plan(app, input, primary, job_id)?;
            Ok(Plan::External(ExternalJob {
                program: plan.program,
                args: plan.args,
                cwd: plan.cwd,
                produced: plan.produced,
                cleanup: plan.cleanup,
            }))
        }
        MediaKind::Model => {
            if !mesh::needs_blender(input, primary) {
                return Ok(Plan::Mesh);
            }
            let plan = mesh::blender_plan(app, input, primary, job_id)?;
            Ok(Plan::External(ExternalJob {
                program: plan.program,
                args: plan.args,
                cwd: None,
                produced: None,
                cleanup: plan.cleanup,
            }))
        }
        MediaKind::Unsupported => Err("Unsupported file".into()),
    }
}

/// Runs a plan and emits `convert:done` on success. Errors are emitted by the
/// caller, which also knows how to clean up a half-written file.
pub async fn run(
    app: &AppHandle,
    registry: &Arc<JobRegistry>,
    job_id: &str,
    input: &Path,
    output: &Path,
    plan: Plan,
) -> Result<(), ConvertError> {
    let started = Instant::now();

    match plan {
        Plan::Ffmpeg { runs } => {
            // Each piece gets its own slice of the progress bar, so a file cut
            // into four still fills one bar once instead of four times.
            let span = 1.0 / runs.len() as f64;
            for (index, spec) in runs.iter().enumerate() {
                ffmpeg::run(
                    app,
                    registry,
                    job_id,
                    input,
                    spec,
                    (index as f64 * span, span),
                )
                .await?;
            }
        }
        Plan::External(job) => external::run(app, registry, job_id, output, job).await?,
        Plan::Pipeline { pre, external } => {
            // The preparation is a real decode, so it gets the first third of
            // the bar; the engine that follows reports nothing at all.
            ffmpeg::run(app, registry, job_id, input, &pre, (0.0, 0.35)).await?;
            external::run(app, registry, job_id, output, external).await?;
            let _ = std::fs::remove_file(&pre.output);
        }
        Plan::Ocr => {
            let (from, to) = (input.to_path_buf(), output.to_path_buf());
            let app = app.clone();
            let result = tokio::task::spawn_blocking(move || ocr::read_with_ocrs(&app, &from, &to))
                .await
                .map_err(|e| ConvertError::Failed(e.to_string()))?;
            result.map_err(ConvertError::Failed)?;
            if registry.is_cancelled(job_id) {
                return Err(ConvertError::Cancelled);
            }
        }
        Plan::Speak { job, post } => {
            let spoken = job.wav.clone();
            let scratch = job.scratch.clone();
            let result = tokio::task::spawn_blocking(move || tts::speak(&job))
                .await
                .map_err(|e| ConvertError::Failed(e.to_string()))?;
            result.map_err(ConvertError::Failed)?;

            match post {
                // Piper only writes WAV, so anything else is a second pass.
                Some(run) => ffmpeg::run(app, registry, job_id, &spoken, &run, (0.9, 0.1)).await?,
                None => std::fs::copy(&spoken, output)
                    .map(|_| ())
                    .map_err(|e| ConvertError::Failed(format!("could not save the audio: {e}")))?,
            }
            let _ = std::fs::remove_dir_all(&scratch);

            if registry.is_cancelled(job_id) {
                return Err(ConvertError::Cancelled);
            }
        }
        Plan::Mesh => {
            let (from, to) = (input.to_path_buf(), output.to_path_buf());
            let result = tokio::task::spawn_blocking(move || mesh::convert(&from, &to))
                .await
                .map_err(|e| ConvertError::Failed(e.to_string()))?;
            result.map_err(ConvertError::Failed)?;
            // In-process work has no process to kill, so cancellation is only
            // observed once it finishes.
            if registry.is_cancelled(job_id) {
                return Err(ConvertError::Cancelled);
            }
        }
    }

    let _ = app.emit(
        EVENT_DONE,
        DonePayload {
            job_id: job_id.to_string(),
            output_path: output.to_string_lossy().into_owned(),
            output_bytes: std::fs::metadata(output).map(|m| m.len()).unwrap_or(0),
            elapsed_ms: started.elapsed().as_millis(),
        },
    );
    Ok(())
}
