fn main() {
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "scan_codex",
            "cleanup_mode",
            "cleanup_preview",
            "cleanup_execute",
            "cleanup_result",
            "cleanup_cancel",
            "cleanup_history",
        ]),
    ))
    .expect("Tauri manifest build failed");
}
