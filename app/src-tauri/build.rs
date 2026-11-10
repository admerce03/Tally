//! Fresh-clone support: `tauri::generate_context!` requires `frontendDist` to exist at compile
//! time, but the frontend is built by npm. If `../ui/dist` is missing, create it with a tiny
//! placeholder page so `cargo build/clippy/test --workspace` work without running npm first.
//! `npm run build` (Vite) replaces the directory contents with the real UI.

use std::path::PathBuf;

const PLACEHOLDER: &str = "<!doctype html><html><head><meta charset=\"utf-8\"><title>Tally</title></head>\
<body><p>Frontend not built. Run <code>npm run build</code> in app/ui.</p></body></html>\n";

fn main() {
    let dist: PathBuf = ["..", "ui", "dist"].iter().collect();
    if !dist.exists() {
        // Best effort: a failure here surfaces as Tauri's own "frontendDist doesn't exist" error.
        if std::fs::create_dir_all(&dist).is_ok() {
            let _ = std::fs::write(dist.join("index.html"), PLACEHOLDER);
        }
    }
    println!("cargo:rerun-if-changed=build.rs");
    tauri_build::build()
}
