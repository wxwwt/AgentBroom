//! Feature-gated developer harness. There is no constructor from a path or real inventory.
//! It deletes only data generated here in a new private TempDir, never Codex data.
use crate::cleanup::{CleanupAnalysis, build_plan};
use crate::{Activity, Category, DiskItem, Inventory, Relation, Thread};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
#[cfg(unix)]
use std::os::unix::fs::MetadataExt;
use std::{
    collections::BTreeMap,
    fs,
    time::{Duration, Instant},
};
type Outcome<T> = Result<T, String>;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Receipt {
    pub plan_id: String,
    pub status: String,
    pub removed: Vec<String>,
    pub remaining: Vec<String>,
    pub observed_removed_logical_bytes: u64,
    pub freed_bytes: Option<u64>,
    pub state_logical_bytes_before: u64,
    pub state_logical_bytes_after: Option<u64>,
    pub error: Option<String>,
}
#[derive(Clone, Debug)]
pub struct Confirmation {
    pub plan_id: String,
    pub selected_ids: Vec<String>,
    pub phrase: String,
    pub acknowledged: bool,
}
struct SyntheticPlan {
    id: String,
    analysis: CleanupAnalysis,
    hashes: BTreeMap<String, String>,
    state: Vec<Thread>,
    issued: Instant,
}
pub struct SyntheticStore {
    directory: tempfile::TempDir,
    counter: u64,
    pending: Option<SyntheticPlan>,
    completed: Option<Receipt>,
    #[cfg(test)]
    fault: Option<usize>,
    #[cfg(test)]
    state_failure: bool,
    #[cfg(test)]
    verify_failure: bool,
}
fn hashes(path: &std::path::Path) -> Outcome<BTreeMap<String, String>> {
    let mut result = BTreeMap::new();
    for entry in fs::read_dir(path).map_err(|_| "snapshotFailed")? {
        let entry = entry.map_err(|_| "snapshotFailed")?;
        let metadata = fs::symlink_metadata(entry.path()).map_err(|_| "snapshotFailed")?;
        if !metadata.is_file() || metadata.len() > 64 * 1024 {
            return Err("unsafeSyntheticEntry".into());
        }
        #[cfg(unix)]
        if metadata.nlink() != 1 {
            return Err("unsafeSyntheticEntry".into());
        }
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| "unsafeSyntheticEntry")?;
        let contents = fs::read(entry.path()).map_err(|_| "snapshotFailed")?;
        result.insert(name, format!("{:x}", Sha256::digest(contents)));
        if result.len() > 128 {
            return Err("snapshotBudget".into());
        }
    }
    Ok(result)
}
fn fingerprint(hashes: &BTreeMap<String, String>) -> String {
    format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(hashes).expect("string map"))
    )
}
impl SyntheticStore {
    /// Creates fixed synthetic content. Environment/CODEX_HOME/user roots are not inputs.
    pub fn new() -> Outcome<Self> {
        if !cfg!(unix) {
            return Err("unsupportedPlatform".into());
        }
        let directory = tempfile::Builder::new()
            .prefix("agentbroom-cleanup-test-")
            .tempdir_in("/tmp")
            .map_err(|_| "sandboxUnavailable")?;
        fs::create_dir(directory.path().join("data")).map_err(|_| "sandboxUnavailable")?;
        fs::create_dir(directory.path().join("receipts")).map_err(|_| "sandboxUnavailable")?;
        let mut records = Vec::new();
        for (id, relation, parent, files) in [
            (
                "a",
                Relation::Root,
                None,
                vec![("a1.jsonl", 31), ("a2.jsonl", 37)],
            ),
            ("b", Relation::Spawned, Some("a"), vec![("b.jsonl", 43)]),
            ("c", Relation::Spawned, Some("b"), vec![("c.jsonl", 47)]),
            ("d", Relation::Fork, None, vec![("d.jsonl", 53)]),
            ("e", Relation::Root, None, vec![("e.jsonl", 59)]),
        ] {
            for (file, size) in &files {
                fs::write(directory.path().join("data").join(file), vec![b'x'; *size])
                    .map_err(|_| "sandboxUnavailable")?;
            }
            records.push(Thread {
                id: id.into(),
                title: format!("Synthetic {id}"),
                project: "Synthetic".into(),
                relation,
                activity: Activity::Completed,
                updated_at: "2026-01-01T00:00:00Z".into(),
                parent_id: parent.map(String::from),
                shared_history_from: None,
                files: files.iter().map(|(f, _)| f.to_string()).collect(),
                segment_ids: vec![],
                own_bytes: files.iter().map(|(_, n)| *n as u64).sum(),
                inclusive_bytes: 0,
                owned_descendant_ids: vec![],
                protection_reasons: vec![],
                source_type: "synthetic".into(),
                forked_from_id: None,
                fork_sources: vec![],
                history_sources: vec![],
                relation_evidence: vec![],
            });
        }
        fs::write(
            directory.path().join("data/state.json"),
            serde_json::to_vec(&records).map_err(|_| "sandboxUnavailable")?,
        )
        .map_err(|_| "sandboxUnavailable")?;
        Ok(Self {
            directory,
            counter: 0,
            pending: None,
            completed: None,
            #[cfg(test)]
            fault: None,
            #[cfg(test)]
            state_failure: false,
            #[cfg(test)]
            verify_failure: false,
        })
    }
    fn scan(&self) -> Outcome<(Inventory, BTreeMap<String, String>, Vec<Thread>)> {
        let data = self.directory.path().join("data");
        let snapshot = hashes(&data)?;
        let state: Vec<Thread> = serde_json::from_slice(
            &fs::read(data.join("state.json")).map_err(|_| "snapshotFailed")?,
        )
        .map_err(|_| "invalidState")?;
        // Exact generated basename universe; changed/extra names cannot become deletion targets.
        let allowed = [
            "a1.jsonl",
            "a2.jsonl",
            "b.jsonl",
            "c.jsonl",
            "d.jsonl",
            "e.jsonl",
            "state.json",
        ];
        if snapshot.keys().any(|x| !allowed.contains(&x.as_str()))
            || state
                .iter()
                .any(|t| t.files.iter().any(|f| !allowed[..6].contains(&f.as_str())))
        {
            return Err("unsafeSyntheticEntry".into());
        }
        let mut items = Vec::new();
        for name in snapshot.keys().filter(|x| x.as_str() != "state.json") {
            let meta = fs::metadata(data.join(name)).map_err(|_| "snapshotFailed")?;
            items.push(DiskItem {
                key: name.clone(),
                category: Category::Sessions,
                logical_bytes: meta.len(),
                allocated_bytes: None,
            });
        }
        Ok((
            Inventory {
                schema_version: 1,
                source_kind: "synthetic".into(),
                dataset_label: "Private generated cleanup test".into(),
                source_fingerprint: fingerprint(&snapshot),
                items,
                threads: state.clone(),
                totals: vec![],
                warnings: vec![],
                execution_enabled: false,
                partial_coverage: false,
            },
            snapshot,
            state,
        ))
    }
    pub fn plan(
        &mut self,
        selected: &[String],
        kept: &[String],
    ) -> Outcome<(String, CleanupAnalysis)> {
        // Invalid replacement previews also revoke the old token.
        self.pending = None;
        let (inventory, hashes, state) = self.scan()?;
        let analysis = build_plan(&inventory, selected, kept, true).map_err(|e| e.code)?;
        self.counter += 1;
        let id = format!(
            "{}-{}",
            self.directory.path().file_name().unwrap().to_string_lossy(),
            self.counter
        );
        self.pending = Some(SyntheticPlan {
            id: id.clone(),
            analysis: analysis.clone(),
            hashes,
            state,
            issued: Instant::now(),
        });
        Ok((id, analysis))
    }
    pub fn execute(&mut self, confirmation: Confirmation) -> Outcome<Receipt> {
        if let Some(receipt) = &self.completed {
            if receipt.plan_id == confirmation.plan_id {
                return Ok(receipt.clone());
            }
        }
        let plan = self.pending.as_ref().ok_or("invalidPlan")?;
        let mut selected = confirmation.selected_ids.clone();
        selected.sort();
        if confirmation.plan_id != plan.id
            || selected != plan.analysis.selected_ids
            || !confirmation.acknowledged
            || confirmation.phrase != "DELETE"
        {
            return Err("confirmationRequired".into());
        }
        if plan.issued.elapsed() > Duration::from_secs(300) {
            self.pending = None;
            return Err("planExpired".into());
        }
        if !plan.analysis.blocked_reasons.is_empty() {
            return Err("blockedPlan".into());
        }
        let now = hashes(&self.directory.path().join("data"));
        if now.as_ref().ok() != Some(&plan.hashes) {
            self.pending = None;
            return Err("stalePlan".into());
        }
        let plan = self.pending.take().expect("validated");
        let mut receipt = Receipt {
            plan_id: plan.id.clone(),
            status: "executing".into(),
            removed: vec![],
            remaining: plan.analysis.executable_files.clone(),
            observed_removed_logical_bytes: 0,
            freed_bytes: None,
            state_logical_bytes_before: fs::metadata(self.directory.path().join("data/state.json"))
                .map_err(|_| "snapshotFailed")?
                .len(),
            state_logical_bytes_after: None,
            error: None,
        };
        let audit = self
            .directory
            .path()
            .join("receipts")
            .join(format!("{}.json", self.counter));
        let save = |r: &Receipt| -> Outcome<()> {
            use std::io::Write;
            let mut file = fs::File::create(&audit).map_err(|_| "auditFailed")?;
            file.write_all(&serde_json::to_vec(r).map_err(|_| "auditFailed")?)
                .map_err(|_| "auditFailed")?;
            file.sync_all().map_err(|_| "auditFailed".into())
        };
        save(&receipt)?; // No deletion if pending audit cannot be written.
        for (ordinal, name) in plan.analysis.executable_files.iter().enumerate() {
            #[cfg(not(test))]
            let _ = ordinal;
            #[cfg(test)]
            if self.fault == Some(ordinal) {
                receipt.error = Some("injectedRemoveFailure".into());
                break;
            }
            if fs::remove_file(self.directory.path().join("data").join(name)).is_err() {
                receipt.error = Some("removeFailed".into());
                break;
            }
            receipt.removed.push(name.clone());
            receipt.remaining.retain(|n| n != name);
            let size = plan
                .analysis
                .observed_files
                .iter()
                .find(|x| x.key == *name)
                .expect("planned file")
                .logical_bytes;
            receipt.observed_removed_logical_bytes += size;
            if save(&receipt).is_err() {
                receipt.error = Some("auditFailed".into());
                break;
            }
        }
        if receipt.error.is_some() {
            receipt.status = "partialFailure".into();
        } else {
            let remaining_state: Vec<_> = plan
                .state
                .iter()
                .filter(|t| !plan.analysis.included_ids.contains(&t.id))
                .cloned()
                .collect();
            let state = self.directory.path().join("data/state.json");
            #[cfg(test)]
            let may_write = !self.state_failure;
            #[cfg(not(test))]
            let may_write = true;
            let update = if may_write {
                serde_json::to_vec(&remaining_state)
                    .map_err(|_| "stateWriteFailed".into())
                    .and_then(|bytes| {
                        fs::write(self.directory.path().join("data/state.next"), bytes)
                            .map_err(|_| "stateWriteFailed".to_string())?;
                        fs::rename(self.directory.path().join("data/state.next"), &state)
                            .map_err(|_| "stateWriteFailed".into())
                    })
            } else {
                Err("stateWriteFailed".into())
            };
            #[cfg(test)]
            if self.verify_failure {
                let _ = fs::write(self.directory.path().join("data/e.jsonl"), b"changed");
            }
            let verified = update.is_ok() && self.verify(&plan, &remaining_state);
            receipt.status = if verified {
                "verified"
            } else {
                "verificationFailed"
            }
            .into();
            if !verified {
                receipt.error = Some(update.err().unwrap_or("preservationCheckFailed".into()));
            }
        }
        receipt.state_logical_bytes_after =
            fs::metadata(self.directory.path().join("data/state.json"))
                .ok()
                .map(|m| m.len());
        if save(&receipt).is_err() {
            receipt.status = "verificationFailed".into();
            receipt.error = Some("auditFailed".into());
        }
        self.completed = Some(receipt.clone());
        Ok(receipt)
    }
    fn verify(&self, plan: &SyntheticPlan, remaining_state: &[Thread]) -> bool {
        let Ok(after) = hashes(&self.directory.path().join("data")) else {
            return false;
        };
        let removed = &plan.analysis.executable_files;
        if removed.iter().any(|f| after.contains_key(f)) {
            return false;
        }
        let expected: BTreeMap<_, _> = plan
            .hashes
            .iter()
            .filter(|(k, _)| k.as_str() != "state.json" && !removed.contains(k))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let actual: BTreeMap<_, _> = after
            .iter()
            .filter(|(k, _)| k.as_str() != "state.json")
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let state = fs::read(self.directory.path().join("data/state.json")).ok();
        expected == actual && state == serde_json::to_vec(remaining_state).ok()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn ids(xs: &[&str]) -> Vec<String> {
        xs.iter().map(|s| s.to_string()).collect()
    }
    fn confirm(id: &str) -> Confirmation {
        Confirmation {
            plan_id: id.into(),
            selected_ids: ids(&["a"]),
            phrase: "DELETE".into(),
            acknowledged: true,
        }
    }
    #[test]
    fn real_disk_delete_preserves_fork_and_unselected() {
        let mut s = SyntheticStore::new().unwrap();
        let (id, p) = s.plan(&ids(&["a"]), &[]).unwrap();
        assert_eq!(p.included_ids, ids(&["a", "b", "c"]));
        assert_eq!(p.observed_logical_bytes, 158);
        let r = s.execute(confirm(&id)).unwrap();
        assert_eq!(r.status, "verified");
        assert_eq!(r.removed.len(), 4);
        assert_eq!(r.freed_bytes, None);
        assert_eq!(s.execute(confirm(&id)).unwrap().removed, r.removed);
        let (_, _, state) = s.scan().unwrap();
        assert_eq!(
            state.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(),
            vec!["d", "e"]
        );
    }
    #[test]
    fn shared_planner_keep_and_external_reference_block_disk_execution() {
        let mut s = SyntheticStore::new().unwrap();
        let (id, p) = s.plan(&ids(&["a"]), &ids(&["c"])).unwrap();
        assert_eq!(p.retained_ids, ids(&["c"]));
        assert!(p.executable_files.is_empty());
        assert!(!p.executable_files.contains(&"c.jsonl".into()));
        assert_eq!(s.execute(confirm(&id)).unwrap_err(), "blockedPlan");
        let state_path = s.directory.path().join("data/state.json");
        let mut state: Vec<Thread> =
            serde_json::from_slice(&fs::read(&state_path).unwrap()).unwrap();
        state[3].history_sources = ids(&["b"]);
        fs::write(state_path, serde_json::to_vec(&state).unwrap()).unwrap();
        let (id, p) = s.plan(&ids(&["a"]), &[]).unwrap();
        assert_eq!(p.outside_dependents, ids(&["d"]));
        assert!(p.executable_files.is_empty());
        assert_eq!(s.execute(confirm(&id)).unwrap_err(), "blockedPlan");
    }
    #[test]
    fn invalid_confirmation_cannot_mutate() {
        for kind in 0..4 {
            let mut s = SyntheticStore::new().unwrap();
            let (id, _) = s.plan(&ids(&["a"]), &[]).unwrap();
            let before = hashes(&s.directory.path().join("data")).unwrap();
            let mut c = confirm(&id);
            match kind {
                0 => c.phrase = "delete".into(),
                1 => c.acknowledged = false,
                2 => c.selected_ids = ids(&["b"]),
                _ => c.plan_id = "forged".into(),
            };
            assert!(s.execute(c).is_err());
            assert_eq!(before, hashes(&s.directory.path().join("data")).unwrap());
        }
    }
    #[test]
    fn same_length_change_and_added_or_missing_entries_are_stale() {
        for kind in 0..4 {
            let mut s = SyntheticStore::new().unwrap();
            let (id, _) = s.plan(&ids(&["a"]), &[]).unwrap();
            let dir = s.directory.path().join("data");
            match kind {
                0 => fs::write(dir.join("a1.jsonl"), vec![b'y'; 31]).unwrap(),
                1 => fs::write(dir.join("new"), b"new").unwrap(),
                2 => fs::remove_file(dir.join("c.jsonl")).unwrap(),
                _ => fs::write(dir.join("state.json"), b"[]").unwrap(),
            };
            let before = hashes(&dir).unwrap();
            assert_eq!(s.execute(confirm(&id)).unwrap_err(), "stalePlan");
            assert_eq!(before, hashes(&dir).unwrap());
        }
    }
    #[test]
    fn new_invalid_and_expired_plan_revoke_previous() {
        let mut s = SyntheticStore::new().unwrap();
        let (old, _) = s.plan(&ids(&["a"]), &[]).unwrap();
        assert!(s.plan(&[], &[]).is_err());
        assert!(s.execute(confirm(&old)).is_err());
        let (id, _) = s.plan(&ids(&["a"]), &[]).unwrap();
        s.pending.as_mut().unwrap().issued = Instant::now() - Duration::from_secs(301);
        assert_eq!(s.execute(confirm(&id)).unwrap_err(), "planExpired");
    }
    #[test]
    fn failed_second_remove_does_not_remove_remaining_or_state() {
        let mut s = SyntheticStore::new().unwrap();
        let (id, _) = s.plan(&ids(&["a"]), &[]).unwrap();
        s.fault = Some(1);
        let r = s.execute(confirm(&id)).unwrap();
        assert_eq!(r.status, "partialFailure");
        assert_eq!(r.removed, ids(&["a1.jsonl"]));
        assert_eq!(r.remaining.len(), 3);
        assert_eq!(s.scan().unwrap().2.len(), 5);
    }
    #[test]
    fn state_or_preservation_failure_cannot_be_verified() {
        for state_failure in [true, false] {
            let mut s = SyntheticStore::new().unwrap();
            let (id, _) = s.plan(&ids(&["a"]), &[]).unwrap();
            s.state_failure = state_failure;
            s.verify_failure = !state_failure;
            assert_eq!(
                s.execute(confirm(&id)).unwrap().status,
                "verificationFailed"
            );
        }
    }
    #[test]
    fn audit_failure_precedes_deletion() {
        let mut s = SyntheticStore::new().unwrap();
        let (id, _) = s.plan(&ids(&["a"]), &[]).unwrap();
        fs::remove_dir(s.directory.path().join("receipts")).unwrap();
        assert_eq!(s.execute(confirm(&id)).unwrap_err(), "auditFailed");
        assert_eq!(s.scan().unwrap().2.len(), 5);
    }
    #[cfg(unix)]
    #[test]
    fn symlink_and_hardlink_replacement_never_touch_outside_sentinel() {
        for link in [false, true] {
            let mut s = SyntheticStore::new().unwrap();
            let (id, _) = s.plan(&ids(&["a"]), &[]).unwrap();
            let outside = tempfile::tempdir_in("/tmp").unwrap();
            let target = outside.path().join("sentinel");
            fs::write(&target, b"untouched").unwrap();
            let file = s.directory.path().join("data/a1.jsonl");
            fs::remove_file(&file).unwrap();
            if link {
                fs::hard_link(&target, &file).unwrap();
            } else {
                std::os::unix::fs::symlink(&target, &file).unwrap();
            }
            assert_eq!(s.execute(confirm(&id)).unwrap_err(), "stalePlan");
            assert_eq!(fs::read(&target).unwrap(), b"untouched");
        }
    }
}
