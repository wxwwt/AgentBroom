//! Read-only Git inspection. No removal, fetch, pruning, checkout or index refresh.
use crate::{Error, fail};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeSet, HashSet},
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const OUTPUT_LIMIT: usize = 8 * 1024 * 1024;
const FILE_LIMIT: usize = 200_000;

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    pub repository: Option<String>,
    pub target: Option<String>,
    #[serde(default)]
    pub projects: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Inventory {
    pub scanned_at: u64,
    pub repositories: Vec<Repository>,
    pub warnings: Vec<String>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Repository {
    pub path: String,
    pub target_ref: Option<String>,
    pub target_oid: Option<String>,
    pub shallow: bool,
    pub worktrees: Vec<Worktree>,
    pub warnings: Vec<String>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Worktree {
    pub path: String,
    pub branch: Option<String>,
    pub head: String,
    pub main_checkout: bool,
    pub locked: bool,
    pub lock_reason: Option<String>,
    pub prunable: bool,
    pub merge_status: String,
    pub ahead_count: Option<u64>,
    pub last_commit_at: Option<i64>,
    pub source_modified_at: Option<u64>,
    pub modified_count: Option<usize>,
    pub untracked_count: Option<usize>,
    pub ignored_count: Option<usize>,
    pub logical_bytes: u64,
    pub complete: bool,
    pub recommendation: String,
    pub reasons: Vec<String>,
}

struct Git {
    deadline: Instant,
}
impl Git {
    fn run(&self, path: &Path, args: &[&str]) -> Result<Vec<u8>, Error> {
        self.configured(path, args, &[])
    }
    fn configured(&self, path: &Path, args: &[&str], config: &[String]) -> Result<Vec<u8>, Error> {
        if Instant::now() >= self.deadline {
            return Err(fail("worktreeScanLimited"));
        }
        let mut command = Command::new("/usr/bin/git");
        // Do not inherit GIT_DIR, alternate indexes, injected config or helper settings.
        for (key, _) in std::env::vars_os() {
            if key.to_string_lossy().starts_with("GIT_") {
                command.env_remove(key);
            }
        }
        command
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_OPTIONAL_LOCKS", "0")
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_NO_LAZY_FETCH", "1")
            .env("LC_ALL", "C")
            .args([
                "--no-optional-locks",
                "-c",
                "core.fsmonitor=false",
                "-c",
                "log.showSignature=false",
                "-c",
                "status.submoduleSummary=false",
                "-c",
                "core.untrackedCache=false",
                "-c",
                "core.hooksPath=/dev/null",
                "-c",
                "maintenance.auto=false",
                "-c",
                "gc.auto=0",
            ]);
        for item in config {
            command.args(["-c", item]);
        }
        let mut child = command
            .arg("-C")
            .arg(path)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| fail("worktreeGitUnavailable"))?;
        let output = child.stdout.take().unwrap();
        let reader = thread::spawn(move || {
            let mut bytes = Vec::new();
            output
                .take((OUTPUT_LIMIT + 1) as u64)
                .read_to_end(&mut bytes)
                .map(|_| bytes)
        });
        let deadline = self.deadline.min(Instant::now() + Duration::from_secs(5));
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break Some(status),
                Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(10)),
                _ => {
                    let _ = child.kill();
                    let _ = child.wait();
                    break None;
                }
            }
        };
        let output = reader
            .join()
            .ok()
            .and_then(Result::ok)
            .ok_or_else(|| fail("worktreeGitUnavailable"))?;
        if output.len() > OUTPUT_LIMIT || status.is_none() {
            return Err(fail("worktreeScanLimited"));
        }
        if !status.unwrap().success() {
            return Err(fail("worktreeGitUnavailable"));
        }
        Ok(output)
    }
    fn text(&self, path: &Path, args: &[&str]) -> Option<String> {
        String::from_utf8(self.run(path, args).ok()?)
            .ok()
            .map(|s| s.trim_end_matches(['\r', '\n']).to_string())
    }
}

fn unix_time() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
fn display(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}
fn oid(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|c| c.is_ascii_hexdigit())
}
fn reason(row: &mut Worktree, value: &str) {
    if !row.reasons.iter().any(|r| r == value) {
        row.reasons.push(value.into());
    }
}

