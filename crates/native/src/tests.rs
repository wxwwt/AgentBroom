use super::*;
use std::{fs::OpenOptions, io::Write, path::Path, time::SystemTime};
const ROOT: &str = "00000000-0000-4000-8000-000000000001";
const CHILD: &str = "00000000-0000-4000-8000-000000000002";
const KEEP: &str = "00000000-0000-4000-8000-000000000005";
struct Fixture {
    _temp: tempfile::TempDir,
    store: LiveCleanup,
}
impl Fixture {
    fn new(compressed: bool, reference: bool) -> Self {
        let binary = PathBuf::from(
            std::env::var_os("AGENTBROOM_TEST_CODEX_BIN")
                .expect("explicit official test runtime required"),
        )
        .canonicalize()
        .unwrap();
        let temp = tempfile::Builder::new()
            .prefix("agentbroom-live-contract-")
            .tempdir_in("/tmp")
            .unwrap();
        let root = temp.path().canonicalize().unwrap();
        let home = root.join("codex");
        let work = root.join("runtime");
        fs::create_dir_all(home.join("sessions/2026/01/01")).unwrap();
        fs::create_dir(&work).unwrap();
        fs::write(
            home.join(".codex-global-state.json"),
            b"{\"pinned-thread-ids\":[]}",
        )
        .unwrap();
        fs::write(
            home.join("config.toml"),
            "model = \"synthetic-model\"\n[features]\njs_repl = false\n",
        )
        .unwrap();
        for (id, parent) in [(ROOT, None), (CHILD, Some(ROOT)), (KEEP, None)] {
            let mut meta = json!({"id":id,"session_id":id,"timestamp":"2026-01-01T00:00:00Z","cwd":work,"originator":"agentbroom-generated-contract","cli_version":"0.162.1","source":parent.map(|p|json!({"subagent":{"thread_spawn":{"parent_thread_id":p,"depth":1}}})).unwrap_or(json!("cli")),"model_provider":"agentbroom_offline"});
            if let Some(p) = parent {
                meta["parent_thread_id"] = json!(p);
            }
            if reference && id == KEEP {
                meta["forked_from_id"] = json!(ROOT);
                meta["forked_from_ordinal_exclusive"] = json!(1);
            }
            let bytes=format!("{}\n{}\n",json!({"timestamp":"2026-01-01T00:00:00Z","type":"session_meta","payload":meta}),json!({"timestamp":"2026-01-01T00:00:01Z","type":"event_msg","payload":{"type":"user_message","message":"Synthetic native contract only","kind":"plain"}})).into_bytes();
            let mut file = home.join(format!(
                "sessions/2026/01/01/rollout-2026-01-01T00-00-00-{id}.jsonl"
            ));
            if compressed && id == CHILD {
                file.set_extension("jsonl.zst");
                fs::write(file, zstd::stream::encode_all(&bytes[..], 3).unwrap()).unwrap();
            } else {
                fs::write(file, bytes).unwrap();
            }
        }
        let mut rpc =
            rpc::Rpc::start(source::native_command(&binary, &home, &work).unwrap()).unwrap();
        for id in [ROOT, CHILD, KEEP] {
            rpc.request("thread/read", json!({"threadId":id,"includeTurns":false}))
                .unwrap();
        }
        drop(rpc);
        // Persist initialized synthetic schemas, matching a pre-existing compatible store.
        // This setup-only checkpoint never targets a real database.
        for name in [
            "state_5.sqlite",
            "goals_1.sqlite",
            "logs_2.sqlite",
            "memories_1.sqlite",
            "queue_1.sqlite",
        ] {
            let db = rusqlite::Connection::open(home.join(name)).unwrap();
            db.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
                .unwrap();
        }
        for p in stamps(&home).unwrap().keys() {
            fs::File::options()
                .write(true)
                .open(home.join(p))
                .unwrap()
                .set_times(
                    fs::FileTimes::new()
                        .set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(1767225600)),
                )
                .unwrap();
        }
        let store = LiveCleanup {
            root_identity: identity(&home).unwrap(),
            root: home,
            binary,
            receipts: root.join("receipts"),
            pending: None,
            closed: false,
            runtime_contract: None,
        };
        Self { _temp: temp, store }
    }
}
fn confirmation(p: &NativePlan) -> Confirmation {
    Confirmation {
        plan_id: p.plan_id.clone(),
        selected_ids: p.analysis.selected_ids.clone(),
        phrase: "DELETE".into(),
        acknowledged: true,
        kept: vec![],
    }
}
#[test]
#[ignore = "explicit native runtime; only newly generated roots"]
fn real_owner_generated_plain_and_compressed_confirm_verify_and_persist() {
    for compressed in [false, true] {
        let mut f = Fixture::new(compressed, false);
        let before = stamps(&f.store.root).unwrap();
        let db_before = database(&f.store.root).unwrap();
        let p = f.store.preview(&[ROOT.into()], &[]).unwrap();
        assert!(p.executable, "{:?}", p.blocked_reasons);
        assert!(!p.test_only);
        assert_eq!(p.native_ids, vec![ROOT, CHILD]);
        assert_eq!(before, stamps(&f.store.root).unwrap());
        assert_eq!(db_before.rows, database(&f.store.root).unwrap().rows);
        let c = confirmation(&p);
        let r = f.store.execute(c.clone()).unwrap();
        assert_eq!(r.status, "verified", "{r:?}");
        assert_eq!(r.removed.len(), 2);
        assert!(r.files_verified && r.state_verified && r.native_notifications_verified);
        assert_eq!(r.freed_bytes, None);
        let after = stamps(&f.store.root).unwrap();
        assert_eq!(after.len(), 1);
        let kept = before.iter().find(|(p, _)| p.contains(KEEP)).unwrap();
        assert_eq!(after.get(kept.0), Some(kept.1));
        assert_eq!(
            db_before.rows.get(KEEP),
            database(&f.store.root).unwrap().rows.get(KEEP)
        );
        assert_eq!(f.store.execute(c).unwrap().plan_id, r.plan_id);
        f.store.pending = None;
        assert_eq!(f.store.receipt(&p.plan_id).unwrap().status, "verified");
    }
}
#[test]
#[ignore = "explicit native runtime; only newly generated roots"]
fn manual_policy_preserves_confirm_guards_and_uses_notices_for_pins_references_and_recent_writes() {
    let mut f = Fixture::new(false, false);
    let before = stamps(&f.store.root).unwrap();
    let p = f.store.preview(&[ROOT.into()], &[CHILD.into()]).unwrap();
    assert!(!p.executable);
    assert!(p.blocked_reasons.contains(&"lockedConversation".into()));
    fs::write(
        f.store.root.join(".codex-global-state.json"),
        serde_json::to_vec(&json!({"pinned-thread-ids":[ROOT]})).unwrap(),
    )
    .unwrap();
    let pinned = f.store.preview(&[ROOT.into()], &[]).unwrap();
    assert!(pinned.executable, "Codex pins are not manual locks");
    fs::write(
        f.store.root.join(".codex-global-state.json"),
        b"{\"pinned-thread-ids\":[]}",
    )
    .unwrap();
    let p = f.store.preview(&[ROOT.into()], &[]).unwrap();
    let mut c = confirmation(&p);
    c.phrase = "delete".into();
    assert!(f.store.execute(c).is_err());
    let mut c = confirmation(&p);
    c.kept = vec![CHILD.into()];
    assert!(f.store.execute(c).is_err());
    f.store.cancel(&p.plan_id);
    assert!(f.store.execute(confirmation(&p)).is_err());
    let p = f.store.preview(&[ROOT.into()], &[]).unwrap();
    f.store.pending.as_mut().unwrap().created = Instant::now() - TTL - Duration::from_secs(1);
    assert_eq!(
        f.store.execute(confirmation(&p)).unwrap_err().code,
        "planExpired"
    );
    let p = f.store.preview(&[ROOT.into()], &[]).unwrap();
    let path = f.store.root.join(&p.analysis.observed_files[0].key);
    OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap()
        .write_all(b"\n")
        .unwrap();
    assert!(f.store.execute(confirmation(&p)).is_err());
    let p = f.store.preview(&[ROOT.into()], &[]).unwrap();
    assert!(p.executable);
    assert!(p.warnings.iter().any(|w| w.contains("recent file writes")));
    assert_eq!(stamps(&f.store.root).unwrap().len(), before.len());
    let mut f = Fixture::new(false, true);
    let p = f.store.preview(&[ROOT.into()], &[]).unwrap();
    assert!(p.executable, "{:?}", p.blocked_reasons);
    assert!(!p.analysis.outside_dependents.is_empty());
    assert!(
        p.warnings
            .iter()
            .any(|w| w.contains("reference this scope"))
    );
}
#[test]
#[ignore = "explicit native runtime; only newly generated roots"]
fn unresolved_durable_intent_cannot_retry_after_restart_and_schema_changes_block() {
    let mut f = Fixture::new(false, false);
    let p = f.store.preview(&[ROOT.into()], &[]).unwrap();
    let r = Receipt {
        plan_id: p.plan_id.clone(),
        selected_ids: vec![ROOT.into()],
        test_only: false,
        status: "unknown".into(),
        removed: vec![],
        remaining: vec![],
        observed_removed_logical_bytes: 0,
        freed_bytes: None,
        error: None,
        concurrent_changes: vec![],
        warnings: vec![],
        source_id: f.store.source_id(),
        expected_ids: p.native_ids.clone(),
        native_notifications_verified: false,
        native_deleted_ids: vec![],
        files_verified: false,
        state_verified: false,
        related_verified: false,
    };
    f.store.write_receipt(&r).unwrap();
    f.store.pending = None;
    assert_eq!(f.store.history().unwrap()[0].status, "unknown");
    assert_eq!(f.store.execute(confirmation(&p)).unwrap().status, "unknown");
    assert_eq!(
        f.store.preview(&[ROOT.into()], &[]).unwrap_err().code,
        "unresolvedPreviousOperation"
    );
    let db = rusqlite::Connection::open(f.store.root.join("state_5.sqlite")).unwrap();
    db.execute("DELETE FROM _sqlx_migrations WHERE version=59", [])
        .unwrap();
    assert_eq!(
        f.store.preview(&[ROOT.into()], &[]).unwrap_err().code,
        "nativeSchemaUnsupported"
    );
}
#[test]
fn input_and_receipt_paths_fail_closed() {
    assert!(valid_id("../outside").is_err());
    assert!(valid_id("").is_err());
    assert!(serde_json::from_value::<Confirmation>(json!({"planId":"x","selectedIds":[ROOT],"phrase":"DELETE","acknowledged":true,"root":"/real"})).is_err());
}
#[test]
fn native_environment_contains_only_explicit_values() {
    let temp = tempfile::tempdir().unwrap();
    let c = source::native_command(
        Path::new("/official/codex"),
        Path::new("/generated/root"),
        temp.path(),
    )
    .unwrap();
    let vars: Vec<_> = c
        .get_envs()
        .map(|(k, _)| k.to_string_lossy().into_owned())
        .collect();
    assert!(!vars.contains(&"OPENAI_API_KEY".into()));
    assert!(!vars.contains(&"SSLKEYLOGFILE".into()));
    assert!(vars.contains(&"CODEX_HOME".into()));
    assert!(!c.get_args().any(|a| a == "thread/resume"));
}

