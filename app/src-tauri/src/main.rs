// Prevents an extra console window on Windows in release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if let Err(e) = tally_app_lib::run() {
        eprintln!("tally: {e:#}");
        std::process::exit(1);
    }
}
