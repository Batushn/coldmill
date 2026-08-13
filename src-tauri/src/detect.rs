//! What is this file, really?
//!
//! Extensions lie, so we read the header first. Three tiers, in order:
//!
//! 1. **Our own signatures** for formats `infer` does not know (glTF binary,
//!    FBX, .blend, binary STL).
//! 2. **`infer`** for everything with a documented magic number.
//! 3. **Extension**, but only for text-based formats that genuinely have no
//!    signature (obj, md, html…) and only when the bytes really do look like
//!    text. A renamed archive still gets rejected.

use std::fs::File;
use std::io::Read;
use std::path::Path;

use crate::model::MediaKind;

/// Enough for every signature `infer` knows about, plus the STL header.
const HEADER_BYTES: usize = 512;

/// Text formats with no magic number, keyed by lowercase extension.
const TEXT_DOCUMENTS: &[&str] = &[
    "txt", "md", "markdown", "html", "htm", "rst", "tex", "latex", "org", "csv", "ipynb", "adoc",
    "asciidoc", "textile", "opml", "man",
];
const TEXT_MODELS: &[&str] = &["obj", "gltf", "dae", "x3d"];

/// Vector pictures are XML, so they reach the text tier like the others.
const TEXT_IMAGES: &[&str] = &["svg"];

/// Mime types `infer` reports that belong to the document module.
const DOCUMENT_MIMES: &[&str] = &[
    "application/pdf",
    "application/msword",
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
    "application/vnd.ms-excel",
    "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
    "application/vnd.ms-powerpoint",
    "application/vnd.openxmlformats-officedocument.presentationml.presentation",
    "application/vnd.oasis.opendocument.text",
    "application/vnd.oasis.opendocument.spreadsheet",
    "application/vnd.oasis.opendocument.presentation",
    "application/epub+zip",
    "application/rtf",
];

pub struct Detection {
    pub kind: MediaKind,
    pub mime: Option<String>,
    pub reason: Option<String>,
}

impl Detection {
    fn known(kind: MediaKind, mime: impl Into<String>) -> Self {
        Self {
            kind,
            mime: Some(mime.into()),
            reason: None,
        }
    }

    fn rejected(reason: impl Into<String>) -> Self {
        Self {
            kind: MediaKind::Unsupported,
            mime: None,
            reason: Some(reason.into()),
        }
    }
}

pub fn detect(path: &Path, size: u64) -> Detection {
    let mut header = [0u8; HEADER_BYTES];
    let read = match File::open(path).and_then(|mut f| f.read(&mut header)) {
        Ok(n) => n,
        Err(err) => return Detection::rejected(format!("Could not read file: {err}")),
    };
    if read == 0 {
        return Detection::rejected("File is empty");
    }
    let header = &header[..read];
    let extension = path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();

    if let Some(found) = signature(header, size, &extension) {
        return found;
    }

    // Raw photographs before `infer`: a CR2 is a TIFF underneath and would
    // otherwise be handed to ffmpeg, which cannot decode it. The list lives in
    // magick.rs, next to the engine that opens them.
    if crate::magick::RAW_INPUTS.contains(&extension.as_str())
        || crate::magick::OTHER_INPUTS.contains(&extension.as_str())
    {
        return Detection::known(MediaKind::Image, format!("image/{extension}"));
    }

    if let Some(found) = infer::get(header) {
        let mime = found.mime_type();
        let kind = match found.matcher_type() {
            infer::MatcherType::Image => MediaKind::Image,
            infer::MatcherType::Audio => MediaKind::Audio,
            infer::MatcherType::Video => MediaKind::Video,
            _ if DOCUMENT_MIMES.contains(&mime) => MediaKind::Document,
            _ => {
                return Detection::rejected(format!("{mime} is not a convertible file"));
            }
        };
        return Detection::known(kind, mime);
    }

    // No signature at all. Only trust the extension if the bytes look like text.
    if looks_like_text(header) {
        if TEXT_DOCUMENTS.contains(&extension.as_str()) {
            return Detection::known(MediaKind::Document, format!("text/{extension}"));
        }
        if TEXT_IMAGES.contains(&extension.as_str()) {
            return Detection::known(MediaKind::Image, "image/svg+xml");
        }
        if TEXT_MODELS.contains(&extension.as_str()) {
            return Detection::known(MediaKind::Model, format!("model/{extension}"));
        }
        if extension == "stl" && header.starts_with(b"solid") {
            return Detection::known(MediaKind::Model, "model/stl");
        }
    }

    Detection::rejected("Unrecognised file type")
}