#[test]
fn application_operation_lock_serializes_multiple_instances() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path().canonicalize().unwrap().join("receipts");
    let first = source::operation_lock(&dir).unwrap();
    assert_eq!(
        source::operation_lock(&dir).unwrap_err().code,
        "cleanupBusy"
    );
    drop(first);
    assert!(source::operation_lock(&dir).is_ok());
}
#[test]
#[ignore = "explicit native runtime; only newly generated roots"]
fn fully_parsed_unrelated_orphan_stays_retained_and_does_not_disable_other_roots() {
    let mut f = Fixture::new(false, false);
    let orphan = "00000000-0000-4000-8000-000000000006";
    let missing = "00000000-0000-4000-8000-000000000009";
    let path = f.store.root.join(format!(
        "sessions/2026/01/01/rollout-2026-01-01T00-00-00-{orphan}.jsonl"
    ));
    fs::write(&path,format!("{}\n",json!({"timestamp":"2026-01-01T00:00:00Z","type":"session_meta","payload":{"id":orphan,"session_id":orphan,"timestamp":"2026-01-01T00:00:00Z","cwd":f.store.root,"originator":"agentbroom-generated-test","cli_version":"0.162.1","source":{"subagent":{"thread_spawn":{"parent_thread_id":missing,"depth":1}}},"parent_thread_id":missing,"model_provider":"agentbroom_offline"}}))).unwrap();
    let work = f._temp.path().join("orphan-runtime");
    fs::create_dir(&work).unwrap();
    let mut rpc =
        rpc::Rpc::start(source::native_command(&f.store.binary, &f.store.root, &work).unwrap())
            .unwrap();
    rpc.request(
        "thread/read",
        json!({"threadId":orphan,"includeTurns":false}),
    )
    .unwrap();
    drop(rpc);
    let p = f.store.preview(&[ROOT.into()], &[]).unwrap();
    assert!(p.executable, "{:?}", p.blocked_reasons);
    assert!(p.retained_ids.contains(&orphan.into()));
    let orphan_plan = f.store.preview(&[orphan.into()], &[]).unwrap();
    assert!(orphan_plan.executable, "{:?}", orphan_plan.blocked_reasons);
    let p = f.store.preview(&[ROOT.into()], &[]).unwrap();
    let before = hash_file(&path, 1024 * 1024).unwrap();
    let result = f.store.execute(confirmation(&p)).unwrap();
    assert_eq!(result.status, "verified", "{result:?}");
    assert_eq!(hash_file(&path, 1024 * 1024).unwrap(), before);
}
#[test]
fn retained_prefix_allows_append_but_detects_existing_data_rewrite() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    fs::create_dir(root.join("sessions")).unwrap();
    let path = root.join("sessions/retained.jsonl");
    fs::write(&path, b"generated retained bytes\n").unwrap();
    let observed = stamps(&root).unwrap()["sessions/retained.jsonl"].clone();
    // Reproduce an append between the global scan and per-file content capture.
    OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap()
        .write_all(b"before capture\n")
        .unwrap();
    let proof = source::capture_retained_prefix(&path, &observed).unwrap();
    OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap()
        .write_all(b"during deletion\n")
        .unwrap();
    assert!(source::verify_retained_prefix(&path, &proof).unwrap());
    // Growth must never hide a rewrite of the original prefix.
    fs::write(
        &path,
        b"rewritten retained bytes\nbefore capture\nduring deletion\nplus growth\n",
    )
    .unwrap();
    assert!(!source::verify_retained_prefix(&path, &proof).unwrap());
}
#[test]
fn frozen_retained_prefix_cannot_be_rebound_to_rewritten_truncated_or_replaced_data() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    fs::create_dir(root.join("sessions")).unwrap();
    let path = root.join("sessions/retained.jsonl");
    fs::write(&path, b"original retained bytes\n").unwrap();
    let observed = stamps(&root).unwrap()["sessions/retained.jsonl"].clone();
    let proof = source::capture_retained_prefix(&path, &observed).unwrap();
    fs::write(&path, b"rewritten retain bytes!\n").unwrap();
    assert!(!source::verify_retained_prefix(&path, &proof).unwrap());
    fs::write(&path, b"truncated").unwrap();
    assert!(source::verify_retained_prefix(&path, &proof).is_err());
    assert!(source::capture_retained_prefix(&path, &observed).is_err());
    fs::rename(&path, root.join("saved-original")).unwrap();
    fs::write(&path, b"original retained bytes\n").unwrap();
    assert!(source::verify_retained_prefix(&path, &proof).is_err());
    assert!(source::capture_retained_prefix(&path, &observed).is_err());
    fs::remove_file(&path).unwrap();
    assert!(source::verify_retained_prefix(&path, &proof).is_err());
    std::os::unix::fs::symlink(root.join("saved-original"), &path).unwrap();
    assert!(source::verify_retained_prefix(&path, &proof).is_err());
}
#[test]
fn retained_content_capture_and_verification_tolerate_a_continuous_append_writer() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    };
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    fs::create_dir(root.join("sessions")).unwrap();
    let path = root.join("sessions/retained.jsonl");
    fs::write(&path, vec![b'x'; 8 * 1024 * 1024]).unwrap();
    let observed = stamps(&root).unwrap()["sessions/retained.jsonl"].clone();
    let running = Arc::new(AtomicBool::new(true));
    let writes = Arc::new(AtomicUsize::new(0));
    let run = running.clone();
    let count = writes.clone();
    let writer_path = path.clone();
    let worker = std::thread::spawn(move || {
        let mut f = OpenOptions::new().append(true).open(writer_path).unwrap();
        while run.load(Ordering::SeqCst) {
            f.write_all(b"concurrent append\n").unwrap();
            count.fetch_add(1, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(1));
        }
    });
    while writes.load(Ordering::SeqCst) == 0 {
        std::thread::yield_now();
    }
    let result = (|| {
        let proof = source::capture_retained_prefix(&path, &observed)?;
        source::verify_retained_prefix(&path, &proof)
    })();
    running.store(false, Ordering::SeqCst);
    worker.join().unwrap();
    assert!(result.unwrap());
    assert!(writes.load(Ordering::SeqCst) > 1);
}
#[test]
#[ignore = "explicit native runtime; only newly generated roots"]
fn unrelated_dangling_fork_does_not_globally_block_execution() {
    let mut f = Fixture::new(false, false);
    let path = f.store.root.join(format!(
        "sessions/2026/01/01/rollout-2026-01-01T00-00-00-{KEEP}.jsonl"
    ));
    let content = fs::read_to_string(&path).unwrap();
    let mut rows: Vec<Value> = content
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    rows[0]["payload"]["forked_from_id"] = json!("00000000-0000-4000-8000-000000000009");
    rows[0]["payload"]["forked_from_ordinal_exclusive"] = json!(1);
    fs::write(
        path,
        rows.iter()
            .map(|row| format!("{row}\n"))
            .collect::<String>(),
    )
    .unwrap();
    let plan = f.store.preview(&[ROOT.into()], &[]).unwrap();
    assert!(plan.executable, "{:?}", plan.blocked_reasons);
    let result = f.store.execute(confirmation(&plan)).unwrap();
    assert_eq!(result.status, "verified", "{result:?}");
}

