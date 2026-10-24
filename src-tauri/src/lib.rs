mod advanced;
mod commands;
mod data;
mod decimate;
mod detect;
mod document;
mod edit;
mod engines;
mod estimate;
mod external;
mod ffmpeg;
mod job;
mod magick;
mod mesh;
mod model;
mod ocr;
mod pdftext;
mod presets;
mod preview3d;
mod probe;
mod queue;
mod settings;
mod speech;
mod thumbs;
mod tts;

use std::sync::Arc;

use queue::JobRegistry;

/// Runs a helper job when the arguments ask for one, returning its exit code;
/// `None` means this is an ordinary launch.
pub fn helper(args: Vec<String>) -> Option<i32> {
    match args.as_slice() {
        [flag, input, output] if flag == pdftext::HELPER_FLAG => {
            Some(pdftext::helper(input, output))
        }
        [flag, target, input, outputs @ ..]
            if flag == data::CONVERT_FLAG && !outputs.is_empty() =>
        {
            Some(data::helper_convert(target, input, outputs))
        }
        [flag, target, input] if flag == data::SHEETS_FLAG => {
            Some(data::helper_sheets(target, input))
        }
        _ => None,
    }
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .manage(Arc::new(JobRegistry::new()))
        .invoke_handler(tauri::generate_handler![
            commands::probe_file,
            commands::supported_targets,
            commands::max_concurrency,
            commands::estimate_output,
            commands::thumbnail,
            commands::scrub_strip,
            commands::setup_state,
            commands::apply_setup,
            commands::convert_files,
            commands::cancel_job,
            commands::cancel_all,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Coldmill");
}
