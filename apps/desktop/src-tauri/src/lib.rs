use agentbroom_core::{InspectError, Inventory};
fn error(code: &str) -> InspectError {
    InspectError {
        code: code.into(),
        message: code.into(),
    }
}
mod native_session {
    use super::*;
    use agentbroom_native::{Confirmation, NativePlan, Receipt};
    #[cfg(feature = "native-cleanup-test")]
    type Store = agentbroom_native_test::NativeSandbox;
    #[cfg(not(feature = "native-cleanup-test"))]
    type Store = agentbroom_native::LiveCleanup;
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicU8, Ordering},
    };
    #[derive(Clone, Default)]
    pub struct Session {
        sandbox: Arc<Mutex<Option<Store>>>,
        // 0=running, 1=waiting for the current worker and cleanup, 2=ready to exit.
        shutdown: Arc<AtomicU8>,
    }
    impl Session {
        #[cfg(test)]
        pub(crate) fn stopped_for_test() -> Self {
            let session = Self::default();
            session.shutdown.store(1, Ordering::SeqCst);
            session
        }
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
    async fn with_store<T: Send + 'static>(
        state: Session,
        action: impl FnOnce(&mut Store) -> Result<T, InspectError> + Send + 'static,
    ) -> Result<T, InspectError> {
        tauri::async_runtime::spawn_blocking(move || {
            state.running()?;
            let mut guard = state.sandbox.try_lock().map_err(|_| error("cleanupBusy"))?;
            state.running()?;
            if guard.is_none() {
                #[cfg(feature = "native-cleanup-test")]
                {
                    let binary = std::env::var_os("AGENTBROOM_TEST_CODEX_BIN")
                        .ok_or_else(|| error("testRuntimeRequired"))?;
                    *guard = Some(Store::generate(std::path::Path::new(&binary))?);
                }
                #[cfg(not(feature = "native-cleanup-test"))]
                {
                    *guard = Some(Store::discover()?);
                }
            }
            action(guard.as_mut().unwrap())
        })
        .await
        .map_err(|_| error("cleanupWorkerFailed"))?
    }
    #[tauri::command]
    pub async fn scan_codex(state: tauri::State<'_, Session>) -> Result<Inventory, InspectError> {
        with_store(state.inner().clone(), |s| s.inventory()).await
    }
    #[tauri::command]
    pub async fn conversation_locks(
        state: tauri::State<'_, Session>,
    ) -> Result<Vec<String>, InspectError> {
        #[cfg(feature = "native-cleanup-test")]
        {
            let _ = state;
            Err(error("testLocksUnavailable"))
        }
        #[cfg(not(feature = "native-cleanup-test"))]
        {
            with_store(state.inner().clone(), |s| s.locked_ids()).await
        }
    }
    #[tauri::command]
    pub async fn set_conversation_lock(
        state: tauri::State<'_, Session>,
        thread_id: String,
        locked: bool,
    ) -> Result<Vec<String>, InspectError> {
        #[cfg(feature = "native-cleanup-test")]
        {
            let _ = (state, thread_id, locked);
            Err(error("testLocksUnavailable"))
        }
        #[cfg(not(feature = "native-cleanup-test"))]
        {
            with_store(state.inner().clone(), move |s| {
                s.set_locked(&thread_id, locked)
            })
            .await
        }
    }

    #[tauri::command]
    pub async fn cleanup_preview(
        state: tauri::State<'_, Session>,
        selected_ids: Vec<String>,
        kept: Vec<String>,
    ) -> Result<NativePlan, InspectError> {
        with_store(state.inner().clone(), move |s| {
            s.preview(&selected_ids, &kept)
        })
        .await
    }
    #[tauri::command]
    pub async fn cleanup_execute(
        state: tauri::State<'_, Session>,
        confirmation: Confirmation,
    ) -> Result<Receipt, InspectError> {
        with_store(state.inner().clone(), move |s| s.execute(confirmation)).await
    }
    #[tauri::command]
    pub async fn cleanup_result(
        state: tauri::State<'_, Session>,
        plan_id: String,
    ) -> Result<Receipt, InspectError> {
        with_store(state.inner().clone(), move |s| s.receipt(&plan_id)).await
    }
    #[tauri::command]
    pub async fn cleanup_cancel(
        state: tauri::State<'_, Session>,
        plan_id: String,
    ) -> Result<(), InspectError> {
        if plan_id.len() > 128 {
            return Err(error("invalidPlan"));
        }
        with_store(state.inner().clone(), move |s| {
            s.cancel(&plan_id);
            Ok(())
        })
        .await
    }
    #[tauri::command]
    pub async fn cleanup_history(
        state: tauri::State<'_, Session>,
    ) -> Result<Vec<Receipt>, InspectError> {
        with_store(state.inner().clone(), |s| s.history()).await
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
use native_session::{
    cleanup_cancel, cleanup_execute, cleanup_history, cleanup_preview, cleanup_result,
    conversation_locks, scan_codex, set_conversation_lock,
};
#[tauri::command]
fn cleanup_mode() -> &'static str {
    if cfg!(feature = "native-cleanup-test") {
        "syntheticNativeTest"
    } else {
        "native"
    }
}
fn app_context<R: tauri::Runtime>() -> tauri::Context<R> {
    tauri::generate_context!()
}
pub fn run() {
    let builder = tauri::Builder::default();
    let builder = builder.manage(native_session::Session::default());
    builder
        .invoke_handler(tauri::generate_handler![
            scan_codex,
            conversation_locks,
            set_conversation_lock,
            cleanup_mode,
            cleanup_preview,
            cleanup_execute,
            cleanup_result,
            cleanup_cancel,
            cleanup_history
        ])
        .build(app_context())
        .expect("Could not launch AgentBroom")
        .run(|app, event| {
            native_session::on_event(app, event);
        });
}