#[test]
fn valid_toml_documents_are_parsed_and_external_stores_remain_unsupported() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    fs::write(
        root.join("config.toml"),
        "model = \"synthetic-model\"\n[features]\njs_repl = false\n",
    )
    .unwrap();
    assert!(source::check_config(&root).is_ok());
    fs::write(
        root.join("config.toml"),
        "sqlite_home = \"/somewhere-else\"\n",
    )
    .unwrap();
    assert_eq!(
        source::check_config(&root).unwrap_err().code,
        "externalDatabaseUnsupported"
    );
    fs::write(root.join("config.toml"), "model = [").unwrap();
    assert_eq!(
        source::check_config(&root).unwrap_err().code,
        "configUnsupported"
    );
}
#[test]
fn manual_locks_are_persistent_source_scoped_and_fail_closed_on_corruption() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let mut store = LiveCleanup {
        root_identity: identity(&root).unwrap(),
        root: root.clone(),
        binary: root.join("not-run"),
        receipts: root.join("receipts"),
        pending: None,
        closed: false,
        runtime_contract: None,
    };
    assert!(store.locked_ids().unwrap().is_empty());
    store.set_locked(ROOT, true).unwrap();
    assert_eq!(store.locked_ids().unwrap(), vec![ROOT]);
    assert!(store.history().unwrap().is_empty());
    let mut other = LiveCleanup {
        root_identity: (0, 0),
        root: root.clone(),
        binary: root.join("not-run"),
        receipts: store.receipts.clone(),
        pending: None,
        closed: false,
        runtime_contract: None,
    };
    assert!(other.locked_ids().unwrap().is_empty());
    other.root_identity = store.root_identity;
    assert_eq!(other.locked_ids().unwrap(), vec![ROOT]);
    other.set_locked(ROOT, false).unwrap();
    assert!(store.locked_ids().unwrap().is_empty());
    assert!(store.set_locked("../../outside", true).is_err());
    fs::write(
        store
            .receipts
            .join("locks")
            .join(format!("{}.json", store.source_id())),
        b"broken",
    )
    .unwrap();
    assert!(store.locked_ids().is_err());
    assert!(store.set_locked(ROOT, false).is_err());
}
#[test]
fn locks_cover_descendants_and_block_parent_deletion_without_blocking_independent_forks() {
    let db = Database {
        storage_profile: "state-v59",
        rows: BTreeMap::new(),
        edges: vec![(ROOT.into(), CHILD.into(), "closed".into())],
    };
    assert_eq!(
        catalog::lock_conflicts(&db, &[CHILD.into()], &BTreeSet::from([ROOT.into()]), &[]),
        vec![ROOT]
    );
    assert_eq!(
        catalog::lock_conflicts(
            &db,
            &[ROOT.into(), CHILD.into()],
            &BTreeSet::from([CHILD.into()]),
            &[]
        ),
        vec![CHILD]
    );
    assert!(
        catalog::lock_conflicts(&db, &[KEEP.into()], &BTreeSet::from([ROOT.into()]), &[])
            .is_empty()
    );
}
#[test]
#[ignore = "explicit native runtime; only newly generated roots"]
fn persisted_locks_block_scope_and_revalidate_across_instances_then_unlock_allows_delete() {
    let mut f = Fixture::new(false, false);
    f.store.set_locked(ROOT, true).unwrap();
    let p = f.store.preview(&[CHILD.into()], &[]).unwrap();
    assert!(!p.executable);
    assert_eq!(p.locked_ids, vec![ROOT]);
    assert!(f.store.execute(confirmation(&p)).is_err());
    f.store.set_locked(ROOT, false).unwrap();
    f.store.set_locked(CHILD, true).unwrap();
    let p = f.store.preview(&[ROOT.into()], &[]).unwrap();
    assert!(!p.executable);
    assert_eq!(p.locked_ids, vec![CHILD]);
    f.store.set_locked(CHILD, false).unwrap();
    let p = f.store.preview(&[ROOT.into()], &[]).unwrap();
    assert!(p.executable);
    // Another instance changes persisted locks without invalidating this process's pending plan.
    locks::write(
        &f.store.receipts,
        &f.store.source_id(),
        BTreeSet::from([CHILD.into()]),
    )
    .unwrap();
    assert!(f.store.execute(confirmation(&p)).is_err());
    assert_eq!(stamps(&f.store.root).unwrap().len(), 3);
    f.store.set_locked(CHILD, false).unwrap();
    let p = f.store.preview(&[ROOT.into()], &[]).unwrap();
    let result = f.store.execute(confirmation(&p)).unwrap();
    assert_eq!(result.status, "verified", "{result:?}");
}

