//! Seven-day operation history, separate from durable at-most-once execution receipts.
use crate::{Error, fail, source};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};
pub(crate) const HISTORY_WARNING: &str = "Operation history could not be saved.";
pub const RETENTION_SECONDS: u64 = 7 * 24 * 60 * 60;
pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OperationEvent {
    pub id: String,
    pub at: u64,
    pub action: String,
    pub status: String,
    pub ids: Vec<String>,
    pub titles: Vec<String>,
    pub count: usize,
    pub bytes: u64,
    pub error: Option<String>,
}
fn valid(id: &str) -> bool {
    !id.is_empty() && id.len() <= 128 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}
pub(crate) fn new_id() -> Result<String, Error> {
    let f = tempfile::Builder::new()
        .prefix("op-")
        .rand_bytes(18)
        .tempfile()
        .map_err(|_| fail("operationLogUnavailable"))?;
    Ok(f.path().file_name().unwrap().to_string_lossy().into())
}
pub(crate) fn write(dir: &Path, mut event: OperationEvent) -> Result<(), Error> {
    if event.id.is_empty() {
        let f = tempfile::Builder::new()
            .prefix("op-")
            .rand_bytes(18)
            .tempfile()
            .map_err(|_| fail("operationLogUnavailable"))?;
        event.id = f.path().file_name().unwrap().to_string_lossy().into();
    }
    if !valid(&event.id) || event.ids.len() > 20000 || event.titles.len() > 256 {
        return Err(fail("operationLogUnavailable"));
    }
    source::private_directory(dir)?;
    let mut file =
        tempfile::NamedTempFile::new_in(dir).map_err(|_| fail("operationLogUnavailable"))?;
    file.write_all(&serde_json::to_vec(&event).map_err(|_| fail("operationLogUnavailable"))?)
        .and_then(|_| file.as_file().sync_all())
        .map_err(|_| fail("operationLogUnavailable"))?;
    file.persist(dir.join(format!("{}.json", event.id)))
        .map_err(|_| fail("operationLogUnavailable"))?;
    fs::File::open(dir)
        .and_then(|f| f.sync_all())
        .map_err(|_| fail("operationLogUnavailable"))?;
    read(dir, now())?;
    Ok(())
}
pub(crate) fn read(dir: &Path, at: u64) -> Result<Vec<OperationEvent>, Error> {
    if !dir.exists() {
        return Ok(vec![]);
    }
    source::guard_root(dir)?;
    let mut out = vec![];
    for (n, item) in fs::read_dir(dir)
        .map_err(|_| fail("operationLogUnavailable"))?
        .enumerate()
    {
        if n >= 10000 {
            return Err(fail("operationLogBudget"));
        }
        let p = item.map_err(|_| fail("operationLogUnavailable"))?.path();
        let Some(id) = p.file_stem().and_then(|v| v.to_str()) else {
            continue;
        };
        if p.extension().is_none_or(|s| s != "json") || !valid(id) {
            continue;
        }
        let event: OperationEvent = serde_json::from_slice(&source::read_file(&p, 1024 * 1024)?)
            .map_err(|_| fail("operationLogUnavailable"))?;
        if event.id != id {
            return Err(fail("operationLogUnavailable"));
        }
        if event.at < at.saturating_sub(RETENTION_SECONDS) {
            fs::remove_file(&p).map_err(|_| fail("operationLogUnavailable"))?;
        } else {
            out.push(event)
        }
    }
    out.sort_by(|a, b| b.at.cmp(&a.at).then_with(|| b.id.cmp(&a.id)));
    Ok(out)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn history_survives_reopen_and_expires_after_seven_days() {
        let t = tempfile::tempdir().unwrap();
        let d = t.path().canonicalize().unwrap().join("activity");
        let base = now();
        for (id, at) in [
            ("old", base - RETENTION_SECONDS - 1),
            ("edge", base + 11 - RETENTION_SECONDS),
            ("new", base + 20),
        ] {
            write(
                &d,
                OperationEvent {
                    id: id.into(),
                    at,
                    action: "lock".into(),
                    status: "verified".into(),
                    ids: vec![],
                    titles: vec![],
                    count: 1,
                    bytes: 0,
                    error: None,
                },
            )
            .unwrap();
        }
        let r = read(&d, base + 11).unwrap();
        assert_eq!(r.len(), 2);
        assert!(!d.join("old.json").exists());
        assert_eq!(read(&d, base + 11).unwrap()[0].id, "new");
    }
}
