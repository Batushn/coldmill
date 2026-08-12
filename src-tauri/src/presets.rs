//! The single place where "Small / Balanced / High" becomes ffmpeg arguments.
//!
//! Nothing in here is exposed to the UI — no codec names, no CRF values, no
//! bitrates. If you want to retune the app, this is the only file to touch.
//!
//! Encoder choice assumes a **GPL** ffmpeg build (see `scripts/fetch-ffmpeg.sh`),
//! which is what ships x264/x265. If you switch the build to LGPL, change
//! `H264_ENCODER` to `libopenh264` and drop the `-crf`/`-preset` pair for it.

use crate::model::{MediaKind, Quality};

const H264_ENCODER: &str = "libx264";
const H264_PRESET: &str = "medium";

/// Formats offered per media type. The UI reads this list through the
/// `supported_targets` command so it can never offer something we cannot build.
pub const IMAGE_TARGETS: &[&str] = &["jpg", "png", "webp", "avif", "tiff", "bmp", "gif"];
pub const AUDIO_TARGETS: &[&str] = &["mp3", "m4a", "aac", "opus", "ogg", "flac", "wav"];
pub const VIDEO_TARGETS: &[&str] = &["mp4", "mkv", "webm", "mov", "avi", "gif"];

pub fn targets(kind: MediaKind) -> &'static [&'static str] {
    match kind {
        MediaKind::Image => IMAGE_TARGETS,
        MediaKind::Audio => AUDIO_TARGETS,
        MediaKind::Video => VIDEO_TARGETS,
        // Documents and models are not ffmpeg's business; see job.rs.
        _ => &[],
    }
}

/// Encoding arguments only. The runner supplies `-i <input>`, the progress
/// flags and the output path around these.
pub fn encode_args(kind: MediaKind, target: &str, quality: Quality) -> Result<Vec<String>, String> {
    let target = target.trim().trim_start_matches('.').to_ascii_lowercase();
    let args = match kind {
        MediaKind::Video => video_args(&target, quality),
        MediaKind::Audio => audio_args(&target, quality),
        MediaKind::Image => image_args(&target, quality),
        _ => None,
    };
    args.ok_or_else(|| format!("No preset for {target} output"))
}

fn s(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|p| p.to_string()).collect()
}

// ---------------------------------------------------------------------------
// Video
// ---------------------------------------------------------------------------

fn video_args(target: &str, q: Quality) -> Option<Vec<String>> {
    // Constant Rate Factor: lower is better looking and bigger.
    let crf = match q {
        Quality::Small => "28",
        Quality::Balanced => "23",
        Quality::High => "18",
    };
    let audio_kbps = match q {
        Quality::Small => "128k",
        Quality::Balanced => "192k",
        Quality::High => "256k",
    };

    let mut args = match target {
        "mp4" | "mov" | "mkv" => {
            let mut v = s(&[
                "-c:v",
                H264_ENCODER,
                "-preset",
                H264_PRESET,
                "-crf",
                crf,
                // Broadest player compatibility; some sources are 10-bit or 4:4:4.
                "-pix_fmt",
                "yuv420p",
                "-c:a",
                "aac",
                "-b:a",
                audio_kbps,
            ]);
            if target != "mkv" {
                // Move the index to the front so the file plays while copying.
                v.extend(s(&["-movflags", "+faststart"]));
            }
            v
        }
        "webm" => {
            // VP9 uses its own CRF scale, and needs -b:v 0 for constant quality.
            let vp9_crf = match q {
                Quality::Small => "36",
                Quality::Balanced => "32",
                Quality::High => "26",
            };
            let opus_kbps = match q {
                Quality::Small => "96k",
                Quality::Balanced => "128k",
                Quality::High => "192k",
            };
            s(&[
                "-c:v",
                "libvpx-vp9",
                "-crf",
                vp9_crf,
                "-b:v",
                "0",
                "-row-mt",
                "1",
                "-c:a",
                "libopus",
                "-b:a",
                opus_kbps,
            ])
        }
        "avi" => {
            // AVI predates modern codecs; mpeg4 + mp3 is what actually plays.
            let qscale = match q {
                Quality::Small => "6",
                Quality::Balanced => "4",
                Quality::High => "2",
            };
            s(&[
                "-c:v",
                "mpeg4",
                "-vtag",
                "xvid",
                "-qscale:v",
                qscale,
                "-c:a",
                "libmp3lame",
                "-b:a",
                audio_kbps,
            ])
        }
        "gif" => return Some(gif_args(q)),
        _ => return None,
    };

    args.extend(s(&["-map_metadata", "0"]));
    Some(args)
}

