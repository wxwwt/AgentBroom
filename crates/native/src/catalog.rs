//! Connect the read-only header inventory to native logical identities and spawn edges.
use crate::{
    Error, fail, locks,
    source::{Database, Stamp},
};
use agentbroom_core::{Activity, Inventory, Relation, Thread};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

pub(crate) fn inspect(
    root: &Path,
    db: &Database,
    files: &BTreeMap<String, Stamp>,
) -> Result<Inventory, Error> {
    let mut inventory = agentbroom_core::codex::scan_store(root)?;
    let mut paths = BTreeMap::new();
    for (id, row) in &db.rows {
        if !locks::uuid(id) {
            return Err(fail("nativeStateUnavailable"));
        }
        let path = row
            .get("rollout_path")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| fail("nativePathMismatch"))?;
        let Ok(key) = crate::source::rollout_key(root, path, files) else {
            continue;
        };
        if paths.insert(key.clone(), id.clone()).is_some() {
            return Err(fail("fileOwnershipUnverified"));
        }
        if inventory
            .threads
            .iter()
            .any(|t| locks::uuid(&t.id) && t.id != *id && t.files.contains(&key))
        {
            return Err(fail("fileOwnershipUnverified"));
        }
        // A budget-skipped header must not hide an identity already proven by the native index.
        let unresolved = inventory
            .threads
            .iter()
            .position(|t| !locks::uuid(&t.id) && t.files.contains(&key));
        let mut recovered = unresolved.map(|i| inventory.threads.remove(i));
        if let Some(t) = inventory.threads.iter_mut().find(|t| t.id == *id) {
            if !t.files.contains(&key) {
                t.files.push(key.clone());
            }
        } else {
            let mut t = recovered.take().unwrap_or_else(|| Thread {
                id: id.clone(),
                title: format!("Conversation · {}", &id[..8]),
                project: String::new(),
                relation: Relation::Unknown,
                activity: Activity::Unknown,
                updated_at: String::new(),
                parent_id: None,
                shared_history_from: None,
                files: vec![],
                segment_ids: vec![],
                own_bytes: 0,
                inclusive_bytes: 0,
                owned_descendant_ids: vec![],
                protection_reasons: vec![],
                source_type: String::new(),
                forked_from_id: None,
                fork_sources: vec![],
                history_sources: vec![],
                relation_evidence: vec![],
            });
            t.id = id.clone();
            t.files = vec![key.clone()];
            if let Some(title) = row.get("title").and_then(serde_json::Value::as_str) {
                t.title = title
                    .chars()
                    .filter(|c| !c.is_control())
                    .take(512)
                    .collect();
            }
            t.relation_evidence
                .push("Logical identity confirmed by the native read-only index".into());
            inventory.threads.push(t);
        }
    }
    let mut children: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (parent, child, status) in &db.edges {
        if matches!(status.as_str(), "open" | "closed") {
            children
                .entry(parent.clone())
                .or_default()
                .push(child.clone());
            if let Some(t) = inventory.threads.iter_mut().find(|t| t.id == *child) {
                t.parent_id = Some(parent.clone());
                t.relation = Relation::Spawned;
            }
        }
    }
    let sizes: BTreeMap<String, u64> = inventory
        .threads
        .iter()
        .map(|t| {
            (
                t.id.clone(),
                t.files
                    .iter()
                    .filter_map(|f| files.get(f))
                    .map(|s| s.length)
                    .sum(),
            )
        })
        .collect();
    for t in &mut inventory.threads {
        t.own_bytes = sizes[&t.id];
        let mut descendants = BTreeSet::new();
        let mut todo = children.get(&t.id).cloned().unwrap_or_default();
        while let Some(id) = todo.pop() {
            if id != t.id && descendants.insert(id.clone()) {
                todo.extend(children.get(&id).into_iter().flatten().cloned());
            }
        }
        t.inclusive_bytes = t.own_bytes
            + descendants
                .iter()
                .filter_map(|id| sizes.get(id))
                .sum::<u64>();
        t.owned_descendant_ids = descendants.into_iter().collect();
    }
    Ok(inventory)
}
pub(crate) fn lock_conflicts(
    db: &Database,
    ids: &[String],
    locked: &BTreeSet<String>,
    header_parents: &[(String, String)],
) -> Vec<String> {
    let mut scope: BTreeSet<String> = ids.iter().cloned().collect();
    let mut todo = ids.to_vec();
    // Locking a parent protects its descendants even when a descendant is selected directly.
    while let Some(id) = todo.pop() {
        let native = db
            .edges
            .iter()
            .filter(|(_, child, status)| {
                child == &id && matches!(status.as_str(), "open" | "closed")
            })
            .map(|(parent, _, _)| parent);
        let headers = header_parents
            .iter()
            .filter(|(_, child)| child == &id)
            .map(|(parent, _)| parent);
        for parent in native.chain(headers) {
            if scope.insert(parent.clone()) {
                todo.push(parent.clone());
            }
        }
    }
    scope
        .into_iter()
        .map(|id| id.to_ascii_lowercase())
        .filter(|id| locked.contains(id))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}
