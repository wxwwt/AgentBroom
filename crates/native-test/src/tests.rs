use super::*;
fn sandbox(shared: bool) -> NativeSandbox {
    let binary = std::env::var_os("AGENTBROOM_TEST_CODEX_BIN")
        .expect("explicit official 0.162.1 test binary required");
    NativeSandbox::generate_case(Path::new(&binary), shared).unwrap()
}
fn confirm(p: &NativePlan) -> Confirmation {
    Confirmation {
        plan_id: p.plan_id.clone(),
        selected_ids: p.analysis.selected_ids.clone(),
        phrase: "DELETE".into(),
        acknowledged: true,
    }
}
#[test]
#[ignore = "requires an explicit official test runtime"]
fn native_desktop_subtree_receipt_and_idempotency() {
    let mut s = sandbox(false);
    let p = s.preview(&[ROOT.into()], &[]).unwrap();
    assert!(p.executable, "{:?}", p.blocked_reasons);
    assert_eq!(p.native_ids, vec![ROOT, CHILD]);
    assert_eq!(p.retained_ids, vec![FORK, KEEP]);
    assert!(!p.analysis.can_execute);
    let before = s.snapshot().unwrap();
    let c = confirm(&p);
    let receipt = s.execute(c.clone()).unwrap();
    assert_eq!(receipt.status, "verified", "{:?}", receipt);
    assert_eq!(receipt.removed.len(), 2);
    assert!(
        receipt.native_notifications_verified && receipt.files_verified && receipt.state_verified
    );
    assert_eq!(receipt.freed_bytes, None);
    assert_eq!(receipt.native_deleted_ids, vec![ROOT, CHILD]);
    assert_eq!(s.execute(c).unwrap().plan_id, receipt.plan_id);
    assert_eq!(s.receipt(&p.plan_id).unwrap().status, "verified");
    assert_eq!(s.delete_requests, 1);
    let after = s.snapshot().unwrap();
    assert_eq!(before.rows.get(FORK), after.rows.get(FORK));
    assert_eq!(before.rows.get(KEEP), after.rows.get(KEEP));
    assert!(!s.inventory().unwrap().threads.iter().any(|t| t.id == ROOT));
    let expected_files: BTreeMap<_, _> = before
        .files
        .iter()
        .filter(|(p, _)| {
            !p.ends_with(&format!("-{ROOT}.jsonl")) && !p.ends_with(&format!("-{CHILD}.jsonl"))
        })
        .map(|(p, v)| (p.clone(), v.clone()))
        .collect();
    assert_eq!(after.files, expected_files);
    assert!(!before.edges.is_empty());
    assert!(after.edges.is_empty());
    // A mirror refresh must remove deleted archived paths as well as active paths.
    let keep_path = s.home.join(format!(
        "sessions/2026/01/01/rollout-2026-01-01T00-00-00-{KEEP}.jsonl"
    ));
    let archived = s.home.join("archived_sessions");
    fs::create_dir(&archived).unwrap();
    let archive = archived.join(keep_path.file_name().unwrap());
    fs::copy(keep_path, &archive).unwrap();
    assert!(
        s.inventory()
            .unwrap()
            .items
            .iter()
            .any(|f| f.key.starts_with("archived_sessions/"))
    );
    fs::remove_file(archive).unwrap();
    assert!(
        !s.inventory()
            .unwrap()
            .items
            .iter()
            .any(|f| f.key.starts_with("archived_sessions/"))
    );
}
#[test]
#[ignore = "requires an explicit official test runtime"]
fn confirmation_stale_expiry_unknown_and_keep_reject_without_writes() {
    let mut s = sandbox(false);
    let p = s.preview(&[ROOT.into()], &[CHILD.into()]).unwrap();
    assert!(!p.executable && p.blocked_reasons.contains(&"keptRecord".into()));
    assert!(s.execute(confirm(&p)).is_err());
    let p = s.preview(&[ROOT.into()], &[]).unwrap();
    let mut c = confirm(&p);
    c.phrase = "delete".into();
    assert!(s.execute(c).is_err());
    let mut c = confirm(&p);
    c.acknowledged = false;
    assert!(s.execute(c).is_err());
    let mut c = confirm(&p);
    c.plan_id = "forged".into();
    assert!(s.execute(c).is_err());
    let mut c = confirm(&p);
    c.selected_ids = vec![KEEP.into()];
    assert!(s.execute(c).is_err());
    s.pending.as_mut().unwrap().created = Instant::now() - TTL - Duration::from_secs(1);
    assert_eq!(s.execute(confirm(&p)).unwrap_err().code, "planExpired");
    let p = s.preview(&[ROOT.into()], &[]).unwrap();
    let file = p.analysis.observed_files[0].key.clone();
    use std::io::Write;
    fs::OpenOptions::new()
        .append(true)
        .open(s.home.join(file))
        .unwrap()
        .write_all(b"\n")
        .unwrap();
    assert_eq!(s.execute(confirm(&p)).unwrap_err().code, "staleSnapshot");
    assert_eq!(s.delete_requests, 0);
}
#[test]
#[ignore = "requires an explicit official test runtime"]
fn external_reference_and_native_scope_mismatch_block() {
    let mut s = sandbox(true);
    let before = s.snapshot().unwrap();
    let p = s.preview(&[ROOT.into()], &[]).unwrap();
    assert!(!p.executable && p.blocked_reasons.contains(&"outsideDependency".into()));
    assert!(s.execute(confirm(&p)).is_err());
    assert_eq!(s.snapshot().unwrap(), before);
    let db = fs::read_dir(&s.home)
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.path())
        .find(|p| p.file_name().unwrap() == "state_5.sqlite")
        .unwrap();
    let connection = Connection::open(db).unwrap();
    connection
        .execute(
            "DELETE FROM thread_spawn_edges WHERE child_thread_id=?",
            [CHILD],
        )
        .unwrap();
    let p = s.preview(&[ROOT.into()], &[]).unwrap();
    assert!(p.blocked_reasons.contains(&"nativeScopeMismatch".into()));
    assert_eq!(s.delete_requests, 0);
}
#[test]
#[ignore = "requires an explicit official test runtime"]
fn native_rejection_is_not_success_and_db_change_revokes_plan() {
    let mut s = sandbox(false);
    let p = s.preview(&[ROOT.into()], &[]).unwrap();
    let db = fs::read_dir(&s.home)
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.path())
        .find(|p| p.file_name().unwrap() == "state_5.sqlite")
        .unwrap();
    Connection::open(db)
        .unwrap()
        .execute("UPDATE threads SET title='changed' WHERE id=?", [KEEP])
        .unwrap();
    assert_eq!(s.execute(confirm(&p)).unwrap_err().code, "staleSnapshot");
    assert_eq!(s.delete_requests, 0);
    let mut other = rpc::Rpc::start({
        let mut c = command(&s.binary, &s.temp.path().canonicalize().unwrap());
        c.args(["-c", "cli_auth_credentials_store=\"file\"", "-c"])
            .arg(format!("sqlite_home={}", json!(s.home.to_str().unwrap())))
            .args(["app-server", "--listen", "stdio://"]);
        c
    })
    .unwrap();
    // A resumed generated writer prevents deletion; no model turn or network request is started.
    other
        .request(
            "thread/resume",
            json!({"threadId":ROOT,"excludeTurns":true}),
        )
        .unwrap();
    let before = s.snapshot().unwrap();
    let p = s.preview(&[ROOT.into()], &[]).unwrap();
    let receipt = s.execute(confirm(&p)).unwrap();
    assert_ne!(receipt.status, "verified");
    assert_eq!(s.snapshot().unwrap(), before);
    assert!(s.server.is_none());
    let calls = s.delete_requests;
    assert_eq!(
        s.preview(&[ROOT.into()], &[]).unwrap_err().code,
        "runtimeStopped"
    );
    assert!(s.execute(confirm(&p)).is_ok()); // Cached failed receipt, no retry.
    assert_eq!(s.delete_requests, calls);
}
#[test]
fn default_api_has_no_data_root_input_and_confirms_strictly() {
    assert!(serde_json::from_value::<Confirmation>(json!({"planId":"x","selectedIds":[ROOT],"phrase":"DELETE","acknowledged":true,"root":"/real"})).is_err());
    assert!(NativeSandbox::generate(Path::new("codex")).is_err());
}