/// GIF needs a generated palette or it looks like 1998. One pass, using the
/// split filter so palettegen and paletteuse share a decode.
fn gif_args(q: Quality) -> Vec<String> {
    let (fps, width, colors) = match q {
        Quality::Small => (10, 480, 128),
        Quality::Balanced => (15, 640, 200),
        Quality::High => (20, 800, 256),
    };
    let filter = format!(
        "fps={fps},scale={width}:-1:flags=lanczos,split[a][b];\
         [a]palettegen=max_colors={colors}[p];[b][p]paletteuse=dither=sierra2_4a"
    );
    let mut v = s(&["-an", "-vf"]);
    v.push(filter);
    v.extend(s(&["-loop", "0"]));
    v
}

// ---------------------------------------------------------------------------
// Audio
// ---------------------------------------------------------------------------

fn audio_args(target: &str, q: Quality) -> Option<Vec<String>> {
    // `-vn` drops cover art streams, which several encoders refuse to mux.
    let mut args = s(&["-vn"]);

    args.extend(match target {
        "mp3" => {
            let kbps = match q {
                Quality::Small => "128k",
                Quality::Balanced => "192k",
                Quality::High => "320k",
            };
            s(&["-c:a", "libmp3lame", "-b:a", kbps])
        }
        "m4a" | "aac" => {
            let kbps = match q {
                Quality::Small => "128k",
                Quality::Balanced => "192k",
                Quality::High => "256k",
            };
            s(&["-c:a", "aac", "-b:a", kbps])
        }
        "opus" => {
            // Opus is efficient enough that these land near mp3 one tier up.
            let kbps = match q {
                Quality::Small => "64k",
                Quality::Balanced => "96k",
                Quality::High => "160k",
            };
            s(&["-c:a", "libopus", "-b:a", kbps])
        }
        "ogg" => {
            let vq = match q {
                Quality::Small => "3",
                Quality::Balanced => "5",
                Quality::High => "8",
            };
            s(&["-c:a", "libvorbis", "-q:a", vq])
        }
        // flac and wav are lossless: quality only changes how hard we squeeze
        // (flac) or the sample width (wav).
        "flac" => {
            let level = match q {
                Quality::Small => "12",
                Quality::Balanced => "8",
                Quality::High => "5",
            };
            s(&["-c:a", "flac", "-compression_level", level])
        }
        "wav" => {
            let codec = match q {
                Quality::Small | Quality::Balanced => "pcm_s16le",
                Quality::High => "pcm_s24le",
            };
            s(&["-c:a", codec])
        }
        _ => return None,
    });

    args.extend(s(&["-map_metadata", "0"]));
    Some(args)
}

// ---------------------------------------------------------------------------
// Image
// ---------------------------------------------------------------------------

