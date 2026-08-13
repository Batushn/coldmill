//! Trimming, splitting, muting and re-framing.
//!
//! Everything the editing panel can ask for turns into ffmpeg arguments here,
//! next to `presets.rs`, so the two places that decide how a file is encoded
//! stay next to each other.
//!
//! A split is not a special kind of job: it is several trims of the same
//! source, run one after another. That keeps one row equal to one job, and the
//! queue never had to learn about fan-out.

use serde::{Deserialize, Serialize};

/// What the output should be shaped like. The source is scaled to cover the
/// target ratio and then centre-cropped — the framing people expect when they
/// move a clip between a phone and a monitor.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Orientation {
    #[default]
    Keep,
    /// 9:16
    Portrait,
    /// 16:9
    Landscape,
    /// 1:1
    Square,
}

impl Orientation {
    fn ratio(self) -> Option<(u32, u32)> {
        match self {
            Orientation::Keep => None,
            Orientation::Portrait => Some((9, 16)),
            Orientation::Landscape => Some((16, 9)),
            Orientation::Square => Some((1, 1)),
        }
    }

    /// Crop the middle out of the source at the target ratio.
    ///
    /// Expressed against the source dimensions rather than a fixed size, so a
    /// 4K clip is not quietly forced down to 1080p on its way to vertical.
    fn filter(self) -> Option<String> {
        let (w, h) = self.ratio()?;
        Some(format!(
            "crop=w=min(iw\\,ih*{w}/{h}):h=min(ih\\,iw*{h}/{w}),\
             scale=trunc(iw/2)*2:trunc(ih/2)*2"
        ))
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct EditSpec {
    /// Seconds from the start of the source.
    pub trim_start: Option<f64>,
    pub trim_end: Option<f64>,
    pub mute: bool,
    pub orientation: Orientation,
    /// Cut points inside the trimmed range, in seconds.
    pub split_points: Vec<f64>,
}

impl EditSpec {
    pub fn is_noop(&self) -> bool {
        self.trim_start.is_none()
            && self.trim_end.is_none()
            && !self.mute
            && self.orientation == Orientation::Keep
            && self.split_points.is_empty()
    }
}

/// One piece of output: where it starts in the source and how long it runs.
#[derive(Debug, Clone, PartialEq)]
pub struct Segment {
    pub start: f64,
    /// `None` means "to the end of the source".
    pub duration: Option<f64>,
}

/// Splits the trimmed range at each cut point. Always returns at least one
/// segment, so callers never have to special-case an unedited file.
pub fn segments(edit: &EditSpec, source_duration: Option<f64>) -> Vec<Segment> {
    let start = edit.trim_start.unwrap_or(0.0).max(0.0);
    let end = match (edit.trim_end, source_duration) {
        (Some(end), Some(total)) => Some(end.min(total)),
        (Some(end), None) => Some(end),
        (None, total) => total,
    };

    let mut cuts: Vec<f64> = edit
        .split_points
        .iter()
        .copied()
        .filter(|point| *point > start + 0.05 && end.map(|e| *point < e - 0.05).unwrap_or(true))
        .collect();
    cuts.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    cuts.dedup();

    let mut segments = Vec::with_capacity(cuts.len() + 1);
    let mut cursor = start;
    for cut in cuts {
        segments.push(Segment {
            start: cursor,
            duration: Some(cut - cursor),
        });
        cursor = cut;
    }
    segments.push(Segment {
        start: cursor,
        duration: end.map(|e| e - cursor).filter(|d| *d > 0.0),
    });
    segments
}

/// Arguments that must sit *before* `-i`: seeking there lets ffmpeg jump ahead
/// instead of decoding and discarding everything up to the cut.
pub fn pre_input_args(segment: &Segment) -> Vec<String> {
    if segment.start <= 0.0 {
        return Vec::new();
    }
    vec!["-ss".into(), format!("{:.3}", segment.start)]
}

/// Arguments that belong with the encoder settings.
pub fn output_args(edit: &EditSpec, segment: &Segment) -> Vec<String> {
    let mut args = Vec::new();
    if let Some(duration) = segment.duration {
        args.push("-t".into());
        args.push(format!("{duration:.3}"));
    }
    if edit.mute {
        args.push("-an".into());
    }
    args
}

/// Folds the re-framing filter into whatever filter the preset already set,
/// rather than adding a second `-vf` that would silently win or lose.
pub fn apply_orientation(mut args: Vec<String>, orientation: Orientation) -> Vec<String> {
    let Some(filter) = orientation.filter() else {
        return args;
    };
    match args.iter().position(|arg| arg == "-vf") {
        // Re-framing runs first, so a preset chain that follows it (a GIF
        // palette, say) sees the final geometry.
        Some(index) if index + 1 < args.len() => {
            args[index + 1] = format!("{filter},{}", args[index + 1]);
        }
        _ => {
            args.push("-vf".into());
            args.push(filter);
        }
    }
    args
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_untouched_file_is_one_whole_segment() {
        let list = segments(&EditSpec::default(), Some(60.0));
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].start, 0.0);
        assert_eq!(list[0].duration, Some(60.0));
    }

