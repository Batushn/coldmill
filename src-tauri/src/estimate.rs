//! Output size estimation.
//!
//! Two stages. Before a job runs, this file guesses from the source's
//! dimensions, duration and the chosen preset. Once ffmpeg starts reporting,
//! `ffmpeg.rs` projects the real figure from bytes-written over progress —
//! which is accurate within a few percent after the first seconds.
//!
//! These are estimates and the UI says so with a `~`. Encoders are
//! content-adaptive: a static screencast and a confetti cannon at the same CRF
//! differ by an order of magnitude.

use serde::{Deserialize, Serialize};

use crate::edit::{self, EditSpec};
use crate::model::{MediaKind, Quality};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EstimateItem {
    pub path: String,
    pub kind: MediaKind,
    pub target_format: String,
    pub size_bytes: u64,
    pub duration_secs: Option<f64>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub fps: Option<f64>,
    #[serde(default)]
    pub edit: EditSpec,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Estimate {
    pub path: String,
    /// `None` when there is nothing honest to say — documents and 3D models
    /// depend far too much on their content.
    pub bytes: Option<u64>,
}

pub fn estimate(item: &EstimateItem, quality: Quality) -> Option<u64> {
    let target = item
        .target_format
        .trim_start_matches('.')
        .to_ascii_lowercase();

    // Trimming a ten-minute clip down to thirty seconds should be reflected
    // here, or the number under the row would be wrong by a factor of twenty.
    let item = &EstimateItem {
        duration_secs: effective_duration(item),
        ..item.clone()
    };

    match item.kind {
        MediaKind::Video => video(item, &target, quality),
        MediaKind::Audio => audio(item, &target, quality),
        MediaKind::Image => image(item, &target, quality),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Video
// ---------------------------------------------------------------------------

/// How much of the source actually reaches the output, once trims and splits
/// are taken into account. Split pieces are summed: together they are still
/// one batch of bytes on disk.
fn effective_duration(item: &EstimateItem) -> Option<f64> {
    if item.edit.is_noop() {
        return item.duration_secs;
    }
    let kept: f64 = edit::segments(&item.edit, item.duration_secs)
        .iter()
        .filter_map(|segment| segment.duration)
        .sum();
    (kept > 0.0).then_some(kept)
}

/// Bits per pixel per frame at each preset. Derived from the CRF values in
/// `presets.rs` against typical 1080p footage.
fn video_bpp(target: &str, quality: Quality) -> f64 {
    let h264 = match quality {
        Quality::Small => 0.028,
        Quality::Balanced => 0.055,
        Quality::High => 0.100,
    };
    match target {
        // VP9 buys roughly a third off H.264 at matching quality.
        "webm" => h264 * 0.7,
        // mpeg4 is an old codec at a fixed qscale; it needs far more.
        "avi" => h264 * 2.6,
        _ => h264,
    }
}

fn audio_kbps(target: &str, quality: Quality) -> f64 {
    match (target, quality) {
        ("webm", Quality::Small) => 96.0,
        ("webm", Quality::Balanced) => 128.0,
        ("webm", Quality::High) => 192.0,
        (_, Quality::Small) => 128.0,
        (_, Quality::Balanced) => 192.0,
        (_, Quality::High) => 256.0,
    }
}

fn video(item: &EstimateItem, target: &str, quality: Quality) -> Option<u64> {
    let duration = item.duration_secs?;

    if target == "gif" {
        // GIF is resized and palettised; frame count drives everything.
        let (fps, width) = match quality {
            Quality::Small => (10.0, 480.0),
            Quality::Balanced => (15.0, 640.0),
            Quality::High => (20.0, 800.0),
        };
        let aspect = match (item.width, item.height) {
            (Some(w), Some(h)) if w > 0 => h as f64 / w as f64,
            _ => 9.0 / 16.0,
        };
        let source_width = item.width.map(|w| w as f64).unwrap_or(width);
        let width = width.min(source_width);
        let pixels = width * (width * aspect);
        // ~0.35 bytes per pixel per frame after palette + LZW.
        return Some((pixels * fps * duration * 0.35) as u64);
    }

    let (width, height) = (item.width?, item.height?);
    let fps = item.fps.unwrap_or(30.0).clamp(1.0, 240.0);
    let pixels = width as f64 * height as f64;

    let video_bps = pixels * fps * video_bpp(target, quality);
    let audio_bps = audio_kbps(target, quality) * 1000.0;
    Some(((video_bps + audio_bps) / 8.0 * duration) as u64)
}

// ---------------------------------------------------------------------------
// Audio
// ---------------------------------------------------------------------------

fn audio(item: &EstimateItem, target: &str, quality: Quality) -> Option<u64> {
    let duration = item.duration_secs?;

    // Lossless formats depend on the source, not on a bitrate.
    let bytes_per_second = match target {
        // 44.1 kHz stereo, 16- or 24-bit.
        "wav" => {
            return Some(
                (duration
                    * 44_100.0
                    * 2.0
                    * if matches!(quality, Quality::High) {
                        3.0
                    } else {
                        2.0
                    }) as u64,
            )
        }
        // FLAC lands around 60% of PCM for music.
        "flac" => return Some((duration * 44_100.0 * 2.0 * 2.0 * 0.6) as u64),
        "mp3" => match quality {
            Quality::Small => 128.0,
            Quality::Balanced => 192.0,
            Quality::High => 320.0,
        },
        "opus" => match quality {
            Quality::Small => 64.0,
            Quality::Balanced => 96.0,
            Quality::High => 160.0,
        },
        "ogg" => match quality {
            Quality::Small => 112.0,
            Quality::Balanced => 160.0,
            Quality::High => 256.0,
        },
        _ => match quality {
            Quality::Small => 128.0,
            Quality::Balanced => 192.0,
            Quality::High => 256.0,
        },
    };

    Some((bytes_per_second * 1000.0 / 8.0 * duration) as u64)
}

// ---------------------------------------------------------------------------
// Image
// ---------------------------------------------------------------------------

/// Bytes per pixel for a photographic source.
fn image_bpp(target: &str, quality: Quality) -> Option<f64> {
    let by_quality = |small: f64, balanced: f64, high: f64| match quality {
        Quality::Small => small,
        Quality::Balanced => balanced,
        Quality::High => high,
    };
    Some(match target {
        "jpg" | "jpeg" => by_quality(0.12, 0.30, 0.85),
        "webp" => by_quality(0.08, 0.22, 0.50),
        "avif" => by_quality(0.04, 0.10, 0.28),
        "png" => 1.6,
        "gif" => 0.5,
        "tiff" => by_quality(1.5, 1.8, 3.0),
        "bmp" => 3.0,
        _ => return None,
    })
}

fn image(item: &EstimateItem, target: &str, quality: Quality) -> Option<u64> {
    let bpp = image_bpp(target, quality)?;
    match (item.width, item.height) {
        (Some(width), Some(height)) => Some((width as f64 * height as f64 * bpp) as u64),
        // No dimensions (an exotic still ffprobe could not measure): scale the
        // source instead of pretending we know nothing.
        _ => Some((item.size_bytes as f64 * bpp / 1.6) as u64),
    }
}

/// Refines a running job's estimate from what ffmpeg has actually written.
/// Ignores the first sliver of the file, where container headers dominate.
pub fn project(written_bytes: u64, fraction: f64) -> Option<u64> {
    (fraction > 0.05).then(|| (written_bytes as f64 / fraction) as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clip() -> EstimateItem {
        EstimateItem {
            path: "clip.mov".into(),
            kind: MediaKind::Video,
            target_format: "mp4".into(),
            size_bytes: 500_000_000,
            duration_secs: Some(60.0),
            width: Some(1920),
            height: Some(1080),
            fps: Some(30.0),
            edit: EditSpec::default(),
        }
    }

    #[test]
    fn balanced_1080p_lands_in_a_believable_range() {
        let bytes = estimate(&clip(), Quality::Balanced).unwrap();
        let mb = bytes as f64 / 1_048_576.0;
        // A minute of balanced 1080p is tens of megabytes, not hundreds.
        assert!((20.0..120.0).contains(&mb), "got {mb} MB");
    }

    #[test]
    fn quality_tiers_are_ordered() {
        let small = estimate(&clip(), Quality::Small).unwrap();
        let balanced = estimate(&clip(), Quality::Balanced).unwrap();
        let high = estimate(&clip(), Quality::High).unwrap();
        assert!(small < balanced && balanced < high);
    }

    #[test]
    fn documents_and_models_report_nothing() {
        let mut item = clip();
        item.kind = MediaKind::Document;
        assert!(estimate(&item, Quality::Balanced).is_none());
        item.kind = MediaKind::Model;
        assert!(estimate(&item, Quality::Balanced).is_none());
    }

    #[test]
    fn a_video_with_no_duration_cannot_be_estimated() {
        let mut item = clip();
        item.duration_secs = None;
        assert!(estimate(&item, Quality::Balanced).is_none());
    }

    #[test]
    fn trimming_shrinks_the_estimate() {
        let whole = estimate(&clip(), Quality::Balanced).unwrap();
        let trimmed = estimate(
            &EstimateItem {
                edit: EditSpec {
                    trim_start: Some(10.0),
                    trim_end: Some(25.0),
                    ..EditSpec::default()
                },
                ..clip()
            },
            Quality::Balanced,
        )
        .unwrap();
        // A quarter of the clip should cost about a quarter of the bytes.
        assert!(trimmed < whole / 3, "{trimmed} vs {whole}");
    }

    #[test]
    fn projection_waits_for_a_meaningful_sample() {
        assert_eq!(project(1_000, 0.01), None);
        assert_eq!(project(1_000_000, 0.5), Some(2_000_000));
    }
}
