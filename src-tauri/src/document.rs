//! Document conversion.
//!
//! Three engines, picked per file:
//!
//! * **Pandoc** (+ Typst as its PDF engine) for text-ish formats — markdown,
//!   docx, odt, html, epub, LaTeX, and since 3.x PowerPoint too — and anything
//!   to PDF.
//! * **The built-in PDF reader** for text out of a PDF. Pandoc has no PDF
//!   reader at all, and this needs nothing installed.
//! * **LibreOffice**, when the machine has it, for what neither can open:
//!   legacy binary Office files, and a PDF turned back into an editable
//!   document.
//!
//! Spreadsheets used to be here too. They are data now (`data.rs`), converted
//! in-process, because LibreOffice was a heavy way to turn cells into a CSV.

use std::path::{Path, PathBuf};

use tauri::AppHandle;

use crate::engines::{self, EngineId};
use crate::pdftext;

/// Formats pandoc can read on its own.
pub const PANDOC_INPUTS: &[&str] = &[
    "md", "markdown", "docx", "odt", "html", "htm", "epub", "rtf", "tex", "latex", "rst", "org",
    "txt", "ipynb", "adoc", "asciidoc", "textile", "opml", "man", "pptx", "fb2", "typ", "wiki",
];

/// Formats pandoc can write. PDF goes through Typst.
pub const PANDOC_OUTPUTS: &[&str] = &[
    "pdf", "docx", "odt", "html", "md", "epub", "rtf", "tex", "rst", "txt", "pptx", "typ", "adoc",
    "org", "fb2",
];

/// Inputs LibreOffice opens. PowerPoint is also pandoc's, but LibreOffice
/// renders the slides themselves, so it is preferred when present.
pub const OFFICE_INPUTS: &[&str] = &["doc", "ppt", "pptx", "odp", "pdf"];

/// Extra outputs unlocked once LibreOffice is available.
pub const OFFICE_OUTPUTS: &[&str] = &["pdf", "docx", "odt", "pptx", "html", "rtf"];

/// What can be produced right now, given which engines are installed.
pub fn targets(app: &AppHandle) -> Vec<String> {
    // PDF text needs nothing, so a PDF always has somewhere to go.
    let mut out: Vec<String> = pdftext::TARGETS.iter().map(|s| s.to_string()).collect();
    let mut add = |list: &[&str]| {
        for target in list {
            if !out.iter().any(|existing| existing == target) {
                out.push(target.to_string());
            }
        }
    };
    if engines::executable(app, EngineId::Pandoc).is_some() {
        add(PANDOC_OUTPUTS);
    }
    if engines::find_libreoffice().is_some() {
        add(OFFICE_OUTPUTS);
    }
    // The order a person scans for: the usual suspects first.
    out.sort_by_key(|target| {
        ["pdf", "docx", "txt", "md", "odt", "html", "epub", "pptx"]
            .iter()
            .position(|first| first == target)
            .unwrap_or(usize::MAX)
    });
    out
}

/// Why a document cannot be queued, if it cannot.
pub fn rejection(app: &AppHandle, extension: &str) -> Option<String> {
    let extension = extension.to_ascii_lowercase();
    let pandoc = engines::executable(app, EngineId::Pandoc).is_some();
    let office = engines::find_libreoffice().is_some();

    // A PDF can always at least give up its text.
    if extension == "pdf" {
        return None;
    }
    if PANDOC_INPUTS.contains(&extension.as_str()) && pandoc {
        return None;
    }
    if OFFICE_INPUTS.contains(&extension.as_str()) {
        return (!office).then(|| {
            if PANDOC_INPUTS.contains(&extension.as_str()) {
                "Turn on the document module in setup to convert this".to_string()
            } else {
                format!(".{extension} files need LibreOffice — install it and re-check in setup")
            }
        });
    }
    if PANDOC_INPUTS.contains(&extension.as_str()) {
        return Some("Turn on the document module in setup to convert this".to_string());
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

    if source_ext == "pdf" && pdftext::is_target(&target_ext) {
        let plan = pdftext::plan(input, output)?;
        return Ok(DocumentPlan {
            program: plan.program,
            args: plan.args,
            cwd: None,
            produced: None,
            cleanup: Vec::new(),
        });
    }

    let pandoc_reads = PANDOC_INPUTS.contains(&source_ext.as_str());
    let pandoc_writes = PANDOC_OUTPUTS.contains(&target_ext.as_str());
    let office_reads = OFFICE_INPUTS.contains(&source_ext.as_str());
    let office = engines::find_libreoffice().is_some();
    let pandoc = engines::executable(app, EngineId::Pandoc).is_some();

    // LibreOffice where it can write the target. PowerPoint going to
    // markdown is pandoc's job even with LibreOffice installed, which has no
    // filter for it.
    let office_writes = OFFICE_OUTPUTS.contains(&target_ext.as_str()) || target_ext == "txt";
    if office_reads && office && (office_writes || !pandoc_reads) {
        // The runner moves LibreOffice's own output onto `output` afterwards.
        return libreoffice_plan(input, &target_ext, job_id);
    }
    if pandoc_reads && pandoc_writes && pandoc {
        return pandoc_plan(app, input, output, &target_ext);
    }
    if office_reads {
        // Says plainly that LibreOffice is missing, which is the actual fix.
        return libreoffice_plan(input, &target_ext, job_id);
    }
    if !pandoc {
        return Err("The document module is not installed".into());
    }
    Err(format!(".{source_ext} cannot be turned into .{target_ext}"))
}

pub fn pandoc_plan(
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

pub fn libreoffice_plan(
    input: &Path,
    target_ext: &str,
    job_id: &str,
) -> Result<DocumentPlan, String> {
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

    let mut args = vec![
        profile_url,
        "--headless".into(),
        "--norestore".into(),
        "--invisible".into(),
    ];
    // Left to itself LibreOffice opens a PDF in Draw, as a picture of a page,
    // and then has no filter to write a word processor format from it:
    // "no export filter", and nothing written. Importing it into Writer is
    // what makes PDF to docx work at all.
    if input
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("pdf"))
    {
        args.push("--infilter=writer_pdf_import".into());
    }
    args.extend([
        "--convert-to".into(),
        target_ext.to_string(),
        "--outdir".into(),
        scratch.to_string_lossy().into_owned(),
        input.to_string_lossy().into_owned(),
    ]);

    Ok(DocumentPlan {
        program: soffice,
        args,
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

    #[test]
    fn spreadsheets_are_data_now() {
        for sheet in ["xls", "xlsx", "ods", "csv"] {
            assert!(!OFFICE_INPUTS.contains(&sheet), "{sheet}");
            assert!(!PANDOC_INPUTS.contains(&sheet), "{sheet}");
            assert!(crate::data::is_input(sheet), "{sheet}");
        }
    }

    #[test]
    fn a_pdf_is_imported_into_writer() {
        let plan = libreoffice_plan(Path::new("/in/report.pdf"), "docx", "test-job");
        // No LibreOffice on the test machine is a fine answer too; what must
        // not happen is a PDF handed over without the Writer import filter.
        if let Ok(plan) = plan {
            assert!(plan
                .args
                .iter()
                .any(|a| a == "--infilter=writer_pdf_import"));
            for dir in plan.cleanup {
                let _ = std::fs::remove_dir_all(dir);
            }
        }
    }
}