#[test]
#[ignore = "explicit native runtime; only newly generated roots"]
fn compression_after_native_indexing_resolves_stale_plain_paths_and_verifies_deletion() {
    let mut f = Fixture::new(false, false);
    for path in stamps(&f.store.root).unwrap().keys() {
        let path = f.store.root.join(path);
        let compressed = path.with_extension("jsonl.zst");
        fs::write(
            &compressed,
            zstd::stream::encode_all(&fs::read(&path).unwrap()[..], 3).unwrap(),
        )
        .unwrap();
        fs::remove_file(path).unwrap();
    }
    // The native database intentionally retains its original .jsonl rollout_path.
    assert!(
        database(&f.store.root).unwrap().rows[ROOT]["rollout_path"]
            .as_str()
            .unwrap()
            .ends_with(".jsonl")
    );
    let p = f.store.preview(&[ROOT.into()], &[]).unwrap();
    assert!(p.executable, "{:?}", p.blocked_reasons);
    assert!(
        p.analysis
            .observed_files
            .iter()
            .all(|f| f.key.ends_with(".jsonl.zst"))
    );
    let result = f.store.execute(confirmation(&p)).unwrap();
    assert_eq!(result.status, "verified", "{result:?}");
    assert_eq!(stamps(&f.store.root).unwrap().len(), 1);
}

