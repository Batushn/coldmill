//! Media type detection from magic bytes.
//!
//! Extensions lie: a `.jpg` that is really a PDF would blow up halfway through
//! a batch. We read the file header instead and only queue what `infer`
//! recognises as image/audio/video.

use std::fs::File;
use std::io::Read;
use std::path::Path;

use crate::model::MediaKind;

/// Enough for every signature `infer` knows about (the longest is ~262 bytes),
/// with room for the ISO-BMFF brand list.
const HEADER_BYTES: usize = 512;

pub struct Detection {
    pub kind: MediaKind,
    pub mime: Option<String>,
    pub reason: Option<String>,
}

pub fn detect(path: &Path) -> Detection {
    let mut header = [0u8; HEADER_BYTES];
    let read = match File::open(path).and_then(|mut f| f.read(&mut header)) {
        Ok(n) => n,
        Err(err) => {
            return Detection {
                kind: MediaKind::Unsupported,
                mime: None,
                reason: Some(format!("Could not read file: {err}")),
            }
        }
    };

    if read == 0 {
        return Detection {
            kind: MediaKind::Unsupported,
            mime: None,
            reason: Some("File is empty".into()),
        };
    }

    match infer::get(&header[..read]) {
        Some(t) => {
            let kind = match t.matcher_type() {
                infer::MatcherType::Image => MediaKind::Image,
                infer::MatcherType::Audio => MediaKind::Audio,
                infer::MatcherType::Video => MediaKind::Video,
                _ => MediaKind::Unsupported,
            };
            let reason = (!kind.is_media())
                .then(|| format!("{} is not an audio, video or image file", t.mime_type()));
            Detection {
                kind,
                mime: Some(t.mime_type().to_string()),
                reason,
            }
        }
        None => Detection {
            kind: MediaKind::Unsupported,
            mime: None,
            reason: Some("Unrecognised file type".into()),
        },
    }
}

/// `infer` classifies some audio-only containers as video (an `.m4a` and an
/// `.mp4` share the same ISO-BMFF header) and treats animated GIFs as images.
/// `probe.rs` calls this with what ffprobe actually found so the grouping
/// matches what will really be encoded.
pub fn refine_with_streams(initial: MediaKind, has_video: bool, has_audio: bool) -> MediaKind {
    match initial {
        // A "video" file with no video stream is really just audio.
        MediaKind::Video if !has_video && has_audio => MediaKind::Audio,
        // An "audio" file that turns out to carry pictures stays audio: cover
        // art is an attached picture, not something the user wants converted.
        other => other,
    }
}
