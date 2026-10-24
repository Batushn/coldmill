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
///
/// JPEG XL is left out on macOS, whose ffmpeg is built without libjxl.
#[cfg(not(target_os = "macos"))]
pub const IMAGE_TARGETS: &[&str] = &[
    "jpg", "png", "webp", "avif", "jxl", "tiff", "bmp", "gif", "ico", "tga",
];
#[cfg(target_os = "macos")]
pub const IMAGE_TARGETS: &[&str] = &[
    "jpg", "png", "webp", "avif", "tiff", "bmp", "gif", "ico", "tga",
];

/// Windows icons are square and at most 256 px a side, and the muxer simply
/// refuses anything else rather than resizing it for us.
///
/// The picture is fitted inside that square and the remainder padded with
/// transparency, so a wide photo keeps all of itself instead of losing its
/// sides to a crop. `max(iw\,ih)` in the pad reads the *scaled* frame, whose
/// longest side is already the square we want, and a source smaller than 256
/// is left at its own size rather than blown up.
const ICO_FILTER: &str = "scale=w=min(256\\,max(iw\\,ih)):h=min(256\\,max(iw\\,ih)):\
     force_original_aspect_ratio=decrease,\
     pad=w=max(iw\\,ih):h=max(iw\\,ih):x=(ow-iw)/2:y=(oh-ih)/2:color=#00000000,\
     format=rgba";
pub const AUDIO_TARGETS: &[&str] = &[
    "mp3", "m4a", "aac", "opus", "ogg", "flac", "wav", "aiff", "wma", "ac3",
];
pub const VIDEO_TARGETS: &[&str] = &[
    "mp4", "mkv", "webm", "mov", "avi", "gif", "webp", "ts", "mpg", "wmv", "ogv",
];

/// Laid over white when going to a format that has no transparency. Left to
/// itself the encoder simply drops the alpha plane, and what was transparent
/// comes out as whatever colour hid underneath it — almost always black.
///
/// `c·a + 255·(1 − a)` per channel, which is compositing over white without
/// a second input. `geq` is slow next to a purpose-built filter, about a
/// second for a 12-megapixel still, but it is in every ffmpeg including the
/// older macOS build, and it only runs on pictures that have transparency.
pub const OVER_WHITE: &str = "format=rgba,\
     geq=r='r(X,Y)*alpha(X,Y)/255+255-alpha(X,Y)':\
     g='g(X,Y)*alpha(X,Y)/255+255-alpha(X,Y)':\
     b='b(X,Y)*alpha(X,Y)/255+255-alpha(X,Y)':a=255";

/// Targets that have nowhere to keep transparency.
pub fn drops_alpha(target: &str) -> bool {
    matches!(target, "jpg" | "jpeg" | "bmp")
}

pub fn targets(kind: MediaKind) -> Vec<&'static str> {
    match kind {
        MediaKind::Image => IMAGE_TARGETS.to_vec(),
        MediaKind::Audio => AUDIO_TARGETS.to_vec(),
        // "Give me the mp3 of this video" is one of the most common things a
        // converter is asked; the soundtrack is just another target.
        MediaKind::Video => VIDEO_TARGETS.iter().chain(AUDIO_TARGETS).copied().collect(),
        // Documents and models are not ffmpeg's business; see job.rs.
        _ => Vec::new(),
    }
}

/// What kind of file a target produces from a source of `kind`. Only a video
/// can change kind, by losing its picture.
pub fn output_kind(kind: MediaKind, target: &str) -> MediaKind {
    let target = target.trim().trim_start_matches('.').to_ascii_lowercase();
    if kind == MediaKind::Video && AUDIO_TARGETS.contains(&target.as_str()) {
        MediaKind::Audio
    } else {
        kind
    }
}