#[cfg(all(test, not(feature = "native-cleanup-test")))]
mod ipc_contract_tests {
    use super::*;
    fn request(cmd: &str, body: serde_json::Value) -> tauri::webview::InvokeRequest {
        tauri::webview::InvokeRequest {
            cmd: cmd.into(),
            callback: tauri::ipc::CallbackFn(0),
            error: tauri::ipc::CallbackFn(1),
            url: "tauri://localhost".parse().unwrap(),
            body: tauri::ipc::InvokeBody::Json(body),
            headers: Default::default(),
            invoke_key: tauri::test::INVOKE_KEY.into(),
        }
    }
    #[test]
    fn lock_commands_pass_the_real_generated_capability_before_the_stopped_worker_guard() {
        // Real generated app context binds build.rs command permissions and capabilities/main.json.
        // A stopped worker ensures these requests cannot discover or read a real source.
        let app = tauri::test::mock_builder()
            .manage(native_session::Session::stopped_for_test())
            .invoke_handler(tauri::generate_handler![
                conversation_locks,
                set_conversation_lock
            ])
            .build(app_context())
            .unwrap();
        let window = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .unwrap();
        for (command, body) in [
            ("conversation_locks", serde_json::json!({})),
            (
                "set_conversation_lock",
                serde_json::json!({"threadId":"00000000-0000-4000-8000-000000000001","locked":true}),
            ),
        ] {
            tauri::test::assert_ipc_response(
                &window,
                request(command, body),
                Err::<InspectError, _>(error("cleanupClosing")),
            );
        }
    }
    #[test]
    fn real_mode_and_invalid_mutation_payloads_use_actual_ipc_without_a_source() {
        let session = native_session::Session::default();
        let app = tauri::test::mock_builder()
            .manage(session)
            .invoke_handler(tauri::generate_handler![
                cleanup_mode,
                cleanup_execute,
                cleanup_cancel
            ])
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        let window = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .unwrap();
        tauri::test::assert_ipc_response(
            &window,
            request("cleanup_mode", serde_json::json!({})),
            Ok::<_, String>("native".to_string()),
        );
        let result = tauri::test::get_ipc_response(
            &window,
            request(
                "cleanup_execute",
                serde_json::json!({"confirmation":{"planId":"never-issued","selectedIds":[],"phrase":"DELETE","acknowledged":true,"root":"/not-accepted"}}),
            ),
        );
        assert!(result.is_err());
        let result = tauri::test::get_ipc_response(
            &window,
            request(
                "cleanup_cancel",
                serde_json::json!({"planId":"x".repeat(129)}),
            ),
        );
        assert!(result.is_err());
    }
}
