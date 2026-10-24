//! Text out of a PDF, with nothing installed.
//!
//! LibreOffice was the only reader here before, and it opens a PDF as a
//! drawing: asked for text it wrote an empty file, and asked for anything else
//! it had no export filter at all. This reads the text layer directly.
//!
//! The parser runs in a child process — this same executable, started with
//! `--pdf-text` — rather than in the app. PDFs in the wild are frequently
//! malformed, the parser can panic on one, and release builds abort on panic:
//! in-process, one bad file would close the window and every job in it.

use std::path::{Path, PathBuf};

/// The flag `main` checks for before starting the app.
pub const HELPER_FLAG: &str = "--pdf-text";

/// What the helper writes when it is asked for these.
pub const TARGETS: &[&str] = &["txt", "md"];

pub fn is_target(target: &str) -> bool {
    TARGETS.contains(&target.trim_start_matches('.').to_ascii_lowercase().as_str())
}

pub struct HelperPlan {
    pub program: PathBuf,
    pub args: Vec<String>,
}

pub fn plan(input: &Path, output: &Path) -> Result<HelperPlan, String> {
    let program =
        std::env::current_exe().map_err(|e| format!("cannot find Coldmill itself: {e}"))?;
    Ok(HelperPlan {
        program,
        args: vec![
            HELPER_FLAG.into(),
            input.to_string_lossy().into_owned(),
            output.to_string_lossy().into_owned(),
        ],
    })
}

/// Entry point of the child process. Returns the exit code.
pub fn helper(input: &str, output: &str) -> i32 {
    // A panic message names a source line in a parser nobody here wrote.
    // What the person needs to read is which file failed and that the file
    // is the reason.
    std::panic::set_hook(Box::new(|_| {
        eprintln!("This PDF could not be read; it is damaged or uses a feature the reader does not understand");
    }));

    match extract(Path::new(input)) {
        Ok(text) => match std::fs::write(output, text) {
            Ok(()) => 0,
            Err(err) => {
                eprintln!("could not write the text: {err}");
                1
            }
        },
        Err(message) => {
            eprintln!("{message}");
            1
        }
    }
}

fn extract(input: &Path) -> Result<String, String> {
    let raw = pdf_extract::extract_text(input).map_err(|e| match e {
        pdf_extract::OutputError::PdfError(inner) if inner.to_string().contains("ncrypt") => {
            "This PDF is password protected".to_string()
        }
        other => format!("This PDF could not be read: {other}"),
    })?;
    let text = tidy(&raw);
    if text.trim().is_empty() {
        // Scans are pictures of pages. Saying so is more useful than an empty
        // file that looks like a successful conversion.
        return Err("This PDF has no text in it — it is probably a scan".into());
    }
    Ok(text)
}

/// The extractor marks every page break with a form feed and pads lines with
/// spaces; neither belongs in a text file.
fn tidy(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut blank_run = 0;
    for line in raw.replace('\u{c}', "\n\n").lines() {
        let line = line.trim_end();
        if line.trim().is_empty() {
            blank_run += 1;
            if blank_run > 1 {
                continue;
            }
        } else {
            blank_run = 0;
        }
        out.push_str(line);
        out.push('\n');
    }
    out.trim_start_matches('\n').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_breaks_and_padding_are_cleaned_up() {
        assert_eq!(
            tidy("\n\nTitle   \n\n\n\nBody\u{c}Next page\n"),
            "Title\n\nBody\n\nNext page\n"
        );
    }

    #[test]
    fn the_helper_is_started_with_its_flag() {
        let plan = plan(Path::new("/in/a.pdf"), Path::new("/out/a.txt")).unwrap();
        assert_eq!(plan.args[0], HELPER_FLAG);
        assert_eq!(plan.args[2], "/out/a.txt");
    }
}
