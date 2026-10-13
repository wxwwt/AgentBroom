use agentbroom_core::{HEADER_LIMIT, Relation, preview, scan_fixture_store};
use std::{
    fs,
    path::{Path, PathBuf},
};
use tempfile::TempDir;
fn source() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/demo-store")
}

#[test]
fn cyclic_parent_graph_is_unknown_and_protected() {
    let d = fixture();
    let p = d.path().join("sessions/orbit.jsonl");
    let body = fs::read_to_string(&p).unwrap();
    let mut h: serde_json::Value = serde_json::from_str(body.lines().next().unwrap()).unwrap();
    h["relation"] = "spawned".into();
    h["parentId"] = "demo-orbit-tests".into();
    fs::write(&p, format!("{}\n", h)).unwrap();
    let i = scan_fixture_store(d.path()).unwrap();
    for id in ["demo-orbit", "demo-orbit-tests"] {
        let t = i.threads.iter().find(|t| t.id == id).unwrap();
        assert_eq!(t.relation, Relation::Unknown);
        assert!(t.protection_reasons.iter().any(|r| r.contains("Cyclic")));
        assert!(preview(&i, &vec![id.into()], &[], &i.source_fingerprint).is_err());
    }
}

#[test]
fn absolute_paths_are_rejected_before_access() {
    let d = fixture();
    let p = d.path().join("fixture-manifest.json");
    let mut m: serde_json::Value = serde_json::from_slice(&fs::read(&p).unwrap()).unwrap();
    m["files"][0]["path"] = "/never-open-this-fixture-path".into();
    fs::write(&p, m.to_string()).unwrap();
    assert_eq!(
        scan_fixture_store(d.path()).unwrap_err().code,
        "invalidFixture"
    );
}

