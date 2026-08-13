//! Shared types. Everything crossing the IPC boundary is camelCase on the JS side.

use serde::{Deserialize, Serialize};

/// What a file actually is. Decided by magic bytes wherever a signature
/// exists; text-based formats (obj, md, svg…) fall back to the extension,
/// which `detect.rs` documents case by case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MediaKind {
    Image,
    Audio,
    Video,
    /// Text documents, office files and PDFs. Needs the document module.
    Document,
    /// 3D meshes and scenes. Needs the 3D module.
    Model,
    /// Nothing we can convert. Shown in the UI, never queued.
    Unsupported,
}

impl MediaKind {
    pub fn is_media(self) -> bool {
        !matches!(self, MediaKind::Unsupported)
    }

    /// Which feature module has to be installed for this kind to convert.
    pub fn module(self) -> Option<ModuleId> {
        match self {
            MediaKind::Image | MediaKind::Audio | MediaKind::Video => Some(ModuleId::Media),
            MediaKind::Document => Some(ModuleId::Documents),
            MediaKind::Model => Some(ModuleId::Models),
            MediaKind::Unsupported => None,
        }
    }
}

/// Optional feature packs. Media is always available (ffmpeg ships with the
/// app); the others pull their engines down on demand.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModuleId {
    Media,
    Documents,
    Models,
}

/// The only user-facing knob. Mapped to real encoder settings in `presets.rs`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Quality {
    Small,
    #[default]
    Balanced,
    High,
}

/// Result of inspecting a dropped file.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileProbe {
    pub path: String,
    pub file_name: String,
    pub size_bytes: u64,
    pub kind: MediaKind,
    /// Detected mime type, e.g. `video/mp4`. `None` when nothing matched.
    pub mime: Option<String>,
    pub extension: Option<String>,
    /// Media duration in seconds. `None` for stills — the UI then shows an
    /// indeterminate progress bar instead of a percentage.
    pub duration_secs: Option<f64>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    /// Frame rate, used by the output size estimate.
    pub fps: Option<f64>,
    /// Triangle count, when the format states it up front (binary STL).
    pub triangles: Option<u64>,
    /// Human readable explanation when `kind` is `Unsupported`.
    pub reason: Option<String>,
}

/// One file the user asked to convert.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConvertItem {
    pub path: String,
    /// Target container/extension without the dot, e.g. `mp4`.
    pub target_format: String,
    pub kind: MediaKind,
    /// Known from the earlier probe. Without it a trim has nothing to measure
    /// against, so the editing panel stays closed.
    pub duration_secs: Option<f64>,
    /// Trimming, splitting, muting and re-framing. Empty for most files.
    #[serde(default)]
    pub edit: crate::edit::EditSpec,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConvertRequest {
    pub items: Vec<ConvertItem>,
    #[serde(default)]
    pub quality: Quality,
    /// `None` means "next to the source file".
    pub output_dir: Option<String>,
}

/// Returned synchronously so the UI can bind a row to a job before work starts.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobCreated {
    pub job_id: String,
    pub path: String,
    /// The first file. Kept separate because it is what "show in folder"
    /// points at.
    pub output_path: String,
    /// Every file this job will write. A split produces several, and a row
    /// claiming one output when three were made is how splitting came to look
    /// broken.
    pub outputs: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressPayload {
    pub job_id: String,
    /// 0.0–1.0, or `None` when the source has no duration (stills).
    pub fraction: Option<f64>,
    pub out_bytes: Option<u64>,
    /// ffmpeg's own speed readout, e.g. `2.4x`.
    pub speed: Option<String>,
    /// Final size projected from what has been written so far. Replaces the
    /// static pre-run estimate as soon as it is trustworthy.
    pub estimated_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DonePayload {
    pub job_id: String,
    pub output_path: String,
    pub outputs: Vec<String>,
    /// Every file added up, not just the first.
    pub output_bytes: u64,
    pub elapsed_ms: u128,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorPayload {
    pub job_id: String,
    /// Last lines of ffmpeg stderr, or our own message.
    pub message: String,
    /// `true` when the user pressed cancel — the UI greys the row out instead
    /// of painting it red.
    pub cancelled: bool,
}

pub const EVENT_PROGRESS: &str = "convert:progress";
pub const EVENT_DONE: &str = "convert:done";
pub const EVENT_ERROR: &str = "convert:error";