/// Only sources authorized for inspection are used; project strings are candidate paths, never commands.
pub fn inspect(request: Request) -> Result<Inventory, Error> {
    let codex = agentbroom_core::codex::default_root()?;
    inspect_at(request, &codex)
}
fn inspect_at(request: Request, codex: &Path) -> Result<Inventory, Error> {
    if request.projects.len() > 512
        || request.repository.as_ref().is_some_and(|s| s.len() > 4096)
        || request.target.as_ref().is_some_and(|s| s.len() > 1024)
    {
        return Err(fail("invalidWorktreeRequest"));
    }
    let git = Git {
        deadline: Instant::now() + Duration::from_secs(45),
    };
    let mut result = Inventory {
        scanned_at: unix_time(),
        repositories: vec![],
        warnings: vec![],
    };
    let mut paths = BTreeSet::new();
    if let Some(path) = request
        .repository
        .as_deref()
        .filter(|s| !s.trim().is_empty())
    {
        let path = PathBuf::from(path);
        if !path.is_absolute() || !path.is_dir() {
            return Err(fail("worktreeRepositoryUnavailable"));
        }
        paths.insert(path);
    } else {
        discover(
            &codex.join("worktrees"),
            0,
            &mut paths,
            &mut result.warnings,
            &mut 0usize,
            git.deadline,
        );
        for path in request
            .projects
            .iter()
            .take(512)
            .filter(|s| s.len() <= 4096)
        {
            let path = PathBuf::from(path);
            if path.is_absolute() && path.is_dir() {
                paths.insert(path);
            }
        }
    }
    let mut common = BTreeSet::new();
    for path in paths {
        if Instant::now() >= git.deadline || common.len() >= 32 {
            result.warnings.push("worktreeScanLimited".into());
            break;
        }
        let Some(dir) = git.text(
            &path,
            &["rev-parse", "--path-format=absolute", "--git-common-dir"],
        ) else {
            if request.repository.is_some() {
                return Err(fail("worktreeRepositoryUnavailable"));
            }
            continue;
        };
        let Ok(dir) = fs::canonicalize(dir) else {
            result.warnings.push("worktreeRepositoryUnavailable".into());
            continue;
        };
        if !common.insert(dir) {
            continue;
        }
        match inspect_repository(&git, &path, request.target.as_deref()) {
            Ok(repo) => result.repositories.push(repo),
            Err(e) => result.warnings.push(e.code),
        }
    }
    result.warnings.sort();
    result.warnings.dedup();
    Ok(result)
}

fn discover(
    path: &Path,
    depth: usize,
    paths: &mut BTreeSet<PathBuf>,
    warnings: &mut Vec<String>,
    count: &mut usize,
    deadline: Instant,
) {
    *count += 1;
    if *count > 4096 || Instant::now() >= deadline {
        warnings.push("worktreeScanLimited".into());
        return;
    }
    if paths.len() >= 256 {
        warnings.push("worktreeScanLimited".into());
        return;
    }
    if fs::symlink_metadata(path)
        .map(|m| !m.is_dir() || m.is_symlink())
        .unwrap_or(true)
    {
        return;
    }
    if path.join(".git").exists() {
        paths.insert(path.to_path_buf());
        return;
    }
    if depth >= 3 {
        return;
    }
    let Ok(entries) = fs::read_dir(path) else {
        warnings.push("worktreeScanIncomplete".into());
        return;
    };
    for entry in entries {
        match entry {
            Ok(e) => discover(&e.path(), depth + 1, paths, warnings, count, deadline),
            Err(_) => warnings.push("worktreeScanIncomplete".into()),
        }
        if paths.len() >= 256 || *count > 4096 || Instant::now() >= deadline {
            warnings.push("worktreeScanLimited".into());
            break;
        }
    }
}

