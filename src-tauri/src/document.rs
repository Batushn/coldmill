//! Document conversion.
//!
//! Two engines, picked per file:
//!
//! * **Pandoc** (+ Typst as its PDF engine) for text-ish formats — markdown,
//!   docx, odt, html, epub, LaTeX — and anything to PDF.
//! * **LibreOffice**, when the machine has it, for the things pandoc cannot
//!   read: legacy binary Office files, spreadsheets, presentations, and PDF as
//!   an *input*. Pandoc has no PDF reader at all, which is why the UI gates
//!   those formats behind a LibreOffice check.

use std::path::{Path, PathBuf};

use tauri::AppHandle;

use crate::engines::{self, EngineId};

/// Formats pandoc can read on its own.
pub const PANDOC_INPUTS: &[&str] = &[
    "md", "markdown", "docx", "odt", "html", "htm", "epub", "rtf", "tex", "latex", "rst", "org",
    "txt", "ipynb", "csv", "adoc", "asciidoc", "textile", "opml", "man",
];

/// Formats pandoc can write. PDF goes through Typst.
pub const PANDOC_OUTPUTS: &[&str] = &[
    "pdf", "docx", "odt", "html", "md", "epub", "rtf", "tex", "rst", "txt",
];

/// Inputs that only LibreOffice can open.
pub const OFFICE_INPUTS: &[&str] = &["doc", "xls", "ppt", "xlsx", "pptx", "pdf", "odp", "ods"];

/// Extra outputs unlocked once LibreOffice is available.
pub const OFFICE_OUTPUTS: &[&str] = &["pdf", "docx", "odt", "xlsx", "csv", "pptx", "html", "txt"];

/// What can be produced right now, given which engines are installed.
pub fn targets(app: &AppHandle) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    if engines::executable(app, EngineId::Pandoc).is_some() {
        out.extend(PANDOC_OUTPUTS.iter().map(|s| s.to_string()));
    }
    if engines::find_libreoffice().is_some() {
        for target in OFFICE_OUTPUTS {
            if !out.iter().any(|existing| existing == target) {
                out.push(target.to_string());
            }
        }
    }
    out
}

/// Why a document cannot be queued, if it cannot.
pub fn rejection(app: &AppHandle, extension: &str) -> Option<String> {
    let extension = extension.to_ascii_lowercase();
    let pandoc = engines::executable(app, EngineId::Pandoc).is_some();
    let office = engines::find_libreoffice().is_some();

    if OFFICE_INPUTS.contains(&extension.as_str()) {
        return (!office).then(|| {
            format!(".{extension} files need LibreOffice — install it and re-check in setup")
        });
    }
    if PANDOC_INPUTS.contains(&extension.as_str()) {
        return (!pandoc).then(|| "The document module is not installed".to_string());
    }
    Some(format!(".{extension} is not a supported document format"))
}

pub struct DocumentPlan {
    pub program: PathBuf,
    pub args: Vec<String>,
    /// Working directory, so relative images in the source resolve.
    pub cwd: Option<PathBuf>,
    /// Where the tool will actually write. LibreOffice picks its own file
    /// name, so the runner moves the result onto the requested path.
    pub produced: Option<PathBuf>,
    /// Scratch directories to delete afterwards.
    pub cleanup: Vec<PathBuf>,
}

pub fn plan(
    app: &AppHandle,
    input: &Path,
    output: &Path,
    job_id: &str,
) -> Result<DocumentPlan, String> {
    let source_ext = input
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let target_ext = output
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();

    let office_only = OFFICE_INPUTS.contains(&source_ext.as_str());
    let pandoc_can_write = PANDOC_OUTPUTS.contains(&target_ext.as_str());

    if !office_only && pandoc_can_write {
        return pandoc_plan(app, input, output, &target_ext);
    }
    // The runner moves LibreOffice's own output onto `output` afterwards.
    libreoffice_plan(input, &target_ext, job_id)
}

fn pandoc_plan(
    app: &AppHandle,
    input: &Path,
    output: &Path,
    target_ext: &str,
) -> Result<DocumentPlan, String> {
    let pandoc =
        engines::executable(app, EngineId::Pandoc).ok_or("The document module is not installed")?;

    let mut args = vec![
        input.to_string_lossy().into_owned(),
        "-o".into(),
        output.to_string_lossy().into_owned(),
    ];

    // Fragments are rarely what someone wants from a file converter.
    if matches!(
        target_ext,
        "html" | "htm" | "tex" | "latex" | "epub" | "rtf"
    ) {
        args.push("--standalone".into());
    }

    if target_ext == "pdf" {
        let typst = engines::executable(app, EngineId::Typst)
            .ok_or("Typst is missing — reinstall the document module")?;
        // Typst instead of LaTeX: same job, 20 MB instead of several GB.
        // Passed as a full path because typst is not on the user's PATH.
        args.push(format!("--pdf-engine={}", typst.to_string_lossy()));
    }

    Ok(DocumentPlan {
        program: pandoc,
        args,
        cwd: input.parent().map(Path::to_path_buf),
        produced: None,
        cleanup: Vec::new(),
    })
}

fn libreoffice_plan(input: &Path, target_ext: &str, job_id: &str) -> Result<DocumentPlan, String> {
    let soffice = engines::find_libreoffice()
        .ok_or("LibreOffice is required for this conversion but was not found")?;

    // LibreOffice derives the output name from the input and refuses to write
    // anywhere else, so it converts into a scratch directory and the runner
    // moves the result.
    let scratch = std::env::temp_dir().join(format!("coldmill-{job_id}"));
    std::fs::create_dir_all(&scratch).map_err(|e| e.to_string())?;

    let stem = input
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "output".into());
    let produced = scratch.join(format!("{stem}.{target_ext}"));

    // Each job gets its own profile: two soffice processes sharing one would
    // fight, and the queue runs several at a time.
    let profile = scratch.join("profile");
    let profile_url = format!("-env:UserInstallation=file:///{}", url_path(&profile));

    Ok(DocumentPlan {
        program: soffice,
        args: vec![
            profile_url,
            "--headless".into(),
            "--norestore".into(),
            "--invisible".into(),
            "--convert-to".into(),
            target_ext.to_string(),
            "--outdir".into(),
            scratch.to_string_lossy().into_owned(),
            input.to_string_lossy().into_owned(),
        ],
        cwd: None,
        produced: Some(produced),
        cleanup: vec![scratch],
    })
}

/// `C:\Users\x` -> `C:/Users/x`, which is what LibreOffice's file:/// URLs want.
fn url_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_paths_become_url_friendly() {
        assert_eq!(
            url_path(Path::new(r"C:\Temp\coldmill\profile")),
            "C:/Temp/coldmill/profile"
        );
    }

    #[test]
    fn office_only_inputs_are_listed_separately_from_pandoc() {
        // PDF must never be claimed by pandoc: it cannot read one.
        assert!(OFFICE_INPUTS.contains(&"pdf"));
        assert!(!PANDOC_INPUTS.contains(&"pdf"));
    }
}
