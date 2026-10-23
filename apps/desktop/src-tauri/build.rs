fn main() {
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "inspect_worktrees",
            "scan_codex",
            "set_conversation_locks",
            "operation_history",
            "diagnostic_inventory",
            "diagnostic_preview",
            "diagnostic_execute",
            "diagnostic_result",
            "diagnostic_cancel",
            "conversation_locks",
            "set_conversation_lock",
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