fn resolve_target(git: &Git, path: &Path, requested: Option<&str>) -> Option<(String, String)> {
    let candidates = if let Some(value) = requested.filter(|v| !v.trim().is_empty()) {
        if value.starts_with("refs/heads/") || value.starts_with("refs/remotes/") {
            vec![value.to_string()]
        } else {
            vec![
                format!("refs/heads/{value}"),
                format!("refs/remotes/{value}"),
            ]
        }
    } else {
        let mut refs = vec![];
        if let Some(value) = git.text(
            path,
            &["symbolic-ref", "--quiet", "refs/remotes/origin/HEAD"],
        ) {
            refs.push(value);
        }
        refs.extend(["refs/heads/main".into(), "refs/heads/master".into()]);
        refs
    };
    for candidate in candidates {
        if git.run(path, &["check-ref-format", &candidate]).is_err() {
            continue;
        }
        if let Some(value) = git
            .text(
                path,
                &["rev-parse", "--verify", &format!("{candidate}^{{commit}}")],
            )
            .filter(|s| oid(s))
        {
            return Some((candidate, value));
        }
    }
    None
}

fn inspect_repository(git: &Git, path: &Path, target: Option<&str>) -> Result<Repository, Error> {
    let output = git.run(path, &["worktree", "list", "--porcelain", "-z"])?;
    let mut rows = parse_worktrees(&output)?;
    let resolved = resolve_target(git, path, target);
    let mut repo = Repository {
        path: rows
            .first()
            .map(|r| r.path.clone())
            .unwrap_or_else(|| display(path)),
        target_ref: resolved.as_ref().map(|r| r.0.clone()),
        target_oid: resolved.as_ref().map(|r| r.1.clone()),
        shallow: git
            .text(path, &["rev-parse", "--is-shallow-repository"])
            .as_deref()
            != Some("false"),
        worktrees: vec![],
        warnings: vec![],
    };
    if resolved.is_none() {
        repo.warnings.push("worktreeTargetUnavailable".into());
    }
    if rows.len() > 128 {
        rows.truncate(128);
        repo.warnings.push("worktreeScanLimited".into());
    }
    for mut row in rows {
        inspect_row(git, path, &mut row, resolved.as_ref(), repo.shallow);
        repo.worktrees.push(row);
    }
    // A changed target cannot substantiate any of the earlier containment results.
    if let Some((reference, start)) = resolved {
        if git
            .text(
                path,
                &["rev-parse", "--verify", &format!("{reference}^{{commit}}")],
            )
            .as_deref()
            != Some(&start)
        {
            repo.warnings.push("worktreeChangedDuringScan".into());
            for row in &mut repo.worktrees {
                row.merge_status = "unknown".into();
                row.ahead_count = None;
                row.complete = false;
                row.recommendation = "review".into();
                reason(row, "worktreeChangedDuringScan");
            }
        }
    }
    Ok(repo)
}

fn parse_worktrees(bytes: &[u8]) -> Result<Vec<Worktree>, Error> {
    let text = std::str::from_utf8(bytes).map_err(|_| fail("worktreePathEncoding"))?;
    let mut rows = vec![];
    for record in text.split("\0\0").filter(|r| !r.is_empty()) {
        let mut row = Worktree {
            path: String::new(),
            branch: None,
            head: String::new(),
            main_checkout: rows.is_empty(),
            locked: false,
            lock_reason: None,
            prunable: false,
            merge_status: "unknown".into(),
            ahead_count: None,
            last_commit_at: None,
            source_modified_at: None,
            modified_count: None,
            untracked_count: None,
            ignored_count: None,
            logical_bytes: 0,
            complete: true,
            recommendation: "review".into(),
            reasons: vec![],
        };
        for line in record.split('\0') {
            let (key, value) = line.split_once(' ').unwrap_or((line, ""));
            match key {
                "worktree" => row.path = value.into(),
                "HEAD" => row.head = value.into(),
                "branch" => {
                    row.branch = Some(value.strip_prefix("refs/heads/").unwrap_or(value).into())
                }
                "locked" => {
                    row.locked = true;
                    row.lock_reason = Some(value.into());
                }
                "prunable" => row.prunable = true,
                "bare" => {
                    row.complete = false;
                    reason(&mut row, "worktreeBare");
                }
                _ => (),
            }
        }
        if row.path.is_empty() || !Path::new(&row.path).is_absolute() {
            return Err(fail("worktreePathEncoding"));
        }
        rows.push(row);
    }
    Ok(rows)
}

