use crate::{Error, fail};
use rusqlite::{Connection, OpenFlags, types::ValueRef};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, OpenOptions},
    io::Read,
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Component, Path, PathBuf},
    process::Command,
    time::Duration,
};
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Stamp {
    pub identity: (u64, u64),
    pub length: u64,
    modified: (i64, i64),
    changed: (i64, i64),
}
impl Stamp {
    pub fn recent(&self) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(i64::MAX, |d| d.as_secs() as i64);
        self.modified.0 > now - 300
    }
}
pub(crate) fn identity(p: &Path) -> Result<(u64, u64), Error> {
    let m = fs::symlink_metadata(p).map_err(|_| fail("sourceUnavailable"))?;
    if m.file_type().is_symlink() {
        return Err(fail("linkedSource"));
    }
    Ok((m.dev(), m.ino()))
}
pub(crate) fn guard_root(path: &Path) -> Result<(), Error> {
    if !path.is_absolute() {
        return Err(fail("sourceUnavailable"));
    }
    for part in path.ancestors() {
        if fs::symlink_metadata(part)
            .map_err(|_| fail("sourceUnavailable"))?
            .file_type()
            .is_symlink()
        {
            return Err(fail("linkedSource"));
        }
    }
    if !path.is_dir() {
        return Err(fail("sourceUnavailable"));
    }
    Ok(())
}
pub(crate) fn safe_file(p: &Path) -> Result<fs::File, Error> {
    for parent in p.parent().into_iter().flat_map(Path::ancestors) {
        if fs::symlink_metadata(parent)
            .map_err(|_| fail("sourceUnavailable"))?
            .file_type()
            .is_symlink()
        {
            return Err(fail("linkedSource"));
        }
    }
    let meta = fs::symlink_metadata(p).map_err(|_| fail("sourceUnavailable"))?;
    if !meta.is_file() || meta.nlink() != 1 {
        return Err(fail("linkedOrSpecialFile"));
    }
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(p)
        .map_err(|_| fail("sourceUnavailable"))?;
    let opened = file.metadata().map_err(|_| fail("sourceUnavailable"))?;
    if (meta.dev(), meta.ino(), meta.len()) != (opened.dev(), opened.ino(), opened.len()) {
        return Err(fail("sourceChanged"));
    }
    Ok(file)
}
pub(crate) fn read_file(p: &Path, max: u64) -> Result<Vec<u8>, Error> {
    let mut out = Vec::new();
    safe_file(p)?
        .take(max + 1)
        .read_to_end(&mut out)
        .map_err(|_| fail("sourceUnavailable"))?;
    if out.len() as u64 > max {
        Err(fail("metadataLimit"))
    } else {
        Ok(out)
    }
}
pub(crate) fn hash_file(p: &Path, max: u64) -> Result<String, Error> {
    let mut file = safe_file(p)?;
    let mut hash = Sha256::new();
    let mut bytes = 0;
    let mut buffer = [0u8; 65536];
    loop {
        let n = file
            .read(&mut buffer)
            .map_err(|_| fail("sourceUnavailable"))?;
        if n == 0 {
            break;
        }
        bytes += n as u64;
        if bytes > max {
            return Err(fail("hashBudget"));
        }
        hash.update(&buffer[..n]);
    }
    Ok(format!("{:x}", hash.finalize()))
}
pub(crate) fn stamps(root: &Path) -> Result<BTreeMap<String, Stamp>, Error> {
    let mut out = BTreeMap::new();
    let mut pending = vec![
        (root.join("sessions"), 0),
        (root.join("archived_sessions"), 0),
    ];
    while let Some((p, depth)) = pending.pop() {
        if depth > 32 || out.len() + pending.len() > 25000 {
            return Err(fail("nativeScopeBudget"));
        }
        let m = match fs::symlink_metadata(&p) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => return Err(fail("sourceUnavailable")),
        };
        if m.file_type().is_symlink() {
            return Err(fail("linkedSource"));
        }
        if m.is_dir() {
            for entry in fs::read_dir(p).map_err(|_| fail("sourceUnavailable"))? {
                pending.push((
                    entry.map_err(|_| fail("sourceUnavailable"))?.path(),
                    depth + 1,
                ));
            }
        } else {
            if !m.is_file() || m.nlink() != 1 {
                return Err(fail("linkedOrSpecialFile"));
            }
            let key = p
                .strip_prefix(root)
                .map_err(|_| fail("sourceUnavailable"))?
                .to_str()
                .ok_or_else(|| fail("sourceUnavailable"))?
                .to_owned();
            if !key.ends_with(".jsonl") && !key.ends_with(".jsonl.zst") {
                return Err(fail("unsupportedSessionFormat"));
            }
            out.insert(
                key,
                Stamp {
                    identity: (m.dev(), m.ino()),
                    length: m.len(),
                    modified: (m.mtime(), m.mtime_nsec()),
                    changed: (m.ctime(), m.ctime_nsec()),
                },
            );
        }
    }
    Ok(out)
}
/// Native indexes can retain the pre-compression rollout path. Resolve only the
/// format's exact compressed sibling; never guess identities from a filename.
pub(crate) fn rollout_key(
    root: &Path,
    stored: &str,
    files: &BTreeMap<String, Stamp>,
) -> Result<String, Error> {
    let relative = Path::new(stored)
        .strip_prefix(root)
        .map_err(|_| fail("nativePathMismatch"))?;
    let key = relative
        .to_str()
        .ok_or_else(|| fail("nativePathMismatch"))?;
    if files.contains_key(key) {
        return Ok(key.into());
    }
    if key.ends_with(".jsonl") {
        let compressed = format!("{key}.zst");
        if files.contains_key(&compressed) {
            return Ok(compressed);
        }
    }
    Err(fail("nativePathMismatch"))
}
pub(crate) fn check_config(root: &Path) -> Result<(), Error> {
    let p = root.join("config.toml");
    if !p.exists() {
        return Ok(());
    }
    let data = read_file(&p, 1024 * 1024)?;
    let config: toml::Table = std::str::from_utf8(&data)
        .map_err(|_| fail("configUnsupported"))?
        .parse()
        .map_err(|_| fail("configUnsupported"))?;
    if let Some(value) = config.get("sqlite_home") {
        let p = value.as_str().ok_or_else(|| fail("configUnsupported"))?;
        if Path::new(p) != root {
            return Err(fail("externalDatabaseUnsupported"));
        }
    }
    for key in ["remote", "thread_store", "remote_environment"] {
        if config.get(key).is_some() {
            return Err(fail("remoteStoreUnsupported"));
        }
    }
    Ok(())
}
pub(crate) fn installed_binary() -> Result<PathBuf, Error> {
    let candidates: Vec<PathBuf> = if let Some(path) = std::env::var_os("AGENTBROOM_CODEX_BIN") {
        vec![PathBuf::from(path)]
    } else {
        vec![
            PathBuf::from(
                "/Applications/ChatGPT.app/Contents/Resources/codex-cli/CodexCLI.app/Contents/MacOS/codex",
            ),
            PathBuf::from("/Applications/Codex.app/Contents/Resources/codex"),
            PathBuf::from("/usr/local/bin/codex"),
            PathBuf::from("/usr/bin/codex"),
        ]
    };
    for p in candidates {
        if p.is_absolute() && p.is_file() {
            return p
                .canonicalize()
                .map_err(|_| fail("nativeRuntimeUnavailable"));
        }
    }
    Err(fail("nativeRuntimeUnavailable"))
}
fn isolated_command(binary: &Path, home: &Path, work: &Path) -> Result<Command, Error> {
    let mut c = Command::new(binary);
    c.env_clear().current_dir(work);
    // All non-source runtime state is private; only CODEX_HOME points at the selected native store.
    for key in [
        "HOME",
        "USERPROFILE",
        "TMPDIR",
        "TMP",
        "TEMP",
        "XDG_CONFIG_HOME",
        "XDG_CACHE_HOME",
        "XDG_DATA_HOME",
    ] {
        c.env(key, work);
    }
    c.env("CODEX_HOME", home)
        .env("PATH", "/usr/bin:/bin")
        .env("LANG", "C.UTF-8");
    Ok(c)
}
pub(crate) fn probe_version(binary: &Path) -> Result<String, Error> {
    let temp = tempfile::tempdir().map_err(|_| fail("nativeRuntimeUnavailable"))?;
    let temp = temp
        .path()
        .canonicalize()
        .map_err(|_| fail("nativeRuntimeUnavailable"))?;
    let mut c = isolated_command(binary, &temp, &temp)?;
    c.arg("--version");
    let v = crate::rpc::Rpc::version(c)?;
    v.trim()
        .strip_prefix("codex-cli ")
        .map(str::to_owned)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| fail("runtimeVersion"))
}
/// Probe only a newly generated, empty home; never initialize/migrate the real source.
/// Bind the result to the executable digest in the owning session.
pub(crate) fn probe_storage(binary: &Path) -> Result<String, Error> {
    let temp = tempfile::Builder::new()
        .prefix("agentbroom-compatibility-")
        .tempdir()
        .map_err(|_| fail("nativeRuntimeUnavailable"))?;
    let base = temp
        .path()
        .canonicalize()
        .map_err(|_| fail("nativeRuntimeUnavailable"))?;
    let home = base.join("codex");
    let work = base.join("runtime");
    fs::create_dir(&home)
        .and_then(|_| fs::create_dir(&work))
        .map_err(|_| fail("nativeRuntimeUnavailable"))?;
    let mut rpc = crate::rpc::Rpc::start(native_command(binary, &home, &work)?)?;
    rpc.request("thread/list", json!({"limit":1}))?;
    drop(rpc);
    // Only these newly generated databases may be checkpointed. The real source
    // is never passed to this helper; auxiliary schema inspection uses immutable RO.
    for file in [
        "state_5.sqlite",
        "goals_1.sqlite",
        "logs_2.sqlite",
        "memories_1.sqlite",
        "queue_1.sqlite",
    ] {
        let db = Connection::open_with_flags(home.join(file), OpenFlags::SQLITE_OPEN_READ_WRITE)
            .map_err(|_| fail("nativeStateUnavailable"))?;
        db.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
            .map_err(|_| fail("nativeStateUnavailable"))?;
    }
    Ok(database(&home)?.storage_profile.into())
}
pub(crate) fn native_command(binary: &Path, root: &Path, work: &Path) -> Result<Command, Error> {
    let work = work
        .canonicalize()
        .map_err(|_| fail("nativeRuntimeUnavailable"))?;
    let mut c = isolated_command(binary, root, &work)?;
    // app-server explicitly rejects --profile; its global CLI overrides are the boundary.
    c.args(["-c","cli_auth_credentials_store=\"ephemeral\"","-c"]).arg(format!("sqlite_home={}",json!(root)))
 .args(["-c","analytics.enabled=false","-c","model_provider=\"agentbroom_offline\"","-c","model_providers.agentbroom_offline={name=\"AgentBroom offline cleanup\",base_url=\"http://127.0.0.1:9/v1\",wire_api=\"responses\",requires_openai_auth=false}","app-server","--listen","stdio://"]);
    Ok(c)
}
pub(crate) fn private_directory(path: &Path) -> Result<(), Error> {
    // Do not follow a pre-existing linked application-state directory.
    let mut current = PathBuf::new();
    for component in path.components() {
        match component {
            Component::RootDir => current.push("/"),
            Component::Normal(n) => {
                current.push(n);
                if current.exists() {
                    if fs::symlink_metadata(&current)
                        .map_err(|_| fail("receiptUnavailable"))?
                        .file_type()
                        .is_symlink()
                    {
                        return Err(fail("receiptUnavailable"));
                    }
                } else {
                    fs::create_dir(&current).map_err(|_| fail("receiptUnavailable"))?;
                    fs::set_permissions(&current, fs::Permissions::from_mode(0o700))
                        .map_err(|_| fail("receiptUnavailable"))?;
                }
            }
            _ => return Err(fail("receiptUnavailable")),
        }
    }
    Ok(())
}
#[derive(Clone)]
pub(crate) struct Database {
    pub storage_profile: &'static str,
    pub rows: BTreeMap<String, Value>,
    pub edges: Vec<(String, String, String)>,
}
impl Database {
    pub fn selection(&self, selected: &[String]) -> Result<(Vec<String>, Vec<String>), Error> {
        if selected.is_empty()
            || selected.len() > 128
            || selected.iter().any(|id| !crate::locks::uuid(id))
            || selected
                .iter()
                .map(|id| id.to_ascii_lowercase())
                .collect::<BTreeSet<_>>()
                .len()
                != selected.len()
        {
            return Err(fail("invalidSelection"));
        }
        let scopes: BTreeMap<String, BTreeSet<String>> = selected
            .iter()
            .map(|id| Ok((id.clone(), self.subtree(id)?.into_iter().collect())))
            .collect::<Result<_, Error>>()?;
        let roots: Vec<String> = selected
            .iter()
            .filter(|id| {
                !scopes
                    .iter()
                    .any(|(other, scope)| other != *id && scope.contains(*id))
            })
            .cloned()
            .collect();
        let ids = scopes
            .values()
            .flat_map(|s| s.iter().cloned())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        Ok((roots, ids))
    }
    pub fn subtree(&self, root: &str) -> Result<Vec<String>, Error> {
        if !self.rows.contains_key(root) {
            return Err(fail("nativeRootMissing"));
        }
        let mut seen = BTreeSet::from([root.to_owned()]);
        let mut todo = vec![root.to_owned()];
        while let Some(p) = todo.pop() {
            for (_, ch, _) in self.edges.iter().filter(|(parent, _, status)| {
                parent == &p && matches!(status.as_str(), "open" | "closed")
            }) {
                if !self.rows.contains_key(ch) || !seen.insert(ch.clone()) || seen.len() > 20000 {
                    return Err(fail("nativeScopeUnresolved"));
                }
                todo.push(ch.clone());
            }
        }
        Ok(seen.into_iter().collect())
    }
}
pub(crate) fn storage_profile(count: usize, digest: &str) -> Result<&'static str, Error> {
    match (count, digest) {
        (59, "b2a35ca432edfeb0545fcbabc63e3e1abb5f4b9de0146e33de8a8a8f9a87cb5e") => Ok("state-v59"),
        (61, "466e7cd4abaee882d5d7fc3048128a6794fb6fbe35b466ede2d31d9e41318050") => Ok("state-v61"),
        _ => Err(fail("nativeSchemaUnsupported")),
    }
}
pub(crate) fn database(root: &Path) -> Result<Database, Error> {
    auxiliary_schemas(root)?;
    let path = root.join("state_5.sqlite");
    drop(safe_file(&path)?);
    let db = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|_| fail("nativeStateUnavailable"))?;
    db.busy_timeout(Duration::from_secs(2))
        .map_err(|_| fail("nativeStateUnavailable"))?;
    db.execute_batch("PRAGMA query_only=ON; BEGIN DEFERRED;")
        .map_err(|_| fail("nativeStateUnavailable"))?;
    let mut q = db
        .prepare("SELECT version,success,checksum FROM _sqlx_migrations ORDER BY version")
        .map_err(|_| fail("nativeSchemaUnsupported"))?;
    let migrations = q
        .query_map([], |r| {
            let checksum: Vec<u8> = r.get(2)?;
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                checksum
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>(),
            ))
        })
        .map_err(|_| fail("nativeSchemaUnsupported"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| fail("nativeSchemaUnsupported"))?;
    let digest = format!(
        "{:x}",
        Sha256::digest(
            serde_json::to_vec(&migrations).map_err(|_| fail("nativeSchemaUnsupported"))?
        )
    );
    // Storage contracts, not runtime version labels. An upgrade with the same
    // schema needs no new version entry; genuinely new schemas still need validation.
    let storage_profile = storage_profile(migrations.len(), &digest)?;
    let mut q = db
        .prepare("SELECT * FROM threads ORDER BY id")
        .map_err(|_| fail("nativeStateUnavailable"))?;
    let columns: Vec<_> = q.column_names().iter().map(|n| n.to_string()).collect();
    let mut cursor = q.query([]).map_err(|_| fail("nativeStateUnavailable"))?;
    let mut rows = BTreeMap::new();
    let mut bytes = 0;
    while let Some(row) = cursor.next().map_err(|_| fail("nativeStateUnavailable"))? {
        if rows.len() >= 25000 {
            return Err(fail("nativeStateBudget"));
        }
        let mut object = serde_json::Map::new();
        for (i, name) in columns.iter().enumerate() {
            let v = match row.get_ref(i).map_err(|_| fail("nativeStateUnavailable"))? {
                ValueRef::Null => Value::Null,
                ValueRef::Integer(v) => json!(v),
                ValueRef::Real(v) => json!(v),
                ValueRef::Text(v) => {
                    bytes += v.len();
                    if v.len() > 1024 * 1024 || bytes > 64 * 1024 * 1024 {
                        return Err(fail("nativeStateBudget"));
                    }
                    json!(std::str::from_utf8(v).map_err(|_| fail("nativeStateUnavailable"))?)
                }
                ValueRef::Blob(v) => json!(v),
            };
            object.insert(name.clone(), v);
        }
        let id = object
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| fail("nativeStateUnavailable"))?
            .to_owned();
        rows.insert(id, Value::Object(object));
    }
    let mut q=db.prepare("SELECT parent_thread_id,child_thread_id,status FROM thread_spawn_edges ORDER BY parent_thread_id,child_thread_id").map_err(|_|fail("nativeStateUnavailable"))?;
    let edges = q
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .map_err(|_| fail("nativeStateUnavailable"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| fail("nativeStateUnavailable"))?;
    if edges.len() > 25000 {
        return Err(fail("nativeStateBudget"));
    }
    Ok(Database {
        storage_profile,
        rows,
        edges,
    })
}
pub(crate) fn same_row(a: &Value, b: &Value) -> bool {
    let (Some(a), Some(b)) = (a.as_object(), b.as_object()) else {
        return false;
    };
    a.len() == b.len()
        && a.iter().all(|(key, value)| {
            matches!(key.as_str(), "updated_at" | "updated_at_ms" | "tokens_used")
                || b.get(key) == Some(value)
        })
}

