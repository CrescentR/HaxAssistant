use std::fs;
use std::path::Path;

/// Copy the runtime files the app needs (datasets, OCR models, OCR sidecar
/// scripts) into `resources/` so Tauri bundles them into every installer.
/// Source folders that are absent (e.g. git-ignored `data/` on CI) still get
/// an empty staged folder, so the bundle config never points at a missing path.
fn stage(src: &Path, dest: &Path, keep: impl Fn(&str) -> bool) {
    let _ = fs::remove_dir_all(dest);
    fs::create_dir_all(dest).expect("create staged resource dir");
    fs::write(dest.join("README.txt"), "Bundled by HexBox build.rs\n").expect("write placeholder");
    println!("cargo:rerun-if-changed={}", src.display());
    let Ok(entries) = fs::read_dir(src) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if path.is_file() && keep(&name) {
            fs::copy(&path, dest.join(&name)).expect("copy bundled resource");
        }
    }
}

/// Recursive copy for the compiled OCR runner folder (`ocr/`), which holds
/// the executable plus its bundled libraries.
fn stage_tree(src: &Path, dest: &Path) {
    fs::create_dir_all(dest).expect("create staged resource dir");
    let Ok(entries) = fs::read_dir(src) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let target = dest.join(entry.file_name());
        if path.is_dir() {
            stage_tree(&path, &target);
        } else if path.is_file() {
            fs::copy(&path, &target).expect("copy bundled runner file");
        }
    }
}

fn main() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace = manifest_dir.join("../../..");
    let staged = manifest_dir.join("resources");

    stage(&workspace.join("data"), &staged.join("data"), |n| {
        n.ends_with(".json")
            && !matches!(
                n,
                "user_settings.json"
                    | "overlay_layout.json"
                    | "main_window.json"
                    | "player_markers.json"
                    | "player_encounters.json"
                    | "augment_offer_stats.json"
            )
    });
    stage(&workspace.join("models"), &staged.join("models"), |n| !n.starts_with('.'));
    stage(&workspace.join("ocr-sidecar"), &staged.join("ocr-sidecar"), |n| {
        n.ends_with(".py") || n.ends_with(".txt") || n.ends_with(".png")
    });

    // Optional: only present after `scripts/build_ocr_runner.py` has run.
    let ocr_src = workspace.join("ocr");
    let ocr_dest = staged.join("ocr");
    let _ = fs::remove_dir_all(&ocr_dest);
    fs::create_dir_all(&ocr_dest).expect("create staged ocr dir");
    fs::write(ocr_dest.join("README.txt"), "Compiled OCR runner (scripts/build_ocr_runner.py)\n")
        .expect("write placeholder");
    println!("cargo:rerun-if-changed={}", ocr_src.display());
    stage_tree(&ocr_src, &ocr_dest);

    tauri_build::build();
}
