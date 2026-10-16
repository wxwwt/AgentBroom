//! Application-owned manual locks. Never writes to a Codex data directory.
use crate::{Error, fail, source};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, fs, io::Write, path::Path};

pub(crate) fn uuid(id: &str) -> bool {
    id.len() == 36
        && id.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct State {
    schema_version: u32,
    ids: BTreeSet<String>,
}
pub(crate) fn read(directory: &Path, source_id: &str) -> Result<BTreeSet<String>, Error> {
    let path = directory.join("locks").join(format!("{source_id}.json"));
    let lock_directory = directory.join("locks");
    match fs::symlink_metadata(&lock_directory) {
        Ok(_) => source::guard_root(&lock_directory)?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
        Err(_) => return Err(fail("lockStateUnavailable")),
    }
    match fs::symlink_metadata(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeSet::new()),
        Err(_) => return Err(fail("lockStateUnavailable")),
        Ok(_) => (),
    }
    // Parent links must be rejected as well as a linked state file.
    source::guard_root(path.parent().unwrap())?;
    let state: State = serde_json::from_slice(&source::read_file(&path, 1024 * 1024)?)
        .map_err(|_| fail("lockStateUnavailable"))?;
    if state.schema_version != 1
        || state.ids.len() > 20000
        || state
            .ids
            .iter()
            .any(|id| !uuid(id) || id != &id.to_ascii_lowercase())
    {
        return Err(fail("lockStateUnavailable"));
    }
    Ok(state.ids)
}
pub(crate) fn write(directory: &Path, source_id: &str, ids: BTreeSet<String>) -> Result<(), Error> {
    if ids.len() > 20000 {
        return Err(fail("lockBudget"));
    }
    let directory = directory.join("locks");
    source::private_directory(&directory)?;
    let mut file =
        tempfile::NamedTempFile::new_in(&directory).map_err(|_| fail("lockStateUnavailable"))?;
    file.write_all(
        &serde_json::to_vec(&State {
            schema_version: 1,
            ids,
        })
        .map_err(|_| fail("lockStateUnavailable"))?,
    )
    .map_err(|_| fail("lockStateUnavailable"))?;
    file.as_file()
        .sync_all()
        .map_err(|_| fail("lockStateUnavailable"))?;
    file.persist(directory.join(format!("{source_id}.json")))
        .map_err(|_| fail("lockStateUnavailable"))?;
    fs::File::open(directory)
        .and_then(|f| f.sync_all())
        .map_err(|_| fail("lockStateUnavailable"))
}
