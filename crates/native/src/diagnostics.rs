//! Fixed-scope Codex crash reports. No renderer-supplied filesystem paths.
use crate::{Error, events, fail, source};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::{Duration, Instant, UNIX_EPOCH},
};
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub key: String,
    pub name: String,
    pub location: String,
    pub bytes: u64,
    pub allocated_bytes: u64,
    pub modified_at: u64,
    pub external: bool,
    #[serde(skip)]
    path: PathBuf,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportInventory {
    pub files: Vec<Report>,
    pub total_bytes: u64,
    pub external_bytes: u64,
    pub external_allocated_bytes: u64,
    pub complete: bool,
    pub warnings: Vec<String>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportPlan {
    pub plan_id: String,
    pub files: Vec<Report>,
    pub bytes: u64,
    pub expires_in_seconds: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportResult {
    pub plan_id: String,
    pub status: String,
    pub removed: Vec<String>,
    pub remaining: Vec<String>,
    pub observed_removed_logical_bytes: u64,
    pub error: Option<String>,
    #[serde(default)]
    pub warnings: Vec<String>,
}
struct Saved {
    view: ReportPlan,
    at: Instant,
    proof: BTreeMap<String, (String, (u64, u64, u64, u64))>,
}
pub struct Diagnostics {
    roots: Vec<(PathBuf, String, bool)>,
    home: PathBuf,
    app: PathBuf,
    pending: Option<Saved>,
}
fn fingerprint(path: &Path) -> Result<(u64, u64, u64, u64), Error> {
    use std::os::unix::fs::MetadataExt;
    let m = fs::symlink_metadata(path).map_err(|_| fail("reportChanged"))?;
    if !m.is_file() || m.nlink() != 1 {
        return Err(fail("linkedOrSpecialFile"));
    }
    let timestamp = m
        .modified()
        .ok()
        .and_then(|v| v.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_nanos() as u64);
    Ok((m.dev(), m.ino(), m.len(), timestamp))
}
fn owned(name: &str, dedicated: bool) -> bool {
    let n = name.to_ascii_lowercase();
    let extension = Path::new(&n)
        .extension()
        .and_then(|v| v.to_str())
        .unwrap_or("");
    let format = matches!(extension, "ips" | "crash")
        || (dedicated
            && (matches!(extension, "dmp" | "log")
                || (matches!(extension, "json" | "meta")
                    && Path::new(&n)
                        .file_stem()
                        .and_then(|v| v.to_str())
                        .is_some_and(crate::locks::uuid))));
    format
        && (dedicated
            || ["codex", "codexcli"].iter().any(|prefix| {
                n.strip_prefix(prefix)
                    .is_some_and(|r| r.starts_with(['-', '_', '.']))
            }))
}
fn report_process(path: &Path) -> Result<bool, Error> {
    use std::io::{BufRead, BufReader, Read};
    let file = source::safe_file(path)?;
    let mut reader = BufReader::new(file.take(65537));
    let mut line = vec![];
    reader
        .read_until(b'\n', &mut line)
        .map_err(|_| fail("reportUnread"))?;
    if line.len() > 65536 {
        return Err(fail("reportUnread"));
    }
    let exact = |name: &str| matches!(name.to_ascii_lowercase().as_str(), "codex" | "codexcli");
    if path
        .extension()
        .and_then(|s| s.to_str())
        .is_some_and(|s| s.eq_ignore_ascii_case("ips"))
    {
        let header: serde_json::Value =
            serde_json::from_slice(&line).map_err(|_| fail("reportUnread"))?;
        return header
            .get("app_name")
            .or_else(|| header.get("name"))
            .and_then(|v| v.as_str())
            .map(exact)
            .ok_or_else(|| fail("reportUnread"));
    }
    let text = std::str::from_utf8(&line).map_err(|_| fail("reportUnread"))?;
    let (key, value) = text.split_once(':').ok_or_else(|| fail("reportUnread"))?;
    if key.trim() != "Process" {
        return Err(fail("reportUnread"));
    }
    Ok(value.split_whitespace().next().is_some_and(exact))
}
impl Diagnostics {
    pub fn discover() -> Result<Self, Error> {
        let user = PathBuf::from(std::env::var_os("HOME").ok_or_else(|| fail("homeUnavailable"))?);
        let home = agentbroom_core::codex::default_root()?;
        let mut roots = vec![
            (home.join("crashes"), "Codex crash directory".into(), true),
            (
                home.join("crash_reports"),
                "Codex crash directory".into(),
                true,
            ),
        ];
        #[cfg(target_os = "macos")]
        {
            roots.push((
                user.join("Library/Logs/DiagnosticReports"),
                "User diagnostic reports".into(),
                false,
            ));
            roots.push((
                PathBuf::from("/Library/Logs/DiagnosticReports"),
                "System diagnostic reports".into(),
                false,
            ));
            for name in ["pending", "completed", "new"] {
                roots.push((
                    user.join("Library/Application Support/Codex/Crashpad")
                        .join(name),
                    "Codex Crashpad reports".into(),
                    true,
                ));
            }
        }
        Ok(Self {
            roots,
            home,
            app: user.join(".agentbroom"),
            pending: None,
        })
    }
    pub fn inventory(&self) -> Result<ReportInventory, Error> {
        self.inventory_with_limit(20000)
    }
    fn inventory_with_limit(&self, limit: usize) -> Result<ReportInventory, Error> {
        let mut files = vec![];
        let mut warnings = vec![];
        let mut seen = BTreeSet::new();
        let mut count = 0;
        'roots: for (root, label, dedicated) in &self.roots {
            if !root.exists() {
                continue;
            }
            if source::guard_root(root).is_err() {
                warnings.push(format!("{label}: unavailable"));
                continue;
            }
            let mut dirs = vec![root.clone()];
            if root.join("Retired").exists() {
                dirs.push(root.join("Retired"));
            }
            for dir in dirs {
                if source::guard_root(&dir).is_err() {
                    warnings.push(format!("{label}: unavailable"));
                    continue;
                }
                let entries = match fs::read_dir(&dir) {
                    Ok(v) => v,
                    Err(_) => {
                        warnings.push(format!("{label}: unavailable"));
                        continue;
                    }
                };
                for entry in entries {
                    let Ok(entry) = entry else {
                        warnings.push(format!("{label}: incomplete"));
                        continue;
                    };
                    let name = entry.file_name().to_string_lossy().into_owned();
                    if !owned(&name, *dedicated) {
                        continue;
                    }
                    let path = entry.path();
                    if !dedicated {
                        match report_process(&path) {
                            Ok(true) => (),
                            Ok(false) => continue,
                            Err(_) => {
                                warnings.push(format!("{label}: unread report skipped"));
                                continue;
                            }
                        }
                    }

                    if count >= limit {
                        warnings.push(
                            "Crash report scan limit reached; showing partial results.".into(),
                        );
                        break 'roots;
                    }
                    count += 1;
                    let Ok(stamp) = fingerprint(&path) else {
                        warnings.push(format!("{label}: linked or unread report skipped"));
                        continue;
                    };
                    if !seen.insert((stamp.0, stamp.1)) {
                        continue;
                    }
                    let key = format!("{:x}", Sha256::digest(path.as_os_str().as_encoded_bytes()));
                    files.push(Report {
                        key,
                        name,
                        location: label.clone(),
                        bytes: stamp.2,
                        allocated_bytes: {
                            use std::os::unix::fs::MetadataExt;
                            fs::symlink_metadata(&path).map_or(0, |m| m.blocks() * 512)
                        },
                        modified_at: stamp.3 / 1_000_000,
                        external: !path.starts_with(&self.home),
                        path,
                    });
                }
            }
        }
        files.sort_by(|a, b| {
            b.modified_at
                .cmp(&a.modified_at)
                .then_with(|| a.key.cmp(&b.key))
        });
        Ok(ReportInventory {
            total_bytes: files.iter().map(|f| f.bytes).sum(),
            external_bytes: files.iter().filter(|f| f.external).map(|f| f.bytes).sum(),
            external_allocated_bytes: files
                .iter()
                .filter(|f| f.external)
                .map(|f| f.allocated_bytes)
                .sum(),
            complete: warnings.is_empty(),
            files,
            warnings,
        })
    }
    pub fn preview(&mut self, keys: &[String]) -> Result<ReportPlan, Error> {
        self.pending = None;
        let selected: BTreeSet<_> = keys.iter().collect();
        if keys.is_empty() || keys.len() > 256 || selected.len() != keys.len() {
            return Err(fail("invalidSelection"));
        }
        let inventory = self.inventory()?;
        let files: Vec<_> = inventory
            .files
            .into_iter()
            .filter(|f| selected.contains(&f.key))
            .collect();
        if files.len() != keys.len() {
            return Err(fail("reportChanged"));
        }
        let mut proof = BTreeMap::new();
        for f in &files {
            let before = fingerprint(&f.path)?;
            let hash = source::hash_file(&f.path, 4 * 1024 * 1024 * 1024)?;
            if fingerprint(&f.path)? != before {
                return Err(fail("reportChanged"));
            }
            proof.insert(f.key.clone(), (hash, before));
        }
        let nonce = tempfile::Builder::new()
            .prefix("reports-")
            .rand_bytes(18)
            .tempfile()
            .map_err(|_| fail("planUnavailable"))?;
        let view = ReportPlan {
            plan_id: nonce.path().file_name().unwrap().to_string_lossy().into(),
            bytes: files.iter().map(|f| f.bytes).sum(),
            files,
            expires_in_seconds: 120,
        };
        self.pending = Some(Saved {
            view: view.clone(),
            at: Instant::now(),
            proof,
        });
        Ok(view)
    }
    fn save(&self, result: &ReportResult) -> Result<(), Error> {
        let dir = self.app.join("diagnostic-receipts");
        source::private_directory(&dir)?;
        let mut f =
            tempfile::NamedTempFile::new_in(&dir).map_err(|_| fail("receiptUnavailable"))?;
        f.write_all(&serde_json::to_vec(result).map_err(|_| fail("receiptUnavailable"))?)
            .and_then(|_| f.as_file().sync_all())
            .map_err(|_| fail("receiptUnavailable"))?;
        f.persist(dir.join(format!("{}.json", result.plan_id)))
            .map_err(|_| fail("receiptUnavailable"))?;
        fs::File::open(dir)
            .and_then(|f| f.sync_all())
            .map_err(|_| fail("receiptUnavailable"))?;
        Ok(())
    }
    pub fn result(&self, id: &str) -> Result<ReportResult, Error> {
        if !id.starts_with("reports-")
            || id.len() > 100
            || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        {
            return Err(fail("invalidPlan"));
        }
        serde_json::from_slice(&source::read_file(
            &self
                .app
                .join("diagnostic-receipts")
                .join(format!("{id}.json")),
            1024 * 1024,
        )?)
        .map_err(|_| fail("receiptUnavailable"))
    }
    pub fn cancel(&mut self, id: &str) {
        if self.pending.as_ref().is_some_and(|s| s.view.plan_id == id) {
            self.pending = None;
        }
    }
    pub fn execute(&mut self, id: &str, phrase: &str, ack: bool) -> Result<ReportResult, Error> {
        if phrase != "DELETE" || !ack {
            return Err(fail("invalidConfirmation"));
        }
        if let Ok(result) = self.result(id) {
            return Ok(result);
        }
        let _guard = source::operation_lock(&self.app.join("cleanup-receipts"))?;
        let saved = self.pending.as_ref().ok_or_else(|| fail("invalidPlan"))?;
        if saved.view.plan_id != id {
            return Err(fail("invalidPlan"));
        }
        if saved.at.elapsed() > Duration::from_secs(120) {
            return Err(fail("planExpired"));
        }
        let current = self.inventory()?;
        for f in &saved.view.files {
            if !current.files.iter().any(|g| g.key == f.key)
                || fingerprint(&f.path)? != saved.proof[&f.key].1
                || source::hash_file(&f.path, 4 * 1024 * 1024 * 1024)? != saved.proof[&f.key].0
            {
                return Err(fail("reportChanged"));
            }
        }
        let saved = self.pending.take().unwrap();
        let mut result = ReportResult {
            plan_id: id.into(),
            status: "unknown".into(),
            removed: vec![],
            remaining: saved.view.files.iter().map(|f| f.key.clone()).collect(),
            observed_removed_logical_bytes: 0,
            error: None,
            warnings: vec![],
        };
        self.save(&result)?;
        let event = |status: &str, count: usize, bytes: u64, error: Option<String>| {
            events::OperationEvent {
                id: id.into(),
                at: events::now(),
                action: "crashCleanup".into(),
                status: status.into(),
                ids: saved.view.files.iter().map(|f| f.key.clone()).collect(),
                titles: saved.view.files.iter().map(|f| f.name.clone()).collect(),
                count,
                bytes,
                error,
            }
        };
        if events::write(
            &self.app.join("activity"),
            event("unknown", saved.view.files.len(), 0, None),
        )
        .is_err()
        {
            result.warnings.push(events::HISTORY_WARNING.into());
        }
        let mut error = None;
        for f in &saved.view.files {
            // Recheck each exact file immediately before removal. Never traverse an IPC path.
            if fingerprint(&f.path).ok() != Some(saved.proof[&f.key].1)
                || source::hash_file(&f.path, 4 * 1024 * 1024 * 1024)
                    .ok()
                    .as_ref()
                    != Some(&saved.proof[&f.key].0)
            {
                error = Some("reportChanged".into());
                break;
            }
            if fs::remove_file(&f.path).is_err() {
                error = Some("reportDeleteFailed".into());
                break;
            }
            if fs::symlink_metadata(&f.path).is_ok() {
                error = Some("reportVerificationFailed".into());
                break;
            }
            result.removed.push(f.key.clone());
            result.observed_removed_logical_bytes += f.bytes;
        }
        result.remaining = saved
            .view
            .files
            .iter()
            .filter(|f| !result.removed.contains(&f.key))
            .map(|f| f.key.clone())
            .collect();
        result.status = if error.is_none() && result.remaining.is_empty() {
            "verified"
        } else if result.removed.is_empty() {
            "verificationFailed"
        } else {
            "partialFailure"
        }
        .into();
        result.error = error;
        if events::write(
            &self.app.join("activity"),
            event(
                &result.status,
                result.removed.len(),
                result.observed_removed_logical_bytes,
                result.error.clone(),
            ),
        )
        .is_err()
            && result.warnings.is_empty()
        {
            result.warnings.push(events::HISTORY_WARNING.into());
        }
        self.save(&result)?;
        Ok(result)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (tempfile::TempDir, Diagnostics) {
        let t = tempfile::tempdir().unwrap();
        let root = t.path().canonicalize().unwrap();
        let reports = root.join("reports");
        fs::create_dir(&reports).unwrap();
        for name in [
            "codex-1.ips",
            "CodexCLI_2.crash",
            "OtherApp-3.ips",
            "codex-notes.txt",
        ] {
            let contents = if name.ends_with(".crash") {
                b"Process: CodexCLI [1]\n".as_slice()
            } else {
                b"{\"app_name\":\"Codex\"}\n".as_slice()
            };
            fs::write(reports.join(name), contents).unwrap();
        }
        let d = Diagnostics {
            roots: vec![(reports, "Generated diagnostic reports".into(), false)],
            home: root.join("home"),
            app: root.join("app"),
            pending: None,
        };
        (t, d)
    }
    #[test]
    fn only_codex_reports_are_counted_and_confirmed_generated_files_removed() {
        let (_t, mut d) = fixture();
        let inv = d.inventory().unwrap();
        assert_eq!(inv.files.len(), 2);
        assert_eq!(inv.external_bytes, 43);
        let keys = inv.files.iter().map(|f| f.key.clone()).collect::<Vec<_>>();
        let p = d.preview(&keys).unwrap();
        assert!(d.execute(&p.plan_id, "delete", true).is_err());
        let r = d.execute(&p.plan_id, "DELETE", true).unwrap();
        assert_eq!(r.status, "verified");
        assert_eq!(r.removed.len(), 2);
        assert_eq!(
            d.execute(&p.plan_id, "DELETE", true).unwrap().removed.len(),
            2
        );
        assert!(d.roots[0].0.join("OtherApp-3.ips").exists());
    }
    #[test]
    fn changed_file_revokes_plan_and_links_are_never_candidates() {
        let (_t, mut d) = fixture();
        std::os::unix::fs::symlink(
            d.roots[0].0.join("codex-1.ips"),
            d.roots[0].0.join("codex-link.ips"),
        )
        .unwrap();
        let inv = d.inventory().unwrap();
        assert_eq!(inv.files.len(), 2);
        assert!(!inv.complete);
        let p = d.preview(&[inv.files[0].key.clone()]).unwrap();
        fs::write(&p.files[0].path, b"changed").unwrap();
        assert!(d.execute(&p.plan_id, "DELETE", true).is_err());
        assert!(p.files[0].path.exists());
    }
}

#[cfg(test)]
mod additional_tests {
    use super::*;
    #[test]
    fn unrelated_process_with_codex_filename_is_not_a_codex_crash() {
        let t = tempfile::tempdir().unwrap();
        let root = t.path().canonicalize().unwrap();
        let p = root.join("codex-other.ips");
        fs::write(&p, b"{\"app_name\":\"OtherApp\"}\n").unwrap();
        assert!(!report_process(&p).unwrap());
        assert!(!owned("CodexDreamSkinStudio-1.ips", false));
    }
    #[test]
    fn crashpad_metadata_is_counted_but_database_settings_are_excluded() {
        let t = tempfile::tempdir().unwrap();
        let root = t.path().canonicalize().unwrap();
        fs::write(
            root.join("00000000-0000-4000-8000-000000000001.json"),
            b"{}",
        )
        .unwrap();
        fs::write(root.join("settings.dat"), b"settings").unwrap();
        let mut d = Diagnostics {
            roots: vec![(root.clone(), "Codex Crashpad reports".into(), true)],
            home: root.join("home"),
            app: root.join("app"),
            pending: None,
        };
        let i = d.inventory().unwrap();
        assert_eq!(i.files.len(), 1);
        assert_eq!(i.total_bytes, 2);
        assert!(d.preview(&["not-a-report-key".into()]).is_err());
    }
}

#[cfg(test)]
mod history_failure_tests {
    use super::*;
    #[test]
    fn broken_activity_log_does_not_strand_a_confirmed_generated_report() {
        let t = tempfile::tempdir().unwrap();
        let root = t.path().canonicalize().unwrap();
        let reports = root.join("reports");
        fs::create_dir(&reports).unwrap();
        fs::write(reports.join("codex-generated.log"), b"generated").unwrap();
        let app = root.join("app");
        fs::create_dir(&app).unwrap();
        fs::write(app.join("activity"), b"unavailable directory").unwrap();
        let mut d = Diagnostics {
            roots: vec![(reports, "Generated".into(), true)],
            home: root.join("home"),
            app,
            pending: None,
        };
        let key = d.inventory().unwrap().files[0].key.clone();
        let p = d.preview(&[key]).unwrap();
        let result = d.execute(&p.plan_id, "DELETE", true).unwrap();
        assert_eq!(result.status, "verified");
        assert_eq!(result.removed.len(), 1);
        assert_eq!(result.warnings, vec![events::HISTORY_WARNING]);
        assert_eq!(d.result(&p.plan_id).unwrap().warnings, result.warnings);
        assert_eq!(
            d.execute(&p.plan_id, "DELETE", true).unwrap().status,
            "verified"
        );
    }
    #[test]
    fn unrelated_files_do_not_consume_report_limit_and_limit_returns_partial_results() {
        let t = tempfile::tempdir().unwrap();
        let root = t.path().canonicalize().unwrap();
        for n in 0..5 {
            fs::write(root.join(format!("other-{n}.ips")), b"other").unwrap();
        }
        for n in 0..2 {
            fs::write(
                root.join(format!("codex-{n}.ips")),
                b"{\"app_name\":\"Codex\"}\n",
            )
            .unwrap();
        }
        let d = Diagnostics {
            roots: vec![(root.clone(), "Generated".into(), false)],
            home: root.join("home"),
            app: root.join("app"),
            pending: None,
        };
        assert!(d.inventory_with_limit(2).unwrap().complete);
        let partial = d.inventory_with_limit(1).unwrap();
        assert_eq!(partial.files.len(), 1);
        assert!(!partial.complete);
        assert_eq!(partial.warnings.len(), 1);
    }
}
