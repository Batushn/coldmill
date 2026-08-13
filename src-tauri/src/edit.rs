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

/// What the output should be shaped like. How the source is made to fit that
/// shape is a separate choice — see [`Fit`].
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

    /// The filter chain that reshapes the frame.
    ///
    /// Every expression is written against the source dimensions rather than a
    /// fixed size, so a 4K clip is not quietly forced down to 1080p on its way
    /// to vertical.
    fn filter(self, fit: Fit) -> Option<String> {
        let (w, h) = self.ratio()?;
        // The largest box at the target ratio that still fits *inside* the
        // source - what cropping keeps.
        let inner = format!("w=min(iw\\,ih*{w}/{h}):h=min(ih\\,iw*{h}/{w})");
        // The smallest box at the target ratio that *contains* the source -
        // what filling pads out to. Rounded to even numbers because most
        // encoders refuse odd dimensions.
        let canvas = format!("w=trunc(max(iw\\,ih*{w}/{h})/2)*2:h=trunc(max(ih\\,iw*{h}/{w})/2)*2");

        Some(match fit {
            Fit::Crop => format!("crop={inner},scale=trunc(iw/2)*2:trunc(ih/2)*2"),
            // Nothing is scaled: the frame is left alone and the canvas grows
            // around it, so filling never costs any detail.
            Fit::Pad => format!("pad={canvas}:x=(ow-iw)/2:y=(oh-ih)/2:color=black"),
            // The same geometry, but the bars are a blown-up blur of the
            // frame instead of black. The backdrop is stretched to the
            // canvas *before* it is blurred: each filter reads its
            // expressions against whatever the filter before it produced,
            // so the canvas has to be measured while the original
            // dimensions are still in hand.
            //
            // `boxblur` rather than `gblur` because it costs the same at
            // any radius, and at this softness nobody can tell them apart.
            Fit::Blur => {
                let blurred = format!(
                    "[fmbg]scale={canvas},boxblur=luma_radius=min(w\\,h)/16:luma_power=2[fmblur]"
                );
                format!("split=2[fmbg][fmfg];{blurred};[fmblur][fmfg]overlay=x=(W-w)/2:y=(H-h)/2")
            }
        })
    }
}

/// How the frame is made to fit a new shape, once [`Orientation`] has decided
/// what that shape is.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Fit {
    /// Keep the middle, lose the edges.
    #[default]
    Crop,
    /// Keep everything, fill the gap with black.
    Pad,
    /// Keep everything, fill the gap with a blurred copy of the frame.
    Blur,
}

/// Brightness, contrast, saturation and hue, in the units ffmpeg's `eq` and
/// `hue` filters already use. Kept as its own struct so `EditSpec::is_noop`
/// can ask one question instead of four.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ColorAdjust {
    /// -1 to 1, 0 leaves it alone.
    pub brightness: f64,
    /// 0 to 2, 1 leaves it alone.
    pub contrast: f64,
    /// 0 to 3, 1 leaves it alone.
    pub saturation: f64,
    /// Degrees around the wheel, 0 leaves it alone.
    pub hue: f64,
}

impl Default for ColorAdjust {
    fn default() -> Self {
        // Not `derive(Default)`: two of these four are neutral at one, not at
        // zero, and a zeroed contrast is a grey rectangle.
        Self {
            brightness: 0.0,
            contrast: 1.0,
            saturation: 1.0,
            hue: 0.0,
        }
    }
}

impl ColorAdjust {
    pub fn is_noop(&self) -> bool {
        *self == Self::default()
    }

