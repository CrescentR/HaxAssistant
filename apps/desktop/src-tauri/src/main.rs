//! Tauri Desktop Host Entrypoint (`apps/desktop/src-tauri/src/main.rs`)
//! Bridges `app-core` (`HexBoxCore`), `reco-engine`, `lcu-client`, and `ocr-sidecar` to the React frontend.

use app_core::HexBoxCore;
use std::path::PathBuf;

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
        .unwrap_or_else(|| std::path::Path::new("."))
        .to_path_buf();

    let core = HexBoxCore::initialize(&root).expect("Failed to initialize HexBoxCore");
    println!(
        "HexBox Desktop Host Ready (Patch {}, {} heroes, {} augments)",
        core.dataset
            .runtime_meta
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or("16.23"),
        core.dataset.champions.len(),
        core.dataset.augments_full.len()
    );
}
