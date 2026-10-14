//! Codex metadata adapter. Database files are stat-only. No agent subprocesses.
use crate::{
    Activity, Category, CategoryTotal, DiskItem, InspectError, Inventory, Relation, Thread, err,
};
use std::path::{Path, PathBuf};

/// Only native callers discover the configured home; browser callers cannot supply a path.
pub fn default_root() -> Result<PathBuf, InspectError> {
    let root = if let Some(p) = std::env::var_os("CODEX_HOME") {
        PathBuf::from(p)
    } else {
        PathBuf::from(
            std::env::var_os("HOME")
                .ok_or_else(|| err("sourceUnavailable", "Home directory unavailable."))?,
        )
        .join(".codex")
    };
    if !root.is_absolute() {
        return Err(err("sourceUnavailable", "Codex home must be absolute."));
    }
    Ok(root)
}
pub fn scan_default() -> Result<Inventory, InspectError> {
    scan_store(&default_root()?)
}

#[cfg(not(unix))]
pub fn scan_store(_: &Path) -> Result<Inventory, InspectError> {
    Err(err(
        "unsupportedPlatform",
        "Real Codex inspection currently requires macOS or Linux. Demo remains available.",
    ))
}
#[cfg(unix)]
pub use unix::scan_store;

#[cfg(unix)]
mod unix {
    use super::*;
    use serde::Deserialize;
    use sha2::{Digest, Sha256};
    use std::{
        collections::{BTreeMap, BTreeSet},
        ffi::{CStr, CString},
        fs::File,
        io::{BufRead, BufReader, Read},
        os::{
            fd::{AsRawFd, FromRawFd},
            unix::{ffi::OsStrExt, fs::MetadataExt},
        },
        path::Component,
    };
    const HEADER: usize = 256 * 1024;
    const INDEX: usize = 4 * 1024 * 1024;
    const BUDGET: usize = 128 * 1024 * 1024;
    const ENTRIES: usize = 250_000;
    const DEPTH: usize = 32;
    #[derive(Deserialize)]
    struct Envelope {
        #[serde(rename = "type")]
        kind: String,
        payload: Meta,
    }
    #[derive(Deserialize)]
    struct Meta {
        id: String,
        #[serde(default)]
        cwd: String,
        #[serde(default)]
        timestamp: String,
        source: serde_json::Value,
        #[serde(default)]
        parent_thread_id: Option<String>,
        #[serde(default)]
        forked_from_id: Option<String>,
        #[serde(default)]
        history_base: Option<History>,
        #[serde(default)]
        thread_source: String,
    }
    #[derive(Deserialize)]
    struct History {
        thread_id: String,
    }
    #[derive(Deserialize)]
    struct IndexRow {
        id: String,
        thread_name: String,
        #[serde(default)]
        updated_at: String,
    }
    #[derive(Deserialize, Default)]
    struct Global {
        #[serde(default, rename = "pinned-thread-ids")]
        pinned: Vec<String>,
    }
    struct State {
        items: Vec<DiskItem>,
        threads: BTreeMap<String, Thread>,
        entries: usize,
        read: usize,
        index: BTreeMap<String, IndexRow>,
        pins: BTreeSet<String>,
        warnings: BTreeSet<String>,
        fingerprint: Sha256,
        file_identities: Vec<(u64, u64)>,
        partial: bool,
        aliases: BTreeMap<String, BTreeSet<String>>,
    }
    fn warning(s: &mut State, msg: &str) {
        s.warnings.insert(msg.into());
    }
    fn excluded(s: &mut State, msg: &str) {
        s.partial = true;
        warning(s, msg);
    }
    fn protect(t: &mut Thread, msg: &str) {
        if !t.protection_reasons.iter().any(|x| x == msg) {
            t.protection_reasons.push(msg.into());
        }
    }
    fn id_ok(s: &str) -> bool {
        s.len() == 36
            && s.bytes().enumerate().all(|(i, c)| {
                if [8, 13, 18, 23].contains(&i) {
                    c == b'-'
                } else {
                    c.is_ascii_hexdigit()
                }
            })
    }
    fn clean(s: &str) -> String {
        s.chars().filter(|c| !c.is_control()).take(512).collect()
    }
    fn open_at(dir: &File, name: &CStr, directory: bool) -> Result<File, InspectError> {
        let flags = libc::O_RDONLY
            | libc::O_CLOEXEC
            | libc::O_NOFOLLOW
            | libc::O_NONBLOCK
            | if directory { libc::O_DIRECTORY } else { 0 };
        let fd = unsafe { libc::openat(dir.as_raw_fd(), name.as_ptr(), flags) };
        if fd < 0 {
            return Err(err(
                "sourceUnavailable",
                "A Codex entry is inaccessible or linked.",
            ));
        }
        Ok(unsafe { File::from_raw_fd(fd) })
    }
    // Anchor every ancestor and child to a directory FD. Never follow a swapped parent symlink.
    fn open_root(root: &Path) -> Result<File, InspectError> {
        if !root.is_absolute() {
            return Err(err("sourceUnavailable", "Expected an absolute Codex home."));
        }
        let fd = unsafe {
            libc::open(
                c"/".as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC,
            )
        };
        if fd < 0 {
            return Err(err("sourceUnavailable", "Root unavailable."));
        }
        let mut dir = unsafe { File::from_raw_fd(fd) };
        for c in root.components() {
            match c {
                Component::RootDir => (),
                Component::Normal(n) => {
                    dir = open_at(
                        &dir,
                        &CString::new(n.as_bytes())
                            .map_err(|_| err("sourceUnavailable", "Invalid source path."))?,
                        true,
                    )?
                }
                _ => {
                    return Err(err(
                        "sourceUnavailable",
                        "Codex home must not contain traversal components.",
                    ));
                }
            }
        }
        Ok(dir)
    }
    struct Dir(*mut libc::DIR);
    impl Drop for Dir {
        fn drop(&mut self) {
            unsafe {
                libc::closedir(self.0);
            }
        }
    }
    fn category(key: &str) -> Category {
        let first = key.split('/').next().unwrap_or("");
        match first {
            "sessions" => Category::Sessions,
            "archived_sessions" => Category::Archives,
            "generated_images" | "generated_audio" | "generated_videos" => Category::Assets,
            "worktrees" => Category::Worktrees,
            "log" | "logs" => Category::Logs,
            "plugins" | "skills" | "bundled" | "tmp" | "browser" | "node" | "vendor_imports" => {
                Category::Cache
            }
            _ if key.ends_with(".sqlite")
                || key.ends_with(".db")
                || key.ends_with("-wal")
                || key.ends_with("-shm") =>
            {
                if first.starts_with("logs_") {
                    Category::Logs
                } else {
                    Category::History
                }
            }
            _ => Category::Other,
        }
    }
    fn bounded(
        s: &mut State,
        dir: &File,
        name: &CStr,
        st: &libc::stat,
        limit: usize,
        line: bool,
    ) -> Result<Vec<u8>, InspectError> {
        if st.st_nlink != 1 || s.read.saturating_add(limit + 1) > BUDGET {
            return Err(err(
                "metadataLimit",
                "Metadata unavailable: link or scan read budget.",
            ));
        }
        let file = open_at(dir, name, false)?;
        let m = file
            .metadata()
            .map_err(|_| err("metadataUnavailable", "Metadata unavailable."))?;
        if !m.is_file()
            || m.dev() != st.st_dev as u64
            || m.ino() != st.st_ino as u64
            || m.nlink() != 1
            || m.len() != st.st_size as u64
        {
            return Err(err("sourceChanged", "File changed while opening."));
        }
        let mut bytes = vec![];
        let mut reader = BufReader::with_capacity(4096, file.take((limit + 1) as u64));
        if line {
            reader.read_until(b'\n', &mut bytes)
        } else {
            reader.read_to_end(&mut bytes)
        }
        .map_err(|_| err("metadataUnavailable", "Read failed."))?;
        s.read += bytes.len() + 4096; // account for bounded read-ahead
        if bytes.len() > limit {
            return Err(err("metadataLimit", "Metadata exceeds the per-file limit."));
        }
        s.fingerprint.update(&bytes);
        Ok(bytes)
    }
    fn unknown(key: &str, size: u64, archive: bool, reason: &str) -> Thread {
        Thread {
            id: format!("unknown:{key}"),
            title: "Unresolved session file".into(),
            project: "Unknown".into(),
            relation: Relation::Unknown,
            activity: if archive {
                Activity::Archived
            } else {
                Activity::Unknown
            },
            updated_at: String::new(),
            parent_id: None,
            shared_history_from: None,
            files: vec![key.into()],
            segment_ids: vec![],
            own_bytes: size,
            inclusive_bytes: size,
            owned_descendant_ids: vec![],
            protection_reasons: vec![reason.into()],
            source_type: "unknown".into(),
            forked_from_id: None,
            fork_sources: vec![],
            history_sources: vec![],
            relation_evidence: vec![],
        }
    }
    fn session(
        s: &mut State,
        key: &str,
        bytes: Result<Vec<u8>, InspectError>,
        size: u64,
        archive: bool,
    ) {
        let parsed = bytes.and_then(|b| {
            serde_json::from_slice::<Envelope>(&b).map_err(|_| {
                err(
                    "metadataUnavailable",
                    "Session header invalid or unsupported.",
                )
            })
        });
        let Ok(e) = parsed else {
            let t = unknown(
                key,
                size,
                archive,
                "Session metadata unread, oversized, linked or unsupported",
            );
            s.threads.insert(t.id.clone(), t);
            return;
        };
        let p = e.payload;
        if e.kind != "session_meta" || !id_ok(&p.id) {
            let t = unknown(
                key,
                size,
                archive,
                "Missing valid session_meta logical identity",
            );
            s.threads.insert(t.id.clone(), t);
            return;
        }
        let sub = p.source.get("subagent");
        let valid_sub = sub.is_some_and(|v| {
            matches!(
                v.as_str(),
                Some("review" | "compact" | "memory_consolidation")
            ) || v.get("other").is_some_and(|v| v.is_string())
                || v.get("thread_spawn").is_some_and(|v| v.is_object())
        });
        let spawn = sub
            .and_then(|v| v.get("thread_spawn"))
            .and_then(|v| v.get("parent_thread_id"))
            .and_then(|v| v.as_str())
            .map(String::from);
        let parent_conflict =
            spawn.is_some() && p.parent_thread_id.is_some() && spawn != p.parent_thread_id;
        let parent = spawn.or(p.parent_thread_id);
        let mut relation = if valid_sub {
            Relation::Spawned
        } else if sub.is_none()
            && matches!(
                p.source.as_str(),
                Some("cli" | "vscode" | "exec" | "appServer" | "app_server")
            )
        {
            Relation::Root
        } else {
            Relation::Unknown
        };
        if sub.is_none() && p.forked_from_id.is_some() && relation == Relation::Root {
            relation = Relation::Fork;
        }
        let physical_id = key
            .rsplit('/')
            .next()
            .and_then(|n| n.strip_prefix("rollout-"))
            .and_then(|n| n.strip_suffix(".jsonl"))
            .and_then(|n| n.get(n.len().saturating_sub(36)..))
            .filter(|id| id_ok(id))
            .map(String::from);
        if let Some(alias) = &physical_id {
            s.aliases
                .entry(alias.clone())
                .or_default()
                .insert(p.id.clone());
        }
        let history: Vec<String> = p.history_base.map(|h| h.thread_id).into_iter().collect();
        let mut t = Thread {
            id: p.id.clone(),
            title: format!(
                "{} · {}",
                if sub.is_some() {
                    "Sub Agent"
                } else {
                    "Conversation"
                },
                &p.id[..8]
            ),
            project: clean(&p.cwd),
            relation,
            activity: if archive {
                Activity::Archived
            } else {
                Activity::Unknown
            },
            updated_at: clean(&p.timestamp),
            parent_id: parent,
            shared_history_from: history.first().cloned(),
            files: vec![key.into()],
            segment_ids: vec![physical_id.unwrap_or_else(|| p.id.clone())],
            own_bytes: size,
            inclusive_bytes: size,
            owned_descendant_ids: vec![],
            protection_reasons: vec![
                "Read-only inspection: cleanup eligibility is not evaluated".into(),
            ],
            source_type: if !p.thread_source.is_empty() {
                clean(&p.thread_source)
            } else if sub.is_some() {
                "subagent".into()
            } else {
                clean(p.source.as_str().unwrap_or("unknown"))
            },
            forked_from_id: p.forked_from_id.clone(),
            fork_sources: p.forked_from_id.into_iter().collect(),
            history_sources: history,
            relation_evidence: vec![
                "Logical identity: session_meta.payload.id (not session_id or filename)".into(),
            ],
        };
        if sub.is_some() {
            t.relation_evidence
                .push("Sub Agent: source.subagent; parent: explicit parent_thread_id".into());
        }
        if t.forked_from_id.is_some() {
            t.relation_evidence
                .push("Fork lineage: forked_from_id; does not prove shared bytes".into());
        }
        if !t.history_sources.is_empty() {
            t.relation_evidence
                .push("Shared history: history_base.thread_id".into());
        }
        if parent_conflict {
            t.relation = Relation::Unknown;
            protect(&mut t, "Conflicting parent metadata");
        }
        if sub.is_none() && t.parent_id.is_some() {
            t.relation = Relation::Unknown;
            protect(
                &mut t,
                "Parent metadata on non-subagent source; relation unresolved",
            );
        }
        if t.relation == Relation::Unknown {
            protect(&mut t, "Unrecognized session source");
        }
        if let Some(old) = s.threads.get_mut(&p.id) {
            if old.forked_from_id != t.forked_from_id {
                for id in &t.fork_sources {
                    old.fork_sources.push(id.clone());
                    protect(old, "Conflicting fork provenance retained conservatively");
                }
            }
            if old.parent_id != t.parent_id
                || old.relation != t.relation
                || old.project != t.project
                || old.source_type != t.source_type
                || old.forked_from_id != t.forked_from_id
            {
                old.relation = Relation::Unknown;
                protect(old, "Conflicting metadata across session segments");
            }
            if old.activity != t.activity {
                old.activity = Activity::Unknown;
                protect(old, "Mixed archived and live segment locations");
            }
            old.files.push(key.into());
            old.segment_ids.extend(t.segment_ids);
            old.own_bytes += size;
            for h in t.history_sources {
                if !old.history_sources.contains(&h) {
                    old.history_sources.push(h);
                }
            }
            for r in t.protection_reasons {
                protect(old, &r);
            }
        } else {
            s.threads.insert(p.id, t);
        }
    }
    fn walk(s: &mut State, dir: &File, prefix: &str, depth: usize) -> Result<(), InspectError> {
        let copy = unsafe { libc::dup(dir.as_raw_fd()) };
        if copy < 0 {
            return Err(err("sourceUnavailable", "Directory unavailable."));
        }
        let raw = unsafe { libc::fdopendir(copy) };
        if raw.is_null() {
            unsafe {
                libc::close(copy);
            }
            return Err(err("sourceUnavailable", "Directory unavailable."));
        }
        let listing = Dir(raw);
        loop {
            #[cfg(target_os = "macos")]
            unsafe {
                *libc::__error() = 0;
            }
            #[cfg(not(target_os = "macos"))]
            unsafe {
                *libc::__errno_location() = 0;
            }
            let entry = unsafe { libc::readdir(listing.0) };
            if entry.is_null() {
                if std::io::Error::last_os_error().raw_os_error().unwrap_or(0) != 0 {
                    return Err(err(
                        "sourceUnavailable",
                        "Directory enumeration failed; inventory incomplete.",
                    ));
                }
                break;
            }
            let name = unsafe { CStr::from_ptr((*entry).d_name.as_ptr()) };
            if name.to_bytes() == b"." || name.to_bytes() == b".." {
                continue;
            }
            s.entries += 1;
            if s.entries > ENTRIES {
                return Err(err(
                    "inventoryLimit",
                    "Entry budget exceeded; scan is incomplete. No totals returned.",
                ));
            }
            let Some(text) = name.to_str().ok() else {
                excluded(s, "Non-UTF8 entries skipped: inventory is partial.");
                continue;
            };
            let key = if prefix.is_empty() {
                text.into()
            } else {
                format!("{prefix}/{text}")
            };
            let mut st: libc::stat = unsafe { std::mem::zeroed() };
            if unsafe {
                libc::fstatat(
                    dir.as_raw_fd(),
                    name.as_ptr(),
                    &mut st,
                    libc::AT_SYMLINK_NOFOLLOW,
                )
            } != 0
            {
                excluded(s, "Inaccessible entries skipped: inventory is partial.");
                continue;
            }
            let kind = st.st_mode & libc::S_IFMT;
            if kind == libc::S_IFDIR {
                if depth >= DEPTH {
                    excluded(s, "Depth limit reached: inventory is partial.");
                    continue;
                }
                match open_at(dir, name, true) {
                    Ok(child) => {
                        let m = child
                            .metadata()
                            .map_err(|_| err("sourceChanged", "Directory changed."))?;
                        if m.dev() != st.st_dev as u64 || m.ino() != st.st_ino as u64 {
                            excluded(s, "Changed directory skipped: inventory is partial.");
                            continue;
                        }
                        walk(s, &child, &key, depth + 1)?;
                    }
                    Err(_) => excluded(
                        s,
                        "Inaccessible or linked directory skipped: inventory is partial.",
                    ),
                }
                continue;
            }
            if kind != libc::S_IFREG {
                excluded(
                    s,
                    "Symlinks and special files skipped; links are never followed. Inventory excludes their targets.",
                );
                continue;
            }
            if key.len() > 2048 {
                excluded(s, "Long paths skipped: inventory is partial.");
                continue;
            }
            let category = category(&key);
            let size = st.st_size.max(0) as u64;
            let allocated = st.st_blocks.max(0) as u64 * 512;
            s.file_identities.push((st.st_dev as u64, st.st_ino as u64));
            s.fingerprint.update(format!(
                "{key}:{size}:{}:{}:{allocated}",
                st.st_dev, st.st_ino
            ));
            s.items.push(DiskItem {
                key: key.clone(),
                category: category.clone(),
                logical_bytes: size,
                allocated_bytes: Some(allocated),
            });
            if key == "session_index.jsonl" {
                match bounded(s, dir, name, &st, INDEX, false) {
                    Ok(b) => {
                        for line in b.split(|c| *c == b'\n').filter(|l| !l.is_empty()) {
                            if let Ok(row) = serde_json::from_slice::<IndexRow>(line) {
                                if id_ok(&row.id) {
                                    s.index.insert(row.id.clone(), row);
                                }
                            } else {
                                excluded(
                                    s,
                                    "Some session index rows are invalid; titles may be incomplete.",
                                );
                            }
                        }
                    }
                    Err(_) => excluded(
                        s,
                        "Session index unavailable or oversized; titles and missing-record checks are incomplete.",
                    ),
                }
            } else if key == ".codex-global-state.json" {
                match bounded(s, dir, name, &st, INDEX, false).and_then(|b| {
                    serde_json::from_slice::<Global>(&b)
                        .map_err(|_| err("metadataUnavailable", "Global state invalid."))
                }) {
                    Ok(g) => s.pins.extend(g.pinned.into_iter().filter(|id| id_ok(id))),
                    Err(_) => excluded(
                        s,
                        "Pinned state unavailable or oversized; pins cannot be fully identified.",
                    ),
                }
            } else if matches!(category, Category::Sessions | Category::Archives) {
                let data = if key.ends_with(".jsonl") {
                    bounded(s, dir, name, &st, HEADER, true)
                } else {
                    Err(err(
                        "unsupportedFormat",
                        "Compressed or unrecognized sessions are stat-only.",
                    ))
                };
                session(s, &key, data, size, category == Category::Archives);
                if s.threads.len() > 20_000 {
                    return Err(err(
                        "inventoryLimit",
                        "Conversation count exceeds graph budget.",
                    ));
                }
            }
        }
        Ok(())
    }
    fn resolve(s: &mut State) {
        let known: BTreeSet<_> = s.threads.keys().cloned().collect();
        for t in s.threads.values_mut() {
            for h in &mut t.history_sources {
                if let Some(ids) = s.aliases.get(h) {
                    if ids.len() == 1 {
                        let owner = ids.first().unwrap();
                        if owner != h {
                            t.relation_evidence.push("history_base physical rollout ID resolved to its session_meta logical ID".into());
                        }
                        *h = owner.clone();
                    } else {
                        t.protection_reasons
                            .push("Ambiguous physical history reference".into());
                        t.relation = Relation::Unknown;
                    }
                } else if !known.contains(h) {
                    t.protection_reasons
                        .push("Physical history reference unavailable".into());
                    if t.relation == Relation::Root {
                        t.relation = Relation::Unknown;
                    }
                }
            }
            t.shared_history_from = t.history_sources.iter().find(|id| *id != &t.id).cloned();
            if t.relation == Relation::Root && t.shared_history_from.is_some() {
                t.relation = Relation::Fork;
            }
            if t.history_sources.iter().any(|id| id == &t.id) {
                t.relation_evidence.push(
                    "History base resolves to another segment of this same logical conversation"
                        .into(),
                );
            }
        }
        for (id, row) in &s.index {
            if let Some(t) = s.threads.get_mut(id) {
                t.title = clean(&row.thread_name);
                t.updated_at = clean(&row.updated_at);
            } else {
                let mut t = unknown(
                    "",
                    0,
                    false,
                    "Indexed title has no matched parseable session_meta; bytes may exist in unread or skipped files",
                );
                t.id = id.clone();
                t.title = clean(&row.thread_name);
                t.updated_at = clean(&row.updated_at);
                t.files.clear();
                s.threads.insert(id.clone(), t);
            }
        }
        for id in &s.pins {
            if let Some(t) = s.threads.get_mut(id) {
                protect(t, "Pinned in Codex desktop state");
            }
        }
        let raw = s.threads.clone();
        for t in s.threads.values_mut() {
            if t.activity != Activity::Archived {
                protect(
                    t,
                    "Live status unknown: no runtime or database activity queried",
                );
            }
        }
        // Validate each chain without borrowing the target node as the cursor.
        for t in s
            .threads
            .values_mut()
            .filter(|t| t.relation == Relation::Spawned)
        {
            let mut seen = BTreeSet::from([t.id.clone()]);
            let mut current = &raw[&t.id];
            let reason = loop {
                if seen.len() > 64 {
                    break Some("Parent depth exceeds metadata graph limit");
                }
                match &current.parent_id {
                    None => break Some("Missing parent identity"),
                    Some(id) if !id_ok(id) || !seen.insert(id.clone()) => {
                        break Some("Invalid or cyclic parent chain");
                    }
                    Some(id) => match raw.get(id) {
                        None => break Some("Parent conversation missing from inspected metadata"),
                        Some(p) if p.relation == Relation::Unknown => {
                            break Some("Parent relationship unresolved");
                        }
                        Some(p) if p.relation != Relation::Spawned => break None,
                        Some(p) => current = p,
                    },
                }
            };
            if let Some(reason) = reason {
                t.relation = Relation::Unknown;
                protect(t, reason);
            }
        }
        for t in raw.values() {
            for h in t.history_sources.iter().filter(|id| *id != &t.id) {
                if let Some(source) = s.threads.get_mut(h) {
                    protect(source, "Shared history used by another inspected record");
                } else if let Some(node) = s.threads.get_mut(&t.id) {
                    protect(node, "Shared history source missing or unresolved");
                }
            }
            for id in &t.fork_sources {
                if let Some(source) = s.threads.get_mut(id) {
                    protect(
                        source,
                        "Fork lineage referenced by another record (shared bytes not confirmed)",
                    );
                } else if let Some(node) = s.threads.get_mut(&t.id) {
                    protect(node, "Fork lineage source missing or unresolved");
                }
            }
        }
        // Adjacency avoids repeated O(n²) full scans. Only confirmed spawned links count.
        let mut children: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for t in s
            .threads
            .values()
            .filter(|t| t.relation == Relation::Spawned)
        {
            if let Some(p) = &t.parent_id {
                children.entry(p.clone()).or_default().push(t.id.clone());
            }
        }
        let sizes: BTreeMap<_, _> = s
            .threads
            .values()
            .map(|t| (t.id.clone(), t.own_bytes))
            .collect();
        for t in s.threads.values_mut() {
            let mut owned = BTreeSet::new();
            let mut todo = children.get(&t.id).cloned().unwrap_or_default();
            while let Some(id) = todo.pop() {
                if owned.insert(id.clone()) {
                    if let Some(c) = children.get(&id) {
                        todo.extend(c.iter().cloned());
                    }
                }
            }
            t.inclusive_bytes = t.own_bytes + owned.iter().map(|id| sizes[id]).sum::<u64>();
            t.owned_descendant_ids = owned.into_iter().collect();
            t.files.sort();
            t.history_sources.sort();
            t.history_sources.dedup();
            t.fork_sources.sort();
            t.fork_sources.dedup();
            t.segment_ids.sort();
            t.segment_ids.dedup();
        }
    }
    pub fn scan_store(root: &Path) -> Result<Inventory, InspectError> {
        let dir = open_root(root)?;
        let mut s = State {
            items: vec![],
            threads: BTreeMap::new(),
            entries: 0,
            read: 0,
            index: BTreeMap::new(),
            pins: BTreeSet::new(),
            warnings: BTreeSet::new(),
            fingerprint: Sha256::new(),
            file_identities: vec![],
            partial: false,
            aliases: BTreeMap::new(),
        };
        walk(&mut s, &dir, "", 0)?;
        if s.threads.len() > 20_000 {
            return Err(err(
                "inventoryLimit",
                "Conversation count exceeds graph budget.",
            ));
        }
        if !s.items.iter().any(|i| {
            matches!(i.category, Category::Sessions | Category::Archives)
                || i.key == "session_index.jsonl"
        }) {
            return Err(err(
                "sourceUnavailable",
                "No supported Codex sessions or index in the configured home.",
            ));
        }
        if !s.items.iter().any(|i| i.key == "session_index.jsonl") {
            warning(
                &mut s,
                "Session index missing; primary titles and missing-record checks unavailable.",
            );
        }
        if !s.items.iter().any(|i| i.key == ".codex-global-state.json") {
            warning(
                &mut s,
                "Desktop pinned state missing; pins cannot be identified.",
            );
        }
        if s.threads.len().saturating_add(
            s.index
                .keys()
                .filter(|id| !s.threads.contains_key(*id))
                .count(),
        ) > 20_000
        {
            return Err(err(
                "inventoryLimit",
                "Indexed conversation count exceeds graph budget.",
            ));
        }
        resolve(&mut s);
        warning(
            &mut s,
            "Live filesystem observation, not an atomic snapshot. Codex may change files during inspection.",
        );
        warning(
            &mut s,
            "SQLite, WAL, SHM, auth/config and assets are stat-only. External sqlite_home and remote stores are not included.",
        );
        warning(
            &mut s,
            "Logical bytes are file lengths; allocated bytes are filesystem blocks with hardlinks deduplicated by first lexical path (category attribution is accounting, not ownership). APFS clones/compression may share blocks; neither value is reclaimable space.",
        );
        warning(
            &mut s,
            "Only bounded session_meta headers, session_index titles and pinned IDs are parsed. Transcript bodies are not parsed; unread/compressed headers remain unknown. No activity or cleanup safety is inferred.",
        );
        let mut measured: Vec<_> = std::mem::take(&mut s.items)
            .into_iter()
            .zip(s.file_identities)
            .collect();
        measured.sort_by(|a, b| a.0.key.cmp(&b.0.key));
        let mut seen = BTreeSet::new();
        s.items = measured
            .into_iter()
            .map(|(mut item, identity)| {
                if !seen.insert(identity) {
                    item.allocated_bytes = Some(0);
                }
                item
            })
            .collect();
        let totals = [
            Category::Sessions,
            Category::Archives,
            Category::History,
            Category::Logs,
            Category::Assets,
            Category::Worktrees,
            Category::Cache,
            Category::Other,
        ]
        .into_iter()
        .map(|category| {
            let rows: Vec<_> = s.items.iter().filter(|x| x.category == category).collect();
            CategoryTotal {
                category,
                logical_bytes: rows.iter().map(|x| x.logical_bytes).sum(),
                allocated_bytes: rows.iter().map(|x| x.allocated_bytes).sum(),
                files: rows.len(),
            }
        })
        .collect();
        s.items.sort_by(|a, b| a.key.cmp(&b.key));
        Ok(Inventory {
            schema_version: 1,
            source_kind: "codex".into(),
            dataset_label: "Local Codex home · read-only observation".into(),
            source_fingerprint: format!("{:x}", s.fingerprint.finalize()),
            items: s.items,
            threads: s.threads.into_values().collect(),
            totals,
            warnings: s.warnings.into_iter().collect(),
            execution_enabled: false,
            partial_coverage: s.partial,
        })
    }
}
