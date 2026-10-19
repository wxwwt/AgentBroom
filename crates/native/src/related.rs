//! Read-only verification of thread-owned metadata. Never performs SQL mutation.
use crate::{Error, fail, source};
use rusqlite::{Connection, OpenFlags, types::ValueRef};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};
#[derive(Default, Debug, Clone)]
pub(crate) struct Snapshot {
    // Content digests only; neither transcripts nor metadata bodies enter receipts.
    pub rows: BTreeMap<String, BTreeMap<String, Vec<String>>>,
    pub revisions: BTreeMap<String, i64>,
}
fn open(root: &Path, file: &str) -> Result<Connection, Error> {
    let path = root.join(file);
    drop(source::safe_file(&path)?);
    let db = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|_| fail("relatedStateUnavailable"))?;
    db.busy_timeout(std::time::Duration::from_secs(2))
        .map_err(|_| fail("relatedStateUnavailable"))?;
    db.execute_batch("PRAGMA query_only=ON; BEGIN DEFERRED;")
        .map_err(|_| fail("relatedStateUnavailable"))?;
    Ok(db)
}
pub(crate) fn snapshot(root: &Path, profile: &str) -> Result<Snapshot, Error> {
    snapshot_tables(root, profile, false)
}
/// Codex may expire logs during startup, before it handles thread/delete.
/// Report that maintenance separately and verify deletion against the initialized
/// log snapshot. All other table baselines remain from before process startup.
pub(crate) fn initialized_logs(root: &Path, before: &mut Snapshot) -> Result<bool, Error> {
    let mut initialized = snapshot_tables(root, "", true)?;
    let key = "logs_2.sqlite/logs";
    let rows = initialized
        .rows
        .remove(key)
        .ok_or_else(|| fail("relatedStateUnavailable"))?;
    let changed = before.rows.get(key) != Some(&rows);
    before.rows.insert(key.into(), rows);
    Ok(changed)
}
fn snapshot_tables(root: &Path, profile: &str, logs_only: bool) -> Result<Snapshot, Error> {
    let mut out = Snapshot::default();
    let mut bytes = 0usize;
    let mut count = 0usize;
    for (file, tables) in [
        (
            "state_5.sqlite",
            &[
                "thread_attachments",
                "thread_dynamic_tools",
                "thread_read_receipts",
                "guardian_review_feedback",
            ][..],
        ),
        (
            "goals_1.sqlite",
            &["thread_goals", "thread_goal_continuation_deferrals"][..],
        ),
        ("logs_2.sqlite", &["logs"][..]),
        ("memories_1.sqlite", &["stage1_outputs", "jobs"][..]),
        ("queue_1.sqlite", &["queued_items"][..]),
    ] {
        if logs_only && file != "logs_2.sqlite" {
            continue;
        }
        let db = open(root, file)?;
        for table in tables {
            if profile == "state-v59"
                && matches!(*table, "thread_read_receipts" | "guardian_review_feedback")
            {
                continue;
            }
            // Names are code-owned literals, never renderer-supplied identifiers.
            let owner = if *table == "jobs" {
                "job_key"
            } else {
                "thread_id"
            };
            let mut q = db
                .prepare(&format!(
                    "SELECT {owner}, * FROM {table} WHERE {owner} IS NOT NULL"
                ))
                .map_err(|_| fail("relatedSchemaUnsupported"))?;
            let columns = q.column_count();
            let mut rows = q.query([]).map_err(|_| fail("relatedStateUnavailable"))?;
            let entries = out.rows.entry(format!("{file}/{table}")).or_default();
            while let Some(row) = rows.next().map_err(|_| fail("relatedStateUnavailable"))? {
                count += 1;
                if count > 250_000 {
                    return Err(fail("relatedStateBudget"));
                }
                let id: String = row.get(0).map_err(|_| fail("relatedStateUnavailable"))?;
                if !crate::locks::uuid(&id) {
                    continue;
                }
                let mut hash = Sha256::new();
                for col in 1..columns {
                    let value = row
                        .get_ref(col)
                        .map_err(|_| fail("relatedStateUnavailable"))?;
                    match value {
                        ValueRef::Null => hash.update([0]),
                        ValueRef::Integer(v) => {
                            hash.update([1]);
                            hash.update(v.to_le_bytes());
                        }
                        ValueRef::Real(v) => {
                            hash.update([2]);
                            hash.update(v.to_bits().to_le_bytes());
                        }
                        ValueRef::Text(v) | ValueRef::Blob(v) => {
                            bytes = bytes.saturating_add(v.len());
                            if bytes > 128 * 1024 * 1024 {
                                return Err(fail("relatedStateBudget"));
                            }
                            hash.update([if matches!(value, ValueRef::Text(_)) {
                                3
                            } else {
                                4
                            }]);
                            hash.update((v.len() as u64).to_le_bytes());
                            hash.update(v);
                        }
                    }
                }
                entries
                    .entry(id.to_ascii_lowercase())
                    .or_default()
                    .push(format!("{:x}", hash.finalize()));
            }
            for hashes in entries.values_mut() {
                hashes.sort();
            }
        }
        if file == "queue_1.sqlite" {
            let mut q = db
                .prepare("SELECT thread_id,revision FROM queued_thread_revisions")
                .map_err(|_| fail("relatedSchemaUnsupported"))?;
            let mut rows = q.query([]).map_err(|_| fail("relatedStateUnavailable"))?;
            while let Some(row) = rows.next().map_err(|_| fail("relatedStateUnavailable"))? {
                count += 1;
                if count > 250_000 {
                    return Err(fail("relatedStateBudget"));
                }
                let id: String = row.get(0).map_err(|_| fail("relatedStateUnavailable"))?;
                out.revisions.insert(
                    id.to_ascii_lowercase(),
                    row.get(1).map_err(|_| fail("relatedStateUnavailable"))?,
                );
            }
        }
    }
    Ok(out)
}
fn contains_multiset(before: &[String], after: &[String]) -> bool {
    let mut remaining = BTreeMap::<&str, usize>::new();
    for hash in after {
        *remaining.entry(hash).or_default() += 1;
    }
    before.iter().all(|hash| {
        remaining.get_mut(hash.as_str()).is_some_and(|n| {
            if *n == 0 {
                false
            } else {
                *n -= 1;
                true
            }
        })
    })
}
pub(crate) fn verify(
    before: &Snapshot,
    after: &Snapshot,
    selected: &[String],
) -> Vec<&'static str> {
    let ids: BTreeSet<_> = selected.iter().map(|s| s.to_ascii_lowercase()).collect();
    let mut errors = BTreeSet::new();
    if before.rows.keys().ne(after.rows.keys()) {
        errors.insert("relatedSchemaChanged");
    }
    for (table, owners) in &after.rows {
        for (id, hashes) in owners {
            if ids.contains(id) && !hashes.is_empty() {
                errors.insert("relatedDataRemaining");
            }
        }
        if let Some(old) = before.rows.get(table) {
            for (id, hashes) in old.iter().filter(|(id, _)| !ids.contains(*id)) {
                if !contains_multiset(hashes, owners.get(id).map(Vec::as_slice).unwrap_or(&[])) {
                    errors.insert("retainedRelatedDataChangedOrMissing");
                }
            }
        }
    }
    // Queue invalidation is emitted only when there was queued content to remove.
    // An empty queue legitimately has neither a revision row nor a new notification.
    let queued = before.rows.get("queue_1.sqlite/queued_items");
    for id in &ids {
        let had_content = queued
            .and_then(|owners| owners.get(id))
            .is_some_and(|rows| !rows.is_empty());
        if had_content
            && after
                .revisions
                .get(id)
                .is_none_or(|new| before.revisions.get(id).is_some_and(|old| new <= old))
        {
            errors.insert("queueInvalidationUnverified");
        }
        if before
            .revisions
            .get(id)
            .is_some_and(|old| after.revisions.get(id).is_none_or(|new| new < old))
        {
            errors.insert("queueInvalidationUnverified");
        }
    }
    for (id, old) in before.revisions.iter().filter(|(id, _)| !ids.contains(*id)) {
        if after.revisions.get(id).is_none_or(|new| new < old) {
            errors.insert("retainedQueueRevisionMissing");
        }
    }
    errors.into_iter().collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    const A: &str = "00000000-0000-4000-8000-000000000001";
    const B: &str = "00000000-0000-4000-8000-000000000002";
    fn snapshots() -> (Snapshot, Snapshot) {
        let before = Snapshot {
            rows: BTreeMap::from([(
                "queue_1.sqlite/queued_items".into(),
                BTreeMap::from([
                    (A.into(), vec!["deleted".into()]),
                    (B.into(), vec!["retained".into()]),
                ]),
            )]),
            revisions: BTreeMap::from([(A.into(), 1), (B.into(), 2)]),
        };
        let mut after = before.clone();
        after
            .rows
            .get_mut("queue_1.sqlite/queued_items")
            .unwrap()
            .remove(A);
        after.revisions.insert(A.into(), 3);
        (before, after)
    }
    #[test]
    fn preserved_queue_invalidation_is_expected_and_unselected_appends_are_allowed() {
        let (before, mut after) = snapshots();
        after
            .rows
            .get_mut("queue_1.sqlite/queued_items")
            .unwrap()
            .get_mut(B)
            .unwrap()
            .push("new".into());
        assert!(verify(&before, &after, &[A.into()]).is_empty());
    }
    #[test]
    fn retained_logs_removed_during_deletion_still_fail_verification() {
        let (mut before, mut after) = snapshots();
        before.rows.insert(
            "logs_2.sqlite/logs".into(),
            BTreeMap::from([(B.into(), vec!["retained-current-log".into()])]),
        );
        after
            .rows
            .insert("logs_2.sqlite/logs".into(), BTreeMap::new());
        assert!(
            verify(&before, &after, &[A.into()]).contains(&"retainedRelatedDataChangedOrMissing")
        );
    }
    #[test]
    fn empty_queue_needs_no_invalidation_but_existing_revision_cannot_disappear() {
        let mut before = Snapshot::default();
        before
            .rows
            .insert("queue_1.sqlite/queued_items".into(), BTreeMap::new());
        let after = before.clone();
        assert!(verify(&before, &after, &[A.into()]).is_empty());
        before.revisions.insert(A.into(), 1);
        assert!(verify(&before, &after, &[A.into()]).contains(&"queueInvalidationUnverified"));
    }
    #[test]
    fn residual_rewritten_retained_and_missing_invalidation_never_verify() {
        let (before, good) = snapshots();
        let mut after = good.clone();
        after
            .rows
            .get_mut("queue_1.sqlite/queued_items")
            .unwrap()
            .insert(A.into(), vec!["left".into()]);
        assert!(verify(&before, &after, &[A.into()]).contains(&"relatedDataRemaining"));
        let mut after = good.clone();
        after
            .rows
            .get_mut("queue_1.sqlite/queued_items")
            .unwrap()
            .insert(B.into(), vec!["changed".into()]);
        assert!(
            verify(&before, &after, &[A.into()]).contains(&"retainedRelatedDataChangedOrMissing")
        );
        let mut after = good;
        after.revisions.insert(A.into(), 1);
        assert!(verify(&before, &after, &[A.into()]).contains(&"queueInvalidationUnverified"));
        assert!(!contains_multiset(&["x".into(), "x".into()], &["x".into()]));
    }
}
