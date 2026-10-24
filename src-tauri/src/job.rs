//! Picks the backend for a job and reports the result the same way for all of
//! them. Everything that knows *which* engine handles *what* lives here.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use tauri::{AppHandle, Emitter};

use crate::advanced::Advanced;
use crate::decimate::MeshEdit;
use crate::edit::{self, EditSpec, Segment};
use crate::engines::{self, EngineId};
use crate::external::ExternalJob;
use crate::ffmpeg::ConvertError;
use crate::model::{DonePayload, MediaKind, ModuleId, Quality, EVENT_DONE};
use crate::queue::JobRegistry;
use crate::settings::Settings;
use crate::{data, document, external, ffmpeg, magick, mesh, ocr, presets, speech, tts};

pub enum Plan {
    /// One entry per piece of output. A plain conversion has exactly one; a
    /// split has one per segment.
    Ffmpeg {
        /// Decoded to a plain picture first. A HEIC photo is assembled from
        /// its tiles by a filter graph ffmpeg builds itself, and ffmpeg then
        /// refuses any further `-vf` on it — a colour change, or the icon
        /// preset's own filter — so those read from the assembled picture.
        stage: Option<ffmpeg::Run>,
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
    Mesh {
        quality: Quality,
        edit: MeshEdit,
    },
    /// Read in-process by the built-in OCR engine.
    Ocr,
    /// Spoken by Piper, then converted if the target is not WAV.
    Speak {
        job: tts::TtsJob,
        post: Option<ffmpeg::Run>,
    },
    /// Engines run one after another, each checked for the file it should
    /// have written. Data going to a document is an HTML table from the app's
    /// own data helper first, then pandoc.
    Chain(Vec<(PathBuf, ExternalJob)>),
    /// Could not be planned. Carried through the queue like any other job so
    /// that one bad file fails its own row instead of the whole batch.
    Failed(String),
}

impl Plan {
    /// Scratch directories the plan made while it was being built, for when
    /// it is dropped without ever running.
    pub fn scratch(&self) -> Vec<PathBuf> {
        match self {
            Plan::External(job) => job.cleanup.clone(),
            Plan::Pipeline { external, .. } => external.cleanup.clone(),
            Plan::Chain(steps) => steps
                .iter()
                .flat_map(|(_, job)| job.cleanup.clone())
                .collect(),
            Plan::Speak { job, .. } => vec![job.scratch.clone()],
            Plan::Ffmpeg {
                stage: Some(stage), ..
            } => stage
                .output
                .parent()
                .map(Path::to_path_buf)
                .into_iter()
                .collect(),
            _ => Vec::new(),
        }
    }
}

/// Target formats currently possible for a kind. Depends on which engines are
/// installed, so the UI refetches this after the setup screen changes.
pub fn targets_for(app: &AppHandle, settings: &Settings, kind: MediaKind) -> Vec<String> {
    match kind {
        MediaKind::Data => return data_targets(app),
        // A PDF's text needs nothing installed, so documents are never
        // switched off entirely; the module decides what else is on offer.
        MediaKind::Document => {
            let mut targets = document::targets(app);
            if settings.tts && tts::available(app) {
                targets.extend(tts::TARGETS.iter().map(|s| s.to_string()));
            }
            return targets;
        }
        _ => {}
    }
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
        // Handled above, before the module check.
        ModuleId::Documents => Vec::new(),
        ModuleId::Models => mesh::targets(app),
    }
}