    #[test]
    fn trimming_moves_both_ends() {
        let edit = EditSpec {
            trim_start: Some(10.0),
            trim_end: Some(40.0),
            ..EditSpec::default()
        };
        let list = segments(&edit, Some(60.0));
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].start, 10.0);
        assert_eq!(list[0].duration, Some(30.0));
    }

    #[test]
    fn splitting_divides_the_trimmed_range() {
        let edit = EditSpec {
            trim_start: Some(10.0),
            trim_end: Some(40.0),
            split_points: vec![20.0, 30.0],
            ..EditSpec::default()
        };
        let list = segments(&edit, Some(60.0));
        assert_eq!(list.len(), 3);
        assert_eq!(
            list[0],
            Segment {
                start: 10.0,
                duration: Some(10.0)
            }
        );
        assert_eq!(
            list[2],
            Segment {
                start: 30.0,
                duration: Some(10.0)
            }
        );
    }

    #[test]
    fn cuts_outside_the_trim_are_ignored() {
        let edit = EditSpec {
            trim_start: Some(10.0),
            trim_end: Some(20.0),
            split_points: vec![5.0, 15.0, 50.0],
            ..EditSpec::default()
        };
        assert_eq!(segments(&edit, Some(60.0)).len(), 2);
    }

    #[test]
    fn seeking_only_appears_when_there_is_something_to_skip() {
        assert!(pre_input_args(&Segment {
            start: 0.0,
            duration: None
        })
        .is_empty());
        assert_eq!(
            pre_input_args(&Segment {
                start: 2.5,
                duration: None
            }),
            vec!["-ss".to_string(), "2.500".to_string()]
        );
    }

    #[test]
    fn muting_and_duration_land_in_the_output_args() {
        let edit = EditSpec {
            mute: true,
            ..EditSpec::default()
        };
        let args = output_args(
            &edit,
            &Segment {
                start: 0.0,
                duration: Some(12.0),
            },
        );
        assert_eq!(args, vec!["-t", "12.000", "-an"]);
    }

    #[test]
    fn orientation_merges_into_an_existing_filter() {
        let args = apply_orientation(vec!["-vf".into(), "fps=15".into()], Orientation::Portrait);
        assert_eq!(args.len(), 2);
        assert!(
            args[1].ends_with(",fps=15"),
            "the preset filter must survive: {}",
            args[1]
        );
        assert!(args[1].starts_with("crop="));
    }

    #[test]
    fn orientation_is_added_when_there_is_no_filter_yet() {
        let args = apply_orientation(vec!["-c:v".into(), "libx264".into()], Orientation::Square);
        assert_eq!(args[2], "-vf");
        assert!(args[3].contains("crop="));
    }

    #[test]
    fn keeping_the_shape_changes_nothing() {
        let args = vec!["-c:v".to_string(), "libx264".to_string()];
        assert_eq!(apply_orientation(args.clone(), Orientation::Keep), args);
    }
}

