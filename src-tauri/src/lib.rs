mod commands;
mod detect;
mod ffmpeg;
mod model;
mod presets;
mod probe;
mod queue;

use std::sync::Arc;

use queue::JobRegistry;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(Arc::new(JobRegistry::new()))
        .invoke_handler(tauri::generate_handler![
            commands::probe_file,
            commands::supported_targets,
            commands::max_concurrency,
            commands::convert_files,
            commands::cancel_job,
            commands::cancel_all,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Coldmill");
}
