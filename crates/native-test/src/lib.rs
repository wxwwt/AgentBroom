//! Developer desktop capability. Every store is newly generated; no existing-root API.
//! Only the dedicated test build links this crate. This is not a live deletion permit.
#[cfg(not(unix))]
compile_error!("Native cleanup test currently supports macOS and Linux only");
use agentbroom_core::{
    Activity, Inventory, cleanup,
    native_scope::{self, NativeScopeEvidence},
};
use agentbroom_native::rpc;
use rusqlite::{Connection, OpenFlags, types::ValueRef};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};
use tempfile::TempDir;
pub type Error = agentbroom_core::InspectError;
fn fail(code: &str) -> Error {
    Error {
        code: code.into(),
        message: code.into(),
    }
}
fn io<T>(result: std::io::Result<T>) -> Result<T, Error> {
    result.map_err(|_| fail("sandboxIO"))
}
pub const ROOT: &str = "00000000-0000-4000-8000-000000000001";
pub const CHILD: &str = "00000000-0000-4000-8000-000000000002";
pub const FORK: &str = "00000000-0000-4000-8000-000000000004";
pub const KEEP: &str = "00000000-0000-4000-8000-000000000005";
const TTL: Duration = Duration::from_secs(120);
pub use agentbroom_native::{Confirmation, NativePlan, Receipt};
#[derive(Clone, PartialEq, Debug)]
struct Snapshot {
    files: BTreeMap<String, (String, u64)>,
    rows: BTreeMap<String, Vec<Value>>,
    edges: Vec<(String, String, String)>,
}
struct StoredPlan {
    view: NativePlan,
    before: Snapshot,
    created: Instant,
    runtime_hash: String,
}
/// Field order ensures the native process exits before its private home is removed.
pub struct NativeSandbox {
    server: Option<rpc::Rpc>,
    temp: TempDir,
    home: PathBuf,
    corpus: PathBuf,
    binary: PathBuf,
    runtime_hash: String,
    runtime_version: String,
    pending: Option<StoredPlan>,
    receipts: BTreeMap<String, Receipt>,
    delete_requests: usize,
}
impl NativeSandbox {
    pub fn generate(binary: &Path) -> Result<Self, Error> {
        Self::generate_case(binary, false)
    }
    /// Both cases are code-generated; the boolean never chooses a filesystem root.
    pub fn generate_case(binary: &Path, shared_reference: bool) -> Result<Self, Error> {
        if !binary.is_absolute() {
            return Err(fail("runtimeUnavailable"));
        }
        let binary = io(binary.canonicalize())?;
        let runtime_hash = digest_file(&binary, 400 * 1024 * 1024)?.0;
        let temp = io(tempfile::Builder::new()
            .prefix("agentbroom-native-desktop-")
            .tempdir_in("/tmp"))?;
        let root = io(temp.path().canonicalize())?;
        for dir in [
            "home", "codex", "tmp", "config", "cache", "data", "project", "corpus",
        ] {
            io(fs::create_dir(root.join(dir)))?;
        }
        let home = root.join("codex");
        let corpus = root.join("corpus");
        io(fs::write(
            home.join("config.toml"),
            format!(
                "cli_auth_credentials_store = \"file\"\nsqlite_home = {}\nmodel_provider = \"synthetic_offline\"\nmodel = \"synthetic-model\"\n[model_providers.synthetic_offline]\nname = \"Synthetic offline\"\nbase_url = \"http://127.0.0.1:9/v1\"\nwire_api = \"responses\"\nrequires_openai_auth = false\n",
                json!(home.to_str().ok_or_else(|| fail("sandboxIO"))?)
            ),
        ))?;
        let mut version = command(&binary, &root);
        version.arg("--version");
        let actual_version = rpc::Rpc::version(version)?;
        let actual_version = actual_version
            .trim()
            .strip_prefix("codex-cli ")
            .ok_or_else(|| fail("runtimeVersion"))?;
        if !native_scope::supported_version(actual_version) {
            return Err(fail("runtimeVersion"));
        }
        write_rollout(&home, &root, ROOT, None, None, None)?;
        write_rollout(&home, &root, CHILD, Some(ROOT), None, None)?;
        write_rollout(&home, &root, KEEP, None, None, None)?;
        write_rollout(
            &home,
            &root,
            FORK,
            None,
            Some(if shared_reference { ROOT } else { KEEP }),
            shared_reference.then_some(ROOT),
        )?;
        let mut cmd = command(&binary, &root);
        cmd.args(["-c", "cli_auth_credentials_store=\"file\"", "-c"])
            .arg(format!("sqlite_home={}", json!(home.to_str().unwrap())))
            .args(["app-server", "--listen", "stdio://"]);
        let mut server = rpc::Rpc::start(cmd)?;
        for id in [ROOT, CHILD, KEEP, FORK] {
            server.request("thread/read", json!({"threadId":id,"includeTurns":false}))?;
        }
        let mut sandbox = Self {
            server: Some(server),
            temp,
            home,
            corpus,
            binary,
            runtime_hash,
            runtime_version: actual_version.to_owned(),
            pending: None,
            receipts: BTreeMap::new(),
            delete_requests: 0,
        };
        if sandbox
            .snapshot()?
            .rows
            .keys()
            .cloned()
            .collect::<BTreeSet<_>>()
            != [ROOT, CHILD, KEEP, FORK]
                .map(str::to_owned)
                .into_iter()
                .collect()
        {
            return Err(fail("nativeFixtureIncomplete"));
        }
        sandbox.inventory()?;
        Ok(sandbox)
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
        // Only generated rollout data is copied; runtime arg0 links elsewhere never weaken the scanner.
        for directory in ["sessions", "archived_sessions"] {
            if self.corpus.join(directory).exists() {
                io(fs::remove_dir_all(self.corpus.join(directory)))?;
            }
        }
        for (relative, _) in corpus_files(&self.home)? {
            let to = self.corpus.join(&relative);
            io(fs::create_dir_all(to.parent().unwrap()))?;
            io(fs::copy(self.home.join(&relative), to))?;
        }
        let mut inventory = agentbroom_core::codex::scan_store(&self.corpus)?;
        // These never-resumed generated fixtures have no live user activity. Keep every other protection.
        for thread in &mut inventory.threads {
            thread.activity = Activity::Completed;
            thread.title = match thread.id.as_str() {
                ROOT => "Synthetic main",
                CHILD => "Synthetic Sub Agent",
                FORK => "Synthetic fork",
                KEEP => "Synthetic retained main",
                _ => return Err(fail("nativeFixtureUnexpected")),
            }
            .into();
            thread.protection_reasons.retain(|r| {
                r != "Read-only inspection: cleanup eligibility is not evaluated"
                    && r != "Live status unknown: no runtime or database activity queried"
            });
        }
        inventory.dataset_label = "Generated native cleanup test · no real Agent data".into();
        Ok(inventory)
    }
    pub fn preview(&mut self, selected: &[String], kept: &[String]) -> Result<NativePlan, Error> {
        self.pending = None;
        if self.server.is_none() {
            return Err(fail("runtimeStopped"));
        }
        // One native root per plan prevents overlap and inter-call partial cascade ambiguity.
        if selected.len() != 1 || kept.len() > 128 {
            return Err(fail("invalidSelection"));
        }
        let inventory = self.inventory()?;
        let analysis = cleanup::plan(&inventory, selected, kept)?;
        let before = self.snapshot()?;
        let native_ids = subtree(&before, &selected[0])?;
        let scope = native_scope::assess(&NativeScopeEvidence {
            runtime_version: self.runtime_version.clone(),
            root_id: selected[0].clone(),
            observed_ids: analysis.included_ids.clone(),
            native_ids: native_ids.clone(),
            observed_complete: !inventory.partial_coverage,
            native_complete: true,
        });
        let mut blocked: BTreeSet<_> = analysis
            .blocked_reasons
            .iter()
            .filter(|r| !matches!(r.as_str(), "realStoreLocked" | "nativeEffectsUnverified"))
            .cloned()
            .collect();
        blocked.extend(scope.blocked_reasons);
        if !scope.aligned {
            blocked.insert("nativeScopeMismatch".into());
        }
        let nonce = io(tempfile::Builder::new()
            .prefix("plan-")
            .rand_bytes(20)
            .tempfile_in(self.temp.path()))?;
        let plan_id = nonce
            .path()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let included: BTreeSet<_> = analysis.included_ids.iter().cloned().collect();
        let view = NativePlan {
            plan_id,
            test_only: true,
            retained_ids: before
                .rows
                .keys()
                .filter(|id| !included.contains(*id))
                .cloned()
                .collect(),
            native_ids,
            executable: blocked.is_empty(),
            blocked_reasons: blocked.into_iter().collect(),
            warnings: vec![],
            locked_ids: vec![],
            expires_in_seconds: TTL.as_secs(),
            analysis,
        };
        self.pending = Some(StoredPlan {
            view: view.clone(),
            before,
            created: Instant::now(),
            runtime_hash: self.runtime_hash.clone(),
        });
        Ok(view)
    }
    pub fn history(&self) -> Result<Vec<Receipt>, Error> {
        Ok(self.receipts.values().take(20).cloned().collect())
    }
    pub fn receipt(&self, id: &str) -> Result<Receipt, Error> {
        if id.len() > 128 {
            return Err(fail("invalidPlan"));
        }
        self.receipts
            .get(id)
            .cloned()
            .ok_or_else(|| fail("receiptUnavailable"))
    }
    pub fn execute(&mut self, confirmation: Confirmation) -> Result<Receipt, Error> {
        if confirmation.phrase != "DELETE"
            || !confirmation.acknowledged
            || confirmation.selected_ids.len() != 1
            || confirmation.plan_id.len() > 128
        {
            return Err(fail("invalidConfirmation"));
        }
        if let Some(receipt) = self.receipts.get(&confirmation.plan_id) {
            // Completed plans are idempotent reads, even after the pending plan was replaced.
            if receipt.selected_ids != confirmation.selected_ids {
                return Err(fail("invalidPlan"));
            }
            return Ok(receipt.clone());
        }
        let plan = self.pending.as_ref().ok_or_else(|| fail("invalidPlan"))?;
        if plan.view.plan_id != confirmation.plan_id
            || plan.view.analysis.selected_ids != confirmation.selected_ids
        {
            return Err(fail("invalidPlan"));
        }
        if !plan.view.executable {
            return Err(fail("planBlocked"));
        }
        if plan.created.elapsed() > TTL {
            self.pending = None;
            return Err(fail("planExpired"));
        }
        if self.snapshot()? != plan.before
            || digest_file(&self.binary, 400 * 1024 * 1024)?.0 != plan.runtime_hash
        {
            self.pending = None;
            return Err(fail("staleSnapshot"));
        }
        let plan = self.pending.take().unwrap();
        let server = self.server.as_mut().ok_or_else(|| fail("runtimeStopped"))?;
        server.notifications.clear();
        self.delete_requests += 1;
        let response = server.request(
            "thread/delete",
            json!({"threadId":confirmation.selected_ids[0]}),
        );
        let mut errors = Vec::new();
        if !matches!(response, Ok(ref value) if *value==json!({})) {
            errors.push("nativeCallFailed");
        }
        let notifications = if errors.is_empty() {
            server.deleted(plan.view.native_ids.len())
        } else {
            Err(fail("nativeCallFailed"))
        };
        let mut native_deleted_ids = notifications.unwrap_or_default();
        native_deleted_ids.sort();
        let notifications_verified = native_deleted_ids == plan.view.native_ids;
        if !notifications_verified {
            errors.push("notificationsUnverified");
            // Stop an unknown in-flight delete before readback; never reuse this process to retry.
            self.server.take();
        }
        let expected_files: BTreeSet<_> = plan
            .view
            .analysis
            .observed_files
            .iter()
            .map(|f| f.key.clone())
            .collect();
        let expected_ids: BTreeSet<_> = plan.view.native_ids.iter().cloned().collect();
        let after = self.snapshot();
        let mut removed = vec![];
        let mut remaining = vec![];
        let mut removed_bytes = 0;
        let (files_verified, state_verified) = match &after {
            Ok(after) => {
                for path in &expected_files {
                    if after.files.contains_key(path) {
                        remaining.push(path.clone());
                    } else {
                        removed.push(path.clone());
                        removed_bytes += plan.before.files.get(path).map_or(0, |f| f.1);
                    }
                }
                let expected: BTreeMap<_, _> = plan
                    .before
                    .files
                    .iter()
                    .filter(|(p, _)| !expected_files.contains(*p))
                    .map(|(p, v)| (p.clone(), v.clone()))
                    .collect();
                let rows: BTreeMap<_, _> = plan
                    .before
                    .rows
                    .iter()
                    .filter(|(id, _)| !expected_ids.contains(*id))
                    .map(|(id, v)| (id.clone(), v.clone()))
                    .collect();
                let edges: Vec<_> = plan
                    .before
                    .edges
                    .iter()
                    .filter(|(p, c, _)| !expected_ids.contains(p) && !expected_ids.contains(c))
                    .cloned()
                    .collect();
                (
                    after.files == expected,
                    after.rows == rows && after.edges == edges,
                )
            }
            Err(_) => {
                errors.push("readbackUnavailable");
                remaining.extend(expected_files.iter().cloned());
                (false, false)
            }
        };
        if !files_verified {
            errors.push("filesUnverified");
        }
        if !state_verified {
            errors.push("stateUnverified");
        }
        let receipt = Receipt {
            plan_id: confirmation.plan_id,
            selected_ids: confirmation.selected_ids,
            test_only: true,
            status: if errors.is_empty() {
                "verified"
            } else if !removed.is_empty() {
                "partialFailure"
            } else {
                "verificationFailed"
            }
            .into(),
            removed,
            remaining,
            observed_removed_logical_bytes: removed_bytes,
            freed_bytes: None,
            error: (!errors.is_empty()).then(|| errors.join(",")),
            concurrent_changes: vec![],
            source_id: String::new(),
            expected_ids: plan.view.native_ids.clone(),
            native_notifications_verified: notifications_verified,
            native_deleted_ids,
            files_verified,
            state_verified,
        };
        if self.receipts.len() >= 20 {
            if let Some(id) = self.receipts.keys().next().cloned() {
                self.receipts.remove(&id);
            }
        }
        self.receipts
            .insert(receipt.plan_id.clone(), receipt.clone());
        Ok(receipt)
    }
    fn snapshot(&self) -> Result<Snapshot, Error> {
        let dbs: Vec<_> = io(fs::read_dir(&self.home))?
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| {
                p.file_name().is_some_and(|n| {
                    n.to_string_lossy().starts_with("state_")
                        && n.to_string_lossy().ends_with(".sqlite")
                })
            })
            .collect();
        if dbs.len() != 1 {
            return Err(fail("nativeStateUnavailable"));
        }
        let db = Connection::open_with_flags(&dbs[0], OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|_| fail("nativeStateUnavailable"))?;
        db.busy_timeout(Duration::from_secs(2))
            .map_err(|_| fail("nativeStateUnavailable"))?;
        let mut rows = BTreeMap::new();
        let mut statement = db
            .prepare("SELECT * FROM threads ORDER BY id")
            .map_err(|_| fail("nativeStateUnavailable"))?;
        let columns = statement.column_count();
        let id_column = statement
            .column_index("id")
            .map_err(|_| fail("nativeStateUnavailable"))?;
        let mut cursor = statement
            .query([])
            .map_err(|_| fail("nativeStateUnavailable"))?;
        while let Some(row) = cursor.next().map_err(|_| fail("nativeStateUnavailable"))? {
            if rows.len() >= 100 {
                return Err(fail("nativeStateBudget"));
            }
            let mut values = vec![];
            for i in 0..columns {
                values.push(
                    match row.get_ref(i).map_err(|_| fail("nativeStateUnavailable"))? {
                        ValueRef::Null => Value::Null,
                        ValueRef::Integer(n) => json!(n),
                        ValueRef::Real(n) => json!(n),
                        ValueRef::Text(s) => json!(
                            std::str::from_utf8(s).map_err(|_| fail("nativeStateUnavailable"))?
                        ),
                        ValueRef::Blob(s) => json!(s),
                    },
                );
            }
            let id = values[id_column]
                .as_str()
                .ok_or_else(|| fail("nativeStateUnavailable"))?
                .to_owned();
            rows.insert(id, values);
        }
        let mut statement=db.prepare("SELECT parent_thread_id,child_thread_id,status FROM thread_spawn_edges ORDER BY parent_thread_id,child_thread_id").map_err(|_| fail("nativeStateUnavailable"))?;
        let edges = statement
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .map_err(|_| fail("nativeStateUnavailable"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| fail("nativeStateUnavailable"))?;
        Ok(Snapshot {
            files: corpus_files(&self.home)?,
            rows,
            edges,
        })
    }
}
fn command(binary: &Path, root: &Path) -> Command {
    let mut command = Command::new(binary);
    command.current_dir(root.join("project")).env_clear();
    for (key, dir) in [
        ("HOME", "home"),
        ("USERPROFILE", "home"),
        ("CODEX_HOME", "codex"),
        ("TMPDIR", "tmp"),
        ("TMP", "tmp"),
        ("TEMP", "tmp"),
        ("XDG_CONFIG_HOME", "config"),
        ("XDG_CACHE_HOME", "cache"),
        ("XDG_DATA_HOME", "data"),
    ] {
        command.env(key, root.join(dir));
    }
    command.env("PATH", "/usr/bin:/bin").env("LANG", "C.UTF-8");
    command
}
fn digest_file(path: &Path, limit: u64) -> Result<(String, u64), Error> {
    use std::{io::Read, os::unix::fs::MetadataExt};
    let meta = io(fs::symlink_metadata(path))?;
    if !meta.is_file() || meta.nlink() != 1 || meta.len() > limit {
        return Err(fail("unsafeTestFile"));
    }
    let mut file = io(fs::File::open(path))?;
    let mut hash = Sha256::new();
    let mut size = 0;
    let mut buf = [0u8; 65536];
    loop {
        let n = io(file.read(&mut buf))?;
        if n == 0 {
            break;
        }
        size += n as u64;
        if size > limit {
            return Err(fail("unsafeTestFile"));
        }
        hash.update(&buf[..n]);
    }
    Ok((format!("{:x}", hash.finalize()), size))
}
fn corpus_files(home: &Path) -> Result<BTreeMap<String, (String, u64)>, Error> {
    let mut files = BTreeMap::new();
    let mut todo = vec![home.join("sessions"), home.join("archived_sessions")];
    while let Some(path) = todo.pop() {
        let meta = match fs::symlink_metadata(&path) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => return Err(fail("sandboxIO")),
        };
        if meta.file_type().is_symlink() {
            return Err(fail("unsafeTestFile"));
        }
        if meta.is_dir() {
            for entry in io(fs::read_dir(path))? {
                todo.push(io(entry)?.path());
            }
        } else if meta.is_file() {
            if files.len() >= 100 {
                return Err(fail("nativeStateBudget"));
            }
            let key = path
                .strip_prefix(home)
                .map_err(|_| fail("unsafeTestFile"))?
                .to_str()
                .ok_or_else(|| fail("unsafeTestFile"))?
                .to_owned();
            files.insert(key, digest_file(&path, 1024 * 1024)?);
        } else {
            return Err(fail("unsafeTestFile"));
        }
        if todo.len() > 100 {
            return Err(fail("nativeStateBudget"));
        }
    }
    Ok(files)
}
fn subtree(snapshot: &Snapshot, root: &str) -> Result<Vec<String>, Error> {
    if !snapshot.rows.contains_key(root) {
        return Err(fail("invalidSelection"));
    }
    let mut seen = BTreeSet::from([root.to_owned()]);
    let mut queue = vec![root.to_owned()];
    while let Some(id) = queue.pop() {
        for (_, child, _) in snapshot.edges.iter().filter(|(parent, _, status)| {
            parent == &id && matches!(status.as_str(), "open" | "closed")
        }) {
            if !snapshot.rows.contains_key(child) || !seen.insert(child.clone()) || seen.len() > 100
            {
                return Err(fail("nativeScopeUnresolved"));
            }
            queue.push(child.clone());
        }
    }
    Ok(seen.into_iter().collect())
}
fn write_rollout(
    home: &Path,
    root: &Path,
    id: &str,
    parent: Option<&str>,
    fork: Option<&str>,
    history: Option<&str>,
) -> Result<(), Error> {
    let dir = home.join("sessions/2026/01/01");
    io(fs::create_dir_all(&dir))?;
    let mut meta = json!({"id":id,"session_id":if parent.is_some(){ROOT}else{id},"timestamp":"2026-01-01T00:00:00Z","cwd":root.join("project"),"originator":"agentbroom-synthetic-test","cli_version":native_scope::VERIFIED_NATIVE_VERSION,"source":parent.map(|p|json!({"subagent":{"thread_spawn":{"parent_thread_id":p,"depth":1}}})).unwrap_or(json!("cli")),"model_provider":"synthetic_offline"});
    if let Some(parent) = parent {
        meta["parent_thread_id"] = json!(parent);
    }
    if let Some(fork) = fork {
        meta["forked_from_id"] = json!(fork);
        meta["forked_from_ordinal_exclusive"] = json!(1);
    }
    if let Some(history) = history {
        let content = io(fs::read(
            dir.join(format!("rollout-2026-01-01T00-00-00-{history}.jsonl")),
        ))?;
        let offset = content
            .iter()
            .position(|b| *b == b'\n')
            .ok_or_else(|| fail("nativeFixtureIncomplete"))?
            + 1;
        meta["history_base"] =
            json!({"thread_id":history,"end_ordinal_exclusive":1,"end_byte_offset":offset});
    }
    io(fs::write(
        dir.join(format!("rollout-2026-01-01T00-00-00-{id}.jsonl")),
        format!(
            "{}\n{}\n",
            json!({"timestamp":"2026-01-01T00:00:00Z","type":"session_meta","payload":meta}),
            json!({"timestamp":"2026-01-01T00:00:01Z","type":"event_msg","payload":{"type":"user_message","message":"Synthetic desktop cleanup test only","kind":"plain"}})
        ),
    ))
}

#[cfg(test)]
mod tests;
