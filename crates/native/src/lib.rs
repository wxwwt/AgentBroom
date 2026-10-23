//! Native deletion owner. Discovery is read-only; only confirmed execution starts a real server.
//! No public constructor accepts an existing source directory, Inventory or renderer path.
mod catalog;
pub mod diagnostics;
pub mod events;
mod locks;
mod related;
pub mod rpc;
mod source;
mod types;
pub mod worktrees;
use agentbroom_core::{InspectError, Inventory, cleanup};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use source::{Database, Stamp, database, hash_file, identity, read_file, stamps};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::PathBuf,
    time::{Duration, Instant},
};
pub use types::{Confirmation, ExecutionFailure, ExecutionPhase, NativePlan, Receipt};
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
    file_owners: BTreeMap<String, String>,
    header_parents: Vec<(String, String)>,
}
pub struct LiveCleanup {
    root: PathBuf,
    root_identity: (u64, u64),
    binary: PathBuf,
    receipts: PathBuf,
    pending: Option<SavedPlan>,
    closed: bool,
    runtime_contract: Option<(String, String)>,
}
struct Evaluation {
    analysis: cleanup::CleanupAnalysis,
    db: Database,
    files: BTreeMap<String, Stamp>,
    ids: Vec<String>,
    roots: Vec<String>,
    blocked: Vec<String>,
    warnings: Vec<String>,
    locked_ids: Vec<String>,
    file_owners: BTreeMap<String, String>,
    header_parents: Vec<(String, String)>,
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
            runtime_contract: None,
        })
    }
    fn event_dir(&self) -> PathBuf {
        self.receipts.parent().unwrap().join("activity")
    }
    pub fn operations(&self) -> Result<Vec<events::OperationEvent>, Error> {
        events::read(&self.event_dir(), events::now())
    }
    fn record(
        &self,
        id: String,
        action: &str,
        status: &str,
        ids: Vec<String>,
        titles: Vec<String>,
        count: usize,
        bytes: u64,
        error: Option<String>,
    ) -> Result<(), Error> {
        events::write(
            &self.event_dir(),
            events::OperationEvent {
                id,
                at: events::now(),
                action: action.into(),
                status: status.into(),
                ids,
                titles,
                count,
                bytes,
                error,
            },
        )
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
        let mut inventory = match database(&self.root) {
            Ok(db) => catalog::inspect(&self.root, &db, &stamps(&self.root)?)?,
            Err(e) => {
                let mut i = agentbroom_core::codex::scan_store(&self.root)?;
                i.warnings
                    .push(format!("Native cleanup unavailable: {}", e.code));
                i
            }
        };
        if self
            .record(
                String::new(),
                "scan",
                "verified",
                vec![],
                vec![],
                inventory.items.len(),
                0,
                None,
            )
            .is_err()
        {
            inventory
                .warnings
                .push("Operation history could not be saved; inventory is still available.".into());
        }
        Ok(inventory)
    }
    pub fn locked_ids(&self) -> Result<Vec<String>, Error> {
        Ok(locks::read(&self.receipts, &self.source_id())?
            .into_iter()
            .collect())
    }
    pub fn set_locked(&mut self, id: &str, locked: bool) -> Result<Vec<String>, Error> {
        self.set_locks(&[id.into()], locked)
    }
    pub fn set_locks(&mut self, selected: &[String], locked: bool) -> Result<Vec<String>, Error> {
        if selected.is_empty()
            || selected.len() > 20000
            || selected.iter().any(|id| !locks::uuid(id))
        {
            return Err(fail("invalidSelection"));
        }
        let _guard = source::operation_lock(&self.receipts)?;
        let mut ids = locks::read(&self.receipts, &self.source_id())?;
        for id in selected {
            let id = id.to_ascii_lowercase();
            if locked {
                ids.insert(id);
            } else {
                ids.remove(&id);
            }
        }
        let event_id = events::new_id().unwrap_or_default();
        let _ = self.record(
            event_id.clone(),
            if locked { "lock" } else { "unlock" },
            "started",
            selected.to_vec(),
            vec![],
            selected.len(),
            0,
            None,
        );
        if let Err(e) = locks::write(&self.receipts, &self.source_id(), ids.clone()) {
            let _ = self.record(
                event_id,
                if locked { "lock" } else { "unlock" },
                "verificationFailed",
                selected.to_vec(),
                vec![],
                selected.len(),
                0,
                Some(e.code.clone()),
            );
            return Err(e);
        }
        self.invalidate();
        let _ = self.record(
            event_id,
            if locked { "lock" } else { "unlock" },
            "verified",
            selected.to_vec(),
            vec![],
            selected.len(),
            0,
            None,
        );
        Ok(ids.into_iter().collect())
    }
    fn evaluate(
        &self,
        selected: &[String],
        kept: &[String],
        _runtime_version: &str,
    ) -> Result<Evaluation, Error> {
        if self.closed {
            return Err(fail("runtimeStopped"));
        }
        if selected.is_empty()
            || selected.len() > 128
            || selected.iter().any(|id| !locks::uuid(id))
            || kept.len() > 20000
        {
            return Err(fail("invalidSelection"));
        }
        source::guard_root(&self.root)?;
        if identity(&self.root)? != self.root_identity {
            return Err(fail("sourceChanged"));
        }
        source::check_config(&self.root)?;
        let files = stamps(&self.root)?;
        let db = database(&self.root)?;
        let (roots, ids) = db.selection(selected)?;
        let inventory = catalog::inspect(&self.root, &db, &files)?;
        let mut locked = locks::read(&self.receipts, &self.source_id())?;
        locked.extend(kept.iter().map(|id| id.to_ascii_lowercase()));
        let header_parents: Vec<_> = inventory
            .threads
            .iter()
            .filter(|t| t.relation != agentbroom_core::Relation::Fork)
            .filter_map(|t| {
                t.parent_id
                    .as_ref()
                    .map(|parent| (parent.clone(), t.id.clone()))
            })
            .collect();
        let locked_ids = catalog::lock_conflicts(&db, &ids, &locked, &header_parents);
        let mut blocked = BTreeSet::new();
        if !locked_ids.is_empty() {
            blocked.insert("lockedConversation".into());
        }
        if ids.len() > 1024 {
            blocked.insert("nativeScopeBudget".into());
        }
        let mut selected_files = BTreeSet::new();
        let mut file_owners = BTreeMap::new();
        for id in &ids {
            let path = db.rows[id]
                .get("rollout_path")
                .and_then(Value::as_str)
                .ok_or_else(|| fail("nativePathMismatch"))?;
            let key = source::rollout_key(&self.root, path, &files)?;
            file_owners.insert(key.clone(), id.clone());
            selected_files.insert(key);
            for t in inventory.threads.iter().filter(|t| t.id == *id) {
                for key in &t.files {
                    file_owners.insert(key.clone(), id.clone());
                }
                selected_files.extend(t.files.iter().cloned());
            }
        }
        if inventory
            .threads
            .iter()
            .any(|t| !ids.contains(&t.id) && t.files.iter().any(|f| selected_files.contains(f)))
        {
            return Err(fail("fileOwnershipUnverified"));
        }
        let observed_files: Vec<_> = selected_files
            .iter()
            .map(|key| {
                let stamp = files
                    .get(key)
                    .ok_or_else(|| fail("fileOwnershipUnverified"))?;
                Ok(agentbroom_core::DiskItem {
                    key: key.clone(),
                    category: if key.starts_with("archived_sessions/") {
                        agentbroom_core::Category::Archives
                    } else {
                        agentbroom_core::Category::Sessions
                    },
                    logical_bytes: stamp.length,
                    allocated_bytes: None,
                })
            })
            .collect::<Result<_, Error>>()?;
        let outside_dependents: Vec<_> = inventory
            .threads
            .iter()
            .filter(|t| {
                !ids.contains(&t.id)
                    && t.fork_sources
                        .iter()
                        .chain(t.history_sources.iter())
                        .chain(t.forked_from_id.iter())
                        .chain(t.shared_history_from.iter())
                        .any(|id| ids.contains(id))
            })
            .map(|t| t.id.clone())
            .collect();
        let mut warnings = Vec::new();
        if !outside_dependents.is_empty() {
            warnings.push("Other conversations reference this scope. The native store decides whether deletion is possible.".into());
        }
        if selected_files.iter().any(|f| files[f].recent()) {
            warnings.push("This scope has recent file writes. Check that you no longer need these conversations.".into());
        }
        if inventory.partial_coverage
            || inventory
                .threads
                .iter()
                .any(|t| ids.contains(&t.id) && t.relation == agentbroom_core::Relation::Unknown)
        {
            warnings.push("Some header relationships are incomplete. This preview uses the native indexed deletion scope.".into());
        }
        let analysis = cleanup::CleanupAnalysis {
            schema_version: 1,
            source_kind: "codex".into(),
            snapshot: inventory.source_fingerprint,
            selected_ids: selected.to_vec(),
            included_ids: ids.clone(),
            retained_ids: db
                .rows
                .keys()
                .filter(|id| !ids.contains(id))
                .cloned()
                .collect(),
            outside_dependents,
            observed_logical_bytes: observed_files.iter().map(|f| f.logical_bytes).sum(),
            observed_files,
            executable_files: vec![],
            blocked_reasons: blocked.iter().cloned().collect(),
            completeness: "observedOnly".into(),
            can_execute: false,
        };
        Ok(Evaluation {
            analysis,
            db,
            files,
            ids,
            roots,
            blocked: blocked.into_iter().collect(),
            warnings,
            locked_ids,
            file_owners,
            header_parents,
        })
    }
    pub fn preview(&mut self, selected: &[String], kept: &[String]) -> Result<NativePlan, Error> {
        self.invalidate();
        let runtime = source::probe_version(&self.binary)?;
        let binary_hash = hash_file(&self.binary, 400 * 1024 * 1024)?;
        let runtime_profile = match &self.runtime_contract {
            Some((hash, profile)) if *hash == binary_hash => profile.clone(),
            _ => {
                let profile = source::probe_storage(&self.binary)?;
                if hash_file(&self.binary, 400 * 1024 * 1024)? != binary_hash {
                    return Err(fail("runtimeChanged"));
                }
                self.runtime_contract = Some((binary_hash.clone(), profile.clone()));
                profile
            }
        };
        let evaluation = self.evaluate(selected, kept, &runtime)?;
        if evaluation.db.storage_profile != runtime_profile {
            return Err(fail("runtimeStorageMismatch"));
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
        let mut view = NativePlan {
            plan_id,
            test_only: false,
            runtime_version: runtime.clone(),
            storage_profile: evaluation.db.storage_profile.into(),
            native_ids: evaluation.ids.clone(),
            root_ids: evaluation.roots.clone(),
            retained_ids: evaluation
                .db
                .rows
                .keys()
                .filter(|id| !evaluation.ids.contains(id))
                .cloned()
                .collect(),
            blocked_reasons: evaluation.blocked.clone(),
            warnings: evaluation.warnings.clone(),
            locked_ids: evaluation.locked_ids.clone(),
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
        if self
            .record(
                String::new(),
                "preview",
                if view.executable { "ready" } else { "blocked" },
                selected.to_vec(),
                vec![],
                view.native_ids.len(),
                view.analysis.observed_logical_bytes,
                None,
            )
            .is_err()
        {
            view.warnings.push(events::HISTORY_WARNING.into());
        }
        self.pending = Some(SavedPlan {
            view: view.clone(),
            kept: kept.to_vec(),
            created: Instant::now(),
            selected_hashes,
            selected_rows,
            binary_hash,
            runtime_version: runtime,
            file_owners: evaluation.file_owners,
            header_parents: evaluation.header_parents,
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
        if e.blocked.contains(&"lockedConversation".into()) {
            return Err(fail("lockedConversation"));
        }
        if e.db.storage_profile != plan.view.storage_profile
            || !e.blocked.is_empty()
            || e.ids != plan.view.native_ids
            || e.roots != plan.view.root_ids
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
    pub fn execute(&mut self, c: Confirmation) -> Result<Receipt, ExecutionFailure> {
        let mut intent = false;
        let result = self.execute_inner(c.clone(), &mut intent);
        result.map_err(|e| {
            if !intent {
                // A rejected preflight has no execution receipt. Preserve its reason in the
                // seven-day activity log without overwriting an earlier durable intent.
                let ids: Vec<_> = c
                    .selected_ids
                    .iter()
                    .filter(|id| locks::uuid(id))
                    .take(128)
                    .cloned()
                    .collect();
                let _ = self.record(
                    String::new(),
                    "cleanup",
                    "blocked",
                    ids.clone(),
                    vec![],
                    ids.len(),
                    0,
                    Some(e.code.clone()),
                );
            }
            ExecutionFailure::new(
                e,
                if intent {
                    ExecutionPhase::Unknown
                } else {
                    ExecutionPhase::NotStarted
                },
            )
        })
    }
    fn execute_inner(&mut self, c: Confirmation, intent: &mut bool) -> Result<Receipt, Error> {
        valid_id(&c.plan_id)?;
        if c.phrase != "DELETE"
            || !c.acknowledged
            || c.selected_ids.is_empty()
            || c.selected_ids.len() > 128
        {
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
        let mut related_before = related::snapshot(&self.root, before.db.storage_profile)?;
        let plan = self.pending.take().unwrap();
        // Capture per-file content before intent, not stale global-scan timestamps.
        // Every proof stays anchored to this extent/digest through native execution.
        let mut retained_prefixes = BTreeMap::new();
        let current_files = stamps(&self.root)?;
        for (path, stamp) in before
            .files
            .iter()
            .filter(|(path, _)| !plan.selected_hashes.contains_key(*path))
        {
            let current = current_files
                .get(path)
                .ok_or_else(|| fail("sourceChanged"))?;
            if stamp.recent() || current.recent() || current != stamp {
                retained_prefixes.insert(
                    path.clone(),
                    source::capture_retained_prefix(&self.root.join(path), stamp)?,
                );
            }
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
            warnings: vec![],
            source_id: self.source_id(),
            expected_ids: plan.view.native_ids.clone(),
            native_notifications_verified: false,
            native_deleted_ids: vec![],
            files_verified: false,
            state_verified: false,
            related_verified: false,
        };
        // Once durable-intent persistence is attempted, remain conservative even if the
        // filesystem reports an error after persisting it. Never offer an automatic retry.
        *intent = true;
        self.write_receipt(&receipt)?;
        let titles: Vec<String> = c
            .selected_ids
            .iter()
            .map(|id| {
                let title = before
                    .db
                    .rows
                    .get(id)
                    .and_then(|r| r.get("title"))
                    .and_then(Value::as_str)
                    .filter(|title| !title.trim().is_empty())
                    .unwrap_or(id);
                title
                    .chars()
                    .filter(|c| !c.is_control())
                    .take(512)
                    .collect()
            })
            .collect();
        if self
            .record(
                receipt.plan_id.clone(),
                "cleanup",
                "unknown",
                receipt.selected_ids.clone(),
                titles.clone(),
                plan.view.native_ids.len(),
                0,
                None,
            )
            .is_err()
        {
            receipt.warnings.push(events::HISTORY_WARNING.into());
        }
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
                if self
                    .record(
                        receipt.plan_id.clone(),
                        "cleanup",
                        &receipt.status,
                        receipt.selected_ids.clone(),
                        titles.clone(),
                        0,
                        0,
                        receipt.error.clone(),
                    )
                    .is_err()
                    && receipt.warnings.is_empty()
                {
                    receipt.warnings.push(events::HISTORY_WARNING.into());
                }
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
        match related::initialized_logs(&self.root, &mut related_before) {
            Ok(true) => receipt.warnings.push("Codex startup changed log records before deletion. Log verification uses the state after startup; other associated data keeps its original baseline.".into()),
            Ok(false) => (),
            Err(_) => errors.push("startupRelatedReadbackUnavailable"),
        }
        if errors.is_empty() {
            let mut expected = BTreeSet::new();
            for root in &plan.view.root_ids {
                // Re-read the remaining authoritative subtree before each RPC. Earlier deletes
                // may remove rows, but must not broaden another root or change its manual locks.
                if plan.created.elapsed() > Duration::from_secs(300) {
                    errors.push("batchTimeBudget");
                    break;
                }
                let step = database(&self.root).and_then(|db| {
                    let ids = db.subtree(root)?;
                    Ok((db, ids))
                });
                let Ok((current_db, current_ids)) = step else {
                    errors.push("batchScopeChanged");
                    break;
                };
                let original: BTreeSet<_> = before.db.subtree(root)?.into_iter().collect();
                let locks_now = locks::read(&self.receipts, &self.source_id());
                let Ok(mut locks_now) = locks_now else {
                    errors.push("lockStateUnavailable");
                    break;
                };
                locks_now.extend(plan.kept.iter().map(|id| id.to_ascii_lowercase()));
                if current_ids.iter().cloned().collect::<BTreeSet<_>>() != original
                    || !catalog::lock_conflicts(
                        &current_db,
                        &current_ids,
                        &locks_now,
                        &plan.header_parents,
                    )
                    .is_empty()
                    || identity(&self.root).ok() != Some(self.root_identity)
                {
                    errors.push("batchScopeChanged");
                    break;
                }
                let rows_unchanged = current_ids
                    .iter()
                    .all(|id| current_db.rows.get(id) == plan.selected_rows.get(id));
                let unchanged = plan
                    .file_owners
                    .iter()
                    .filter(|(_, owner)| original.contains(*owner))
                    .all(|(path, _)| {
                        plan.selected_hashes.get(path).is_some_and(|expected| {
                            hash_file(&self.root.join(path), 4 * 1024 * 1024 * 1024)
                                .is_ok_and(|actual| &actual == expected)
                        })
                    });
                if !unchanged || !rows_unchanged {
                    errors.push("batchFilesChanged");
                    break;
                }
                match server.request("thread/delete", json!({"threadId":root})) {
                    Ok(v) if v == json!({}) => {
                        expected.extend(original);
                        match server.deleted(expected.len()) {
                            Ok(mut ids) => {
                                ids.sort();
                                receipt.native_deleted_ids = ids;
                                if receipt
                                    .native_deleted_ids
                                    .iter()
                                    .cloned()
                                    .collect::<BTreeSet<_>>()
                                    != expected
                                {
                                    errors.push("nativeNotificationsUnverified");
                                    break;
                                }
                            }
                            Err(_) => {
                                errors.push("nativeNotificationsUnavailable");
                                break;
                            }
                        }
                    }
                    _ => {
                        errors.push("nativeRejectedOrUnknown");
                        break;
                    }
                }
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
                        Some(new)
                            if retained_prefixes.get(path).is_some_and(|proof| {
                                source::verify_retained_prefix(&self.root.join(path), proof)
                                    .unwrap_or(false)
                            }) =>
                        {
                            if new != old {
                                receipt.concurrent_changes.push(path.clone());
                            }
                        }
                        Some(new) if new == old && !retained_prefixes.contains_key(path) => {}
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
        match related::snapshot(&self.root, before.db.storage_profile) {
            Ok(after) => {
                let problems = related::verify(&related_before, &after, &plan.view.native_ids);
                receipt.related_verified = problems.is_empty();
                errors.extend(problems);
            }
            Err(_) => errors.push("relatedReadbackUnavailable"),
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
        if self
            .record(
                receipt.plan_id.clone(),
                "cleanup",
                &receipt.status,
                receipt.selected_ids.clone(),
                titles,
                receipt.removed.len(),
                receipt.observed_removed_logical_bytes,
                receipt.error.clone(),
            )
            .is_err()
            && receipt.warnings.is_empty()
        {
            receipt.warnings.push(events::HISTORY_WARNING.into());
        }
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