/// Signatures `infer` does not carry.
fn signature(header: &[u8], size: u64, extension: &str) -> Option<Detection> {
    if header.starts_with(b"glTF") {
        return Some(Detection::known(MediaKind::Model, "model/gltf-binary"));
    }
    if header.starts_with(b"Kaydara FBX Binary") {
        return Some(Detection::known(MediaKind::Model, "model/fbx"));
    }
    if header.starts_with(b"BLENDER") {
        return Some(Detection::known(MediaKind::Model, "application/x-blender"));
    }
    if header.starts_with(b"ply\n") || header.starts_with(b"ply\r") {
        return Some(Detection::known(MediaKind::Model, "model/ply"));
    }
    if is_binary_stl(header, size) {
        return Some(Detection::known(MediaKind::Model, "model/stl"));
    }
    // ASCII STL starts with "solid", which is not unique enough on its own.
    if extension == "stl" && header.starts_with(b"solid") {
        return Some(Detection::known(MediaKind::Model, "model/stl"));
    }
    None
}

/// Binary STL has no magic number, but its layout is self-describing: an
/// 80-byte header, a u32 triangle count, then exactly 50 bytes per triangle.
fn is_binary_stl(header: &[u8], size: u64) -> bool {
    if header.len() < 84 || size < 84 {
        return false;
    }
    let triangles = u32::from_le_bytes([header[80], header[81], header[82], header[83]]) as u64;
    size == 84 + triangles * 50
}

/// A cheap "is this text?" check: valid UTF-8 and no control bytes beyond the
/// usual whitespace. The tail of the sample may cut a multi-byte character in
/// half, so only the decodable prefix is judged.
fn looks_like_text(header: &[u8]) -> bool {
    let text = match std::str::from_utf8(header) {
        Ok(text) => text,
        Err(err) if err.valid_up_to() > 0 => std::str::from_utf8(&header[..err.valid_up_to()])
            .expect("valid_up_to is a valid boundary"),
        Err(_) => return false,
    };
    !text
        .chars()
        .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t' | '\u{c}'))
}

/// `infer` classifies audio-only MP4 containers as video (an `.m4a` and an
/// `.mp4` share the same ISO-BMFF header). `probe.rs` calls this with what
/// ffprobe actually found so the grouping matches what will be encoded.
pub fn refine_with_streams(initial: MediaKind, has_video: bool, has_audio: bool) -> MediaKind {
    match initial {
        // A "video" file with no video stream is really just audio.
        MediaKind::Video if !has_video && has_audio => MediaKind::Audio,
        // An "audio" file that turns out to carry pictures stays audio: cover
        // art is an attached picture, not something the user wants converted.
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binary_stl_is_recognised_by_its_layout() {
        let mut header = vec![0u8; 84];
        header[80..84].copy_from_slice(&12u32.to_le_bytes());
        assert!(is_binary_stl(&header, 84 + 12 * 50));
        assert!(!is_binary_stl(&header, 999));
    }

    #[test]
    fn text_detection_rejects_binary() {
        assert!(looks_like_text(b"# heading\r\nsome text\t here"));
        assert!(!looks_like_text(&[0x00, 0x01, 0x02, 0x03]));
    }

    #[test]
    fn truncated_utf8_still_counts_as_text() {
        // Multi-byte character cut in half by the 512-byte sample boundary.
        let mut sample = b"merhaba d".to_vec();
        sample.push(0xc3);
        assert!(looks_like_text(&sample));
    }
}