#[test]
fn header_ancestry_and_uuid_case_do_not_bypass_manual_locks() {
    let db = Database {
        storage_profile: "state-v59",
        rows: BTreeMap::new(),
        edges: vec![],
    };
    let parent = "abcdef00-0000-4000-8000-000000000001";
    let headers = vec![(parent.to_ascii_uppercase(), CHILD.into())];
    assert_eq!(
        catalog::lock_conflicts(
            &db,
            &[CHILD.into()],
            &BTreeSet::from([parent.into()]),
            &headers
        ),
        vec![parent]
    );
    assert_eq!(
        catalog::lock_conflicts(
            &db,
            &[parent.to_ascii_uppercase()],
            &BTreeSet::from([parent.into()]),
            &[]
        ),
        vec![parent]
    );
}

#[test]
fn native_index_recovers_unread_headers_without_creating_fake_protection() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    fs::create_dir(root.join("sessions")).unwrap();
    let path = root.join("sessions/generated.jsonl");
    fs::write(&path, b"unread synthetic header\n").unwrap();
    let db = Database {
        storage_profile: "state-v59",
        rows: BTreeMap::from([(
            ROOT.into(),
            json!({"rollout_path":path,"title":"Synthetic recovered identity"}),
        )]),
        edges: vec![],
    };
    let inv = catalog::inspect(&root, &db, &stamps(&root).unwrap()).unwrap();
    assert_eq!(inv.threads.len(), 1);
    assert_eq!(inv.threads[0].id, ROOT);
    assert_eq!(inv.threads[0].files, vec!["sessions/generated.jsonl"]);
    assert_eq!(inv.threads[0].own_bytes, 24);
    assert!(catalog::lock_conflicts(&db, &[ROOT.into()], &BTreeSet::new(), &[]).is_empty());
}

#[test]
#[ignore = "explicit native runtime; only newly generated roots"]
fn copied_fork_reference_is_a_notice_and_native_outcome_preserves_the_fork() {
    let mut f = Fixture::new(false, true);
    let before = stamps(&f.store.root).unwrap();
    let keep = before.keys().find(|p| p.contains(KEEP)).unwrap().clone();
    let keep_hash = hash_file(&f.store.root.join(&keep), 1024 * 1024).unwrap();
    let p = f.store.preview(&[ROOT.into()], &[]).unwrap();
    assert!(p.executable);
    let result = f.store.execute(confirmation(&p)).unwrap();
    assert_eq!(
        hash_file(&f.store.root.join(keep), 1024 * 1024).unwrap(),
        keep_hash
    );
    assert!(database(&f.store.root).unwrap().rows.contains_key(KEEP));
    if result.status != "verified" {
        // A native reference/writer rejection is allowed, but it must leave the selected data intact.
        assert_eq!(result.status, "verificationFailed", "{result:?}");
        assert!(result.removed.is_empty(), "{result:?}");
        assert!(database(&f.store.root).unwrap().rows.contains_key(ROOT));
        assert_eq!(stamps(&f.store.root).unwrap().len(), before.len());
    }
}

