#![cfg(unix)]
use agentbroom_core::{Relation, codex::scan_store, preview};
use serde_json::{Value, json};
use std::{
    fs,
    os::unix::fs::{MetadataExt, symlink},
    path::Path,
};
const ROOT: &str = "00000000-0000-4000-8000-000000000001";
const CHILD: &str = "00000000-0000-4000-8000-000000000002";
const FORK: &str = "00000000-0000-4000-8000-000000000003";
const GRAND: &str = "00000000-0000-4000-8000-000000000004";
fn store() -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix("agentbroom-test-")
        .tempdir_in("/private/tmp")
        .unwrap_or_else(|_| {
            tempfile::Builder::new()
                .prefix("agentbroom-test-")
                .tempdir_in("/tmp")
                .unwrap()
        })
}
fn write(root: &Path, name: &str, data: &[u8]) {
    let p = root.join(name);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, data).unwrap();
}
fn meta(id: &str, source: Value) -> Value {
    json!({"type":"session_meta","payload":{"id":id,"session_id":ROOT,"source":source,"cwd":"/synthetic/project","timestamp":"2026-01-01T00:00:00Z"}})
}
fn session(root: &Path, name: &str, v: Value) {
    let b = format!("{v}\n{{\"type\":\"response_item\",\"payload\":\"PRIVATE_BODY_SENTINEL\"}}\n");
    write(root, name, b.as_bytes());
}
fn child(id: &str, parent: &str) -> Value {
    let mut v = meta(
        id,
        json!({"subagent":{"thread_spawn":{"parent_thread_id":parent}}}),
    );
    v["payload"]["parent_thread_id"] = json!(parent);
    v
}
fn snapshots(root: &Path) -> Vec<(String, u64, u64, Vec<u8>)> {
    let mut v = vec![];
    fn walk(p: &Path, r: &Path, v: &mut Vec<(String, u64, u64, Vec<u8>)>) {
        for e in fs::read_dir(p).unwrap() {
            let p = e.unwrap().path();
            let m = fs::symlink_metadata(&p).unwrap();
            if m.is_dir() {
                walk(&p, r, v)
            } else if m.is_file() {
                v.push((
                    p.strip_prefix(r).unwrap().to_string_lossy().into(),
                    m.len(),
                    m.mtime() as u64,
                    fs::read(&p).unwrap(),
                ));
            }
        }
    }
    walk(root, root, &mut v);
    v.sort_by(|a, b| a.0.cmp(&b.0));
    v
}
#[test]
fn groups_logical_ids_not_session_ids_and_keeps_forks_independent() {
    let d = store();
    let r = d.path();
    session(r, "sessions/main-a.jsonl", meta(ROOT, json!("vscode")));
    session(r, "sessions/main-b.jsonl", meta(ROOT, json!("vscode")));
    let mut c = child(CHILD, ROOT);
    c["payload"]["forked_from_id"] = json!(ROOT);
    session(r, "sessions/child.jsonl", c);
    session(r, "sessions/grand.jsonl", child(GRAND, CHILD));
    let mut f = meta(FORK, json!("vscode"));
    f["payload"]["forked_from_id"] = json!(ROOT);
    f["payload"]["history_base"] = json!({"thread_id":ROOT,"end_ordinal_exclusive":3});
    session(r, "sessions/fork.jsonl", f);
    write(
        r,
        "session_index.jsonl",
        format!(
            "{}\n",
            json!({"id":ROOT,"thread_name":"Synthetic root","updated_at":"2026-02-01"})
        )
        .as_bytes(),
    );
    write(
        r,
        ".codex-global-state.json",
        format!(
            "{}",
            json!({"pinned-thread-ids":[ROOT],"secret_unused":"DO_NOT_EXPORT"})
        )
        .as_bytes(),
    );
    for name in [
        "state_5.sqlite",
        "state_5.sqlite-wal",
        "state_5.sqlite-shm",
        "auth.json",
        "config.toml",
    ] {
        write(r, name, b"DO_NOT_OPEN_BODY");
    }
    let before = snapshots(r);
    let inv = scan_store(r).unwrap();
    assert_eq!(before, snapshots(r));
    assert!(!inv.execution_enabled);
    assert_eq!(inv.threads.len(), 4);
    let root = inv.threads.iter().find(|t| t.id == ROOT).unwrap();
    let fork = inv.threads.iter().find(|t| t.id == FORK).unwrap();
    let c = inv.threads.iter().find(|t| t.id == CHILD).unwrap();
    assert_eq!(root.files.len(), 2);
    assert_eq!(c.relation, Relation::Spawned);
    assert_eq!(fork.relation, Relation::Fork);
    assert_eq!(root.owned_descendant_ids.len(), 2);
    assert!(!root.owned_descendant_ids.contains(&FORK.to_string()));
    assert_eq!(
        root.inclusive_bytes,
        root.own_bytes
            + inv
                .threads
                .iter()
                .filter(|t| t.id == CHILD || t.id == GRAND)
                .map(|t| t.own_bytes)
                .sum::<u64>()
    );
    assert!(root.protection_reasons.iter().any(|r| r.contains("Pinned")));
    assert!(
        root.protection_reasons
            .iter()
            .any(|r| r.contains("Shared history"))
    );
    assert_eq!(
        inv.totals.iter().map(|t| t.logical_bytes).sum::<u64>(),
        inv.items.iter().map(|i| i.logical_bytes).sum::<u64>()
    );
    assert_eq!(
        preview(&inv, &[], &[], &inv.source_fingerprint)
            .unwrap_err()
            .code,
        "inspectionOnly"
    );
    let out = serde_json::to_string(&inv).unwrap();
    assert!(!out.contains("PRIVATE_BODY_SENTINEL"));
    assert!(!out.contains("DO_NOT_EXPORT"));
    assert!(!out.contains("DO_NOT_OPEN_BODY"));
}
#[test]
fn missing_parent_cycles_and_conflicting_segments_stay_unresolved() {
    let d = store();
    let r = d.path();
    session(r, "sessions/a.jsonl", child(CHILD, GRAND));
    session(r, "sessions/b.jsonl", child(GRAND, CHILD));
    session(r, "sessions/root-a.jsonl", meta(ROOT, json!("cli")));
    let mut v = meta(ROOT, json!("cli"));
    v["payload"]["cwd"] = json!("/different");
    session(r, "sessions/root-b.jsonl", v);
    let inv = scan_store(r).unwrap();
    assert!(inv.threads.iter().all(|t| t.relation == Relation::Unknown));
    assert!(
        inv.threads
            .iter()
            .all(|t| t.owned_descendant_ids.is_empty())
    );
}
#[test]
fn all_subagent_kinds_require_explicit_parent_and_unrecognized_source_is_unknown() {
    let d = store();
    let r = d.path();
    session(r, "sessions/main.jsonl", meta(ROOT, json!("vscode")));
    let mut g = meta(CHILD, json!({"subagent":{"other":"guardian"}}));
    g["payload"]["parent_thread_id"] = json!(ROOT);
    session(r, "sessions/guardian.jsonl", g);
    session(
        r,
        "sessions/unknown.jsonl",
        meta(FORK, json!("future-unrecognized-source")),
    );
    session(
        r,
        "sessions/missing.jsonl",
        child(GRAND, "00000000-0000-4000-8000-999999999999"),
    );
    let i = scan_store(r).unwrap();
    assert_eq!(
        i.threads.iter().find(|t| t.id == CHILD).unwrap().relation,
        Relation::Spawned
    );
    assert_eq!(
        i.threads.iter().find(|t| t.id == FORK).unwrap().relation,
        Relation::Unknown
    );
    assert_eq!(
        i.threads.iter().find(|t| t.id == GRAND).unwrap().relation,
        Relation::Unknown
    );
}
#[test]
fn does_not_follow_directory_file_root_links_or_open_hardlinked_metadata() {
    let d = store();
    let r = d.path();
    let outside = store();
    session(outside.path(), "private.jsonl", meta(ROOT, json!("cli")));
    fs::create_dir_all(r.join("sessions")).unwrap();
    symlink(outside.path(), r.join("sessions/linked-dir")).unwrap();
    symlink(
        outside.path().join("private.jsonl"),
        r.join("sessions/link.jsonl"),
    )
    .unwrap();
    fs::hard_link(
        outside.path().join("private.jsonl"),
        r.join("sessions/hard.jsonl"),
    )
    .unwrap();
    let before = snapshots(outside.path());
    let i = scan_store(r).unwrap();
    assert_eq!(before, snapshots(outside.path()));
    assert_eq!(i.items.len(), 1);
    assert_eq!(i.threads[0].relation, Relation::Unknown);
    let link = r.join("root-link");
    symlink(outside.path(), &link).unwrap();
    assert!(scan_store(&link).is_err());
}
#[test]
fn bounded_headers_compressed_and_index_only_records_are_protected() {
    let d = store();
    let r = d.path();
    write(r, "sessions/huge.jsonl", &vec![b'x'; 300_000]);
    write(r, "archived_sessions/opaque.jsonl.zst", b"opaque");
    write(
        r,
        "session_index.jsonl",
        format!(
            "{}\n",
            json!({"id":ROOT,"thread_name":"Missing rollout","updated_at":"2026-01-01"})
        )
        .as_bytes(),
    );
    let inv = scan_store(r).unwrap();
    assert_eq!(inv.threads.len(), 3);
    assert!(
        inv.threads
            .iter()
            .all(|t| t.relation == Relation::Unknown && !t.protection_reasons.is_empty())
    );
    assert_eq!(
        inv.threads.iter().find(|t| t.id == ROOT).unwrap().own_bytes,
        0
    );
}
#[test]
fn allocated_hardlink_bytes_deduplicate_but_logical_lengths_do_not() {
    let d = store();
    let r = d.path();
    session(r, "sessions/a.jsonl", meta(ROOT, json!("cli")));
    write(r, "assets/data.bin", &vec![0u8; 32_000]);
    fs::hard_link(r.join("assets/data.bin"), r.join("assets/copy.bin")).unwrap();
    let i = scan_store(r).unwrap();
    let items: Vec<_> = i
        .items
        .iter()
        .filter(|i| i.key.starts_with("assets/"))
        .collect();
    assert_eq!(items.iter().map(|i| i.logical_bytes).sum::<u64>(), 64_000);
    assert_eq!(
        items
            .iter()
            .map(|i| i.allocated_bytes.unwrap())
            .sum::<u64>(),
        fs::metadata(r.join("assets/data.bin")).unwrap().blocks() * 512
    );
}
#[test]
fn physical_history_base_maps_segments_to_logical_owner_not_independent_forks() {
    let d = store();
    let r = d.path();
    const PHYSICAL: &str = "00000000-0000-4000-8000-000000000005";
    session(
        r,
        &format!("sessions/rollout-2026-01-01-{PHYSICAL}.jsonl"),
        meta(ROOT, json!("cli")),
    );
    let mut continuation = meta(ROOT, json!("cli"));
    continuation["payload"]["history_base"] = json!({"thread_id":PHYSICAL});
    session(r, "sessions/continuation.jsonl", continuation);
    let mut fork = meta(FORK, json!("cli"));
    fork["payload"]["history_base"] = json!({"thread_id":PHYSICAL});
    session(r, "sessions/fork.jsonl", fork);
    let i = scan_store(r).unwrap();
    let root = i.threads.iter().find(|t| t.id == ROOT).unwrap();
    let fork = i.threads.iter().find(|t| t.id == FORK).unwrap();
    assert_eq!(root.relation, Relation::Root);
    assert_eq!(root.files.len(), 2);
    assert_eq!(fork.relation, Relation::Fork);
    assert_eq!(fork.shared_history_from.as_deref(), Some(ROOT));
    assert!(
        root.protection_reasons
            .iter()
            .any(|r| r.contains("Shared history"))
    );
}
#[test]
fn deep_parent_chain_is_bounded_and_excess_nodes_are_unknown() {
    let d = store();
    let r = d.path();
    let ids: Vec<_> = (0..80)
        .map(|i| format!("00000000-0000-4000-8000-{i:012}"))
        .collect();
    session(r, "sessions/main.jsonl", meta(&ids[0], json!("cli")));
    for i in 1..80 {
        session(
            r,
            &format!("sessions/{i}.jsonl"),
            child(&ids[i], &ids[i - 1]),
        );
    }
    let inv = scan_store(r).unwrap();
    let root = inv.threads.iter().find(|t| t.id == ids[0]).unwrap();
    assert!(root.owned_descendant_ids.len() <= 64);
    let last = inv.threads.iter().find(|t| t.id == ids[79]).unwrap();
    assert_eq!(last.relation, Relation::Unknown);
    assert!(last.protection_reasons.iter().any(|r| r.contains("depth")));
}
#[test]
fn cross_category_hardlink_allocation_is_deterministic_and_coverage_is_explicit() {
    let d = store();
    let r = d.path();
    session(r, "sessions/main.jsonl", meta(ROOT, json!("cli")));
    write(r, "generated_images/z.bin", &vec![0u8; 8192]);
    fs::create_dir_all(r.join("worktrees")).unwrap();
    fs::hard_link(r.join("generated_images/z.bin"), r.join("worktrees/a.bin")).unwrap();
    symlink(r.join("generated_images/z.bin"), r.join("symlink.bin")).unwrap();
    let inv = scan_store(r).unwrap();
    assert!(inv.partial_coverage);
    assert_eq!(
        inv.items
            .iter()
            .find(|i| i.key == "worktrees/a.bin")
            .unwrap()
            .allocated_bytes,
        Some(0)
    );
    assert!(
        inv.items
            .iter()
            .find(|i| i.key == "generated_images/z.bin")
            .unwrap()
            .allocated_bytes
            .unwrap()
            > 0
    );
}