    /// `None` when nothing has been moved, so an untouched file never picks up
    /// a filter — and with it a decode/encode round trip it did not need.
    fn filter(&self) -> Option<String> {
        if self.is_noop() {
            return None;
        }
        let mut chain = format!(
            "eq=brightness={:.3}:contrast={:.3}:saturation={:.3}",
            self.brightness.clamp(-1.0, 1.0),
            self.contrast.clamp(0.0, 2.0),
            self.saturation.clamp(0.0, 3.0),
        );
        if self.hue != 0.0 {
            // `eq` cannot rotate hue, so this is a second filter rather than
            // another parameter.
            chain.push_str(&format!(",hue=h={:.1}", self.hue.clamp(-180.0, 180.0)));
        }
        Some(chain)
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
    /// Only means anything when `orientation` is not `Keep`.
    pub fit: Fit,
    pub color: ColorAdjust,
    /// Cut points inside the trimmed range, in seconds.
    pub split_points: Vec<f64>,
}

impl EditSpec {
    pub fn is_noop(&self) -> bool {
        self.trim_start.is_none()
            && self.trim_end.is_none()
            && !self.mute
            && self.orientation == Orientation::Keep
            && self.color.is_noop()
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
/// How far apart two cuts must be to be worth writing a file between them.
/// Shared with the UI so the piece count it shows is the count it gets.
pub const MIN_SEGMENT_SECS: f64 = 0.05;

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
        .filter(|point| {
            *point > start + MIN_SEGMENT_SECS
                && end.map(|e| *point < e - MIN_SEGMENT_SECS).unwrap_or(true)
        })
        .collect();
    cuts.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    // Exact `dedup` would let two cuts a millisecond apart through and write a
    // segment of essentially no length. Collapse anything closer than the same
    // margin used to keep cuts off the trim edges.
    cuts.dedup_by(|later, earlier| *later - *earlier < MIN_SEGMENT_SECS);

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
pub fn apply_video_filters(args: Vec<String>, edit: &EditSpec) -> Vec<String> {
    // Geometry first, colour second: a colour filter does not care about the
    // frame's shape, and re-framing a picture that has already been graded
    // would be doing the grading on pixels that get thrown away.
    let mut chain: Vec<String> = Vec::new();
    if let Some(filter) = edit.orientation.filter(edit.fit) {
        chain.push(filter);
    }
    if let Some(filter) = edit.color.filter() {
        chain.push(filter);
    }
    if chain.is_empty() {
        return args;
    }
    prepend_filter(args, &chain.join(","))
}

fn prepend_filter(mut args: Vec<String>, filter: &str) -> Vec<String> {
    match args.iter().position(|arg| arg == "-vf") {
        // Re-framing runs first, so a preset chain that follows it (a GIF
        // palette, say) sees the final geometry.
        Some(index) if index + 1 < args.len() => {
            args[index + 1] = format!("{filter},{}", args[index + 1]);
        }
        _ => {
            args.push("-vf".into());
            args.push(filter.to_string());
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

    /// What a user gets when they press "split here" three times without
    /// moving the playhead: nothing at all. Worth pinning down, because it
    /// looked from the outside like splitting was broken.
    #[test]
    fn an_untouched_colour_block_adds_no_filter() {
        let args = apply_video_filters(vec!["-c:v".into(), "png".into()], &EditSpec::default());
        assert!(
            !args.iter().any(|arg| arg == "-vf"),
            "a file nobody graded should not pay for a filter: {args:?}"
        );
    }

    #[test]
    fn colour_and_shape_share_one_chain() {
        let edit = EditSpec {
            orientation: Orientation::Square,
            color: ColorAdjust {
                saturation: 1.4,
                ..ColorAdjust::default()
            },
            ..EditSpec::default()
        };
        let args = apply_video_filters(vec!["-c:v".into(), "libx264".into()], &edit);
        let chain = args.last().expect("a filter chain");
        // Geometry first: grading pixels that a crop then throws away is work
        // for nothing.
        assert!(chain.starts_with("crop="), "{chain}");
        assert!(chain.contains("eq=brightness=0.000"), "{chain}");
        assert!(chain.contains("saturation=1.400"), "{chain}");
    }

    #[test]
    fn hue_is_left_out_when_it_is_not_turned() {
        let edit = EditSpec {
            color: ColorAdjust {
                contrast: 1.2,
                ..ColorAdjust::default()
            },
            ..EditSpec::default()
        };
        let args = apply_video_filters(Vec::new(), &edit);
        assert!(!args.last().unwrap().contains("hue="), "{args:?}");
    }

    #[test]
    fn a_graded_file_is_not_a_noop() {
        let edit = EditSpec {
            color: ColorAdjust {
                brightness: 0.2,
                ..ColorAdjust::default()
            },
            ..EditSpec::default()
        };
        assert!(!edit.is_noop());
        assert!(EditSpec::default().is_noop());
    }

    #[test]
    fn cuts_a_hair_apart_do_not_make_an_empty_file() {
        // Two clicks a millisecond apart used to survive `dedup` and produce a
        // segment of no length between them.
        let twitchy = EditSpec {
            split_points: vec![30.0, 30.001, 30.002],
            ..EditSpec::default()
        };
        assert_eq!(segments(&twitchy, Some(60.0)).len(), 2);

        // Genuinely separate cuts still each get their own piece.
        let deliberate = EditSpec {
            split_points: vec![15.0, 30.0, 45.0],
            ..EditSpec::default()
        };
        assert_eq!(segments(&deliberate, Some(60.0)).len(), 4);
    }

    #[test]
    fn repeated_cuts_in_one_spot_are_one_cut() {
        let stuck_at_the_start = EditSpec {
            split_points: vec![0.0, 0.0, 0.0],
            ..EditSpec::default()
        };
        assert_eq!(segments(&stuck_at_the_start, Some(60.0)).len(), 1);

        let stuck_in_the_middle = EditSpec {
            split_points: vec![30.0, 30.0, 30.0],
            ..EditSpec::default()
        };
        assert_eq!(segments(&stuck_in_the_middle, Some(60.0)).len(), 2);
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
        let args = apply_video_filters(
            vec!["-vf".into(), "fps=15".into()],
            &EditSpec {
                orientation: Orientation::Portrait,
                ..EditSpec::default()
            },
        );
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
        let args = apply_video_filters(
            vec!["-c:v".into(), "libx264".into()],
            &EditSpec {
                orientation: Orientation::Square,
                ..EditSpec::default()
            },
        );
        assert_eq!(args[2], "-vf");
        assert!(args[3].contains("crop="));
    }

    #[test]
    fn keeping_the_shape_changes_nothing() {
        let args = vec!["-c:v".to_string(), "libx264".to_string()];
        assert_eq!(
            apply_video_filters(args.clone(), &EditSpec::default()),
            args
        );
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

    /// Like `run`, but hands back what the command said on stderr — where
    /// ffmpeg prints filter metadata.
    fn run_capture(program: &PathBuf, args: &[String]) -> String {
        let out = Command::new(program).args(args).output().expect("spawn");
        format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
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
        args.extend(apply_video_filters(
            vec!["-c:v".into(), "libx264".into(), "-crf".into(), "28".into()],
            &edit,
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

    #[test]
    #[ignore]
    fn splitting_into_three_writes_three_files() {
        let (Some(ffmpeg), Some(ffprobe)) = (sidecar("ffmpeg-"), sidecar("ffprobe-")) else {
            eprintln!("no sidecars — run scripts/fetch-ffmpeg.sh");
            return;
        };

        let work = std::env::temp_dir().join("coldmill-split-test");
        std::fs::create_dir_all(&work).unwrap();
        let source = work.join("source.mp4").to_string_lossy().into_owned();

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
                "testsrc=duration=9:size=640x360:rate=25",
                "-pix_fmt",
                "yuv420p",
                &source,
            ]
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>(),
        );

        let edit = EditSpec {
            split_points: vec![3.0, 6.0],
            ..EditSpec::default()
        };
        let pieces = segments(&edit, Some(9.0));
        assert_eq!(pieces.len(), 3, "two cuts should mean three pieces");

        for (index, piece) in pieces.iter().enumerate() {
            let out = work
                .join(format!("clip-{}.mp4", index + 1))
                .to_string_lossy()
                .into_owned();
            let mut args: Vec<String> = vec![
                "-hide_banner".into(),
                "-loglevel".into(),
                "error".into(),
                "-y".into(),
            ];
            args.extend(pre_input_args(piece));
            args.extend(["-i".to_string(), source.clone()]);
            args.extend(["-c:v".into(), "libx264".into(), "-crf".into(), "28".into()]);
            args.extend(output_args(&edit, piece));
            args.push(out.clone());
            run(&ffmpeg, &args);

            let duration: f64 = probe(&ffprobe, &out, "format=duration", &[])
                .parse()
                .expect("duration");
            assert!(
                (duration - 3.0).abs() < 0.3,
                "piece {} should be about three seconds, got {duration}",
                index + 1
            );
        }
    }

    #[test]
    #[ignore]
    fn every_fill_mode_lands_on_the_target_ratio() {
        let (Some(ffmpeg), Some(ffprobe)) = (sidecar("ffmpeg-"), sidecar("ffprobe-")) else {
            eprintln!("no sidecars - run scripts/fetch-ffmpeg.sh");
            return;
        };

        let work = std::env::temp_dir().join("coldmill-fill-test");
        std::fs::create_dir_all(&work).unwrap();
        let source = work.join("wide.mp4").to_string_lossy().into_owned();

        // Landscape, so going portrait leaves a real gap to fill.
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
                "testsrc=duration=2:size=640x360:rate=25",
                "-pix_fmt",
                "yuv420p",
                &source,
            ]
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>(),
        );

        for (fit, name, expect_height) in [
            // Cropping keeps the height and loses the sides.
            (Fit::Crop, "crop", 360.0),
            // Filling keeps the full width and grows the canvas instead:
            // 640 wide at 9:16 is 1138 tall (rounded to even).
            (Fit::Pad, "pad", 1138.0),
            (Fit::Blur, "blur", 1138.0),
        ] {
            let edit = EditSpec {
                orientation: Orientation::Portrait,
                fit,
                ..EditSpec::default()
            };
            let out = work
                .join(format!("{name}.mp4"))
                .to_string_lossy()
                .into_owned();
            let mut args: Vec<String> = vec![
                "-hide_banner".into(),
                "-loglevel".into(),
                "error".into(),
                "-y".into(),
                "-i".into(),
                source.clone(),
            ];
            args.extend(apply_video_filters(
                vec!["-c:v".into(), "libx264".into(), "-crf".into(), "30".into()],
                &edit,
            ));
            args.extend(output_args(&edit, &segments(&edit, Some(2.0))[0]));
            args.push(out.clone());
            run(&ffmpeg, &args);

            let size = probe(
                &ffprobe,
                &out,
                "stream=width,height",
                &["-select_streams", "v:0"],
            );
            let (width, height) = size.split_once(',').expect("dimensions");
            let width: f64 = width.parse().unwrap();
            let height: f64 = height.parse().unwrap();
            assert!(
                (width / height - 9.0 / 16.0).abs() < 0.02,
                "{name} should be about 9:16, got {size}"
            );
            assert!(
                (height - expect_height).abs() < 4.0,
                "{name} should be about {expect_height} tall, got {size}"
            );
        }
    }

    #[test]
    #[ignore]
    fn ffmpeg_actually_brightens_the_picture() {
        let (Some(ffmpeg), Some(ffprobe)) = (sidecar("ffmpeg-"), sidecar("ffprobe-")) else {
            eprintln!("no sidecars - run scripts/fetch-ffmpeg.sh");
            return;
        };

        let work = std::env::temp_dir().join("coldmill-colour-test");
        std::fs::create_dir_all(&work).unwrap();
        let source = work.join("grey.png").to_string_lossy().into_owned();

        // A flat mid-grey, so the average is the only thing that can move.
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
                "color=c=gray:size=64x64:duration=1:rate=1",
                "-frames:v",
                "1",
                &source,
            ]
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>(),
        );

        let mut averages = Vec::new();
        for brightness in [-0.3, 0.0, 0.3] {
            let edit = EditSpec {
                color: ColorAdjust {
                    brightness,
                    ..ColorAdjust::default()
                },
                ..EditSpec::default()
            };
            let out = work
                .join(format!("at{brightness}.png"))
                .to_string_lossy()
                .into_owned();
            let mut args: Vec<String> = vec![
                "-hide_banner".into(),
                "-loglevel".into(),
                "error".into(),
                "-y".into(),
                "-i".into(),
                source.clone(),
            ];
            args.extend(apply_video_filters(
                vec!["-c:v".into(), "png".into(), "-frames:v".into(), "1".into()],
                &edit,
            ));
            args.push(out.clone());
            run(&ffmpeg, &args);

            // ffprobe reads back the average luma the encoder actually wrote.
            let stats = run_capture(
                &ffmpeg,
                // No -loglevel here: `metadata=print` writes at info level,
                // and quietening ffmpeg quietens the very thing being read.
                &[
                    "-hide_banner",
                    "-i",
                    &out,
                    "-vf",
                    "signalstats,metadata=print:key=lavfi.signalstats.YAVG",
                    "-f",
                    "null",
                    "-",
                ]
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>(),
            );
            let average: f64 = stats
                .split("YAVG=")
                .nth(1)
                .and_then(|rest| rest.split_whitespace().next())
                .and_then(|value| value.parse().ok())
                .unwrap_or_else(|| panic!("no YAVG in {stats}"));
            averages.push(average);
        }

        assert!(
            averages[0] < averages[1] && averages[1] < averages[2],
            "brightness should move the picture in the direction asked for: {averages:?}"
        );
        let _ = &ffprobe;
    }

    #[test]
    #[ignore]
    fn an_icon_comes_out_square_and_within_the_format_limit() {
        let Some(ffmpeg) = sidecar("ffmpeg-") else {
            eprintln!("no sidecars - run scripts/fetch-ffmpeg.sh");
            return;
        };

        let work = std::env::temp_dir().join("coldmill-icon-test");
        std::fs::create_dir_all(&work).unwrap();

        // A wide source and a small one: the first has to be shrunk and padded
        // to a square, the second left alone rather than blown up.
        for (size, expect) in [("1200x800", 256u32), ("64x64", 64)] {
            let source = work
                .join(format!("src-{size}.png"))
                .to_string_lossy()
                .into_owned();
            let out = work
                .join(format!("out-{size}.ico"))
                .to_string_lossy()
                .into_owned();
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
                    &format!("testsrc=duration=1:size={size}:rate=1"),
                    "-frames:v",
                    "1",
                    &source,
                ]
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>(),
            );

            let mut args: Vec<String> = vec![
                "-hide_banner".into(),
                "-loglevel".into(),
                "error".into(),
                "-y".into(),
                "-i".into(),
                source,
            ];
            args.extend(
                crate::presets::encode_args(
                    crate::model::MediaKind::Image,
                    "ico",
                    crate::model::Quality::Balanced,
                )
                .unwrap(),
            );
            args.push(out.clone());
            run(&ffmpeg, &args);

            // The ICO directory says what is inside, and a zero means 256.
            let bytes = std::fs::read(&out).unwrap();
            assert_eq!(&bytes[0..4], &[0, 0, 1, 0], "not an icon: {out}");
            let width = if bytes[6] == 0 { 256 } else { bytes[6] as u32 };
            let height = if bytes[7] == 0 { 256 } else { bytes[7] as u32 };
            assert_eq!((width, height), (expect, expect), "{size} -> {out}");
        }
    }
}