fn image_args(target: &str, q: Quality) -> Option<Vec<String>> {
    // Animated sources (GIF, APNG) would otherwise emit a numbered sequence.
    let mut args = s(&["-frames:v", "1"]);

    args.extend(match target {
        "jpg" | "jpeg" => {
            // mjpeg -q:v runs 2 (best) to 31 (worst).
            let qv = match q {
                Quality::Small => "9",
                Quality::Balanced => "5",
                Quality::High => "2",
            };
            s(&["-c:v", "mjpeg", "-q:v", qv, "-pix_fmt", "yuvj420p"])
        }
        "png" => {
            // Lossless; the level only trades encode time for file size.
            let level = match q {
                Quality::Small => "9",
                Quality::Balanced => "6",
                Quality::High => "3",
            };
            s(&["-c:v", "png", "-compression_level", level])
        }
        "webp" => {
            let quality = match q {
                Quality::Small => "60",
                Quality::Balanced => "85",
                Quality::High => "95",
            };
            s(&["-c:v", "libwebp", "-quality", quality, "-preset", "picture"])
        }
        "avif" => {
            let crf = match q {
                Quality::Small => "40",
                Quality::Balanced => "30",
                Quality::High => "22",
            };
            s(&[
                "-c:v",
                "libaom-av1",
                "-still-picture",
                "1",
                "-crf",
                crf,
                // AV1 stills are slow; 6 keeps a batch of photos bearable.
                "-cpu-used",
                "6",
                "-pix_fmt",
                "yuv420p",
            ])
        }
        "tiff" => {
            let algo = match q {
                Quality::Small => "deflate",
                Quality::Balanced => "lzw",
                Quality::High => "raw",
            };
            s(&["-c:v", "tiff", "-compression_algo", algo])
        }
        "bmp" => s(&["-c:v", "bmp"]),
        "gif" => s(&["-c:v", "gif"]),
        _ => return None,
    });

    Some(args)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_advertised_target_has_a_preset() {
        for (kind, list) in [
            (MediaKind::Image, IMAGE_TARGETS),
            (MediaKind::Audio, AUDIO_TARGETS),
            (MediaKind::Video, VIDEO_TARGETS),
        ] {
            for target in list {
                for q in [Quality::Small, Quality::Balanced, Quality::High] {
                    assert!(
                        encode_args(kind, target, q).is_ok(),
                        "missing preset: {kind:?} -> {target} @ {q:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn unknown_target_is_rejected() {
        assert!(encode_args(MediaKind::Audio, "mp4", Quality::Balanced).is_err());
        assert!(encode_args(MediaKind::Unsupported, "png", Quality::Balanced).is_err());
    }

    #[test]
    fn leading_dot_is_tolerated() {
        assert!(encode_args(MediaKind::Image, ".PNG", Quality::High).is_ok());
    }

    /// Actually runs every preset through the bundled ffmpeg. Ignored by
    /// default because it needs the sidecars on disk and takes a minute:
    ///
    ///   cargo test -- --ignored --nocapture
    #[test]
    #[ignore]
    fn every_preset_survives_real_ffmpeg() {
        use std::path::PathBuf;
        use std::process::Command;

        let ffmpeg = locate_ffmpeg().expect("run scripts/fetch-ffmpeg.sh first");
        let work = std::env::temp_dir().join("coldmill-preset-smoke");
        let _ = std::fs::remove_dir_all(&work);
        std::fs::create_dir_all(&work).unwrap();

        let run = |args: Vec<String>| -> Result<(), String> {
            let out = Command::new(&ffmpeg)
                .args(["-hide_banner", "-loglevel", "error", "-y"])
                .args(&args)
                .output()
                .map_err(|e| e.to_string())?;
            if out.status.success() {
                Ok(())
            } else {
                Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
            }
        };

        // Sources: a short clip, a tone, and a still.
        let video_src = work.join("src.mp4");
        let audio_src = work.join("src.wav");
        let image_src = work.join("src.png");
        run(vec![
            "-f".into(),
            "lavfi".into(),
            "-i".into(),
            "testsrc=duration=1:size=320x240:rate=15".into(),
            "-f".into(),
            "lavfi".into(),
            "-i".into(),
            "sine=frequency=440:duration=1".into(),
            "-shortest".into(),
            "-pix_fmt".into(),
            "yuv420p".into(),
            video_src.to_string_lossy().into_owned(),
        ])
        .expect("could not build the test clip");
        run(vec![
            "-f".into(),
            "lavfi".into(),
            "-i".into(),
            "sine=frequency=440:duration=1".into(),
            audio_src.to_string_lossy().into_owned(),
        ])
        .expect("could not build the test tone");
        run(vec![
            "-f".into(),
            "lavfi".into(),
            "-i".into(),
            "testsrc=duration=1:size=320x240:rate=1".into(),
            "-frames:v".into(),
            "1".into(),
            image_src.to_string_lossy().into_owned(),
        ])
        .expect("could not build the test still");

        let mut failures: Vec<String> = Vec::new();
        for (kind, source, list) in [
            (MediaKind::Video, &video_src, VIDEO_TARGETS),
            (MediaKind::Audio, &audio_src, AUDIO_TARGETS),
            (MediaKind::Image, &image_src, IMAGE_TARGETS),
        ] {
            for target in list {
                for quality in [Quality::Small, Quality::Balanced, Quality::High] {
                    let out: PathBuf = work.join(format!("{kind:?}-{target}-{quality:?}.{target}"));
                    let mut args = vec!["-i".to_string(), source.to_string_lossy().into_owned()];
                    args.extend(encode_args(kind, target, quality).unwrap());
                    args.push(out.to_string_lossy().into_owned());

                    match run(args) {
                        Ok(()) if std::fs::metadata(&out).map(|m| m.len()).unwrap_or(0) > 0 => {
                            println!("ok   {kind:?} -> {target} @ {quality:?}")
                        }
                        Ok(()) => failures
                            .push(format!("{kind:?} -> {target} @ {quality:?}: empty output")),
                        Err(err) => {
                            failures.push(format!("{kind:?} -> {target} @ {quality:?}: {err}"))
                        }
                    }
                }
            }
        }

        assert!(failures.is_empty(), "\n{}", failures.join("\n"));
    }

    fn locate_ffmpeg() -> Option<std::path::PathBuf> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("binaries");
        std::fs::read_dir(dir).ok()?.flatten().find_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            name.starts_with("ffmpeg-").then(|| entry.path())
        })
    }
}