#[test]
fn catalog_identity_keys_are_unambiguous() {
    let mut i = scan_fixture_store(&source()).unwrap();
    let base = i
        .threads
        .iter()
        .find(|t| t.id == "demo-notebook")
        .unwrap()
        .clone();
    i.threads = ["demo-a", "demo-b", "demo-a|demo-b"]
        .iter()
        .map(|id| {
            let mut t = base.clone();
            t.id = (*id).into();
            t
        })
        .collect();
    let catalog = agentbroom_core::web_catalog(i).unwrap();
    assert_eq!(catalog.previews.len(), 8);
    assert_eq!(
        catalog.previews["[\"demo-a|demo-b\"]"].selected_ids.len(),
        1
    );
    assert_eq!(
        catalog.previews["[\"demo-a\",\"demo-b\"]"]
            .selected_ids
            .len(),
        2
    );
}
fn copy_tree(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).unwrap();
    for entry in fs::read_dir(src).unwrap() {
        let e = entry.unwrap();
        if e.file_type().unwrap().is_dir() {
            copy_tree(&e.path(), &dst.join(e.file_name()))
        } else {
            fs::copy(e.path(), dst.join(e.file_name())).unwrap();
        }
    }
}
fn fixture() -> TempDir {
    let d = TempDir::new().unwrap();
    copy_tree(&source(), d.path());
    d
}
fn id(s: &str) -> Vec<String> {
    vec![s.into()]
}
fn fingerprint_files(root: &Path) -> Vec<(String, Vec<u8>)> {
    let mut rows = vec![];
    for e in fs::read_dir(root).unwrap() {
        let e = e.unwrap();
        if e.file_type().unwrap().is_dir() {
            rows.extend(fingerprint_files(&e.path()))
        } else {
            rows.push((
                e.path().to_string_lossy().into(),
                fs::read(e.path()).unwrap(),
            ))
        }
    }
    rows.sort();
    rows
}
#[test]
fn actual_measurement_segments_and_forks() {
    let i = scan_fixture_store(&source()).unwrap();
    let a = i.threads.iter().find(|t| t.id == "demo-atlas").unwrap();
    assert_eq!(a.files.len(), 2);
    assert_eq!(a.owned_descendant_ids, id("demo-atlas-helper"));
    assert!(
        a.protection_reasons
            .iter()
            .any(|r| r.contains("independent fork"))
    );
    assert!(!a.owned_descendant_ids.contains(&"demo-atlas-fork".into()));
    assert_eq!(
        i.totals.iter().map(|t| t.logical_bytes).sum::<u64>(),
        i.items.iter().map(|t| t.logical_bytes).sum::<u64>()
    );
    assert!(!i.execution_enabled);
}
#[test]
fn source_unchanged_and_preview_is_not_execution() {
    let d = fixture();
    let before = fingerprint_files(d.path());
    let i = scan_fixture_store(d.path()).unwrap();
    let p = preview(&i, &id("demo-orbit"), &[], &i.source_fingerprint).unwrap();
    assert_eq!(p.affected_ids, vec!["demo-orbit", "demo-orbit-tests"]);
    assert!(!p.execution_enabled);
    assert_eq!(before, fingerprint_files(d.path()));
}
#[test]
fn rejects_protected_unknown_kept_duplicate_and_stale() {
    let i = scan_fixture_store(&source()).unwrap();
    for s in ["demo-atlas", "demo-sprout", "missing", "demo-orbit-tests"] {
        assert!(preview(&i, &id(s), &[], &i.source_fingerprint).is_err())
    }
    assert!(
        preview(
            &i,
            &vec!["demo-orbit".into(), "demo-orbit".into()],
            &[],
            &i.source_fingerprint
        )
        .is_err()
    );
    assert!(
        preview(
            &i,
            &id("demo-orbit"),
            &id("demo-orbit"),
            &i.source_fingerprint
        )
        .is_err()
    );
    assert_eq!(
        preview(&i, &id("demo-orbit"), &[], "stale")
            .unwrap_err()
            .code,
        "staleSnapshot"
    );
}
#[test]
fn rejects_duplicate_kept_ids() {
    let i = scan_fixture_store(&source()).unwrap();
    assert!(
        preview(
            &i,
            &[],
            &vec!["demo-notebook".into(); 2],
            &i.source_fingerprint
        )
        .is_err()
    );
}
#[test]
fn oversized_and_compressed_are_protected() {
    let i = scan_fixture_store(&source()).unwrap();
    let unknown: Vec<_> = i
        .threads
        .iter()
        .filter(|t| t.relation == Relation::Unknown)
        .collect();
    assert_eq!(unknown.len(), 2);
    assert!(unknown.iter().all(|t| !t.protection_reasons.is_empty()));
}
#[test]
fn multi_megabyte_header_is_bounded_unknown() {
    let d = fixture();
    fs::write(
        d.path().join("sessions/oversized-header.jsonl"),
        vec![b'x'; HEADER_LIMIT * 64],
    )
    .unwrap();
    let i = scan_fixture_store(d.path()).unwrap();
    assert!(i.warnings.iter().any(|w| w.contains("bounded metadata")));
    assert!(
        i.threads
            .iter()
            .find(|t| t.files.contains(&"sessions/oversized-header.jsonl".into()))
            .unwrap()
            .protection_reasons
            .len()
            > 0
    );
}
#[test]
fn wrong_marker_and_parent_path_are_rejected() {
    let d = fixture();
    let p = d.path().join("fixture-manifest.json");
    let mut m: serde_json::Value = serde_json::from_slice(&fs::read(&p).unwrap()).unwrap();
    m["kind"] = "actual-agent-store".into();
    fs::write(&p, m.to_string()).unwrap();
    assert!(scan_fixture_store(d.path()).is_err());
    m["kind"] = "agentbroom-synthetic-fixture".into();
    m["files"][0]["path"] = "../outside".into();
    fs::write(&p, m.to_string()).unwrap();
    assert!(scan_fixture_store(d.path()).is_err());
}
#[test]
fn missing_parent_and_conflicting_segments_protect() {
    let d = fixture();
    for (path, field, value) in [
        ("sessions/orbit-tests.jsonl", "parentId", "demo-missing"),
        ("sessions/atlas-2.jsonl", "title", "Conflicting title"),
    ] {
        let p = d.path().join(path);
        let body = fs::read_to_string(&p).unwrap();
        let mut h: serde_json::Value = serde_json::from_str(body.lines().next().unwrap()).unwrap();
        h[field] = value.into();
        fs::write(p, format!("{}\n", h)).unwrap();
    }
    let i = scan_fixture_store(d.path()).unwrap();
    assert!(
        i.threads
            .iter()
            .find(|t| t.id == "demo-orbit-tests")
            .unwrap()
            .protection_reasons
            .iter()
            .any(|r| r.contains("parent"))
    );
    assert!(
        i.threads
            .iter()
            .find(|t| t.id == "demo-atlas")
            .unwrap()
            .protection_reasons
            .iter()
            .any(|r| r.contains("Conflicting"))
    );
}
#[cfg(unix)]
#[test]
fn links_are_rejected() {
    use std::os::unix::fs::symlink;
    let d = fixture();
    let p = d.path().join("sessions/orbit.jsonl");
    fs::remove_file(&p).unwrap();
    symlink(source().join("sessions/orbit.jsonl"), &p).unwrap();
    assert!(scan_fixture_store(d.path()).is_err());
    fs::remove_file(&p).unwrap();
    fs::hard_link(d.path().join("sessions/atlas-1.jsonl"), &p).unwrap();
    assert!(scan_fixture_store(d.path()).is_err());
}
