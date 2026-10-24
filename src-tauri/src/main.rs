// Keep the console window hidden in release builds on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // The app starts itself as a helper for work that is safer in a separate
    // process; see pdftext.rs. Those runs never open a window.
    if let Some(code) = coldmill_lib::helper(std::env::args().skip(1).collect()) {
        std::process::exit(code);
    }
    coldmill_lib::run()
}
