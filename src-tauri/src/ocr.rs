//! Reading text out of pictures.
//!
//! Two engines, in that order of preference:
//!
//! * **Tesseract**, when the machine already has it. It remains the better
//!   reader for awkward scans, and someone who installed it wants it used.
//! * **ocrs**, otherwise. It is a Rust library rather than a program, so it is
//!   compiled into the app and only its two model files are downloaded —
//!   twelve megabytes, and no binary that has to exist for the platform.
//!
//! Tesseract publishes a Windows installer and nothing at all for Linux, which
//! is why it cannot be the only option: half our users could never install it
//! from inside the app.

use std::path::{Path, PathBuf};

use tauri::AppHandle;

use crate::engines::{self, EngineId};

/// What an image can be turned into. Markdown is the same text with the
/// paragraph breaks kept as blank lines.
pub const TARGETS: &[&str] = &["txt", "md"];

pub fn is_target(extension: &str) -> bool {
    TARGETS.contains(
        &extension
            .trim_start_matches('.')
            .to_ascii_lowercase()
            .as_str(),
    )
}

/// Either engine will do, but the built-in one needs both of its models.
pub fn available(app: &AppHandle) -> bool {
    find_tesseract().is_some()
        || (engines::executable(app, EngineId::OcrDetection).is_some()
            && engines::executable(app, EngineId::OcrRecognition).is_some())
}

/// Tesseract is a system install, like LibreOffice: looked for, never fetched.
pub fn find_tesseract() -> Option<PathBuf> {
    let candidates: &[&str] = if cfg!(windows) {
        &[
            r"C:\Program Files\Tesseract-OCR\tesseract.exe",
            r"C:\Program Files (x86)\Tesseract-OCR\tesseract.exe",
        ]
    } else {
        &["/usr/bin/tesseract", "/usr/local/bin/tesseract"]
    };

    for candidate in candidates {
        let path = PathBuf::from(candidate);
        if path.is_file() {
            return Some(path);
        }
    }

    let exe = if cfg!(windows) {
        "tesseract.exe"
    } else {
        "tesseract"
    };
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|dir| dir.join(exe))
            .find(|path| path.is_file())
    })
}

pub struct TesseractPlan {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub produced: PathBuf,
    pub cleanup: Vec<PathBuf>,
}

/// Tesseract appends its own extension, so it writes into a scratch directory
/// and the runner moves the result onto the requested path.
pub fn tesseract_plan(input: &Path, job_id: &str) -> Option<TesseractPlan> {
    let tesseract = find_tesseract()?;
    let scratch = std::env::temp_dir().join(format!("coldmill-{job_id}"));
    std::fs::create_dir_all(&scratch).ok()?;
    let stem = scratch.join("text");

    Some(TesseractPlan {
        program: tesseract,
        args: vec![
            input.to_string_lossy().into_owned(),
            stem.to_string_lossy().into_owned(),
        ],
        produced: scratch.join("text.txt"),
        cleanup: vec![scratch],
    })
}

/// Runs the built-in engine. Blocking and slow enough to deserve its own
/// thread — the caller puts it on one.
pub fn read_with_ocrs(app: &AppHandle, input: &Path, output: &Path) -> Result<(), String> {
    let detection = engines::executable(app, EngineId::OcrDetection)
        .ok_or("The OCR models are missing — reinstall the text module")?;
    let recognition = engines::executable(app, EngineId::OcrRecognition)
        .ok_or("The OCR models are missing — reinstall the text module")?;

    let text = read_image(&detection, &recognition, input)?;

    // Markdown wants a blank line between paragraphs; plain text does not.
    let markdown = output
        .extension()
        .map(|e| e.eq_ignore_ascii_case("md"))
        .unwrap_or(false);
    let body = if markdown {
        text.lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>()
            .join("\n\n")
    } else {
        text
    };

    std::fs::write(output, body).map_err(|e| format!("could not write the text: {e}"))
}

/// The reading itself, kept free of app state so it can be tested against a
/// real picture without a running window.
pub fn read_image(detection: &Path, recognition: &Path, input: &Path) -> Result<String, String> {
    use ocrs::{ImageSource, OcrEngine, OcrEngineParams};

    let engine = OcrEngine::new(OcrEngineParams {
        detection_model: Some(
            rten::Model::load_file(detection).map_err(|e| format!("detection model: {e}"))?,
        ),
        recognition_model: Some(
            rten::Model::load_file(recognition).map_err(|e| format!("recognition model: {e}"))?,
        ),
        ..Default::default()
    })
    .map_err(|e| format!("could not start the OCR engine: {e}"))?;

    let image = image::open(input)
        .map_err(|e| format!("could not open the image: {e}"))?
        .into_rgb8();
    let source = ImageSource::from_bytes(image.as_raw(), image.dimensions())
        .map_err(|e| format!("could not read the image: {e}"))?;

    let prepared = engine
        .prepare_input(source)
        .map_err(|e| format!("could not prepare the image: {e}"))?;
    engine
        .get_text(&prepared)
        .map_err(|e| format!("could not read any text: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_text_formats_are_offered() {
        assert!(is_target("txt"));
        assert!(is_target(".MD"));
        assert!(!is_target("png"));
    }

    /// Reads a real picture of words. Ignored by default: it needs the two
    /// model files, which the app downloads rather than the repository
    /// carrying. Point it at them and run:
    ///
    ///   OCR_MODELS=<dir> cargo test -- --ignored --nocapture
    #[test]
    #[ignore]
    fn it_reads_words_off_a_picture() {
        let Some(dir) = std::env::var_os("OCR_MODELS").map(PathBuf::from) else {
            eprintln!("set OCR_MODELS to the directory holding the .rten files");
            return;
        };
        let image = std::env::var_os("OCR_IMAGE")
            .map(PathBuf::from)
            .expect("set OCR_IMAGE to a picture with words in it");

        let text = read_image(
            &dir.join("text-detection.rten"),
            &dir.join("text-recognition.rten"),
            &image,
        )
        .expect("read");

        println!("--- read back ---\n{text}\n---");
        assert!(
            !text.trim().is_empty(),
            "the engine found no text at all in {}",
            image.display()
        );
    }
}