/// Always the built-in formats; documents and an office PDF on top, where the
/// engines for them are installed.
fn data_targets(app: &AppHandle) -> Vec<String> {
    let mut out: Vec<String> = data::TARGETS.iter().map(|s| s.to_string()).collect();
    let mut add = |list: &[&str]| {
        for target in list {
            if !out.iter().any(|existing| existing == target) {
                out.push(target.to_string());
            }
        }
    };
    if engines::executable(app, EngineId::Pandoc).is_some() {
        add(data::PANDOC_TARGETS);
    }
    if engines::find_libreoffice().is_some() {
        add(data::OFFICE_TARGETS);
    }
    out
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
    match kind {
        MediaKind::Data => {
            return (!data::is_input(extension))
                .then(|| format!(".{extension} is not a supported data format"));
        }
        // Gated per engine rather than by the module switch: see targets_for.
        MediaKind::Document => return document::rejection(app, extension),
        _ => {}
    }
    if !settings.enabled(module) {
        return Some(match module {
            ModuleId::Documents => "Turn on the document module in setup to convert this".into(),
            ModuleId::Models => "Turn on the 3D module in setup to convert this".into(),
            ModuleId::Media => "The media module is unavailable".into(),
        });
    }
    match module {
        // ffmpeg cannot open a vector or a raw photograph; ImageMagick can,
        // and it is optional.
        ModuleId::Media if magick::handles(extension) => (!settings.extra_images
            || !magick::available(app))
        .then(|| format!(".{extension} needs the extra image formats — turn them on in setup")),
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
    pub advanced: &'a Advanced,
    pub edit: &'a EditSpec,
    pub segments: &'a [Segment],
    pub job_id: &'a str,
    /// From the probe. `None` means ffprobe could not open the file.
    pub width: Option<u32>,
    pub has_alpha: bool,
}

pub fn build(app: &AppHandle, request: BuildRequest) -> Result<Plan, String> {
    let BuildRequest {
        kind,
        input,
        outputs,
        target,
        quality,
        advanced,
        edit,
        segments,
        job_id,
        width,
        has_alpha,
    } = request;
    let extension = input
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
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
        // PostScript vectors and raw photographs go through ImageMagick
        // instead, and so does an SVG or HEIC this ffmpeg could not open.
        MediaKind::Image
            if magick::handles(&extension) || (magick::fallback(&extension) && width.is_none()) =>
        {
            let plan = magick::plan(app, input, primary, quality)?;
            Ok(Plan::External(ExternalJob {
                program: plan.program,
                args: plan.args,
                cwd: None,
                produced: None,
                cleanup: Vec::new(),
            }))
        }
        MediaKind::Image | MediaKind::Audio | MediaKind::Video => Ok(Plan::Ffmpeg {
            stage: (kind == MediaKind::Image && matches!(extension.as_str(), "heic" | "heif"))
                .then(|| {
                    let scratch = std::env::temp_dir().join(format!("coldmill-{job_id}"));
                    ffmpeg::Run {
                        pre_input: Vec::new(),
                        // Lossless and quick: it only lives until the encode.
                        encode: ["-frames:v", "1", "-c:v", "png", "-compression_level", "1"]
                            .map(String::from)
                            .to_vec(),
                        output: scratch.join("assembled.png"),
                        total_secs: None,
                    }
                }),
            runs: media_runs(&MediaRequest {
                kind,
                extension: &extension,
                outputs,
                target,
                quality,
                advanced,
                edit,
                segments,
                width,
                has_alpha,
            })?,
        }),
        MediaKind::Data => data_plan(app, input, outputs, target, &extension, job_id),
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
                encode: advanced.apply(
                    presets::encode_args(MediaKind::Audio, target, quality).unwrap_or_default(),
                    MediaKind::Audio,
                ),
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
                return Ok(Plan::Mesh {
                    quality,
                    edit: edit.mesh,
                });
            }
            let plan = mesh::blender_plan(app, input, primary, job_id, quality, edit.mesh)?;
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

/// Everything an ffmpeg encode is decided from. Kept free of the app handle so
/// the arguments it produces can be run through a real ffmpeg in tests.
pub struct MediaRequest<'a> {
    pub kind: MediaKind,
    /// The source's lowercase extension.
    pub extension: &'a str,
    pub outputs: &'a [PathBuf],
    pub target: &'a str,
    pub quality: Quality,
    pub advanced: &'a Advanced,
    pub edit: &'a EditSpec,
    pub segments: &'a [Segment],
    pub width: Option<u32>,
    pub has_alpha: bool,
}

