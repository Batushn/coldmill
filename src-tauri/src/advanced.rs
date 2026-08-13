//! Overrides for people who already know what they want.
//!
//! The three quality presets stay the whole story for everyone else: this
//! module never invents arguments, it only replaces ones `presets.rs` already
//! chose. That ordering matters — an override that had to describe a complete
//! encode would mean a second, competing set of presets, and the two would
//! drift apart the first time either was retuned.
//!
//! Every field is optional and every unset field means "leave the preset
//! alone", so an empty `Advanced` is exactly the same encode as no advanced
//! settings at all.

use serde::Deserialize;

use crate::model::MediaKind;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Advanced {
    /// Target video bitrate in kbit/s. Setting this switches the encode from
    /// constant quality to constant bitrate.
    pub video_kbps: Option<u32>,
    /// Constant Rate Factor: lower is better looking and bigger. Ignored when
    /// a bitrate is set, because the two cannot both be in charge.
    pub crf: Option<u8>,
    /// x264's speed/size trade-off, from `ultrafast` to `veryslow`.
    pub encoder_preset: Option<String>,
    pub fps: Option<f64>,
    /// Cap the tall side. The other side follows, so nothing is stretched.
    pub max_height: Option<u32>,
    pub audio_kbps: Option<u32>,
    pub sample_rate: Option<u32>,
    /// 1 for mono, 2 for stereo.
    pub channels: Option<u8>,
}

/// Names x264 understands. Anything else is dropped rather than passed on:
/// ffmpeg would refuse the whole job over a typo.
const ENCODER_PRESETS: &[&str] = &[
    "ultrafast",
    "superfast",
    "veryfast",
    "faster",
    "fast",
    "medium",
    "slow",
    "slower",
    "veryslow",
];

impl Advanced {
    pub fn is_empty(&self) -> bool {
        self.video_kbps.is_none()
            && self.crf.is_none()
            && self.encoder_preset.is_none()
            && self.fps.is_none()
            && self.max_height.is_none()
            && self.audio_kbps.is_none()
            && self.sample_rate.is_none()
            && self.channels.is_none()
    }

    /// Rewrites a preset's arguments in place.
    pub fn apply(&self, mut args: Vec<String>, kind: MediaKind) -> Vec<String> {
        if self.is_empty() {
            return args;
        }

        let has_video = matches!(kind, MediaKind::Video | MediaKind::Image);
        let x264 = args.iter().any(|arg| arg == "libx264");

        if has_video {
            if let Some(kbps) = self.video_kbps.filter(|kbps| *kbps > 0) {
                // A constant quality flag left next to a bitrate wins and the
                // bitrate is silently ignored, so the quality flag has to go.
                remove_flag(&mut args, "-crf");
                remove_flag(&mut args, "-qscale:v");
                set_flag(&mut args, "-b:v", &format!("{kbps}k"));
            } else if let Some(crf) = self.crf {
                // Only where there is a `-crf` to replace: the AVI path is
                // scaled by `-qscale:v`, whose numbers mean something else
                // entirely, and VP9's CRF range is not x264's.
                replace_flag(&mut args, "-crf", &crf.to_string());
            }

            if let Some(preset) = self.encoder_preset.as_deref() {
                if x264 && ENCODER_PRESETS.contains(&preset) {
                    replace_flag(&mut args, "-preset", preset);
                }
            }

            if let Some(fps) = self.fps.filter(|fps| *fps > 0.0) {
                set_flag(&mut args, "-r", &format!("{fps}"));
            }

            if let Some(height) = self.max_height.filter(|height| *height > 0) {
                // `min` rather than a plain height so a small source is never
                // blown up, and -2 keeps the width even for the encoder.
                prepend_filter(&mut args, &format!("scale=-2:min({height}\\,ih)"));
            }
        }

        // Audio settings are worth honouring on a video too: its soundtrack is
        // encoded by the same arguments.
        if !matches!(kind, MediaKind::Image) {
            if let Some(kbps) = self.audio_kbps.filter(|kbps| *kbps > 0) {
                // Only where the preset already sets one: FLAC and WAV are
                // lossless and a bitrate would be meaningless.
                replace_flag(&mut args, "-b:a", &format!("{kbps}k"));
            }
            if let Some(rate) = self.sample_rate.filter(|rate| *rate > 0) {
                set_flag(&mut args, "-ar", &rate.to_string());
            }
            if let Some(channels) = self.channels.filter(|channels| *channels > 0) {
                set_flag(&mut args, "-ac", &channels.to_string());
            }
        }

        args
    }
}

fn position(args: &[String], flag: &str) -> Option<usize> {
    args.iter()
        .position(|arg| arg == flag)
        .filter(|index| index + 1 < args.len())
}