fn inspect_row(
    git: &Git,
    repository: &Path,
    row: &mut Worktree,
    target: Option<&(String, String)>,
    shallow: bool,
) {
    if row.main_checkout {
        reason(row, "worktreeMainCheckout");
    }
    if row.locked {
        reason(row, "worktreeLocked");
    }
    if row.prunable {
        reason(row, "worktreeMissing");
    }
    if row.reasons.iter().any(|r| r == "worktreeBare") {
        row.recommendation = "keep".into();
        return;
    }
    let path = PathBuf::from(&row.path);
    if fs::symlink_metadata(&path)
        .map(|m| !m.is_dir() || m.is_symlink())
        .unwrap_or(true)
    {
        row.complete = false;
        reason(row, "worktreeMissing");
        row.recommendation = "keep".into();
        return;
    }
    let root = match nofollow_directory(&path) {
        Ok(root) => root,
        Err(_) => {
            row.complete = false;
            reason(row, "worktreeScanIncomplete");
            return;
        }
    };
    if oid(&row.head) {
        row.last_commit_at = git
            .text(repository, &["show", "-s", "--format=%ct", &row.head])
            .and_then(|v| v.parse().ok());
        if let Some((_, target)) = target {
            row.ahead_count = git
                .text(
                    repository,
                    &["rev-list", "--count", &format!("{target}..{}", row.head)],
                )
                .and_then(|v| v.parse().ok());
            row.merge_status = match row.ahead_count {
                Some(0) => "contained",
                Some(_) if !shallow => "notContained",
                _ => "unknown",
            }
            .into();
        }
    }
    if row.merge_status != "contained" {
        reason(
            row,
            if row.merge_status == "unknown" {
                "worktreeMergeUnknown"
            } else {
                "worktreeNotContained"
            },
        );
    }
    // Git status may invoke clean/process filters when comparing files. Disable configured
    // filters before status; a resulting dirty report is conservative, never permission to delete.
    let config = git.run(&path, &["config", "--null", "--list"]);
    let mut filter_overrides = vec![];
    if let Ok(config) = &config {
        if let Ok(config) = std::str::from_utf8(config) {
            for entry in config.split('\0') {
                let key = entry.split('\n').next().unwrap_or("");
                if key.starts_with("filter.")
                    && (key.ends_with(".clean") || key.ends_with(".process"))
                {
                    filter_overrides.push(format!("{key}="));
                }
                if key.starts_with("filter.") && key.ends_with(".required") {
                    filter_overrides.push(format!("{key}=false"));
                }
            }
        } else {
            row.complete = false;
        }
    } else {
        row.complete = false;
    }
    if path.join(".gitmodules").exists() {
        row.complete = false;
        reason(row, "worktreeNestedRepository");
    }
    let index_hidden = match git.run(&path, &["ls-files", "-v", "-z"]) {
        Ok(entries) => entries
            .split(|c| *c == 0)
            .filter(|e| !e.is_empty())
            .any(|e| {
                e.first()
                    .is_some_and(|tag| tag.is_ascii_lowercase() || *tag == b'S')
            }),
        Err(_) => {
            row.complete = false;
            true
        }
    };
    if row.complete {
        match git
            .configured(
                &path,
                &[
                    "status",
                    "--porcelain=v1",
                    "-z",
                    "--untracked-files=all",
                    "--ignore-submodules=all",
                ],
                &filter_overrides,
            )
            .and_then(|v| parse_status(&v))
        {
            Ok((modified, untracked)) => {
                row.modified_count = Some(modified);
                row.untracked_count = Some(untracked);
            }
            Err(_) => {
                row.complete = false;
                reason(row, "worktreeStatusUnavailable");
            }
        }
    }
    if index_hidden {
        // assume-unchanged / skip-worktree deliberately hide files from status.
        row.complete = false;
        row.modified_count = None;
        reason(row, "worktreeIndexHidden");
    }
    if row.modified_count.is_some_and(|n| n > 0) {
        reason(row, "worktreeDirty");
    }
    if row.untracked_count.is_some_and(|n| n > 0) {
        reason(row, "worktreeUntracked");
    }
    match git.run(
        &path,
        &[
            "ls-files",
            "--others",
            "--ignored",
            "--exclude-standard",
            "--directory",
            "-z",
        ],
    ) {
        Ok(entries) => {
            row.ignored_count = Some(entries.split(|c| *c == 0).filter(|e| !e.is_empty()).count());
        }
        Err(_) => {
            row.complete = false;
        }
    }
    if row.ignored_count.is_some_and(|n| n > 0) {
        reason(row, "worktreeIgnored");
    }
    match git.run(
        &path,
        &[
            "ls-files",
            "--cached",
            "--others",
            "--exclude-standard",
            "-z",
        ],
    ) {
        Ok(entries) => {
            for (n, file) in entries
                .split(|c| *c == 0)
                .filter(|e| !e.is_empty())
                .enumerate()
            {
                if n >= FILE_LIMIT || Instant::now() >= git.deadline {
                    row.complete = false;
                    break;
                }
                let Ok(file) = std::str::from_utf8(file) else {
                    row.complete = false;
                    continue;
                };
                let relative = Path::new(file);
                if relative
                    .components()
                    .any(|c| !matches!(c, std::path::Component::Normal(_)))
                {
                    row.complete = false;
                    continue;
                }
                if relative.components().any(|c| {
                    matches!(
                        c.as_os_str().to_str(),
                        Some(
                            "node_modules"
                                | "target"
                                | "dist"
                                | "build"
                                | ".venv"
                                | "vendor"
                                | ".git"
                        )
                    )
                }) {
                    continue;
                }
                match source_modified(&root, relative) {
                    Ok(Some(time)) => {
                        row.source_modified_at = Some(row.source_modified_at.unwrap_or(0).max(time))
                    }
                    Ok(None) => (),
                    Err(_) => row.complete = false,
                }
            }
        }
        Err(_) => row.complete = false,
    }
    measure(root, row, git.deadline);
    if git
        .text(&path, &["rev-parse", "--verify", "HEAD"])
        .as_deref()
        != Some(&row.head)
    {
        row.complete = false;
        row.merge_status = "unknown".into();
        row.ahead_count = None;
        reason(row, "worktreeChangedDuringScan");
    }
    if !row.complete {
        reason(row, "worktreeScanIncomplete");
    }
    let must_keep = row.main_checkout
        || row.locked
        || row.prunable
        || row.modified_count.is_some_and(|n| n > 0)
        || row.untracked_count.is_some_and(|n| n > 0)
        || row.merge_status == "notContained";
    row.recommendation = if must_keep {
        "keep"
    } else if !row.complete
        || row.merge_status == "unknown"
        || row.ignored_count.is_some_and(|n| n > 0)
    {
        "review"
    } else {
        "candidate"
    }
    .into();
}

