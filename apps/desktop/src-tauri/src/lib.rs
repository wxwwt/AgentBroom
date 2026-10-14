use agentbroom_core::{InspectError, Inventory};
#[tauri::command]
async fn scan_codex() -> Result<Inventory, InspectError> {
    static SCAN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if SCAN.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return Err(InspectError {
            code: "scanBusy".into(),
            message: "A local inspection is already running.".into(),
        });
    }
    let result = tauri::async_runtime::spawn_blocking(agentbroom_core::codex::scan_default).await;
    SCAN.store(false, std::sync::atomic::Ordering::SeqCst);
    result.map_err(|_| InspectError {
        code: "scanFailed".into(),
        message: "Local inspection failed.".into(),
    })?
}
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![scan_codex])
        .run(tauri::generate_context!())
        .expect("Could not launch AgentBroom");
}
