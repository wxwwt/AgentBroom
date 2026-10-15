use super::*;
use std::{fs::OpenOptions, io::Write, time::SystemTime};
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
fn real_owner_rejects_keep_reference_recent_cancel_stale_and_bad_confirmations() {
    let mut f = Fixture::new(false, false);
    let before = stamps(&f.store.root).unwrap();
    let p = f.store.preview(&[ROOT.into()], &[CHILD.into()]).unwrap();
    assert!(!p.executable);
    assert!(p.blocked_reasons.contains(&"keptRecord".into()));
    fs::write(
        f.store.root.join(".codex-global-state.json"),
        serde_json::to_vec(&json!({"pinned-thread-ids":[ROOT]})).unwrap(),
    )
    .unwrap();
    let pinned = f.store.preview(&[ROOT.into()], &[]).unwrap();
    assert!(!pinned.executable && pinned.blocked_reasons.contains(&"protectedRecord".into()));
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
    assert!(!p.executable && p.blocked_reasons.contains(&"recentWriteProtected".into()));
    assert_eq!(stamps(&f.store.root).unwrap().len(), before.len());
    let mut f = Fixture::new(false, true);
    let p = f.store.preview(&[ROOT.into()], &[]).unwrap();
    assert!(!p.executable && p.blocked_reasons.contains(&"outsideDependency".into()));
    assert!(f.store.execute(confirmation(&p)).is_err());
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
        source_id: f.store.source_id(),
        expected_ids: p.native_ids.clone(),
        native_notifications_verified: false,
        native_deleted_ids: vec![],
        files_verified: false,
        state_verified: false,
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
    assert!(f.store.preview(&[orphan.into()], &[]).is_err());
    let p = f.store.preview(&[ROOT.into()], &[]).unwrap();
    let before = hash_file(&path, 1024 * 1024).unwrap();
    let result = f.store.execute(confirmation(&p)).unwrap();
    assert_eq!(result.status, "verified", "{result:?}");
    assert_eq!(hash_file(&path, 1024 * 1024).unwrap(), before);
}
#[test]
fn retained_prefix_allows_append_but_detects_existing_data_rewrite() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().canonicalize().unwrap().join("generated.jsonl");
    fs::write(&path, b"generated retained bytes\n").unwrap();
    let size = fs::metadata(&path).unwrap().len();
    let hash = source::hash_prefix(&path, size).unwrap();
    OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap()
        .write_all(b"new append\n")
        .unwrap();
    assert_eq!(source::hash_prefix(&path, size).unwrap(), hash);
    fs::write(&path, b"rewritten retained bytes\nnew append\n").unwrap();
    assert_ne!(source::hash_prefix(&path, size).unwrap(), hash);
}
#[test]
fn stale_stamp_cannot_be_rebound_to_a_rewritten_prefix() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    fs::create_dir(root.join("sessions")).unwrap();
    let path = root.join("sessions/retained.jsonl");
    fs::write(&path, b"original retained bytes\n").unwrap();
    let old = stamps(&root).unwrap()["sessions/retained.jsonl"].clone();
    fs::write(&path, b"rewritten retained bytes\nappended\n").unwrap();
    assert_eq!(
        source::checked_prefix(&path, old.length, &old)
            .unwrap_err()
            .code,
        "sourceChanged"
    );
    let new = stamps(&root).unwrap()["sessions/retained.jsonl"].clone();
    assert!(source::checked_prefix(&path, new.length, &new).is_ok());
}
#[test]
#[ignore = "explicit native runtime; only newly generated roots"]
fn dangling_unselected_fork_reference_still_blocks_execution() {
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
    assert!(!plan.executable);
    assert!(plan.blocked_reasons.contains(&"graphUnresolved".into()));
    assert!(f.store.execute(confirmation(&plan)).is_err());
}