/// Change a flag only if the preset already uses it.
fn replace_flag(args: &mut [String], flag: &str, value: &str) {
    if let Some(index) = position(args, flag) {
        args[index + 1] = value.to_string();
    }
}

/// Change a flag, adding it when the preset does not use it.
fn set_flag(args: &mut Vec<String>, flag: &str, value: &str) {
    match position(args, flag) {
        Some(index) => args[index + 1] = value.to_string(),
        None => {
            args.push(flag.to_string());
            args.push(value.to_string());
        }
    }
}

fn remove_flag(args: &mut Vec<String>, flag: &str) {
    if let Some(index) = position(args, flag) {
        args.drain(index..index + 2);
    }
}

/// Puts a filter at the front of the chain, the same way re-framing does, so a
/// preset's own filters still see the final geometry.
fn prepend_filter(args: &mut Vec<String>, filter: &str) {
    match position(args, "-vf") {
        Some(index) => args[index + 1] = format!("{filter},{}", args[index + 1]),
        None => {
            args.push("-vf".into());
            args.push(filter.to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn preset() -> Vec<String> {
        [
            "-c:v", "libx264", "-preset", "medium", "-crf", "23", "-c:a", "aac", "-b:a", "192k",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect()
    }

    fn value(args: &[String], flag: &str) -> Option<String> {
        position(args, flag).map(|index| args[index + 1].clone())
    }

    #[test]
    fn nothing_set_changes_nothing() {
        let untouched = Advanced::default().apply(preset(), MediaKind::Video);
        assert_eq!(untouched, preset());
    }

    #[test]
    fn a_bitrate_takes_the_place_of_constant_quality() {
        let advanced = Advanced {
            video_kbps: Some(4000),
            ..Advanced::default()
        };
        let args = advanced.apply(preset(), MediaKind::Video);
        assert_eq!(value(&args, "-b:v").as_deref(), Some("4000k"));
        assert!(
            !args.iter().any(|arg| arg == "-crf"),
            "crf would win over the bitrate and has to go: {args:?}"
        );
    }

    #[test]
    fn a_bitrate_wins_over_a_crf_asked_for_at_the_same_time() {
        let advanced = Advanced {
            video_kbps: Some(4000),
            crf: Some(15),
            ..Advanced::default()
        };
        let args = advanced.apply(preset(), MediaKind::Video);
        assert_eq!(value(&args, "-b:v").as_deref(), Some("4000k"));
        assert!(!args.iter().any(|arg| arg == "-crf"));
    }

    #[test]
    fn a_crf_replaces_the_presets_number() {
        let advanced = Advanced {
            crf: Some(30),
            ..Advanced::default()
        };
        let args = advanced.apply(preset(), MediaKind::Video);
        assert_eq!(value(&args, "-crf").as_deref(), Some("30"));
    }

    #[test]
    fn a_crf_is_not_invented_where_the_preset_has_none() {
        // AVI is scaled by -qscale:v, where 30 would be a wildly different
        // picture than it is on x264's scale.
        let avi: Vec<String> = ["-c:v", "mpeg4", "-qscale:v", "4"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let advanced = Advanced {
            crf: Some(30),
            ..Advanced::default()
        };
        assert_eq!(advanced.apply(avi.clone(), MediaKind::Video), avi);
    }

    #[test]
    fn an_unknown_encoder_preset_is_dropped_rather_than_passed_on() {
        let advanced = Advanced {
            encoder_preset: Some("blazing".into()),
            ..Advanced::default()
        };
        let args = advanced.apply(preset(), MediaKind::Video);
        assert_eq!(value(&args, "-preset").as_deref(), Some("medium"));
    }

    #[test]
    fn a_height_cap_never_enlarges_and_keeps_the_width_even() {
        let advanced = Advanced {
            max_height: Some(720),
            ..Advanced::default()
        };
        let args = advanced.apply(preset(), MediaKind::Video);
        assert_eq!(
            value(&args, "-vf").as_deref(),
            Some("scale=-2:min(720\\,ih)")
        );
    }

    #[test]
    fn a_height_cap_joins_a_filter_the_preset_already_has() {
        let mut with_filter = preset();
        with_filter.extend(["-vf".to_string(), "fps=15".to_string()]);
        let advanced = Advanced {
            max_height: Some(720),
            ..Advanced::default()
        };
        let args = advanced.apply(with_filter, MediaKind::Video);
        assert_eq!(
            value(&args, "-vf").as_deref(),
            Some("scale=-2:min(720\\,ih),fps=15")
        );
    }

    #[test]
    fn a_lossless_target_keeps_its_silence_about_bitrate() {
        let flac: Vec<String> = ["-c:a", "flac"].iter().map(|s| s.to_string()).collect();
        let advanced = Advanced {
            audio_kbps: Some(320),
            ..Advanced::default()
        };
        assert_eq!(advanced.apply(flac.clone(), MediaKind::Audio), flac);
    }

    #[test]
    fn an_image_is_not_given_audio_settings() {
        let png: Vec<String> = ["-c:v", "png"].iter().map(|s| s.to_string()).collect();
        let advanced = Advanced {
            channels: Some(1),
            sample_rate: Some(48000),
            ..Advanced::default()
        };
        assert_eq!(advanced.apply(png.clone(), MediaKind::Image), png);
    }
}

/// The overrides only matter if ffmpeg actually honours them, which no amount
/// of argument-shape testing can tell us.
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

    fn run(program: &PathBuf, args: &[String]) {
        let out = Command::new(program).args(args).output().expect("spawn");
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    fn probe(ffprobe: &PathBuf, file: &str, entries: &str) -> String {
        let out = Command::new(ffprobe)
            .args([
                "-v",
                "error",
                "-select_streams",
                "v:0",
                "-show_entries",
                entries,
                "-of",
                "csv=p=0",
                file,
            ])
            .output()
            .expect("spawn");
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    #[test]
    #[ignore]
    fn ffmpeg_honours_a_height_cap_and_a_frame_rate() {
        let (Some(ffmpeg), Some(ffprobe)) = (sidecar("ffmpeg-"), sidecar("ffprobe-")) else {
            eprintln!("no sidecars - run scripts/fetch-ffmpeg.sh");
            return;
        };

        let work = std::env::temp_dir().join("coldmill-advanced-test");
        std::fs::create_dir_all(&work).unwrap();
        let source = work.join("source.mp4").to_string_lossy().into_owned();
        let result = work.join("capped.mp4").to_string_lossy().into_owned();

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
                "testsrc=duration=3:size=1920x1080:rate=30",
                "-pix_fmt",
                "yuv420p",
                &source,
            ]
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>(),
        );

        let advanced = Advanced {
            max_height: Some(480),
            fps: Some(12.0),
            encoder_preset: Some("ultrafast".into()),
            ..Advanced::default()
        };
        let preset =
            crate::presets::encode_args(MediaKind::Video, "mp4", crate::model::Quality::Balanced)
                .unwrap();

        let mut args: Vec<String> = vec![
            "-hide_banner".into(),
            "-loglevel".into(),
            "error".into(),
            "-y".into(),
            "-i".into(),
            source.clone(),
        ];
        args.extend(advanced.apply(preset, MediaKind::Video));
        args.push(result.clone());
        run(&ffmpeg, &args);

        let size = probe(&ffprobe, &result, "stream=width,height");
        assert_eq!(size, "854,480", "the cap should have taken effect");

        let rate = probe(&ffprobe, &result, "stream=r_frame_rate");
        assert_eq!(rate, "12/1", "the frame rate should have taken effect");
    }

    #[test]
    #[ignore]
    fn a_bitrate_override_actually_changes_the_size() {
        let (Some(ffmpeg), Some(ffprobe)) = (sidecar("ffmpeg-"), sidecar("ffprobe-")) else {
            eprintln!("no sidecars - run scripts/fetch-ffmpeg.sh");
            return;
        };

        let work = std::env::temp_dir().join("coldmill-advanced-test");
        std::fs::create_dir_all(&work).unwrap();
        let source = work.join("noisy.mp4").to_string_lossy().into_owned();

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
                "testsrc=duration=4:size=640x360:rate=25",
                "-pix_fmt",
                "yuv420p",
                &source,
            ]
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>(),
        );

        let mut sizes = Vec::new();
        for kbps in [200u32, 3000] {
            let out = work
                .join(format!("at-{kbps}.mp4"))
                .to_string_lossy()
                .into_owned();
            let advanced = Advanced {
                video_kbps: Some(kbps),
                ..Advanced::default()
            };
            let preset = crate::presets::encode_args(
                MediaKind::Video,
                "mp4",
                crate::model::Quality::Balanced,
            )
            .unwrap();

            let mut args: Vec<String> = vec![
                "-hide_banner".into(),
                "-loglevel".into(),
                "error".into(),
                "-y".into(),
                "-i".into(),
                source.clone(),
            ];
            args.extend(advanced.apply(preset, MediaKind::Video));
            args.push(out.clone());
            run(&ffmpeg, &args);
            sizes.push(std::fs::metadata(&out).unwrap().len());

            let _ = probe(&ffprobe, &out, "stream=width");
        }

        // Not fifteen times bigger, despite fifteen times the bitrate: the
        // test pattern is so easy to encode that x264 undershoots both
        // targets. The point is that the flag reaches the encoder and moves
        // the size in the right direction, which three times over does show.
        assert!(
            sizes[1] > sizes[0] * 3,
            "more bitrate should mean a plainly bigger file, got {sizes:?}"
        );
    }
}
