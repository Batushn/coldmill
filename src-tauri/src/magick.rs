//! ImageMagick, for the pictures ffmpeg cannot open.
//!
//! ffmpeg turned out to read more than expected — JPEG XL, PSD, DDS, EXR, DPX
//! and TGA all decode with the build we ship — so this is not a replacement
//! for it. It fills the two real gaps: **vector** files, which ffmpeg has no
//! rasteriser for, and **camera raw**, which it does not decode at all.
//!
//! Conversions ffmpeg already handles keep going through ffmpeg. There is no
//! reason to move a working path onto a second engine, and the quality presets
//! live over there.

use std::path::{Path, PathBuf};

use tauri::AppHandle;

use crate::engines::{self, EngineId};
use crate::model::Quality;

/// Vector sources. ImageMagick rasterises these; ffmpeg cannot.
pub const VECTOR_INPUTS: &[&str] = &["svg", "svgz", "eps", "ai"];

/// Camera raw. Every manufacturer has its own, and none of them are ffmpeg's.
pub const RAW_INPUTS: &[&str] = &[
    "cr2", "cr3", "nef", "arw", "dng", "orf", "rw2", "raf", "srw", "pef",
];

/// Apple's still format. Newer ffmpeg builds sometimes manage it and sometimes
/// do not, depending on how they were compiled; ImageMagick is dependable.
pub const OTHER_INPUTS: &[&str] = &["heic", "heif"];

/// Whether this input needs ImageMagick rather than ffmpeg.
pub fn handles(extension: &str) -> bool {
    let extension = extension.trim_start_matches('.').to_ascii_lowercase();
    VECTOR_INPUTS.contains(&extension.as_str())
        || RAW_INPUTS.contains(&extension.as_str())
        || OTHER_INPUTS.contains(&extension.as_str())
}

pub fn available(app: &AppHandle) -> bool {
    engines::executable(app, EngineId::ImageMagick).is_some()
}

pub struct MagickPlan {
    pub program: PathBuf,
    pub args: Vec<String>,
}

pub fn plan(
    app: &AppHandle,
    input: &Path,
    output: &Path,
    quality: Quality,
) -> Result<MagickPlan, String> {
    let program = engines::executable(app, EngineId::ImageMagick)
        .ok_or("ImageMagick is not installed — turn on the extra image formats in setup")?;

    let extension = input
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();

    let mut args: Vec<String> = Vec::new();

    // Density has to precede the input: it decides how finely a vector is
    // rendered, and after the fact there is nothing left to decide.
    if VECTOR_INPUTS.contains(&extension.as_str()) {
        args.push("-density".into());
        args.push(
            match quality {
                Quality::Small => "96",
                Quality::Balanced => "150",
                Quality::High => "300",
            }
            .into(),
        );
    }

    args.push(input.to_string_lossy().into_owned());

    // Vectors and raw both come out with an alpha channel or a colour profile
    // that surprises people when they open the result; flatten onto white.
    if VECTOR_INPUTS.contains(&extension.as_str()) {
        args.push("-background".into());
        args.push("white".into());
        args.push("-flatten".into());
    }

    args.push("-quality".into());
    args.push(
        match quality {
            Quality::Small => "60",
            Quality::Balanced => "85",
            Quality::High => "95",
        }
        .into(),
    );

    args.push(output.to_string_lossy().into_owned());
    Ok(MagickPlan { program, args })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_takes_only_what_ffmpeg_cannot() {
        assert!(handles("svg"));
        assert!(handles(".CR2"));
        assert!(handles("heic"));
        // ffmpeg reads these perfectly well, so they stay with ffmpeg.
        assert!(!handles("png"));
        assert!(!handles("jxl"));
        assert!(!handles("psd"));
    }

    #[test]
    fn pdf_is_left_to_the_document_module() {
        // It is a page of text far more often than it is a drawing, and
        // LibreOffice already reads it.
        assert!(!handles("pdf"));
    }
}