pub(crate) fn operation_lock(directory: &Path) -> Result<fs::File, Error> {
    private_directory(directory)?;
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(directory.join(".operation.lock"))
        .map_err(|_| fail("cleanupBusy"))?;
    let meta = file.metadata().map_err(|_| fail("cleanupBusy"))?;
    if !meta.is_file() || meta.nlink() != 1 {
        return Err(fail("cleanupBusy"));
    }
    use std::os::fd::AsRawFd;
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        return Err(fail("cleanupBusy"));
    }
    Ok(file)
}

pub(crate) fn checked_prefix(path: &Path, length: u64, expected: &Stamp) -> Result<String, Error> {
    if length > 4 * 1024 * 1024 * 1024 {
        return Err(fail("hashBudget"));
    }
    let mut file = safe_file(path)?;
    let stamp = |m: fs::Metadata| Stamp {
        identity: (m.dev(), m.ino()),
        length: m.len(),
        modified: (m.mtime(), m.mtime_nsec()),
        changed: (m.ctime(), m.ctime_nsec()),
    };
    let before = stamp(file.metadata().map_err(|_| fail("sourceUnavailable"))?);
    if &before != expected || before.length < length {
        return Err(fail("sourceChanged"));
    }
    let mut hash = Sha256::new();
    let mut size = 0;
    let mut buffer = [0u8; 65536];
    {
        let mut reader = (&mut file).take(length);
        loop {
            let n = reader
                .read(&mut buffer)
                .map_err(|_| fail("sourceUnavailable"))?;
            if n == 0 {
                break;
            }
            size += n as u64;
            hash.update(&buffer[..n]);
        }
    }
    let after = stamp(file.metadata().map_err(|_| fail("sourceUnavailable"))?);
    if size != length || after != before {
        return Err(fail("sourceChanged"));
    }
    Ok(format!("{:x}", hash.finalize()))
}
#[cfg(test)]
pub(crate) fn hash_prefix(path: &Path, length: u64) -> Result<String, Error> {
    let m = fs::symlink_metadata(path).map_err(|_| fail("sourceUnavailable"))?;
    let stamp = Stamp {
        identity: (m.dev(), m.ino()),
        length: m.len(),
        modified: (m.mtime(), m.mtime_nsec()),
        changed: (m.ctime(), m.ctime_nsec()),
    };
    checked_prefix(path, length, &stamp)
}

