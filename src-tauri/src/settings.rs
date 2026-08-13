//! Which optional modules the user turned on, remembered between runs.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::model::ModuleId;

/// Everything defaults to off: media always works, and nobody should be
/// downloading hundreds of megabytes they never asked for.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// False until the first-run module picker has been answered.
    pub setup_done: bool,
    pub documents: bool,
    pub models: bool,
    /// 3D with Blender: adds fbx, dae, ply and .blend at the cost of a large
    /// download. Off means the built-in converter handles stl/obj/glb only.
    pub blender: bool,
    /// Speech to text: turns video and audio into transcripts and subtitles.
    pub speech: bool,
    /// Reading text out of pictures.
    pub ocr: bool,
    /// Reading documents aloud.
    pub tts: bool,
}

impl Settings {
    pub fn enabled(&self, module: ModuleId) -> bool {
        match module {
            ModuleId::Media => true,
            ModuleId::Documents => self.documents,
            ModuleId::Models => self.models,
        }
    }
}

fn settings_path(app: &AppHandle) -> Option<PathBuf> {
    Some(app.path().app_config_dir().ok()?.join("settings.json"))
}

/// Missing or unreadable settings are not an error: a fresh install simply
/// gets the defaults and the setup screen.
pub fn load(app: &AppHandle) -> Settings {
    settings_path(app)
        .and_then(|path| std::fs::read(path).ok())
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

pub fn save(app: &AppHandle, settings: &Settings) -> Result<(), String> {
    let path = settings_path(app).ok_or("no config directory")?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_vec_pretty(settings).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| format!("could not save settings: {e}"))
}
