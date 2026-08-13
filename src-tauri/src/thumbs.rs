//! Thumbnails and hover-scrub strips, drawn by the ffmpeg we already ship.
//!
//! Nothing is added to the installer for this: it is the same sidecar that does
//! the converting. Two things are generated per file, and only when asked for:
//!
//! * a **poster** — one small frame, or a waveform for audio. Cheap, requested
//!   as soon as a row appears.
//! * a **scrub strip** — a row of frames tiled into one image, which the UI
//!   slides under the cursor. Far more expensive, so it is only built when the
//!   pointer actually lands on a video.
//!
//! Both are cached on disk keyed by path, size and mtime, so a file dropped
//! twice costs one ffmpeg run, and re-opening the app costs none.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::{AppHandle, Manager};
use tauri_plugin_shell::ShellExt;

use crate::model::MediaKind;

/// Poster width. Wide enough for the grid tile on a HiDPI screen, small
/// enough that a hundred of them do not add up to anything.
const POSTER_WIDTH: u32 = 320;

/// Frames in a scrub strip. 40 gives a smooth-feeling scrub across a whole
/// video without the strip becoming a megabyte.
const STRIP_FRAMES: u32 = 40;
const STRIP_FRAME_WIDTH: u32 = 160;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScrubStrip {
    /// The tiled frames, as a data URI.
    pub data_uri: String,
    pub frames: u32,
}