#[test]
fn batch_selection_deduplicates_spawned_children_and_rejects_duplicate_inputs() {
    let db = Database {
        storage_profile: "state-v59",
        rows: BTreeMap::from([
            (ROOT.into(), json!({})),
            (CHILD.into(), json!({})),
            (KEEP.into(), json!({})),
        ]),
        edges: vec![(ROOT.into(), CHILD.into(), "closed".into())],
    };
    let (roots, ids) = db
        .selection(&[CHILD.into(), ROOT.into(), KEEP.into()])
        .unwrap();
    assert_eq!(roots, vec![ROOT, KEEP]);
    assert_eq!(ids, vec![ROOT, CHILD, KEEP]);
    assert!(db.selection(&[ROOT.into(), ROOT.into()]).is_err());
}
#[test]
#[ignore = "explicit native runtime; generated multi-selection only"]
fn batch_multiple_roots_and_parent_child_selection_confirm_once_and_verify() {
    for selected in [
        vec![ROOT.into(), CHILD.into()],
        vec![CHILD.into(), KEEP.into()],
    ] {
        let mut f = Fixture::new(false, false);
        let p = f.store.preview(&selected, &[]).unwrap();
        assert!(p.executable, "{:?}", p.blocked_reasons);
        assert_eq!(p.native_ids.len(), 2);
        let expected_roots = if selected.contains(&ROOT.to_string()) {
            1
        } else {
            2
        };
        assert_eq!(p.root_ids.len(), expected_roots);
        let r = f.store.execute(confirmation(&p)).unwrap();
        assert_eq!(r.status, "verified", "{r:?}");
        assert_eq!(r.native_deleted_ids.len(), 2);
        assert_eq!(stamps(&f.store.root).unwrap().len(), 1);
        assert!(
            f.store
                .operations()
                .unwrap()
                .iter()
                .any(|e| e.action == "cleanup" && e.status == "verified")
        );
    }
}
#[test]
#[ignore = "explicit native runtime; generated multi-selection only"]
fn batch_manual_lock_blocks_all_roots_before_any_native_delete() {
    let mut f = Fixture::new(false, false);
    f.store.set_locks(&[KEEP.into()], true).unwrap();
    let p = f.store.preview(&[ROOT.into(), KEEP.into()], &[]).unwrap();
    assert!(!p.executable);
    assert_eq!(p.locked_ids, vec![KEEP]);
    assert!(f.store.execute(confirmation(&p)).is_err());
    assert_eq!(stamps(&f.store.root).unwrap().len(), 3);
}

#[test]
#[ignore = "explicit native runtime; only newly generated roots"]
fn broken_activity_log_does_not_block_locks_preview_or_durable_cleanup() {
    let mut f = Fixture::new(false, false);
    let dir = f.store.event_dir();
    fs::write(&dir, b"generated unavailable directory").unwrap();
    assert_eq!(f.store.set_locks(&[ROOT.into()], true).unwrap(), vec![ROOT]);
    assert_eq!(f.store.locked_ids().unwrap(), vec![ROOT]);
    assert!(f.store.set_locks(&[ROOT.into()], false).unwrap().is_empty());
    let p = f.store.preview(&[ROOT.into()], &[]).unwrap();
    assert!(p.executable);
    assert!(p.warnings.iter().any(|w| w == events::HISTORY_WARNING));
    let c = confirmation(&p);
    let receipt = f.store.execute(c.clone()).unwrap();
    assert_eq!(receipt.status, "verified", "{receipt:?}");
    assert_eq!(receipt.warnings, vec![events::HISTORY_WARNING]);
    assert_eq!(f.store.execute(c).unwrap().warnings, receipt.warnings);
    assert_eq!(f.store.receipt(&p.plan_id).unwrap().status, "verified");
}

fn seed_related(f: &Fixture) {
    for (file, statements) in [
        (
            "state_5.sqlite",
            vec![
                "INSERT INTO thread_attachments(id,thread_id,attachment_type,identity_key,payload,created_at) VALUES(?1,?1,'synthetic',?1,'{}',1)",
                "INSERT INTO thread_dynamic_tools(thread_id,position,name,description,input_schema) VALUES(?1,0,'synthetic','generated','{}')",
            ],
        ),
        (
            "goals_1.sqlite",
            vec![
                "INSERT INTO thread_goals(thread_id,goal_id,objective,status,created_at_ms,updated_at_ms) VALUES(?1,?1,'generated','active',1,1)",
                "INSERT INTO thread_goal_continuation_deferrals VALUES(?1)",
            ],
        ),
        (
            "logs_2.sqlite",
            vec![
                "INSERT INTO logs(ts,ts_nanos,level,target,thread_id,feedback_log_body) VALUES(CAST(strftime('%s','now') AS INTEGER),0,'INFO','synthetic',?1,'generated')",
            ],
        ),
        (
            "memories_1.sqlite",
            vec![
                "INSERT INTO stage1_outputs(thread_id,source_updated_at,raw_memory,rollout_summary,generated_at) VALUES(?1,1,'generated','generated',1)",
                "INSERT INTO jobs(kind,job_key,status,retry_remaining) VALUES('memory_stage1',?1,'done',0)",
            ],
        ),
        (
            "queue_1.sqlite",
            vec!["INSERT INTO queued_items VALUES(?1,?1,'{}',1,1,1)"],
        ),
    ] {
        let db = rusqlite::Connection::open(f.store.root.join(file)).unwrap();
        for sql in statements {
            for id in [ROOT, CHILD, KEEP] {
                db.execute(sql, [id]).unwrap();
            }
        }
        if file == "state_5.sqlite"
            && db
                .query_row(
                    "SELECT count(*) FROM sqlite_master WHERE name='thread_read_receipts'",
                    [],
                    |r| r.get::<_, i64>(0),
                )
                .unwrap()
                == 1
        {
            for id in [ROOT, CHILD, KEEP] {
                db.execute(
                    "INSERT INTO thread_read_receipts VALUES(?1,NULL,'generated',NULL)",
                    [id],
                )
                .unwrap();
                db.execute(
                    "INSERT INTO guardian_review_feedback VALUES(?1,?1,X'010203')",
                    [id],
                )
                .unwrap();
            }
        }
    }
}
#[test]
#[ignore = "explicit native runtime; generated associated metadata only"]
fn native_delete_verifies_all_related_tables_and_preserves_unselected_metadata() {
    let mut f = Fixture::new(false, false);
    seed_related(&f);
    let profile = database(&f.store.root).unwrap().storage_profile;
    let before = related::snapshot(&f.store.root, profile).unwrap();
    let p = f.store.preview(&[ROOT.into()], &[]).unwrap();
    assert_eq!(p.storage_profile, profile);
    assert!(p.runtime_version.starts_with("0."));
    let r = f.store.execute(confirmation(&p)).unwrap();
    let after = related::snapshot(&f.store.root, profile).unwrap();
    for (table, owners) in &before.rows {
        assert_eq!(
            owners.get(KEEP),
            after.rows[table].get(KEEP),
            "retained {table}"
        );
        assert!(!after.rows[table].contains_key(ROOT), "residual {table}");
        assert!(!after.rows[table].contains_key(CHILD), "residual {table}");
        assert!(!owners[ROOT].is_empty(), "empty test seed {table}");
    }
    assert_eq!(r.status, "verified", "{r:?}");
    assert!(r.related_verified);
    for id in [ROOT, CHILD] {
        assert!(after.revisions[id] > before.revisions[id]);
    }
    assert_eq!(after.revisions[KEEP], before.revisions[KEEP]);
    assert!(database(&f.store.root).unwrap().rows.contains_key(KEEP));
}
#[test]
fn storage_profiles_accept_verified_shapes_and_reject_unknown_or_modified_history() {
    assert_eq!(
        source::storage_profile(
            59,
            "b2a35ca432edfeb0545fcbabc63e3e1abb5f4b9de0146e33de8a8a8f9a87cb5e"
        )
        .unwrap(),
        "state-v59"
    );
    assert_eq!(
        source::storage_profile(
            61,
            "466e7cd4abaee882d5d7fc3048128a6794fb6fbe35b466ede2d31d9e41318050"
        )
        .unwrap(),
        "state-v61"
    );
    assert!(source::storage_profile(61, "modified").is_err());
    assert!(source::storage_profile(62, "future").is_err());
}