fn parse_status(bytes: &[u8]) -> Result<(usize, usize), Error> {
    let mut parts = bytes.split(|c| *c == 0).filter(|p| !p.is_empty());
    let (mut modified, mut untracked) = (0, 0);
    while let Some(entry) = parts.next() {
        if entry.len() < 4 || entry[2] != b' ' {
            return Err(fail("worktreeStatusUnavailable"));
        }
        if &entry[..2] == b"??" {
            untracked += 1;
        } else {
            modified += 1;
            if (entry[..2].contains(&b'R') || entry[..2].contains(&b'C')) && parts.next().is_none()
            {
                return Err(fail("worktreeStatusUnavailable"));
            }
        }
    }
    Ok((modified, untracked))
}

// Directory descriptors and openat prevent following a replaced descendant symlink.
fn nofollow_directory(path: &Path) -> std::io::Result<fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_DIRECTORY | libc::O_CLOEXEC)
        .open(path)
}
fn open_directory_at(parent: &fs::File, name: &std::ffi::CStr) -> std::io::Result<fs::File> {
    use std::os::fd::{AsRawFd, FromRawFd};
    let fd = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            name.as_ptr(),
            libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_DIRECTORY | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(unsafe { fs::File::from_raw_fd(fd) })
    }
}
fn source_modified(root: &fs::File, relative: &Path) -> std::io::Result<Option<u64>> {
    use std::{
        ffi::CString,
        os::{
            fd::{AsRawFd, FromRawFd},
            unix::ffi::OsStrExt,
        },
    };
    let mut dir = root.try_clone()?;
    let mut parts = relative.components().peekable();
    while let Some(part) = parts.next() {
        let name = CString::new(part.as_os_str().as_bytes())?;
        if parts.peek().is_some() {
            dir = open_directory_at(&dir, &name)?;
        } else {
            let mut meta = std::mem::MaybeUninit::<libc::stat>::uninit();
            if unsafe {
                libc::fstatat(
                    dir.as_raw_fd(),
                    name.as_ptr(),
                    meta.as_mut_ptr(),
                    libc::AT_SYMLINK_NOFOLLOW,
                )
            } < 0
            {
                let err = std::io::Error::last_os_error();
                return if err.kind() == std::io::ErrorKind::NotFound {
                    Ok(None)
                } else {
                    Err(err)
                };
            }
            let meta = unsafe { meta.assume_init() };
            if meta.st_mode & libc::S_IFMT != libc::S_IFREG {
                return Ok(None);
            }
            let fd = unsafe {
                libc::openat(
                    dir.as_raw_fd(),
                    name.as_ptr(),
                    libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC,
                )
            };
            if fd < 0 {
                return Err(std::io::Error::last_os_error());
            }
            let file = unsafe { fs::File::from_raw_fd(fd) };
            let meta = file.metadata()?;
            if !meta.is_file() {
                return Ok(None);
            }
            return Ok(meta
                .modified()?
                .duration_since(UNIX_EPOCH)
                .ok()
                .map(|t| t.as_secs()));
        }
    }
    Ok(None)
}
struct Directory(*mut libc::DIR);
impl Drop for Directory {
    fn drop(&mut self) {
        unsafe {
            libc::closedir(self.0);
        }
    }
}
fn measure(root: fs::File, row: &mut Worktree, deadline: Instant) {
    use std::{
        ffi::CStr,
        os::fd::{AsRawFd, IntoRawFd},
    };
    let deadline = deadline.min(Instant::now() + Duration::from_secs(2));
    let mut todo = vec![(root, true)];
    let mut seen = HashSet::new();
    let mut count = 0;
    while let Some((dir, is_root)) = todo.pop() {
        let fd = match dir.try_clone() {
            Ok(f) => f.into_raw_fd(),
            Err(_) => {
                row.complete = false;
                continue;
            }
        };
        let listing = unsafe { libc::fdopendir(fd) };
        if listing.is_null() {
            unsafe {
                libc::close(fd);
            }
            row.complete = false;
            continue;
        }
        let listing = Directory(listing);
        loop {
            if count > FILE_LIMIT || Instant::now() >= deadline {
                row.complete = false;
                return;
            }
            // Clear errno to distinguish end-of-directory from an enumeration error.
            #[cfg(target_os = "macos")]
            unsafe {
                *libc::__error() = 0;
            }
            #[cfg(target_os = "linux")]
            unsafe {
                *libc::__errno_location() = 0;
            }
            let entry = unsafe { libc::readdir(listing.0) };
            if entry.is_null() {
                if std::io::Error::last_os_error().raw_os_error().unwrap_or(0) != 0 {
                    row.complete = false;
                }
                break;
            }
            let name = unsafe { CStr::from_ptr((*entry).d_name.as_ptr()) };
            if matches!(name.to_bytes(), b"." | b"..") {
                continue;
            }
            count += 1;
            if name.to_bytes() == b".git" {
                if !is_root {
                    row.complete = false;
                    reason(row, "worktreeNestedRepository");
                }
                continue;
            }
            let mut meta = std::mem::MaybeUninit::<libc::stat>::uninit();
            if unsafe {
                libc::fstatat(
                    dir.as_raw_fd(),
                    name.as_ptr(),
                    meta.as_mut_ptr(),
                    libc::AT_SYMLINK_NOFOLLOW,
                )
            } < 0
            {
                row.complete = false;
                continue;
            }
            let meta = unsafe { meta.assume_init() };
            if meta.st_mode & libc::S_IFMT == libc::S_IFDIR {
                match open_directory_at(&dir, name) {
                    Ok(child) => todo.push((child, false)),
                    Err(_) => row.complete = false,
                }
            } else if matches!(meta.st_mode & libc::S_IFMT, libc::S_IFREG | libc::S_IFLNK)
                && seen.insert((meta.st_dev, meta.st_ino))
            {
                row.logical_bytes = row.logical_bytes.saturating_add(meta.st_size.max(0) as u64);
            }
        }
    }
}

#[cfg(test)]
mod tests;