/// One ffmpeg run per output.
pub fn media_runs(request: &MediaRequest) -> Result<Vec<ffmpeg::Run>, String> {
    let MediaRequest {
        kind,
        extension,
        outputs,
        target,
        quality,
        advanced,
        edit,
        segments,
        width,
        has_alpha,
    } = *request;

    // A video going to mp3 is encoded as the audio it becomes.
    let produces = presets::output_kind(kind, target);
    let target_ext = target.trim_start_matches('.').to_ascii_lowercase();

    // Overrides sit on top of the preset rather than beside it, so
    // there is only ever one description of an encode.
    let mut preset = advanced.apply(presets::encode_args(kind, target, quality)?, produces);
    if kind == MediaKind::Image && has_alpha && presets::drops_alpha(&target_ext) {
        preset = edit::prepend_filter(preset, presets::OVER_WHITE);
    }

    // A vector has no pixels until it is drawn, and drawn at its own
    // nominal size an icon comes out the size of an icon. The same
    // densities ImageMagick is given: 96, 150 and 300 dpi.
    let render_width = (kind == MediaKind::Image && magick::VECTOR_INPUTS.contains(&extension))
        .then_some(width)
        .flatten()
        .map(|width| {
            let scale = match quality {
                Quality::Small => 1.0,
                Quality::Balanced => 150.0 / 96.0,
                Quality::High => 300.0 / 96.0,
            };
            ((width as f64 * scale).round() as u32).clamp(1, 8192)
        });

    // Muting is about a video's soundtrack. On the way to an mp3 the
    // soundtrack is the whole file, and `-an` beside the preset's
    // `-vn` would leave ffmpeg nothing at all to write.
    let unmuted = EditSpec {
        mute: false,
        ..edit.clone()
    };
    let edit = if produces == MediaKind::Audio && kind == MediaKind::Video {
        &unmuted
    } else {
        edit
    };

    Ok(segments
        .iter()
        .zip(outputs)
        .map(|(segment, output)| {
            // Re-framing is folded into the preset filter; trimming and
            // muting are plain options appended after it. A soundtrack
            // has no frame to re-frame.
            let mut encode = if produces == MediaKind::Audio {
                preset.clone()
            } else {
                edit::apply_video_filters(preset.clone(), edit)
            };
            encode.extend(edit::output_args(edit, segment));

            let mut pre_input = edit::pre_input_args(segment);
            if let Some(width) = render_width {
                pre_input.extend(["-width".to_string(), width.to_string()]);
            }

            ffmpeg::Run {
                pre_input,
                encode,
                output: output.clone(),
                total_secs: segment.duration,
            }
        })
        .collect())
}

fn data_plan(
    app: &AppHandle,
    input: &Path,
    outputs: &[PathBuf],
    target: &str,
    extension: &str,
    job_id: &str,
) -> Result<Plan, String> {
    let target = target.trim_start_matches('.').to_ascii_lowercase();
    let output = outputs.first().ok_or("no output path")?;
    let office = engines::find_libreoffice().is_some();

    // LibreOffice first for a workbook going to a workbook or a PDF, when it
    // is there: it carries formulas, formats and page setup across, where the
    // built-in path carries values. Without it, values are what you get.
    let workbook_to_workbook =
        data::WORKBOOK_INPUTS.contains(&extension) && matches!(target.as_str(), "xlsx" | "ods");
    let to_office =
        data::OFFICE_TARGETS.contains(&target.as_str()) && data::office_can_open(extension);
    if office && (workbook_to_workbook || to_office) {
        let plan = document::libreoffice_plan(input, &target, job_id)?;
        return Ok(Plan::External(ExternalJob {
            program: plan.program,
            args: plan.args,
            cwd: plan.cwd,
            produced: plan.produced,
            cleanup: plan.cleanup,
        }));
    }

    let helper = |target: &str, outputs: &[PathBuf]| -> Result<ExternalJob, String> {
        Ok(ExternalJob {
            program: std::env::current_exe()
                .map_err(|e| format!("cannot find Coldmill itself: {e}"))?,
            args: data::helper_args(target, input, outputs),
            cwd: None,
            produced: None,
            cleanup: Vec::new(),
        })
    };

    if data::TARGETS.contains(&target.as_str()) {
        return Ok(Plan::External(helper(&target, outputs)?));
    }

    if data::PANDOC_TARGETS.contains(&target.as_str()) {
        let scratch = std::env::temp_dir().join(format!("coldmill-{job_id}"));
        let html = scratch.join("table.html");
        // Planned before the scratch directory exists, so a missing module
        // leaves nothing behind.
        let mut plan = document::pandoc_plan(app, &html, output, &target).map_err(|_| {
            if data::OFFICE_TARGETS.contains(&target.as_str()) && !data::office_can_open(extension)
            {
                format!(".{extension} to .{target} needs the document module")
            } else {
                "Turn on the document module in setup to make documents from data".to_string()
            }
        })?;
        std::fs::create_dir_all(&scratch).map_err(|e| e.to_string())?;
        plan.cleanup.push(scratch);
        return Ok(Plan::Chain(vec![
            (html.clone(), helper("html", std::slice::from_ref(&html))?),
            (
                output.to_path_buf(),
                ExternalJob {
                    program: plan.program,
                    args: plan.args,
                    cwd: plan.cwd,
                    produced: plan.produced,
                    cleanup: plan.cleanup,
                },
            ),
        ]));
    }

    Err(format!(".{extension} cannot be turned into .{target}"))
}