fn cache_dir(app: &AppHandle) -> Option<PathBuf> {
    let dir = app.path().app_cache_dir().ok()?.join("thumbs");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

/// A file that changed on disk must not keep its old picture.
fn cache_key(path: &Path, suffix: &str) -> String {
    let mut hasher = DefaultHasher::new();
    path.to_string_lossy().hash(&mut hasher);
    if let Ok(meta) = std::fs::metadata(path) {
        meta.len().hash(&mut hasher);
        if let Ok(modified) = meta.modified() {
            if let Ok(age) = modified.duration_since(std::time::UNIX_EPOCH) {
                age.as_secs().hash(&mut hasher);
            }
        }
    }
    format!("{:016x}{suffix}.jpg", hasher.finish())
}

fn to_data_uri(bytes: &[u8]) -> String {
    use base64::Engine;
    format!(
        "data:image/jpeg;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    )
}

async fn run_ffmpeg(app: &AppHandle, args: Vec<String>) -> Result<(), String> {
    let output = app
        .shell()
        .sidecar("ffmpeg")
        .map_err(|e| format!("ffmpeg sidecar missing: {e}"))?
        .args(args)
        .output()
        .await
        .map_err(|e| format!("could not start ffmpeg: {e}"))?;

    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

/// One representative frame, or a waveform for audio. `None` for kinds that
/// have nothing to show.
pub async fn poster(
    app: &AppHandle,
    path: &str,
    kind: MediaKind,
    duration_secs: Option<f64>,
) -> Result<Option<String>, String> {
    let source = Path::new(path);
    let Some(dir) = cache_dir(app) else {
        return Ok(None);
    };
    let cached = dir.join(cache_key(source, "-poster"));
    if let Ok(bytes) = std::fs::read(&cached) {
        return Ok(Some(to_data_uri(&bytes)));
    }

    let target = cached.to_string_lossy().into_owned();
    let mut args: Vec<String> = vec!["-hide_banner".into(), "-loglevel".into(), "error".into()];

    match kind {
        MediaKind::Video => {
            // A frame from a tenth of the way in: the first frame of a video is
            // very often black or a title card.
            if let Some(duration) = duration_secs.filter(|d| *d > 1.0) {
                args.push("-ss".into());
                args.push(format!("{:.2}", duration * 0.1));
            }
            args.extend(["-i".into(), path.to_string()]);
            args.extend([
                "-frames:v".into(),
                "1".into(),
                "-vf".into(),
                format!("scale={POSTER_WIDTH}:-2"),
            ]);
        }
        MediaKind::Image => {
            args.extend(["-i".into(), path.to_string()]);
            args.extend([
                "-frames:v".into(),
                "1".into(),
                "-vf".into(),
                format!("scale='min({POSTER_WIDTH},iw)':-2"),
            ]);
        }
        MediaKind::Audio => {
            args.extend(["-i".into(), path.to_string()]);
            args.extend([
                "-filter_complex".into(),
                format!(
                    "showwavespic=s={POSTER_WIDTH}x100:colors=#4d7cff|#6d8cff:split_channels=0"
                ),
                "-frames:v".into(),
                "1".into(),
            ]);
        }
        MediaKind::Model => {
            // Drawn here rather than by ffmpeg, which has never heard of a
            // mesh. The PPM is a scratch file only because ffmpeg reads
            // files; it is deleted as soon as the JPEG exists.
            let Some(ppm) = model_ppm(source) else {
                return Ok(None);
            };
            let scratch = dir.join(cache_key(source, "-model.ppm"));
            if std::fs::write(&scratch, ppm).is_err() {
                return Ok(None);
            }
            args.extend(["-i".into(), scratch.to_string_lossy().into_owned()]);
            args.extend(["-frames:v".into(), "1".into()]);

            args.extend(["-q:v".into(), "5".into(), "-y".into(), target]);
            let drawn = run_ffmpeg(app, args).await;
            let _ = std::fs::remove_file(&scratch);
            drawn?;

            return match std::fs::read(&cached) {
                Ok(bytes) => Ok(Some(to_data_uri(&bytes))),
                Err(_) => Ok(None),
            };
        }
        _ => return Ok(None),
    }

    args.extend(["-q:v".into(), "5".into(), "-y".into(), target]);
    run_ffmpeg(app, args).await?;

    match std::fs::read(&cached) {
        Ok(bytes) => Ok(Some(to_data_uri(&bytes))),
        Err(_) => Ok(None),
    }
}

/// Reading a mesh can mean parsing megabytes of ASCII, so it happens off the
/// async runtime. `None` for the formats only Blender can open: the built-in
/// reader will not guess at a file it cannot honestly parse.
fn model_ppm(source: &Path) -> Option<Vec<u8>> {
    let mesh = crate::mesh::read(source).ok()?;
    crate::preview3d::render(&mesh, POSTER_WIDTH as usize, POSTER_WIDTH as usize)
}

/// A row of frames spanning the whole video, tiled into a single image.
pub async fn strip(
    app: &AppHandle,
    path: &str,
    duration_secs: f64,
) -> Result<Option<ScrubStrip>, String> {
    if duration_secs <= 0.0 {
        return Ok(None);
    }
    let source = Path::new(path);
    let Some(dir) = cache_dir(app) else {
        return Ok(None);
    };
    let cached = dir.join(cache_key(source, "-strip"));
    if let Ok(bytes) = std::fs::read(&cached) {
        return Ok(Some(ScrubStrip {
            data_uri: to_data_uri(&bytes),
            frames: STRIP_FRAMES,
        }));
    }

    // One frame every duration/N seconds, laid out left to right. The fps
    // filter takes a rate, so the interval becomes its reciprocal.
    let fps = STRIP_FRAMES as f64 / duration_secs;
    let filter = format!("fps={fps:.6},scale={STRIP_FRAME_WIDTH}:-2,tile={STRIP_FRAMES}x1",);

    run_ffmpeg(
        app,
        vec![
            "-hide_banner".into(),
            "-loglevel".into(),
            "error".into(),
            "-i".into(),
            path.to_string(),
            "-vf".into(),
            filter,
            "-frames:v".into(),
            "1".into(),
            "-q:v".into(),
            "6".into(),
            "-y".into(),
            cached.to_string_lossy().into_owned(),
        ],
    )
    .await?;

    match std::fs::read(&cached) {
        Ok(bytes) => Ok(Some(ScrubStrip {
            data_uri: to_data_uri(&bytes),
            frames: STRIP_FRAMES,
        })),
        Err(_) => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_keys_separate_poster_from_strip() {
        let path = Path::new("/videos/clip.mp4");
        assert_ne!(cache_key(path, "-poster"), cache_key(path, "-strip"));
    }

    #[test]
    fn cache_keys_are_stable_for_the_same_file() {
        let path = Path::new("/videos/clip.mp4");
        assert_eq!(cache_key(path, "-poster"), cache_key(path, "-poster"));
    }

    #[test]
    fn data_uris_are_declared_as_jpeg() {
        assert!(to_data_uri(&[1, 2, 3]).starts_with("data:image/jpeg;base64,"));
    }
}
