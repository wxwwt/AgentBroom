//! Native deletion owner. Discovery is read-only; only confirmed execution starts a real server.
//! No public constructor accepts an existing source directory, Inventory or renderer path.
pub mod rpc;
mod source;
mod types;
use agentbroom_core::{
    InspectError, Inventory, cleanup,
    native_scope::{self, NativeScopeEvidence},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use source::{Database, Stamp, database, hash_file, identity, read_file, stamps};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
pub use types::{Confirmation, NativePlan, Receipt};
pub type Error = InspectError;
pub(crate) fn fail(code: &str) -> Error {
    Error {
        code: code.into(),
        message: code.into(),
    }
}
const TTL: Duration = Duration::from_secs(120);
struct SavedPlan {
    view: NativePlan,
    kept: Vec<String>,
    created: Instant,
    selected_hashes: BTreeMap<String, String>,
    selected_rows: BTreeMap<String, Value>,
    binary_hash: String,
    runtime_version: String,
}
pub struct LiveCleanup {
    root: PathBuf,
    root_identity: (u64, u64),
    binary: PathBuf,
    receipts: PathBuf,
    pending: Option<SavedPlan>,
    closed: bool,
}
struct Evaluation {
    analysis: cleanup::CleanupAnalysis,
    db: Database,
    files: BTreeMap<String, Stamp>,
    ids: Vec<String>,
    blocked: Vec<String>,
}
impl LiveCleanup {
    /// Resolves the source and installed runtime internally. Does not launch native maintenance.
    pub fn discover() -> Result<Self, Error> {
        let root = agentbroom_core::codex::default_root()?;
        source::guard_root(&root)?;
        let root = root.canonicalize().map_err(|_| fail("sourceUnavailable"))?;
        let binary = source::installed_binary()?;
        let receipts =
            PathBuf::from(std::env::var_os("HOME").ok_or_else(|| fail("homeUnavailable"))?)
                .join(".agentbroom/cleanup-receipts");
        Ok(Self {
            root_identity: identity(&root)?,
            root,
            binary,
            receipts,
            pending: None,
            closed: false,
        })
    }
    pub fn invalidate(&mut self) {
        self.pending = None;
    }
    pub fn cancel(&mut self, id: &str) {
        if self.pending.as_ref().is_some_and(|p| p.view.plan_id == id) {
            self.pending = None;
        }
    }
    pub fn inventory(&mut self) -> Result<Inventory, Error> {
        self.invalidate();
        agentbroom_core::codex::scan_store(&self.root)
    }
    fn evaluate(
        &self,
        selected: &[String],
        kept: &[String],
        runtime_version: &str,
    ) -> Result<Evaluation, Error> {
        if self.closed {
            return Err(fail("runtimeStopped"));
        }
        if selected.len() != 1 || kept.len() > 128 {
            return Err(fail("invalidSelection"));
        }
        source::guard_root(&self.root)?;
        if identity(&self.root)? != self.root_identity {
            return Err(fail("sourceChanged"));
        }
        source::check_config(&self.root)?;
        let pinned = source::check_pins(&self.root)?;
        let files = stamps(&self.root)?;
        let mut inventory = agentbroom_core::codex::scan_store(&self.root)?;
        // Index-only entries have no physical deletion target. Real malformed files remain blockers.
        inventory.threads.retain(|t| !t.files.is_empty());
        let observed: BTreeSet<_> = inventory
            .threads
            .iter()
            .flat_map(|t| t.files.iter().cloned())
            .collect();
        if observed != files.keys().cloned().collect() {
            return Err(fail("nativeScopeIncomplete"));
        }
        // Full source scans can be partial because unrelated cache links are deliberately excluded.
        // The cleanup corpus above is independently exhaustive, rejects links/special files and budgets.
        inventory.partial_coverage = false;
        let analysis = cleanup::plan(&inventory, selected, kept)?;
        let db = database(&self.root)?;
        let ids = db.subtree(&selected[0])?;
        // A fully parsed orphan outside the authoritative subtree cannot be silently selected.
        // Its explicit history/fork references still participate in the shared dependency checks.
        // Unread headers, cycles, ambiguous aliases, or an orphan inside this subtree still block.
        let graph_complete = inventory
            .threads
            .iter()
            .filter(|t| t.relation == agentbroom_core::Relation::Unknown)
            .all(|t| {
                !ids.contains(&t.id)
                    && t.source_type == "subagent"
                    && t.protection_reasons.iter().all(|r| {
                        matches!(
                            r.as_str(),
                            "Read-only inspection: cleanup eligibility is not evaluated"
                                | "Live status unknown: no runtime or database activity queried"
                                | "Missing parent identity"
                                | "Parent conversation missing from inspected metadata"
                        )
                    })
            });

        let known_ids: BTreeSet<_> = inventory.threads.iter().map(|t| t.id.clone()).collect();
        // Only the explicitly permitted orphan-parent gap can be waived. Missing fork/history
        // targets and any non-orphan parent gap remain independent global graph blockers.
        let references_complete = inventory.threads.iter().all(|t| {
            let sources = t
                .fork_sources
                .iter()
                .chain(t.history_sources.iter())
                .chain(t.forked_from_id.iter())
                .chain(t.shared_history_from.iter());
            sources.clone().all(|source| known_ids.contains(source))
                && t.parent_id.as_ref().is_none_or(|parent| {
                    known_ids.contains(parent)
                        || (t.relation == agentbroom_core::Relation::Unknown
                            && t.source_type == "subagent"
                            && !ids.contains(&t.id))
                })
        });
        let mut blocked: BTreeSet<_> = analysis
            .blocked_reasons
            .iter()
            .filter(|r| {
                !matches!(
                    r.as_str(),
                    "realStoreLocked"
                        | "nativeEffectsUnverified"
                        | "activityUnverified"
                        | "protectedRecord"
                )
            })
            .cloned()
            .collect();
        for t in inventory
            .threads
            .iter()
            .filter(|t| analysis.included_ids.contains(&t.id))
        {
            for reason in &t.protection_reasons {
                if !matches!(
                    reason.as_str(),
                    "Read-only inspection: cleanup eligibility is not evaluated"
                        | "Live status unknown: no runtime or database activity queried"
                ) {
                    blocked.insert("protectedRecord".into());
                }
            }
        }
        let scope = native_scope::assess(&NativeScopeEvidence {
            runtime_version: runtime_version.to_owned(),
            root_id: selected[0].clone(),
            observed_ids: analysis.included_ids.clone(),
            native_ids: ids.clone(),
            observed_complete: graph_complete,
            native_complete: true,
        });
        blocked.extend(scope.blocked_reasons);
        if graph_complete && references_complete {
            blocked.remove("graphUnresolved");
        }
        if ids
            .iter()
            .any(|id| pinned.contains(&id.to_ascii_lowercase()))
        {
            blocked.insert("protectedRecord".into());
        }
        if ids.len() > 1024 {
            blocked.insert("nativeScopeBudget".into());
        }
        for file in &analysis.observed_files {
            if files.get(&file.key).is_some_and(Stamp::recent) {
                blocked.insert("recentWriteProtected".into());
            }
        }
        if std::env::var("CODEX_THREAD_ID")
            .ok()
            .is_some_and(|id| ids.contains(&id))
        {
            blocked.insert("currentConversationProtected".into());
        }
        for id in &ids {
            let row = db
                .rows
                .get(id)
                .ok_or_else(|| fail("nativeStateUnavailable"))?;
            let path = row
                .get("rollout_path")
                .and_then(Value::as_str)
                .ok_or_else(|| fail("nativeStateUnavailable"))?;
            let path = Path::new(path);
            if !path.starts_with(&self.root)
                || !analysis
                    .observed_files
                    .iter()
                    .any(|f| self.root.join(&f.key) == path)
            {
                blocked.insert("nativePathMismatch".into());
            }
        }
        Ok(Evaluation {
            analysis,
            db,
            files,
            ids,
            blocked: blocked.into_iter().collect(),
        })
    }
    pub fn preview(&mut self, selected: &[String], kept: &[String]) -> Result<NativePlan, Error> {
        self.invalidate();
        let runtime = source::probe_version(&self.binary)?;
        let evaluation = self.evaluate(selected, kept, &runtime)?;
        if !native_scope::supported_version(&runtime) {
            return Err(fail("nativeVersionUnsupported"));
        }
        let nonce = tempfile::Builder::new()
            .prefix("live-")
            .rand_bytes(20)
            .tempfile()
            .map_err(|_| fail("planUnavailable"))?;
        let plan_id = nonce
            .path()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let view = NativePlan {
            plan_id,
            test_only: false,
            native_ids: evaluation.ids.clone(),
            retained_ids: evaluation
                .db
                .rows
                .keys()
                .filter(|id| !evaluation.ids.contains(id))
                .cloned()
                .collect(),
            blocked_reasons: evaluation.blocked.clone(),
            executable: evaluation.blocked.is_empty(),
            expires_in_seconds: TTL.as_secs(),
            analysis: evaluation.analysis,
        };
        if self.unresolved(&view.native_ids)? {
            return Err(fail("unresolvedPreviousOperation"));
        }
        let selected_hashes = view
            .analysis
            .observed_files
            .iter()
            .map(|f| {
                Ok((
                    f.key.clone(),
                    hash_file(&self.root.join(&f.key), 4 * 1024 * 1024 * 1024)?,
                ))
            })
            .collect::<Result<_, Error>>()?;
        let selected_rows = evaluation
            .db
            .rows
            .into_iter()
            .filter(|(id, _)| view.native_ids.contains(id))
            .collect();
        let binary_hash = hash_file(&self.binary, 400 * 1024 * 1024)?;
        self.pending = Some(SavedPlan {
            view: view.clone(),
            kept: kept.to_vec(),
            created: Instant::now(),
            selected_hashes,
            selected_rows,
            binary_hash,
            runtime_version: runtime,
        });
        Ok(view)
    }
    fn revalidate(&self, plan: &SavedPlan) -> Result<Evaluation, Error> {
        if plan.created.elapsed() > TTL {
            return Err(fail("planExpired"));
        }
        let runtime = source::probe_version(&self.binary)?;
        if runtime != plan.runtime_version {
            return Err(fail("runtimeChanged"));
        }
        let e = self.evaluate(&plan.view.analysis.selected_ids, &plan.kept, &runtime)?;
        if !e.blocked.is_empty()
            || e.ids != plan.view.native_ids
            || e.analysis.included_ids != plan.view.analysis.included_ids
        {
            return Err(fail("staleSnapshot"));
        }
        let selected_rows: BTreeMap<_, _> =
            e.db.rows
                .iter()
                .filter(|(id, _)| e.ids.contains(id))
                .map(|(id, row)| (id.clone(), row.clone()))
                .collect();
        let paths: BTreeSet<_> = e
            .analysis
            .observed_files
            .iter()
            .map(|f| f.key.clone())
            .collect();
        if paths != plan.selected_hashes.keys().cloned().collect()
            || selected_rows != plan.selected_rows
        {
            return Err(fail("staleSnapshot"));
        }
        for (path, expected) in &plan.selected_hashes {
            if hash_file(&self.root.join(path), 4 * 1024 * 1024 * 1024)? != *expected {
                return Err(fail("staleSnapshot"));
            }
        }
        if hash_file(&self.binary, 400 * 1024 * 1024)? != plan.binary_hash {
            return Err(fail("runtimeChanged"));
        }
        Ok(e)
    }
    fn source_id(&self) -> String {
        format!(
            "{:x}",
            Sha256::digest(format!("{}:{:?}", self.root.display(), self.root_identity).as_bytes())
        )
    }
    fn unresolved(&self, ids: &[String]) -> Result<bool, Error> {
        if !self.receipts.exists() {
            return Ok(false);
        }
        let mut count = 0;
        for entry in fs::read_dir(&self.receipts).map_err(|_| fail("receiptUnavailable"))? {
            count += 1;
            if count > 10000 {
                return Err(fail("receiptBudget"));
            }
            let path = entry.map_err(|_| fail("receiptUnavailable"))?.path();
            if path.extension().is_none_or(|e| e != "json") {
                continue;
            }
            let r: Receipt = serde_json::from_slice(&read_file(&path, 1024 * 1024)?)
                .map_err(|_| fail("receiptUnavailable"))?;
            if r.source_id == self.source_id()
                && r.status == "unknown"
                && r.expected_ids.iter().any(|id| ids.contains(id))
            {
                return Ok(true);
            }
        }
        Ok(false)
    }
    pub fn history(&self) -> Result<Vec<Receipt>, Error> {
        if !self.receipts.exists() {
            return Ok(vec![]);
        }
        let mut entries = Vec::new();
        for entry in fs::read_dir(&self.receipts).map_err(|_| fail("receiptUnavailable"))? {
            let entry = entry.map_err(|_| fail("receiptUnavailable"))?;
            if entry.path().extension().is_some_and(|e| e == "json") {
                entries.push(entry);
            }
            if entries.len() > 10000 {
                return Err(fail("receiptBudget"));
            }
        }
        entries.sort_by_key(|entry| {
            std::cmp::Reverse(entry.metadata().ok().and_then(|m| m.modified().ok()))
        });
        let mut out = Vec::new();
        for entry in entries {
            let data = read_file(&entry.path(), 1024 * 1024)?;
            let receipt: Receipt =
                serde_json::from_slice(&data).map_err(|_| fail("receiptUnavailable"))?;
            if receipt.source_id == self.source_id() {
                out.push(receipt);
                if out.len() == 20 {
                    break;
                }
            }
        }
        Ok(out)
    }
    pub fn receipt(&self, id: &str) -> Result<Receipt, Error> {
        valid_id(id)?;
        let path = self.receipts.join(format!("{id}.json"));
        let data = read_file(&path, 1024 * 1024)?;
        let receipt: Receipt =
            serde_json::from_slice(&data).map_err(|_| fail("receiptUnavailable"))?;
        if receipt.plan_id != id || receipt.source_id != self.source_id() {
            return Err(fail("invalidPlan"));
        }
        Ok(receipt)
    }
    fn write_receipt(&self, receipt: &Receipt) -> Result<(), Error> {
        source::private_directory(&self.receipts)?;
        let mut file = tempfile::NamedTempFile::new_in(&self.receipts)
            .map_err(|_| fail("receiptUnavailable"))?;
        use std::io::Write;
        file.write_all(&serde_json::to_vec(receipt).map_err(|_| fail("receiptUnavailable"))?)
            .map_err(|_| fail("receiptUnavailable"))?;
        file.as_file()
            .sync_all()
            .map_err(|_| fail("receiptUnavailable"))?;
        file.persist(self.receipts.join(format!("{}.json", receipt.plan_id)))
            .map_err(|_| fail("receiptUnavailable"))?;
        fs::File::open(&self.receipts)
            .and_then(|f| f.sync_all())
            .map_err(|_| fail("receiptUnavailable"))
    }
    pub fn execute(&mut self, c: Confirmation) -> Result<Receipt, Error> {
        valid_id(&c.plan_id)?;
        if c.phrase != "DELETE" || !c.acknowledged || c.selected_ids.len() != 1 {
            return Err(fail("invalidConfirmation"));
        }
        if let Ok(receipt) = self.receipt(&c.plan_id) {
            if receipt.selected_ids != c.selected_ids {
                return Err(fail("invalidPlan"));
            }
            // An executing/unknown persisted receipt never authorizes another native call after restart.
            return Ok(receipt);
        }
        let _operation_lock = source::operation_lock(&self.receipts)?;
        let plan = self.pending.as_ref().ok_or_else(|| fail("invalidPlan"))?;
        if self.unresolved(&plan.view.native_ids)? {
            return Err(fail("unresolvedPreviousOperation"));
        }
        if c.plan_id != plan.view.plan_id
            || c.selected_ids != plan.view.analysis.selected_ids
            || c.kept != plan.kept
        {
            return Err(fail("invalidPlan"));
        }
        if !plan.view.executable {
            return Err(fail("planBlocked"));
        }
        let before = self.revalidate(plan)?;
        let plan = self.pending.take().unwrap();
        // Kernel stamps prove unchanged retained files. For recently written files, preserve a
        // content prefix so legitimate concurrent appends cannot hide rewrites of existing bytes.
        let mut retained_prefixes = BTreeMap::new();
        for (path, stamp) in before
            .files
            .iter()
            .filter(|(path, stamp)| !plan.selected_hashes.contains_key(*path) && stamp.recent())
        {
            retained_prefixes.insert(
                path.clone(),
                source::checked_prefix(&self.root.join(path), stamp.length, stamp)?,
            );
        }
        let mut receipt = Receipt {
            plan_id: c.plan_id,
            selected_ids: c.selected_ids.clone(),
            test_only: false,
            status: "unknown".into(),
            removed: vec![],
            remaining: plan.selected_hashes.keys().cloned().collect(),
            observed_removed_logical_bytes: 0,
            freed_bytes: None,
            error: Some("Execution started; never automatically retry.".into()),
            concurrent_changes: vec![],
            source_id: self.source_id(),
            expected_ids: plan.view.native_ids.clone(),
            native_notifications_verified: false,
            native_deleted_ids: vec![],
            files_verified: false,
            state_verified: false,
        };
        self.write_receipt(&receipt)?;
        // This is the only real-store process start, after exact user confirmation and durable intent.
        let temp = tempfile::Builder::new()
            .prefix("agentbroom-native-operation-")
            .tempdir()
            .map_err(|_| fail("runtimeUnavailable"))?;
        let mut server = match rpc::Rpc::start(source::native_command(
            &self.binary,
            &self.root,
            temp.path(),
        )?) {
            Ok(server) => server,
            Err(error) => {
                receipt.status = "verificationFailed".into();
                receipt.error = Some(error.code);
                self.write_receipt(&receipt)?;
                return Ok(receipt);
            }
        };
        // Startup may touch native indexes. Never broaden a plan because a fresh native process started.
        let validation = self.revalidate(&plan);
        let mut errors = vec![];
        if validation.is_err() {
            errors.push("staleAfterNativeInitialization");
        }
        if errors.is_empty() {
            match server.request("thread/delete", json!({"threadId":c.selected_ids[0]})) {
                Ok(v) if v == json!({}) => match server.deleted(plan.view.native_ids.len()) {
                    Ok(mut ids) => {
                        ids.sort();
                        receipt.native_deleted_ids = ids;
                    }
                    Err(_) => errors.push("nativeNotificationsUnavailable"),
                },
                _ => errors.push("nativeRejectedOrUnknown"),
            }
        }
        receipt.native_notifications_verified = receipt.native_deleted_ids == plan.view.native_ids;
        if !receipt.native_notifications_verified {
            errors.push("nativeNotificationsUnverified");
        }
        let mut server = Some(server);
        // An acknowledged complete delete can be read back while its SQLite reader state is live.
        // Unknown/partial RPC outcomes must stop first so no outstanding delete can mutate afterwards.
        if !errors.is_empty() {
            drop(server.take());
            self.closed = true;
        }
        let mut readback_error = None;
        match (stamps(&self.root), database(&self.root)) {
            (Ok(after_files), Ok(after_db)) => {
                receipt.removed = plan
                    .selected_hashes
                    .keys()
                    .filter(|p| !after_files.contains_key(*p))
                    .cloned()
                    .collect();
                receipt.remaining = plan
                    .selected_hashes
                    .keys()
                    .filter(|p| after_files.contains_key(*p))
                    .cloned()
                    .collect();
                receipt.observed_removed_logical_bytes = receipt
                    .removed
                    .iter()
                    .filter_map(|p| before.files.get(p).map(|s| s.length))
                    .sum();
                receipt.files_verified = receipt.remaining.is_empty();
                for (path, old) in before
                    .files
                    .iter()
                    .filter(|(p, _)| !plan.selected_hashes.contains_key(*p))
                {
                    match after_files.get(path) {
                        Some(new) if new == old => {}
                        Some(new)
                            if new.identity == old.identity
                                && new.length >= old.length
                                && retained_prefixes.get(path).is_some_and(|hash| {
                                    source::checked_prefix(&self.root.join(path), old.length, new)
                                        .is_ok_and(|after| after == *hash)
                                }) =>
                        {
                            receipt.concurrent_changes.push(path.clone())
                        }
                        _ => {
                            receipt.files_verified = false;
                            errors.push("retainedFileChangedOrMissing");
                        }
                    }
                }
                if after_files.keys().any(|p| !before.files.contains_key(p)) {
                    receipt
                        .concurrent_changes
                        .push("New rollout files appeared concurrently".into());
                }
                receipt.state_verified = plan
                    .view
                    .native_ids
                    .iter()
                    .all(|id| !after_db.rows.contains_key(id));
                for (id, old) in before
                    .db
                    .rows
                    .iter()
                    .filter(|(id, _)| !plan.view.native_ids.contains(id))
                {
                    match after_db.rows.get(id) {
                        Some(new) if source::same_row(old, new) => {}
                        _ => {
                            receipt.state_verified = false;
                            errors.push("retainedStateChangedOrMissing");
                        }
                    }
                }
                let expected: Vec<_> = before
                    .db
                    .edges
                    .iter()
                    .filter(|(p, ch, _)| {
                        !plan.view.native_ids.contains(p) && !plan.view.native_ids.contains(ch)
                    })
                    .cloned()
                    .collect();
                if after_db.edges != expected {
                    receipt.state_verified = false;
                    errors.push("spawnGraphChanged");
                }
            }
            (files, db) => {
                readback_error = files.err().or_else(|| db.err()).map(|e| e.code);
                errors.push("readbackUnavailable");
            }
        }
        drop(server);
        if !receipt.files_verified {
            errors.push("filesUnverified");
        }
        if !receipt.state_verified {
            errors.push("stateUnverified");
        }
        errors.sort();
        errors.dedup();
        receipt.status = if errors.is_empty() {
            "verified"
        } else if !receipt.removed.is_empty() {
            "partialFailure"
        } else {
            "verificationFailed"
        }
        .into();
        receipt.error = (!errors.is_empty()).then(|| {
            format!(
                "{}{}",
                errors.join(","),
                readback_error.map_or(String::new(), |e| format!(":{e}"))
            )
        });
        self.write_receipt(&receipt)?;
        Ok(receipt)
    }
}
fn valid_id(id: &str) -> Result<(), Error> {
    if id.is_empty()
        || id.len() > 100
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
        || id == "."
        || id == ".."
    {
        Err(fail("invalidPlan"))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests;