/// Runs a plan and emits `convert:done` on success. Errors are emitted by the
/// caller, which also knows how to clean up a half-written file.
pub async fn run(
    app: &AppHandle,
    registry: &Arc<JobRegistry>,
    job_id: &str,
    input: &Path,
    outputs: &[PathBuf],
    plan: Plan,
) -> Result<(), ConvertError> {
    let started = Instant::now();
    let output = outputs
        .first()
        .ok_or_else(|| ConvertError::Failed("a job needs somewhere to write".into()))?;

    match plan {
        Plan::Failed(message) => return Err(ConvertError::Failed(message)),
        Plan::Ffmpeg { stage, runs } => {
            let staged = async {
                let source = match &stage {
                    Some(stage) => {
                        if let Some(dir) = stage.output.parent() {
                            std::fs::create_dir_all(dir)
                                .map_err(|e| ConvertError::Failed(e.to_string()))?;
                        }
                        ffmpeg::run(app, registry, job_id, input, stage, (0.0, 0.5)).await?;
                        stage.output.clone()
                    }
                    None => input.to_path_buf(),
                };
                // Each piece gets its own slice of the progress bar, so a file
                // cut into four still fills one bar once instead of four times.
                let span = 1.0 / runs.len() as f64;
                for (index, spec) in runs.iter().enumerate() {
                    ffmpeg::run(
                        app,
                        registry,
                        job_id,
                        &source,
                        spec,
                        (index as f64 * span, span),
                    )
                    .await?;
                }
                Ok::<(), ConvertError>(())
            }
            .await;
            if let Some(dir) = stage.as_ref().and_then(|stage| stage.output.parent()) {
                let _ = std::fs::remove_dir_all(dir);
            }
            staged?;
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
            let spoke = async {
                tokio::task::spawn_blocking(move || tts::speak(&job))
                    .await
                    .map_err(|e| ConvertError::Failed(e.to_string()))?
                    .map_err(ConvertError::Failed)?;

                match post {
                    // Piper only writes WAV, so anything else is a second pass.
                    Some(run) => {
                        ffmpeg::run(app, registry, job_id, &spoken, &run, (0.9, 0.1)).await
                    }
                    None => std::fs::copy(&spoken, output).map(|_| ()).map_err(|e| {
                        ConvertError::Failed(format!("could not save the audio: {e}"))
                    }),
                }
            }
            .await;
            // The scratch WAV goes whether or not the speaking worked; it used
            // to be left behind in the temp folder every time a job failed.
            let _ = std::fs::remove_dir_all(&scratch);
            spoke?;

            if registry.is_cancelled(job_id) {
                return Err(ConvertError::Cancelled);
            }
        }
        Plan::Chain(steps) => {
            // Every step's scratch goes, even the ones a failure skipped.
            let cleanup: Vec<PathBuf> = steps
                .iter()
                .flat_map(|(_, job)| job.cleanup.clone())
                .collect();
            let mut result = Ok(());
            for (writes, job) in steps {
                result = external::run(app, registry, job_id, &writes, job).await;
                if result.is_err() {
                    break;
                }
            }
            for directory in cleanup {
                let _ = std::fs::remove_dir_all(directory);
            }
            result?;
        }
        Plan::Mesh { quality, edit } => {
            let (from, to) = (input.to_path_buf(), output.to_path_buf());
            let result =
                tokio::task::spawn_blocking(move || mesh::convert(&from, &to, quality, edit))
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

    // Only count what actually landed: a segment can be empty if the source
    // turned out shorter than the cut asked for.
    let written: Vec<&PathBuf> = outputs.iter().filter(|path| path.is_file()).collect();
    let _ = app.emit(
        EVENT_DONE,
        DonePayload {
            job_id: job_id.to_string(),
            output_path: output.to_string_lossy().into_owned(),
            outputs: written
                .iter()
                .map(|path| path.to_string_lossy().into_owned())
                .collect(),
            output_bytes: written
                .iter()
                .filter_map(|path| std::fs::metadata(path).ok())
                .map(|meta| meta.len())
                .sum(),
            elapsed_ms: started.elapsed().as_millis(),
        },
    );
    Ok(())
}

/// The arguments `media_runs` builds, run through the bundled ffmpeg and the
/// results measured. Ignored by default because they need the sidecars on
/// disk:
///
///   cargo test -- --ignored --nocapture
#[cfg(test)]
mod against_real_ffmpeg {
    use super::*;
    use std::process::Command;

    static NO_ADVANCED: Advanced = Advanced {
        video_kbps: None,
        crf: None,
        encoder_preset: None,
        fps: None,
        max_height: None,
        audio_kbps: None,
        sample_rate: None,
        channels: None,
    };

    fn sidecar(prefix: &str) -> PathBuf {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("binaries");
        std::fs::read_dir(dir)
            .ok()
            .and_then(|entries| {
                entries.flatten().find_map(|entry| {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    name.starts_with(prefix).then(|| entry.path())
                })
            })
            .expect("run scripts/fetch-ffmpeg.sh first")
    }

    fn ffmpeg(args: &[String]) {
        let out = Command::new(sidecar("ffmpeg-"))
            .args(["-hide_banner", "-loglevel", "error", "-y"])
            .args(args)
            .output()
            .expect("spawn ffmpeg");
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    fn probe(path: &Path, entries: &str) -> String {
        let out = Command::new(sidecar("ffprobe-"))
            .args(["-v", "error", "-show_entries", entries, "-of", "csv=p=0"])
            .arg(path)
            .output()
            .expect("spawn ffprobe");
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    fn work(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("coldmill-job-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(name)
    }

    /// Runs what `media_runs` planned, exactly as `ffmpeg.rs` lays it out.
    fn convert(input: &Path, request: &MediaRequest) {
        for run in media_runs(request).unwrap() {
            let mut args = run.pre_input.clone();
            args.extend(["-i".to_string(), input.to_string_lossy().into_owned()]);
            args.extend(run.encode.clone());
            args.push(run.output.to_string_lossy().into_owned());
            ffmpeg(&args);
        }
    }

    fn request<'a>(
        kind: MediaKind,
        extension: &'a str,
        outputs: &'a [PathBuf],
        target: &'a str,
        edit: &'a EditSpec,
        segments: &'a [Segment],
    ) -> MediaRequest<'a> {
        MediaRequest {
            kind,
            extension,
            outputs,
            target,
            quality: Quality::Balanced,
            advanced: &NO_ADVANCED,
            edit,
            segments,
            width: None,
            has_alpha: false,
        }
    }

    #[test]
    #[ignore]
    fn the_soundtrack_comes_out_trimmed_even_when_the_video_was_muted() {
        let source = work("clip.mp4");
        ffmpeg(&[
            "-f".into(),
            "lavfi".into(),
            "-i".into(),
            "testsrc=duration=4:size=320x240:rate=15".into(),
            "-f".into(),
            "lavfi".into(),
            "-i".into(),
            "sine=frequency=440:duration=4".into(),
            "-shortest".into(),
            "-pix_fmt".into(),
            "yuv420p".into(),
            source.to_string_lossy().into_owned(),
        ]);

        // Mute is on, which on its own would strip the only stream left.
        let edit = EditSpec {
            trim_start: Some(1.0),
            trim_end: Some(3.0),
            mute: true,
            ..EditSpec::default()
        };
        let segments = edit::segments(&edit, Some(4.0));
        let outputs = [work("clip.mp3")];
        convert(
            &source,
            &request(MediaKind::Video, "mp4", &outputs, "mp3", &edit, &segments),
        );

        assert_eq!(probe(&outputs[0], "stream=codec_type"), "audio");
        let seconds: f64 = probe(&outputs[0], "format=duration").parse().unwrap();
        assert!((seconds - 2.0).abs() < 0.15, "{seconds}");
    }

    #[test]
    #[ignore]
    fn transparency_lands_on_white_not_black() {
        let source = work("logo.png");
        ffmpeg(&[
            "-f".into(),
            "lavfi".into(),
            "-i".into(),
            "color=c=black@0.0:size=64x64,format=rgba".into(),
            "-frames:v".into(),
            "1".into(),
            source.to_string_lossy().into_owned(),
        ]);
        let edit = EditSpec::default();
        let segments = edit::segments(&edit, None);
        let outputs = [work("logo.jpg")];
        let mut plan = request(MediaKind::Image, "png", &outputs, "jpg", &edit, &segments);
        plan.has_alpha = true;
        convert(&source, &plan);

        let picture = image::open(&outputs[0]).unwrap().to_rgb8();
        let corner = picture.get_pixel(0, 0);
        assert!(corner.0.iter().all(|channel| *channel > 245), "{corner:?}");
    }

    /// Not on macOS: that ffmpeg is built without librsvg, so an SVG there
    /// goes to ImageMagick instead.
    #[test]
    #[ignore]
    #[cfg(not(target_os = "macos"))]
    fn a_vector_is_drawn_larger_at_higher_quality() {
        let source = work("icon.svg");
        std::fs::write(
            &source,
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="96" height="48"><rect width="96" height="48" fill="#4d7cff"/></svg>"##,
        )
        .unwrap();
        let edit = EditSpec::default();
        let segments = edit::segments(&edit, None);
        let outputs = [work("icon.png")];
        let mut plan = request(MediaKind::Image, "svg", &outputs, "png", &edit, &segments);
        plan.width = Some(96);
        plan.quality = Quality::High;
        convert(&source, &plan);

        // 300 dpi against SVG's nominal 96.
        assert_eq!(probe(&outputs[0], "stream=width,height"), "300,150");
    }

    #[test]
    #[ignore]
    fn a_clip_becomes_an_animated_webp() {
        let source = work("loop.mp4");
        ffmpeg(&[
            "-f".into(),
            "lavfi".into(),
            "-i".into(),
            "testsrc=duration=2:size=1280x720:rate=30".into(),
            "-pix_fmt".into(),
            "yuv420p".into(),
            source.to_string_lossy().into_owned(),
        ]);
        let edit = EditSpec::default();
        let segments = edit::segments(&edit, Some(2.0));
        let outputs = [work("loop.webp")];
        convert(
            &source,
            &request(MediaKind::Video, "mp4", &outputs, "webp", &edit, &segments),
        );
        let bytes = std::fs::read(&outputs[0]).unwrap();
        // RIFF....WEBP with an animation chunk.
        assert_eq!(&bytes[8..12], b"WEBP");
        assert!(bytes.windows(4).any(|w| w == b"ANIM"));
    }
}
