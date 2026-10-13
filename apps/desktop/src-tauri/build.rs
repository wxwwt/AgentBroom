fn main() {
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&["scan_demo", "preview_demo", "demo_report"]),
    ))
    .expect("Tauri manifest build failed");
}
