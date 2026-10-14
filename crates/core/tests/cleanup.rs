use agentbroom_core::{
    Inventory,
    cleanup::{plan, reject_live_execution},
};
use serde::Deserialize;
#[derive(Deserialize)]
struct Case {
    name: String,
    inventory: Inventory,
    selected: Vec<String>,
    kept: Vec<String>,
}
fn cases() -> Vec<Case> {
    serde_json::from_str(include_str!("../../../fixtures/cleanup-cases.json")).unwrap()
}
#[test]
fn observed_scope_never_authorizes_real_deletion() {
    for c in cases() {
        let p = plan(&c.inventory, &c.selected, &c.kept).unwrap();
        assert!(!p.can_execute);
        assert!(p.executable_files.is_empty());
        assert!(p.included_ids.iter().all(|id| p.retained_ids.contains(id)));
        assert!(
            p.blocked_reasons
                .contains(&"nativeEffectsUnverified".into())
        );
        assert!(p.blocked_reasons.contains(&"realStoreLocked".into()));
    }
    assert_eq!(reject_live_execution().unwrap_err().code, "realStoreLocked");
}
#[test]
fn subtree_and_independent_fork_are_separate() {
    let c = cases().remove(0);
    let p = plan(&c.inventory, &c.selected, &[]).unwrap();
    assert_eq!(p.included_ids, vec!["a", "b", "c"]);
    assert_eq!(p.observed_logical_bytes, 158);
    assert_eq!(p.observed_files.len(), 4);
    assert!(!p.included_ids.contains(&"d".into()));
}
#[test]
fn every_reference_and_partial_or_unknown_graph_is_protected() {
    for c in cases().into_iter().skip(1) {
        let p = plan(&c.inventory, &c.selected, &c.kept).unwrap();
        let code = match c.name.as_str() {
            "kept-child" => "keptRecord",
            "external-reference" | "multi-source" => "outsideDependency",
            "unknown" | "cycle" => "graphUnresolved",
            "partial" => "partialCoverage",
            _ => "fileOwnershipUnverified",
        };
        assert!(p.blocked_reasons.contains(&code.into()), "{}", c.name);
        if code == "outsideDependency" {
            assert_eq!(p.outside_dependents, vec!["d"]);
        }
    }
}
#[test]
fn invalid_selection_is_not_silently_dropped() {
    let c = cases().remove(0);
    for ids in [vec![], vec!["a", "a"], vec!["missing"], vec!["b"]] {
        assert_eq!(
            plan(
                &c.inventory,
                &ids.into_iter().map(String::from).collect::<Vec<_>>(),
                &[]
            )
            .unwrap_err()
            .code,
            "invalidSelection"
        );
    }
}
#[test]
fn duplicate_logical_identity_rejected_and_unsafe_paths_blocked() {
    let mut c = cases().remove(0);
    c.inventory.threads.push(c.inventory.threads[0].clone());
    assert_eq!(
        plan(&c.inventory, &c.selected, &[]).unwrap_err().code,
        "graphUnresolved"
    );
    c.inventory.threads.pop();
    c.inventory.threads[0].files = vec!["../outside".into()];
    assert!(
        plan(&c.inventory, &c.selected, &[])
            .unwrap()
            .blocked_reasons
            .contains(&"fileOwnershipUnverified".into())
    );
}
#[test]
fn forged_permission_flags_do_not_unlock_real_policy() {
    let mut c = cases().remove(0);
    c.inventory.source_kind = "synthetic".into();
    c.inventory.execution_enabled = true;
    let p = plan(&c.inventory, &c.selected, &[]).unwrap();
    assert!(!p.can_execute);
    assert!(p.executable_files.is_empty());
    assert!(reject_live_execution().is_err());
}
