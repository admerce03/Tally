//! Fresh-clone support: `tauri::generate_context!` requires `frontendDist` (with an
//! `index.html`) to exist at compile time, but the frontend is built by npm. If
//! `../ui/dist/index.html` is missing, create a tiny placeholder page so
//! `cargo build/clippy/test --workspace` work without running npm first.
//! `npm run build` (Vite) replaces the directory contents with the real UI.

use std::path::PathBuf;

const PLACEHOLDER: &str = "<!doctype html><html><head><meta charset=\"utf-8\"><title>Tally</title></head>\
<body><p>Frontend not built. Run <code>npm run build</code> in app/ui.</p></body></html>\n";

fn main() {
    let manifest_dir = std::env::var_os("CARGO_MANIFEST_DIR").map(PathBuf::from);
    if let Some(manifest_dir) = manifest_dir {
        let dist = manifest_dir.join("..").join("ui").join("dist");
        let index = dist.join("index.html");
        if !index.exists() {
            // Best effort: a failure here surfaces as Tauri's own "frontendDist" error.
            if std::fs::create_dir_all(&dist).is_ok() {
                let _ = std::fs::write(&index, PLACEHOLDER);
            }
        }
    }
    println!("cargo:rerun-if-changed=build.rs");
    tauri_build::build()
}