/// The arguments above are only right if ffmpeg agrees. Ignored by default
/// because it needs the sidecars:
///
///   cargo test -- --ignored --nocapture
#[cfg(test)]
mod against_real_ffmpeg {
    use super::*;
    use std::path::PathBuf;
    use std::process::Command;

    fn sidecar(prefix: &str) -> Option<PathBuf> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("binaries");
        std::fs::read_dir(dir).ok()?.flatten().find_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            name.starts_with(prefix).then(|| entry.path())
        })
    }

    fn run(program: &PathBuf, args: &[String]) -> String {
        let out = Command::new(program).args(args).output().expect("spawn");
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    fn probe(ffprobe: &PathBuf, file: &str, entries: &str, stream: &[&str]) -> String {
        let mut args: Vec<String> = vec!["-v".into(), "error".into()];
        args.extend(stream.iter().map(|s| s.to_string()));
        args.extend([
            "-show_entries".into(),
            entries.into(),
            "-of".into(),
            "csv=p=0".into(),
            file.into(),
        ]);
        run(ffprobe, &args)
    }

    #[test]
    #[ignore]
    fn a_trim_a_mute_and_a_reframe_all_land() {
        let (Some(ffmpeg), Some(ffprobe)) = (sidecar("ffmpeg-"), sidecar("ffprobe-")) else {
            eprintln!("no sidecars — run scripts/fetch-ffmpeg.sh");
            return;
        };

        let work = std::env::temp_dir().join("coldmill-edit-test");
        std::fs::create_dir_all(&work).unwrap();
        let source = work.join("source.mp4").to_string_lossy().into_owned();
        let result = work.join("edited.mp4").to_string_lossy().into_owned();

        // Eight seconds of 16:9 with a tone on it.
        run(
            &ffmpeg,
            &[
                "-hide_banner",
                "-loglevel",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "testsrc=duration=8:size=1280x720:rate=25",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:duration=8",
                "-shortest",
                "-pix_fmt",
                "yuv420p",
                &source,
            ]
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>(),
        );

        let edit = EditSpec {
            trim_start: Some(2.0),
            trim_end: Some(5.0),
            mute: true,
            orientation: Orientation::Portrait,
            ..EditSpec::default()
        };
        let piece = &segments(&edit, Some(8.0))[0];

        let mut args: Vec<String> = vec![
            "-hide_banner".into(),
            "-loglevel".into(),
            "error".into(),
            "-y".into(),
        ];
        args.extend(pre_input_args(piece));
        args.extend(["-i".to_string(), source.clone()]);
        args.extend(apply_orientation(
            vec!["-c:v".into(), "libx264".into(), "-crf".into(), "28".into()],
            edit.orientation,
        ));
        args.extend(output_args(&edit, piece));
        args.push(result.clone());
        run(&ffmpeg, &args);

        let duration: f64 = probe(&ffprobe, &result, "format=duration", &[])
            .parse()
            .expect("duration");
        assert!(
            (duration - 3.0).abs() < 0.2,
            "trim should leave three seconds, got {duration}"
        );

        let size = probe(
            &ffprobe,
            &result,
            "stream=width,height",
            &["-select_streams", "v:0"],
        );
        let (width, height) = size.split_once(',').expect("dimensions");
        let ratio: f64 = width.parse::<f64>().unwrap() / height.parse::<f64>().unwrap();
        assert!(
            (ratio - 9.0 / 16.0).abs() < 0.02,
            "portrait should be about 9:16, got {size}"
        );

        assert!(
            probe(
                &ffprobe,
                &result,
                "stream=codec_type",
                &["-select_streams", "a"]
            )
            .is_empty(),
            "mute should leave no audio stream"
        );
    }
}
