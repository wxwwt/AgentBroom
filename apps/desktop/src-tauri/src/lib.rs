use agentbroom_core::{InspectError, Inventory};
fn error(code: &str) -> InspectError {
    InspectError {
        code: code.into(),
        message: code.into(),
    }
}
#[cfg(feature = "native-cleanup-test")]
mod native_test {
    use super::*;
    use agentbroom_native_test::{Confirmation, NativePlan, NativeSandbox, Receipt};
    use std::{
        path::Path,
        sync::{
            Arc, Mutex,
            atomic::{AtomicU8, Ordering},
        },
    };
    #[derive(Clone, Default)]
    pub struct Session {
        sandbox: Arc<Mutex<Option<NativeSandbox>>>,
        // 0=running, 1=waiting for the current worker and cleanup, 2=ready to exit.
        shutdown: Arc<AtomicU8>,
    }
    impl Session {
        fn running(&self) -> Result<(), InspectError> {
            if self.shutdown.load(Ordering::SeqCst) != 0 {
                Err(error("cleanupClosing"))
            } else {
                Ok(())
            }
        }
        fn finish_shutdown(&self) {
            let mut guard = self
                .sandbox
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            guard.take(); // Drop the owned server first, then its generated temporary home.
            self.shutdown.store(2, Ordering::SeqCst);
        }
    }
    pub fn on_event(app: &tauri::AppHandle, event: tauri::RunEvent) {
        use tauri::Manager;
        let state = app.state::<Session>();
        match event {
            tauri::RunEvent::ExitRequested { api, .. } => {
                if state.shutdown.load(Ordering::SeqCst) == 2 {
                    return;
                }
                api.prevent_exit();
                if state
                    .shutdown
                    .compare_exchange(0, 1, Ordering::SeqCst, Ordering::SeqCst)
                    .is_ok()
                {
                    let session = state.inner().clone();
                    let handle = app.clone();
                    tauri::async_runtime::spawn_blocking(move || {
                        session.finish_shutdown();
                        handle.exit(0);
                    });
                }
            }
            // Tauri's run loop calls process::exit, so Drop on managed state alone is insufficient.
            tauri::RunEvent::Exit => state.finish_shutdown(),
            _ => {}
        }
    }
    async fn with_sandbox<T: Send + 'static>(
        state: Session,
        action: impl FnOnce(&mut NativeSandbox) -> Result<T, InspectError> + Send + 'static,
    ) -> Result<T, InspectError> {
        tauri::async_runtime::spawn_blocking(move || {
            state.running()?;
            let mut guard = state.sandbox.try_lock().map_err(|_| error("cleanupBusy"))?;
            state.running()?;
            if guard.is_none() {
                let binary = std::env::var_os("AGENTBROOM_TEST_CODEX_BIN")
                    .ok_or_else(|| error("testRuntimeRequired"))?;
                *guard = Some(NativeSandbox::generate(Path::new(&binary))?);
            }
            action(guard.as_mut().unwrap())
        })
        .await
        .map_err(|_| error("cleanupWorkerFailed"))?
    }
    #[tauri::command]
    pub async fn scan_codex(state: tauri::State<'_, Session>) -> Result<Inventory, InspectError> {
        with_sandbox(state.inner().clone(), |s| s.inventory()).await
    }
    #[tauri::command]
    pub async fn cleanup_preview(
        state: tauri::State<'_, Session>,
        selected_ids: Vec<String>,
        kept: Vec<String>,
    ) -> Result<NativePlan, InspectError> {
        with_sandbox(state.inner().clone(), move |s| {
            s.preview(&selected_ids, &kept)
        })
        .await
    }
    #[tauri::command]
    pub async fn cleanup_execute(
        state: tauri::State<'_, Session>,
        confirmation: Confirmation,
    ) -> Result<Receipt, InspectError> {
        with_sandbox(state.inner().clone(), move |s| s.execute(confirmation)).await
    }
    #[tauri::command]
    pub async fn cleanup_result(
        state: tauri::State<'_, Session>,
        plan_id: String,
    ) -> Result<Receipt, InspectError> {
        with_sandbox(state.inner().clone(), move |s| s.receipt(&plan_id)).await
    }
    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn shutdown_waits_for_current_worker_before_closing() {
            let session = Session::default();
            let worker = session.sandbox.lock().unwrap();
            session.shutdown.store(1, Ordering::SeqCst);
            let closing = session.clone();
            let (done_tx, done_rx) = std::sync::mpsc::channel();
            let thread = std::thread::spawn(move || {
                closing.finish_shutdown();
                done_tx.send(()).unwrap();
            });
            assert!(
                done_rx
                    .recv_timeout(std::time::Duration::from_millis(50))
                    .is_err()
            );
            drop(worker);
            done_rx
                .recv_timeout(std::time::Duration::from_secs(1))
                .unwrap();
            thread.join().unwrap();
            assert_eq!(session.shutdown.load(Ordering::SeqCst), 2);
        }
        #[test]
        fn shutdown_blocks_new_workers_and_is_idempotent() {
            let session = Session::default();
            assert!(session.running().is_ok());
            session.shutdown.store(1, Ordering::SeqCst);
            assert_eq!(session.running().unwrap_err().code, "cleanupClosing");
            session.finish_shutdown();
            session.finish_shutdown();
            assert_eq!(session.shutdown.load(Ordering::SeqCst), 2);
            assert!(session.running().is_err());
        }
    }
}
#[cfg(feature = "native-cleanup-test")]
use native_test::{cleanup_execute, cleanup_preview, cleanup_result, scan_codex};
#[tauri::command]
fn cleanup_mode() -> &'static str {
    if cfg!(feature = "native-cleanup-test") {
        "syntheticNativeTest"
    } else {
        "readOnly"
    }
}
#[cfg(not(feature = "native-cleanup-test"))]
#[tauri::command]
async fn scan_codex() -> Result<Inventory, InspectError> {
    static SCAN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if SCAN.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return Err(error("scanBusy"));
    }
    let result = tauri::async_runtime::spawn_blocking(agentbroom_core::codex::scan_default).await;
    SCAN.store(false, std::sync::atomic::Ordering::SeqCst);
    result.map_err(|_| error("scanFailed"))?
}
#[cfg(not(feature = "native-cleanup-test"))]
#[tauri::command]
fn cleanup_execute() -> Result<(), InspectError> {
    agentbroom_core::cleanup::reject_live_execution()
}
#[cfg(not(feature = "native-cleanup-test"))]
#[tauri::command]
fn cleanup_preview() -> Result<(), InspectError> {
    agentbroom_core::cleanup::reject_live_execution()
}
#[cfg(not(feature = "native-cleanup-test"))]
#[tauri::command]
fn cleanup_result() -> Result<(), InspectError> {
    agentbroom_core::cleanup::reject_live_execution()
}
pub fn run() {
    let builder = tauri::Builder::default();
    #[cfg(feature = "native-cleanup-test")]
    let builder = builder.manage(native_test::Session::default());
    builder
        .invoke_handler(tauri::generate_handler![
            scan_codex,
            cleanup_mode,
            cleanup_preview,
            cleanup_execute,
            cleanup_result
        ])
        .build(tauri::generate_context!())
        .expect("Could not launch AgentBroom")
        .run(|app, event| {
            #[cfg(feature = "native-cleanup-test")]
            native_test::on_event(app, event);
            #[cfg(not(feature = "native-cleanup-test"))]
            let _ = (app, event);
        });
}
#[cfg(test)]
mod tests {
    #[cfg(not(feature = "native-cleanup-test"))]
    #[test]
    fn normal_desktop_cannot_execute_or_issue_write_plans() {
        assert_eq!(super::cleanup_mode(), "readOnly");
        assert_eq!(
            super::cleanup_execute().unwrap_err().code,
            "realStoreLocked"
        );
        assert!(super::cleanup_preview().is_err());
        assert!(super::cleanup_result().is_err());
    }
}
