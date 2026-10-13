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
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            scan_demo,
            preview_demo,
            demo_report
        ])
        .run(tauri::generate_context!())
        .expect("Could not launch AgentBroom");
}
