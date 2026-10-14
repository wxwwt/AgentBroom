use agentbroom_core::{InspectError, Inventory, Preview, preview, report, scan_fixture_store};
use tauri::Manager;
fn inventory(app: &tauri::AppHandle) -> Result<Inventory, InspectError> {
    let root = app
        .path()
        .resource_dir()
        .map_err(|_| InspectError {
            code: "demoUnavailable".into(),
            message: "Demo resources are unavailable.".into(),
        })?
        .join("demo-store");
    scan_fixture_store(&root)
}
#[tauri::command]
fn scan_demo(app: tauri::AppHandle) -> Result<Inventory, InspectError> {
    inventory(&app)
}
#[tauri::command(rename_all = "camelCase")]
fn preview_demo(
    app: tauri::AppHandle,
    selected_ids: Vec<String>,
    kept_ids: Vec<String>,
    source_fingerprint: String,
) -> Result<Preview, InspectError> {
    preview(
        &inventory(&app)?,
        &selected_ids,
        &kept_ids,
        &source_fingerprint,
    )
}
#[tauri::command(rename_all = "camelCase")]
fn demo_report(
    app: tauri::AppHandle,
    selected_ids: Vec<String>,
    kept_ids: Vec<String>,
    source_fingerprint: String,
) -> Result<String, InspectError> {
    report(
        &inventory(&app)?,
        &selected_ids,
        &kept_ids,
        &source_fingerprint,
    )
}
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
        .invoke_handler(tauri::generate_handler![
            scan_demo,
            scan_codex,
            preview_demo,
            demo_report
        ])
        .run(tauri::generate_context!())
        .expect("Could not launch AgentBroom");
}
