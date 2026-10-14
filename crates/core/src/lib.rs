//! Read-only inventory. The demo and Codex adapters expose no mutation API.
pub mod codex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, Read},
    path::{Component, Path, PathBuf},
    time::UNIX_EPOCH,
};

pub const HEADER_LIMIT: usize = 64 * 1024;
const MAX_FILES: usize = 128;
const MAX_IDS: usize = 32;
const TOTAL_HEADER_LIMIT: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct InspectError {
    pub code: String,
    pub message: String,
}
impl std::fmt::Display for InspectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}
impl std::error::Error for InspectError {}
fn err(code: &str, message: &str) -> InspectError {
    InspectError {
        code: code.into(),
        message: message.into(),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Category {
    Sessions,
    Archives,
    History,
    Logs,
    Assets,
    Worktrees,
    Cache,
    Other,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Relation {
    Root,
    Spawned,
    Fork,
    Unknown,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Activity {
    Completed,
    Active,
    Archived,
    Unknown,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Manifest {
    kind: String,
    schema_version: u32,
    files: Vec<ManifestItem>,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ManifestItem {
    path: String,
    category: Category,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Header {
    kind: String,
    thread_id: String,
    segment_id: String,
    title: String,
    project: String,
    relation: Relation,
    activity: Activity,
    updated_at: String,
    parent_id: Option<String>,
    shared_history_from: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskItem {
    pub key: String,
    pub category: Category,
    pub logical_bytes: u64,
    pub allocated_bytes: Option<u64>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Thread {
    pub id: String,
    pub title: String,
    pub project: String,
    pub relation: Relation,
    pub activity: Activity,
    pub updated_at: String,
    pub parent_id: Option<String>,
    pub shared_history_from: Option<String>,
    pub files: Vec<String>,
    pub segment_ids: Vec<String>,
    pub own_bytes: u64,
    pub inclusive_bytes: u64,
    pub owned_descendant_ids: Vec<String>,
    pub protection_reasons: Vec<String>,
    #[serde(default)]
    pub source_type: String,
    #[serde(default)]
    pub forked_from_id: Option<String>,
    #[serde(default)]
    pub fork_sources: Vec<String>,
    #[serde(default)]
    pub history_sources: Vec<String>,
    #[serde(default)]
    pub relation_evidence: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryTotal {
    pub category: Category,
    pub logical_bytes: u64,
    pub allocated_bytes: Option<u64>,
    pub files: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Inventory {
    pub schema_version: u32,
    pub source_kind: String,
    pub dataset_label: String,
    pub source_fingerprint: String,
    pub items: Vec<DiskItem>,
    pub threads: Vec<Thread>,
    pub totals: Vec<CategoryTotal>,
    pub warnings: Vec<String>,
    pub execution_enabled: bool,
    #[serde(default)]
    pub partial_coverage: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    pub schema_version: u32,
    pub source_kind: String,
    pub source_fingerprint: String,
    pub selected_ids: Vec<String>,
    pub affected_ids: Vec<String>,
    pub rows: Vec<Thread>,
    pub logical_bytes: u64,
    pub execution_enabled: bool,
    pub notices: Vec<String>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WebCatalog {
    pub inventory: Inventory,
    pub previews: BTreeMap<String, Preview>,
}

fn protected(t: &mut Thread, reason: &str) {
    if !t.protection_reasons.iter().any(|r| r == reason) {
        t.protection_reasons.push(reason.into());
    }
}
fn guarded_path(root: &Path, relative: &str) -> Result<PathBuf, InspectError> {
    if relative.is_empty() || relative.len() > 256 {
        return Err(err("invalidFixture", "Invalid fixture path."));
    }
    let mut path = root.to_path_buf();
    for part in Path::new(relative).components() {
        let Component::Normal(name) = part else {
            return Err(err(
                "invalidFixture",
                "Fixture path must stay inside its root.",
            ));
        };
        path.push(name);
        let meta = fs::symlink_metadata(&path)
            .map_err(|_| err("invalidFixture", "A fixture path is unavailable."))?;
        if meta.file_type().is_symlink() {
            return Err(err(
                "invalidFixture",
                "Linked fixture paths are not supported.",
            ));
        }
    }
    let meta = fs::symlink_metadata(&path)
        .map_err(|_| err("invalidFixture", "Fixture metadata is unavailable."))?;
    if !meta.is_file() {
        return Err(err("invalidFixture", "Expected a regular fixture file."));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if meta.nlink() != 1 {
            return Err(err(
                "invalidFixture",
                "Hard-linked fixture files are not supported.",
            ));
        }
    }
    Ok(path)
}
fn open_readonly(path: &Path) -> Result<File, InspectError> {
    let before = fs::symlink_metadata(path)
        .map_err(|_| err("invalidFixture", "Fixture metadata is unavailable."))?;
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    let file = options
        .open(path)
        .map_err(|_| err("invalidFixture", "Could not open a fixture read-only."))?;
    let opened = file
        .metadata()
        .map_err(|_| err("invalidFixture", "Opened fixture metadata is unavailable."))?;
    if !opened.is_file() || opened.len() != before.len() {
        return Err(err("invalidFixture", "Fixture changed while being opened."));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if before.dev() != opened.dev() || before.ino() != opened.ino() || opened.nlink() != 1 {
            return Err(err(
                "invalidFixture",
                "Fixture identity changed while being opened.",
            ));
        }
    }
    Ok(file)
}
fn allocated(meta: &fs::Metadata) -> Option<u64> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Some(meta.blocks().saturating_mul(512))
    }
    #[cfg(not(unix))]
    {
        let _ = meta;
        None
    }
}
fn read_bounded(path: &Path, limit: usize, line: bool) -> Result<Vec<u8>, InspectError> {
    let file = open_readonly(path)?;
    let mut bytes = Vec::new();
    if line {
        BufReader::new(file.take((limit + 1) as u64)).read_until(b'\n', &mut bytes)
    } else {
        file.take((limit + 1) as u64).read_to_end(&mut bytes)
    }
    .map_err(|_| err("invalidFixture", "Could not read fixture metadata."))?;
    if bytes.len() > limit {
        return Err(err(
            "metadataLimit",
            "Header exceeds the bounded metadata budget.",
        ));
    }
    Ok(bytes)
}
fn valid_header(h: &Header) -> bool {
    h.kind == "agentbroom-demo-session"
        && h.thread_id.starts_with("demo-")
        && h.thread_id.len() <= 80
        && h.segment_id.len() <= 80
        && !h.segment_id.is_empty()
        && h.title.len() <= 240
        && h.project.len() <= 120
        && h.updated_at.len() <= 40
        && h.parent_id.as_ref().is_none_or(|x| x.len() <= 80)
        && h.shared_history_from.as_ref().is_none_or(|x| x.len() <= 80)
}
fn unknown(key: &str, size: u64, reason: &str) -> Thread {
    Thread {
        id: format!("unknown:{key}"),
        title: "Unresolved sample record".into(),
        project: "Unclassified".into(),
        relation: Relation::Unknown,
        activity: Activity::Unknown,
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

/// Caller supplies only the fixed packaged/repository fixture root. No discovery occurs.
pub fn scan_fixture_store(root: &Path) -> Result<Inventory, InspectError> {
    let root_meta = fs::symlink_metadata(root)
        .map_err(|_| err("demoUnavailable", "Bundled demo dataset is unavailable."))?;
    if !root_meta.is_dir() || root_meta.file_type().is_symlink() {
        return Err(err(
            "invalidFixture",
            "Demo root must be a regular directory.",
        ));
    }
    let manifest_path = guarded_path(root, "fixture-manifest.json")?;
    let manifest_bytes = read_bounded(&manifest_path, HEADER_LIMIT, false)?;
    let manifest: Manifest = serde_json::from_slice(&manifest_bytes)
        .map_err(|_| err("invalidFixture", "Invalid synthetic fixture manifest."))?;
    if manifest.kind != "agentbroom-synthetic-fixture"
        || manifest.schema_version != 1
        || manifest.files.len() > MAX_FILES
    {
        return Err(err(
            "invalidFixture",
            "This is not a supported synthetic demo store.",
        ));
    }
    let mut seen = BTreeSet::new();
    let mut fingerprint = Sha256::new();
    fingerprint.update(&manifest_bytes);
    let mut items = Vec::new();
    let mut threads = BTreeMap::<String, Thread>::new();
    let mut warnings = Vec::new();
    let mut header_bytes = 0;
    for entry in manifest.files {
        if !seen.insert(entry.path.clone()) {
            return Err(err("invalidFixture", "Duplicate fixture file."));
        }
        let path = guarded_path(root, &entry.path)?;
        let meta = fs::metadata(&path)
            .map_err(|_| err("invalidFixture", "Fixture metadata unavailable."))?;
        fingerprint.update(entry.path.as_bytes());
        fingerprint.update(meta.len().to_le_bytes());
        fingerprint.update(
            meta.modified()
                .ok()
                .and_then(|x| x.duration_since(UNIX_EPOCH).ok())
                .map(|x| x.as_nanos())
                .unwrap_or(0)
                .to_le_bytes(),
        );
        items.push(DiskItem {
            key: entry.path.clone(),
            category: entry.category.clone(),
            logical_bytes: meta.len(),
            allocated_bytes: allocated(&meta),
        });
        if !matches!(entry.category, Category::Sessions | Category::Archives) {
            continue;
        }
        let bytes = if entry.path.ends_with(".zst") {
            Err(err(
                "compressedUnread",
                "Compressed sample content was not inspected.",
            ))
        } else if header_bytes + 1 >= TOTAL_HEADER_LIMIT {
            Err(err("metadataLimit", "Aggregate metadata budget reached."))
        } else {
            let budget = HEADER_LIMIT.min(TOTAL_HEADER_LIMIT - header_bytes - 1);
            // Reserve the maximum read, including the sentinel, even on errors.
            header_bytes += budget + 1;
            read_bounded(&path, budget, true)
        };
        let parsed = match bytes {
            Ok(bytes) => {
                fingerprint.update(&bytes);
                serde_json::from_slice::<Header>(&bytes)
                    .ok()
                    .filter(valid_header)
                    .ok_or_else(|| {
                        err(
                            "unknownMetadata",
                            "Sample metadata is incomplete or malformed.",
                        )
                    })
            }
            Err(e) => Err(e),
        };
        match parsed {
            Err(e) => {
                warnings.push(format!("{}: {}", entry.path, e.message));
                let t = unknown(&entry.path, meta.len(), &e.message);
                threads.insert(t.id.clone(), t);
            }
            Ok(h) => {
                let t = threads
                    .entry(h.thread_id.clone())
                    .or_insert_with(|| Thread {
                        id: h.thread_id.clone(),
                        title: h.title.clone(),
                        project: h.project.clone(),
                        relation: h.relation.clone(),
                        activity: h.activity.clone(),
                        updated_at: h.updated_at.clone(),
                        parent_id: h.parent_id.clone(),
                        shared_history_from: h.shared_history_from.clone(),
                        files: vec![],
                        segment_ids: vec![],
                        own_bytes: 0,
                        inclusive_bytes: 0,
                        owned_descendant_ids: vec![],
                        protection_reasons: vec![],
                        source_type: "demo".into(),
                        forked_from_id: None,
                        fork_sources: vec![],
                        history_sources: vec![],
                        relation_evidence: vec![],
                    });
                if t.title != h.title
                    || t.project != h.project
                    || t.relation != h.relation
                    || t.activity != h.activity
                    || t.parent_id != h.parent_id
                    || t.shared_history_from != h.shared_history_from
                    || t.segment_ids.contains(&h.segment_id)
                {
                    protected(t, "Conflicting segment metadata");
                }
                t.files.push(entry.path);
                t.segment_ids.push(h.segment_id);
                t.own_bytes += meta.len();
            }
        }
    }
    let raw = threads.clone();
    for t in threads.values_mut() {
        if t.activity == Activity::Active {
            protected(t, "Active conversation");
        }
        if t.activity == Activity::Unknown || t.relation == Relation::Unknown {
            protected(t, "Incomplete relationship metadata");
        }
        if t.relation == Relation::Spawned {
            let mut current = t.id.clone();
            let mut visited = BTreeSet::new();
            loop {
                if !visited.insert(current.clone()) {
                    protected(t, "Cyclic parent relationship");
                    t.relation = Relation::Unknown;
                    break;
                }
                let Some(node) = raw.get(&current) else {
                    protected(t, "Missing parent record");
                    t.relation = Relation::Unknown;
                    break;
                };
                if node.relation != Relation::Spawned {
                    break;
                }
                if let Some(parent) = &node.parent_id {
                    current = parent.clone();
                } else {
                    protected(t, "Missing parent identity");
                    t.relation = Relation::Unknown;
                    break;
                }
            }
        }
    }
    for source in raw.values().filter_map(|t| t.shared_history_from.as_ref()) {
        if let Some(t) = threads.get_mut(source) {
            protected(t, "Referenced by an independent fork");
        }
    }
    for t in threads.values_mut() {
        if let Some(source) = &t.shared_history_from {
            if !raw.contains_key(source) {
                protected(t, "Missing shared history source");
            }
        }
    }
    let checked = threads.clone();
    for t in threads.values_mut() {
        let mut owned = BTreeSet::new();
        let mut frontier = vec![t.id.clone()];
        while let Some(parent) = frontier.pop() {
            for child in checked.values().filter(|c| {
                c.relation == Relation::Spawned && c.parent_id.as_deref() == Some(&parent)
            }) {
                if owned.insert(child.id.clone()) {
                    frontier.push(child.id.clone());
                }
            }
        }
        t.inclusive_bytes = t.own_bytes;
        for id in &owned {
            let child = &checked[id];
            t.inclusive_bytes += child.own_bytes;
            if !child.protection_reasons.is_empty() {
                protected(t, "A spawned record is protected");
            }
        }
        t.owned_descendant_ids = owned.into_iter().collect();
    }
    let totals = [
        Category::Sessions,
        Category::Archives,
        Category::History,
        Category::Logs,
        Category::Assets,
    ]
    .into_iter()
    .map(|category| {
        let rows: Vec<_> = items.iter().filter(|x| x.category == category).collect();
        CategoryTotal {
            category,
            logical_bytes: rows.iter().map(|x| x.logical_bytes).sum(),
            allocated_bytes: rows.iter().map(|x| x.allocated_bytes).sum(),
            files: rows.len(),
        }
    })
    .collect();
    Ok(Inventory {
        schema_version: 1,
        source_kind: "demo".into(),
        dataset_label: "Bundled synthetic demo store".into(),
        source_fingerprint: format!("{:x}", fingerprint.finalize()),
        items,
        threads: threads.into_values().collect(),
        totals,
        warnings,
        execution_enabled: false,
        partial_coverage: false,
    })
}

pub fn preview(
    inventory: &Inventory,
    selected: &[String],
    kept: &[String],
    expected_fingerprint: &str,
) -> Result<Preview, InspectError> {
    if inventory.source_kind != "demo" {
        return Err(err(
            "inspectionOnly",
            "Real Codex data supports inspection only; no plan or report export.",
        ));
    }
    if inventory.source_fingerprint != expected_fingerprint {
        return Err(err(
            "staleSnapshot",
            "The sample inventory changed. Rescan before reviewing.",
        ));
    }
    for ids in [selected, kept] {
        let unique: BTreeSet<_> = ids.iter().collect();
        if ids.len() > MAX_IDS
            || unique.len() != ids.len()
            || ids
                .iter()
                .any(|id| !inventory.threads.iter().any(|t| &t.id == id))
        {
            return Err(err(
                "invalidSelection",
                "Unknown, duplicate or over-budget sample identity.",
            ));
        }
    }
    let mut affected = BTreeSet::new();
    for id in selected {
        let t = inventory
            .threads
            .iter()
            .find(|t| &t.id == id)
            .expect("identity validated");
        if t.relation == Relation::Spawned || !t.protection_reasons.is_empty() || kept.contains(id)
        {
            return Err(err(
                "invalidSelection",
                "Protected, spawned or kept records cannot be selected.",
            ));
        }
        affected.insert(id.clone());
        affected.extend(t.owned_descendant_ids.iter().cloned());
    }
    if affected.iter().any(|id| kept.contains(id)) {
        return Err(err(
            "invalidSelection",
            "A selected group contains a kept record.",
        ));
    }
    let rows: Vec<_> = inventory
        .threads
        .iter()
        .filter(|t| affected.contains(&t.id))
        .cloned()
        .collect();
    Ok(Preview {
        schema_version: 1,
        source_kind: "demo".into(),
        source_fingerprint: inventory.source_fingerprint.clone(),
        selected_ids: selected.to_vec(),
        affected_ids: affected.into_iter().collect(),
        logical_bytes: rows.iter().map(|t| t.own_bytes).sum(),
        rows,
        execution_enabled: false,
        notices: vec![
            "Inspection only. No deletion is available.".into(),
            "These bytes describe synthetic files, not recoverable or freed space.".into(),
        ],
    })
}
pub fn report(
    inventory: &Inventory,
    selected: &[String],
    kept: &[String],
    fingerprint: &str,
) -> Result<String, InspectError> {
    let p = preview(inventory, selected, kept, fingerprint)?;
    serde_json::to_string_pretty(&serde_json::json!({"schemaVersion":1,"sourceKind":"demo","executionEnabled":false,"datasetLabel":inventory.dataset_label,"totals":inventory.totals,"warnings":inventory.warnings,"preview":p,"limitations":["No real agent data inspected","No deletion or database maintenance","Keep choices are session-local"]})).map_err(|_| err("reportError", "Could not render sample report."))
}
pub fn web_catalog(inventory: Inventory) -> Result<WebCatalog, InspectError> {
    let eligible: Vec<String> = inventory
        .threads
        .iter()
        .filter(|t| t.relation != Relation::Spawned && t.protection_reasons.is_empty())
        .map(|t| t.id.clone())
        .collect();
    if eligible.len() > 8 {
        return Err(err(
            "invalidFixture",
            "Browser catalog is limited to eight reviewable sample roots.",
        ));
    }
    let mut previews = BTreeMap::new();
    for mask in 0..(1usize << eligible.len()) {
        let ids: Vec<_> = eligible
            .iter()
            .enumerate()
            .filter(|(i, _)| mask & (1 << i) != 0)
            .map(|(_, id)| id.clone())
            .collect();
        previews.insert(
            serde_json::to_string(&ids)
                .map_err(|_| err("invalidFixture", "Could not encode fixture identities."))?,
            preview(&inventory, &ids, &[], &inventory.source_fingerprint)?,
        );
    }
    Ok(WebCatalog {
        inventory,
        previews,
    })
}
pub mod cleanup;
#[cfg(feature = "synthetic-cleanup")]
pub mod synthetic_cleanup;

pub mod native_scope;
