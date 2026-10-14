//! Observation plans are never execution capabilities. No source I/O in this module.
use crate::{Activity, Category, DiskItem, InspectError, Inventory, Relation};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupAnalysis {
    pub schema_version: u32,
    pub source_kind: String,
    pub snapshot: String,
    pub selected_ids: Vec<String>,
    pub included_ids: Vec<String>,
    pub retained_ids: Vec<String>,
    pub outside_dependents: Vec<String>,
    pub observed_files: Vec<DiskItem>,
    pub executable_files: Vec<String>,
    pub observed_logical_bytes: u64,
    pub blocked_reasons: Vec<String>,
    pub completeness: String,
    pub can_execute: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupCandidate {
    pub id: String,
    pub selectable: bool,
}
pub fn candidates(inventory: &Inventory) -> Vec<CleanupCandidate> {
    inventory
        .threads
        .iter()
        .map(|t| CleanupCandidate {
            id: t.id.clone(),
            selectable: matches!(t.relation, Relation::Root | Relation::Fork)
                && !t.files.is_empty(),
        })
        .collect()
}
fn error(code: &str) -> InspectError {
    InspectError {
        code: code.into(),
        message: code.into(),
    }
}
/// A real analysis is deliberately not a SyntheticPlan and has no execution token.
pub fn plan(
    inventory: &Inventory,
    selected: &[String],
    kept: &[String],
) -> Result<CleanupAnalysis, InspectError> {
    build_plan(inventory, selected, kept, false)
}
pub fn reject_live_execution() -> Result<(), InspectError> {
    Err(error("realStoreLocked"))
}
fn safe_path(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 2048
        && !path.contains('\\')
        && path
            .split('/')
            .all(|p| !p.is_empty() && p != "." && p != "..")
}
/// The only scope planner. Synthetic execution calls this on its own scanned inventory.
/// `synthetic` is policy selection only; it cannot grant a filesystem capability.
pub(crate) fn build_plan(
    inventory: &Inventory,
    selected: &[String],
    kept: &[String],
    synthetic: bool,
) -> Result<CleanupAnalysis, InspectError> {
    if selected.is_empty()
        || selected.len() > 128
        || inventory.threads.len() > 20_000
        || inventory.items.len() > 250_000
    {
        return Err(error("invalidSelection"));
    }
    let selected_set: BTreeSet<_> = selected.iter().cloned().collect();
    if selected_set.len() != selected.len() {
        return Err(error("invalidSelection"));
    }
    let mut index = BTreeMap::new();
    for t in &inventory.threads {
        if t.id.is_empty() || t.id.len() > 128 || index.insert(t.id.clone(), t).is_some() {
            return Err(error("graphUnresolved"));
        }
    }
    for id in &selected_set {
        let Some(t) = index.get(id) else {
            return Err(error("invalidSelection"));
        };
        if !matches!(t.relation, Relation::Root | Relation::Fork) || t.files.is_empty() {
            return Err(error("invalidSelection"));
        }
    }
    let mut blocked = BTreeSet::new();
    if !synthetic {
        blocked.extend([
            "realStoreLocked".to_string(),
            "nativeEffectsUnverified".to_string(),
        ]);
    }
    if inventory.partial_coverage {
        blocked.insert("partialCoverage".into());
    }
    let mut children: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for t in &inventory.threads {
        if t.relation == Relation::Unknown {
            blocked.insert("graphUnresolved".into());
        }
        if t.relation == Relation::Spawned {
            if let Some(parent) = t.parent_id.as_ref().filter(|p| index.contains_key(*p)) {
                children
                    .entry(parent.clone())
                    .or_default()
                    .push(t.id.clone());
            } else {
                blocked.insert("graphUnresolved".into());
            }
            let mut chain = BTreeSet::new();
            let mut node = Some(t.id.clone());
            while let Some(id) = node {
                if !chain.insert(id.clone()) || chain.len() > 64 {
                    blocked.insert("graphUnresolved".into());
                    break;
                }
                let Some(item) = index.get(&id) else {
                    blocked.insert("graphUnresolved".into());
                    break;
                };
                node = if item.relation == Relation::Spawned {
                    item.parent_id.clone()
                } else {
                    None
                };
            }
        }
    }
    let mut included = selected_set.clone();
    let mut queue: Vec<_> = selected_set.iter().cloned().collect();
    while let Some(id) = queue.pop() {
        for child in children.get(&id).into_iter().flatten() {
            if included.insert(child.clone()) {
                queue.push(child.clone());
            }
        }
    }
    let kept: BTreeSet<_> = kept.iter().cloned().collect();
    let mut retained = BTreeSet::new();
    for id in &included {
        let t = index[id];
        if !synthetic
            || kept.contains(id)
            || !t.protection_reasons.is_empty()
            || matches!(t.activity, Activity::Active | Activity::Unknown)
            || t.relation == Relation::Unknown
        {
            retained.insert(id.clone());
        }
        if kept.contains(id) {
            blocked.insert("keptRecord".into());
        }
        if !t.protection_reasons.is_empty() {
            blocked.insert("protectedRecord".into());
        }
        if matches!(t.activity, Activity::Active | Activity::Unknown) {
            blocked.insert("activityUnverified".into());
        }
    }
    let mut outside = BTreeSet::new();
    for t in &inventory.threads {
        let mut sources: BTreeSet<&String> = t
            .fork_sources
            .iter()
            .chain(t.history_sources.iter())
            .collect();
        sources.extend(t.forked_from_id.iter());
        sources.extend(t.shared_history_from.iter());
        if t.relation == Relation::Fork {
            sources.extend(t.parent_id.iter());
        }
        for source in sources {
            if !index.contains_key(source) {
                blocked.insert("graphUnresolved".into());
            }
            if included.contains(source) && !included.contains(&t.id) {
                outside.insert(t.id.clone());
            }
        }
    }
    if !outside.is_empty() {
        blocked.insert("outsideDependency".into());
        retained.extend(outside.iter().cloned());
    }
    // All file ownership claims must be unique, including claims by unselected records.
    let mut owners: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for t in &inventory.threads {
        for file in &t.files {
            owners.entry(file.clone()).or_default().insert(t.id.clone());
        }
    }
    let mut measured = BTreeMap::new();
    for item in &inventory.items {
        if measured.insert(item.key.clone(), item).is_some() {
            blocked.insert("fileOwnershipUnverified".into());
        }
    }
    let mut files = BTreeSet::new();
    let mut executable = BTreeSet::new();
    for id in &included {
        for file in &index[id].files {
            files.insert(file.clone());
            if !retained.contains(id) {
                executable.insert(file.clone());
            }
            if !safe_path(file)
                || owners[file].len() != 1
                || measured
                    .get(file)
                    .is_none_or(|x| !matches!(x.category, Category::Sessions | Category::Archives))
            {
                blocked.insert("fileOwnershipUnverified".into());
            }
        }
    }
    let observed_files: Vec<_> = files
        .iter()
        .filter_map(|f| measured.get(f).map(|x| (*x).clone()))
        .collect();
    let observed_logical_bytes = observed_files
        .iter()
        .try_fold(0u64, |n, f| n.checked_add(f.logical_bytes))
        .ok_or_else(|| error("fileOwnershipUnverified"))?;
    if !blocked.is_empty() {
        executable.clear();
    }
    Ok(CleanupAnalysis {
        schema_version: 1,
        source_kind: inventory.source_kind.clone(),
        snapshot: inventory.source_fingerprint.clone(),
        selected_ids: selected_set.into_iter().collect(),
        included_ids: included.into_iter().collect(),
        retained_ids: retained.into_iter().collect(),
        outside_dependents: outside.into_iter().collect(),
        observed_files,
        executable_files: executable.into_iter().collect(),
        observed_logical_bytes,
        blocked_reasons: blocked.into_iter().collect(),
        completeness: if synthetic {
            "syntheticExact"
        } else {
            "observedOnly"
        }
        .into(),
        can_execute: false,
    })
}
