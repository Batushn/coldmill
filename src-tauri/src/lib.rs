mod commands;
mod detect;
mod document;
mod edit;
mod engines;
mod estimate;
mod external;
mod ffmpeg;
mod job;
mod mesh;
mod model;
mod ocr;
mod presets;
mod probe;
mod queue;
mod settings;
mod speech;
mod thumbs;

use std::sync::Arc;

use queue::JobRegistry;

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