fn auxiliary_schemas(root: &Path) -> Result<(), Error> {
    for (name, expected) in [
        (
            "goals_1.sqlite",
            "76c73a55796b80e7e230000b1c61a37c44e67625fcb4b430305f91e54c052486",
        ),
        (
            "logs_2.sqlite",
            "b2e38decf7c08f9149a25d9f8a55c583c26ac5530bc7421880f5b47910391b0c",
        ),
        (
            "memories_1.sqlite",
            "c47df09396d47f977c21dd2207860c2dc5d52ddbee3d7e8e65eb0a994e94eccd",
        ),
        (
            "queue_1.sqlite",
            "9aeccd3543b231cb7ddaee9095fbd7e15b1297ad29d42514ec80c5bd88a3da70",
        ),
    ] {
        let path = root.join(name);
        drop(safe_file(&path)?);
        // Checkpointed migration metadata only. immutable avoids creating SHM on inactive auxiliary DBs.
        // Missing/uncheckpointed migration changes fail closed; this is not used for live thread rows.
        let encoded = path
            .as_os_str()
            .as_encoded_bytes()
            .iter()
            .map(|b| {
                if b.is_ascii_alphanumeric() || b"/._-".contains(b) {
                    (*b as char).to_string()
                } else {
                    format!("%{b:02X}")
                }
            })
            .collect::<String>();
        let db = Connection::open_with_flags(
            format!("file:{encoded}?mode=ro&immutable=1"),
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
        )
        .map_err(|_| fail("nativeSchemaUnsupported"))?;
        let mut q = db
            .prepare("SELECT version,success,checksum FROM _sqlx_migrations ORDER BY version")
            .map_err(|_| fail("nativeSchemaUnsupported"))?;
        let records = q
            .query_map([], |r| {
                let checksum: Vec<u8> = r.get(2)?;
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, i64>(1)?,
                    checksum
                        .iter()
                        .map(|b| format!("{b:02x}"))
                        .collect::<String>(),
                ))
            })
            .map_err(|_| fail("nativeSchemaUnsupported"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| fail("nativeSchemaUnsupported"))?;
        let digest = format!(
            "{:x}",
            Sha256::digest(
                serde_json::to_vec(&records).map_err(|_| fail("nativeSchemaUnsupported"))?
            )
        );
        if digest != expected {
            return Err(fail("nativeSchemaUnsupported"));
        }
    }
    Ok(())
}