#[test]
fn compressed_metadata_groups_with_plain_children_without_exporting_body() {
    let dir = store();
    let root = dir.path();
    let bytes = format!(
        "{}\n{}\n",
        meta(ROOT, json!("cli")),
        json!({"type":"response_item","payload":"PRIVATE_BODY_SENTINEL"})
    );
    write(
        root,
        &format!("sessions/rollout-2026-01-01T00-00-00-{ROOT}.jsonl.zst"),
        &zstd::stream::encode_all(bytes.as_bytes(), 3).unwrap(),
    );
    session(
        root,
        &format!("sessions/rollout-2026-01-01T00-00-00-{CHILD}.jsonl"),
        child(CHILD, ROOT),
    );
    let inv = scan_store(root).unwrap();
    assert_eq!(inv.threads.len(), 2);
    assert_eq!(
        inv.threads
            .iter()
            .find(|t| t.id == ROOT)
            .unwrap()
            .owned_descendant_ids,
        vec![CHILD]
    );
    assert!(
        !serde_json::to_string(&inv)
            .unwrap()
            .contains("PRIVATE_BODY_SENTINEL")
    );
}
#[test]
fn compressed_oversized_and_bad_headers_remain_unknown() {
    for payload in [
        b"not a zstd frame".to_vec(),
        zstd::stream::encode_all(format!("{}\n", "x".repeat(512 * 1024)).as_bytes(), 3).unwrap(),
    ] {
        let dir = store();
        write(
            dir.path(),
            &format!("sessions/rollout-2026-01-01T00-00-00-{ROOT}.jsonl.zst"),
            &payload,
        );
        let inv = scan_store(dir.path()).unwrap();
        assert_eq!(inv.threads[0].relation, Relation::Unknown);
        assert!(!inv.execution_enabled);
    }
}