#[test]
#[ignore = "explicit native runtime; generated source and diagnostic version wrapper only"]
fn compatible_runtime_is_not_rejected_for_a_new_version_label() {
    use std::os::unix::fs::PermissionsExt;
    let mut f = Fixture::new(false, false);
    let official = f.store.binary.clone();
    let wrapper = f._temp.path().join("version-wrapper");
    // Change only --version; actual initialize/read/delete protocol stays official.
    fs::write(&wrapper,format!("#!/bin/sh\nif [ \"$1\" = --version ]; then printf '%s\\n' 'codex-cli 99.0.0'; else exec '{}' \"$@\"; fi\n",official.display())).unwrap();
    fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o700)).unwrap();
    f.store.binary = wrapper.canonicalize().unwrap();
    let files_before = stamps(&f.store.root).unwrap();
    let rows_before = database(&f.store.root).unwrap().rows;
    let p = f.store.preview(&[ROOT.into()], &[]).unwrap();
    assert_eq!(p.runtime_version, "99.0.0");
    assert!(p.executable);
    assert_eq!(stamps(&f.store.root).unwrap(), files_before);
    assert_eq!(database(&f.store.root).unwrap().rows, rows_before);
    // A mismatched cached storage contract is refused before source initialization.
    let hash = hash_file(&f.store.binary, 400 * 1024 * 1024).unwrap();
    let other = if p.storage_profile == "state-v59" {
        "state-v61"
    } else {
        "state-v59"
    };
    f.store.runtime_contract = Some((hash, other.into()));
    assert_eq!(
        f.store.preview(&[ROOT.into()], &[]).unwrap_err().code,
        "runtimeStorageMismatch"
    );
    assert_eq!(database(&f.store.root).unwrap().rows, rows_before);
    assert_eq!(stamps(&f.store.root).unwrap(), files_before);
}