/// Encoding arguments only. The runner supplies `-i <input>`, the progress
/// flags and the output path around these.
pub fn encode_args(kind: MediaKind, target: &str, quality: Quality) -> Result<Vec<String>, String> {
    let target = target.trim().trim_start_matches('.').to_ascii_lowercase();
    let args = match output_kind(kind, &target) {
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
        "mp4" | "mov" | "mkv" | "ts" => {
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
            if matches!(target, "mp4" | "mov") {
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
        "mpg" => {
            // MPEG-2 with MP2 audio: what DVD authoring tools and old set-top
            // players expect. Like AVI it is a fixed-quantiser codec.
            let qscale = match q {
                Quality::Small => "8",
                Quality::Balanced => "4",
                Quality::High => "2",
            };
            s(&[
                "-c:v",
                "mpeg2video",
                "-qscale:v",
                qscale,
                "-pix_fmt",
                "yuv420p",
                "-c:a",
                "mp2",
                "-b:a",
                audio_kbps,
            ])
        }
        "wmv" => {
            // Windows Media 8: the newest WMV ffmpeg can write, and the one
            // every Windows since XP plays without a codec pack.
            let qscale = match q {
                Quality::Small => "8",
                Quality::Balanced => "4",
                Quality::High => "2",
            };
            s(&[
                "-c:v",
                "wmv2",
                "-qscale:v",
                qscale,
                "-c:a",
                "wmav2",
                "-b:a",
                audio_kbps,
            ])
        }
        "ogv" => {
            // Theora's quality scale runs 0–10, higher is better.
            let (theora, vorbis) = match q {
                Quality::Small => ("5", "3"),
                Quality::Balanced => ("7", "5"),
                Quality::High => ("9", "7"),
            };
            s(&[
                "-c:v",
                "libtheora",
                "-q:v",
                theora,
                "-pix_fmt",
                "yuv420p",
                "-c:a",
                "libvorbis",
                "-q:a",
                vorbis,
            ])
        }
        "gif" => return Some(gif_args(q)),
        "webp" => return Some(animated_webp_args(q)),
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

/// Animated WebP: the same frame budget as a GIF, at a fraction of the size
/// and without the 256-colour palette.
fn animated_webp_args(q: Quality) -> Vec<String> {
    let (fps, width, quality) = match q {
        Quality::Small => (10, 480, "50"),
        Quality::Balanced => (15, 640, "70"),
        Quality::High => (20, 800, "85"),
    };
    let mut v = s(&["-an", "-vf"]);
    v.push(format!(
        "fps={fps},scale='min({width},iw)':-2:flags=lanczos"
    ));
    v.extend(s(&[
        "-c:v",
        "libwebp_anim",
        "-quality",
        quality,
        "-loop",
        "0",
    ]));
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
        // WAV's big-endian sibling, which is what Logic and older Mac
        // software reach for.
        "aiff" => {
            let codec = match q {
                Quality::Small | Quality::Balanced => "pcm_s16be",
                Quality::High => "pcm_s24be",
            };
            s(&["-c:a", codec])
        }
        "wma" => {
            let kbps = match q {
                Quality::Small => "128k",
                Quality::Balanced => "192k",
                Quality::High => "256k",
            };
            s(&["-c:a", "wmav2", "-b:a", kbps])
        }
        // Dolby Digital, for DVD and home cinema. 5.1 sources stay 5.1.
        "ac3" => {
            let kbps = match q {
                Quality::Small => "192k",
                Quality::Balanced => "384k",
                Quality::High => "640k",
            };
            s(&["-c:a", "ac3", "-b:a", kbps])
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
        "jxl" => {
            // Butteraugli distance: 0 is lossless, 1 is visually lossless.
            let distance = match q {
                Quality::Small => "3",
                Quality::Balanced => "1.5",
                Quality::High => "0.5",
            };
            s(&["-c:v", "libjxl", "-distance", distance, "-effort", "7"])
        }
        "bmp" => s(&["-c:v", "bmp"]),
        "tga" => s(&["-c:v", "targa"]),
        // PNG inside the icon rather than BMP: it is the modern encoding, it
        // is what carries the alpha the padding relies on, and every Windows
        // since Vista reads it.
        "ico" => s(&["-c:v", "png", "-vf", ICO_FILTER]),
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
    fn a_video_can_lose_its_picture() {
        assert_eq!(output_kind(MediaKind::Video, "mp3"), MediaKind::Audio);
        assert_eq!(output_kind(MediaKind::Video, "mp4"), MediaKind::Video);
        assert_eq!(output_kind(MediaKind::Audio, "mp3"), MediaKind::Audio);
        let args = encode_args(MediaKind::Video, "mp3", Quality::Balanced).unwrap();
        assert!(args.contains(&"-vn".to_string()), "{args:?}");
        assert!(targets(MediaKind::Video).contains(&"flac"));
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
            // A soundtrack taken out of a video.
            (MediaKind::Video, &video_src, AUDIO_TARGETS),
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

        // Transparency laid over white on the way to JPEG.
        let alpha_src = work.join("alpha.png");
        run(vec![
            "-f".into(),
            "lavfi".into(),
            "-i".into(),
            "color=c=red@0.5:size=64x64,format=rgba".into(),
            "-frames:v".into(),
            "1".into(),
            alpha_src.to_string_lossy().into_owned(),
        ])
        .expect("could not build the transparent still");
        let flattened = work.join("alpha.jpg");
        let mut args = vec!["-i".to_string(), alpha_src.to_string_lossy().into_owned()];
        args.extend(encode_args(MediaKind::Image, "jpg", Quality::High).unwrap());
        args.extend(["-vf".to_string(), OVER_WHITE.to_string()]);
        args.push(flattened.to_string_lossy().into_owned());
        if let Err(err) = run(args) {
            failures.push(format!("transparent -> jpg over white: {err}"));
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