#[test]
#[ignore = "explicit native runtime; generated expired and current log rows only"]
fn expired_retained_logs_are_reported_as_startup_maintenance_and_deletion_still_verifies() {
    let mut f = Fixture::new(false, false);
    seed_related(&f);
    let profile = database(&f.store.root).unwrap().storage_profile;
    let before = related::snapshot(&f.store.root, profile).unwrap();
    let expired_id = {
        let db = rusqlite::Connection::open(f.store.root.join("logs_2.sqlite")).unwrap();
        db.execute("INSERT INTO logs(ts,ts_nanos,level,target,thread_id,feedback_log_body) VALUES(1,0,'INFO','generated-expired',?1,'generated expired log')",[KEEP]).unwrap();
        db.last_insert_rowid()
    };
    let p = f.store.preview(&[ROOT.into()], &[]).unwrap();
    // Preview stays read-only, including the old retained log row.
    {
        let db = rusqlite::Connection::open_with_flags(
            f.store.root.join("logs_2.sqlite"),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        assert_eq!(
            db.query_row("SELECT count(*) FROM logs WHERE id=?1", [expired_id], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
    }
    let r = f.store.execute(confirmation(&p)).unwrap();
    assert_eq!(r.status, "verified", "{r:?}");
    assert!(r.related_verified);
    assert!(
        r.warnings
            .iter()
            .any(|w| w.starts_with("Codex startup changed log records"))
    );
    let after = related::snapshot(&f.store.root, profile).unwrap();
    // All current retained metadata remains, including its non-expired log row.
    assert!(related::verify(&before, &after, &[ROOT.into(), CHILD.into()]).is_empty());
    let db = rusqlite::Connection::open_with_flags(
        f.store.root.join("logs_2.sqlite"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    assert_eq!(
        db.query_row("SELECT count(*) FROM logs WHERE id=?1", [expired_id], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(f.store.receipt(&p.plan_id).unwrap().warnings, r.warnings);
}

#[test]
fn rejected_preflight_has_an_explicit_phase_and_activity_without_execution_receipt() {
    let t = tempfile::tempdir().unwrap();
    let root = t.path().canonicalize().unwrap();
    let file = root.join("synthetic-keep.jsonl");
    fs::write(&file, b"synthetic only").unwrap();
    let mut store = LiveCleanup {
        root_identity: identity(&root).unwrap(),
        root: root.clone(),
        binary: root.join("must-never-run"),
        receipts: root.join("receipts"),
        pending: None,
        closed: false,
        runtime_contract: None,
    };
    let error = store
        .execute(Confirmation {
            plan_id: "synthetic-plan".into(),
            selected_ids: vec![ROOT.into()],
            phrase: "DELETE".into(),
            acknowledged: true,
            kept: vec![],
        })
        .unwrap_err();
    assert_eq!(error.code, "invalidPlan");
    assert_eq!(error.execution_phase, ExecutionPhase::NotStarted);
    assert_eq!(fs::read(&file).unwrap(), b"synthetic only");
    assert!(store.history().unwrap().is_empty());
    let events = store.operations().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].status, "blocked");
    assert_eq!(events[0].error.as_deref(), Some("invalidPlan"));
    assert_eq!(events[0].ids, vec![ROOT]);
}
#[test]
#[ignore = "explicit native runtime; only newly generated roots"]
fn related_preflight_failure_keeps_generated_threads_and_records_reason() {
    let mut f = Fixture::new(false, false);
    let p = f.store.preview(&[ROOT.into()], &[]).unwrap();
    let before = stamps(&f.store.root).unwrap();
    fs::rename(
        f.store.root.join("memories_1.sqlite"),
        f.store.root.join("memories_1.sqlite.saved"),
    )
    .unwrap();
    let err = f.store.execute(confirmation(&p)).unwrap_err();
    assert_eq!(err.execution_phase, ExecutionPhase::NotStarted);
    assert!(f.store.history().unwrap().is_empty());
    assert_eq!(stamps(&f.store.root).unwrap(), before);
    assert!(
        f.store
            .operations()
            .unwrap()
            .iter()
            .any(|e| e.action == "cleanup"
                && e.status == "blocked"
                && e.error.as_deref() == Some(err.code.as_str()))
    );
}

#[test]
#[ignore = "explicit official runtime; only newly generated roots"]
fn native_cleanup_verifies_while_an_unselected_conversation_continuously_appends() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    };
    let mut f = Fixture::new(false, false);
    let p = f.store.preview(&[ROOT.into()], &[]).unwrap();
    let kept = stamps(&f.store.root)
        .unwrap()
        .keys()
        .find(|p| p.contains(KEEP))
        .unwrap()
        .clone();
    let path = f.store.root.join(&kept);
    let original = fs::read(&path).unwrap();
    let running = Arc::new(AtomicBool::new(true));
    let count = Arc::new(AtomicUsize::new(0));
    let run = running.clone();
    let writes = count.clone();
    let worker = std::thread::spawn(move || {
        let mut file = OpenOptions::new().append(true).open(path).unwrap();
        let line = format!(
            "{}\n",
            json!({"timestamp":"2026-01-01T00:00:02Z","type":"event_msg","payload":{"type":"user_message","message":"Synthetic concurrent append","kind":"plain"}})
        );
        while run.load(Ordering::SeqCst) {
            file.write_all(line.as_bytes()).unwrap();
            writes.fetch_add(1, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(5));
        }
    });
    while count.load(Ordering::SeqCst) == 0 {
        std::thread::yield_now();
    }
    let result = f.store.execute(confirmation(&p));
    running.store(false, Ordering::SeqCst);
    worker.join().unwrap();
    let receipt = result.unwrap();
    assert_eq!(receipt.status, "verified", "{receipt:?}");
    assert!(receipt.files_verified && receipt.state_verified && receipt.related_verified);
    assert!(receipt.concurrent_changes.contains(&kept));
    assert!(count.load(Ordering::SeqCst) > 1);
    let remaining = fs::read(f.store.root.join(&kept)).unwrap();
    assert!(remaining.starts_with(&original));
    assert!(database(&f.store.root).unwrap().rows.contains_key(KEEP));
    assert!(receipt.native_deleted_ids.contains(&ROOT.into()));
    assert!(receipt.native_deleted_ids.contains(&CHILD.into()));
}

#[test]
#[ignore = "explicit official runtime; only newly generated roots"]
fn native_cleanup_does_not_verify_a_retained_prefix_rewrite_after_intent() {
    use std::{
        os::unix::fs::FileExt,
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
    };
    let mut f = Fixture::new(false, false);
    let p = f.store.preview(&[ROOT.into()], &[]).unwrap();
    let kept = stamps(&f.store.root)
        .unwrap()
        .keys()
        .find(|p| p.contains(KEEP))
        .unwrap()
        .clone();
    let path = f.store.root.join(&kept);
    let bytes = fs::read(&path).unwrap();
    let offset = bytes
        .windows(b"Synthetic native contract only".len())
        .position(|s| s == b"Synthetic native contract only")
        .unwrap() as u64;
    let intent = f.store.receipts.join(format!("{}.json", p.plan_id));
    let running = Arc::new(AtomicBool::new(true));
    let run = running.clone();
    let worker = std::thread::spawn(move || {
        let started = Instant::now();
        while run.load(Ordering::SeqCst) && started.elapsed() < Duration::from_secs(45) {
            if intent.exists() {
                let file = OpenOptions::new().write(true).open(&path).unwrap();
                file.write_all_at(b"R", offset).unwrap();
                // A rewrite accompanied by growth must not be mistaken for pure append.
                OpenOptions::new()
                    .append(true)
                    .open(&path)
                    .unwrap()
                    .write_all(b"\n")
                    .unwrap();
                return true;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        false
    });
    let result = f.store.execute(confirmation(&p));
    running.store(false, Ordering::SeqCst);
    assert!(worker.join().unwrap());
    let receipt = result.unwrap();
    assert_ne!(receipt.status, "verified");
    assert!(!receipt.files_verified);
    assert!(
        receipt
            .error
            .as_deref()
            .unwrap()
            .contains("retainedFileChangedOrMissing"),
        "{receipt:?}"
    );
    assert!(database(&f.store.root).unwrap().rows.contains_key(KEEP));
}
