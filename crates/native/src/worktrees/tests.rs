use super::*;
use std::{collections::BTreeMap, os::unix::fs::symlink};

struct Fixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
    linked: PathBuf,
}
fn command(path: &Path, args: &[&str]) -> String {
    let out = Command::new("/usr/bin/git")
        .arg("-C")
        .arg(path)
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_AUTHOR_DATE", "2020-01-01T00:00:00Z")
        .env("GIT_COMMITTER_DATE", "2020-01-01T00:00:00Z")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{:?}: {}",
        args,
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap().trim().into()
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("source repo");
        fs::create_dir(&root).unwrap();
        command(&root, &["init", "-b", "main"]);
        command(
            &root,
            &["config", "user.email", "synthetic@example.invalid"],
        );
        command(&root, &["config", "user.name", "Synthetic"]);
        fs::write(root.join("source.txt"), "base\n").unwrap();
        command(&root, &["add", "source.txt"]);
        command(&root, &["commit", "-m", "base"]);
        let linked = temp.path().join("linked\nwith spaces");
        command(
            &root,
            &["worktree", "add", "-b", "feature", linked.to_str().unwrap()],
        );
        Self {
            _temp: temp,
            root,
            linked,
        }
    }
    fn scan(&self) -> Inventory {
        inspect_at(
            Request {
                repository: Some(display(&self.root)),
                ..Default::default()
            },
            &self._temp.path().join("not-codex"),
        )
        .unwrap()
    }
    fn commit(&self) {
        fs::write(self.linked.join("source.txt"), "feature\n").unwrap();
        command(&self.linked, &["add", "source.txt"]);
        command(&self.linked, &["commit", "-m", "feature"]);
    }
}
fn linked(inventory: &Inventory) -> &Worktree {
    inventory.repositories[0]
        .worktrees
        .iter()
        .find(|w| !w.main_checkout)
        .unwrap()
}
fn snapshot(path: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut all = BTreeMap::new();
    let mut todo = vec![path.to_path_buf()];
    while let Some(dir) = todo.pop() {
        for entry in fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let meta = fs::symlink_metadata(entry.path()).unwrap();
            if meta.is_dir() {
                todo.push(entry.path());
            } else if meta.is_file() {
                all.insert(entry.path(), fs::read(entry.path()).unwrap());
            }
        }
    }
    all
}
#[test]
fn merged_clean_worktree_is_candidate_and_source_is_unchanged() {
    let f = Fixture::new();
    f.commit();
    command(&f.root, &["merge", "--no-ff", "feature", "-m", "merged"]);
    let before = snapshot(f._temp.path());
    let inventory = f.scan();
    let row = linked(&inventory);
    assert_eq!(row.merge_status, "contained");
    assert_eq!(row.ahead_count, Some(0));
    assert_eq!(row.recommendation, "candidate");
    assert_eq!(row.last_commit_at, Some(1577836800));
    assert!(row.source_modified_at.unwrap() > 1577836800);
    assert_eq!(row.path, display(&f.linked.canonicalize().unwrap()));
    assert_eq!(snapshot(f._temp.path()), before);
    assert_eq!(
        inventory.repositories[0].worktrees[0].recommendation,
        "keep"
    );
}
#[test]
fn merged_then_new_commit_is_not_contained_and_squash_is_not_claimed_merged() {
    let f = Fixture::new();
    f.commit();
    command(&f.root, &["merge", "--squash", "feature"]);
    command(&f.root, &["commit", "-m", "squash"]);
    assert_eq!(linked(&f.scan()).merge_status, "notContained");
    command(&f.root, &["merge", "feature", "-m", "merge"]);
    fs::write(f.linked.join("new.txt"), "new").unwrap();
    command(&f.linked, &["add", "new.txt"]);
    command(&f.linked, &["commit", "-m", "after merge"]);
    let inv = f.scan();
    assert_eq!(linked(&inv).ahead_count, Some(1));
    assert_eq!(linked(&inv).recommendation, "keep");
}
#[test]
fn dirty_untracked_ignored_and_locked_reasons_are_independent() {
    let f = Fixture::new();
    fs::write(f.linked.join("source.txt"), "uncommitted").unwrap();
    fs::write(f.linked.join("draft.txt"), "draft").unwrap();
    fs::write(f.linked.join(".gitignore"), "docs/\n").unwrap();
    fs::create_dir(f.linked.join("docs")).unwrap();
    fs::write(f.linked.join("docs/private.md"), "private").unwrap();
    command(
        &f.root,
        &[
            "worktree",
            "lock",
            f.linked.to_str().unwrap(),
            "--reason",
            "Keep this",
        ],
    );
    let inv = f.scan();
    let row = linked(&inv);
    assert_eq!(row.modified_count, Some(1));
    assert_eq!(row.untracked_count, Some(2));
    assert_eq!(row.ignored_count, Some(1));
    for code in [
        "worktreeDirty",
        "worktreeUntracked",
        "worktreeIgnored",
        "worktreeLocked",
    ] {
        assert!(row.reasons.contains(&code.into()));
    }
    assert_eq!(row.lock_reason.as_deref(), Some("Keep this"));
    assert_eq!(row.recommendation, "keep");
    assert!(row.logical_bytes >= "private".len() as u64);
}
#[test]
fn ignored_private_files_require_review_even_when_git_is_clean() {
    let f = Fixture::new();
    fs::write(f.root.join(".git/info/exclude"), "docs/\n").unwrap();
    fs::create_dir(f.linked.join("docs")).unwrap();
    fs::write(f.linked.join("docs/private.md"), "private").unwrap();
    let inv = f.scan();
    let row = linked(&inv);
    assert_eq!(row.modified_count, Some(0));
    assert_eq!(row.untracked_count, Some(0));
    assert_eq!(row.recommendation, "review");
}
#[test]
fn missing_target_is_unknown_and_detached_head_can_be_checked() {
    let f = Fixture::new();
    command(&f.linked, &["checkout", "--detach"]);
    assert_eq!(linked(&f.scan()).merge_status, "contained");
    assert_eq!(linked(&f.scan()).branch, None);
    let inv = inspect_at(
        Request {
            repository: Some(display(&f.root)),
            target: Some("absent".into()),
            ..Default::default()
        },
        f._temp.path(),
    )
    .unwrap();
    assert_eq!(linked(&inv).merge_status, "unknown");
    assert_eq!(linked(&inv).recommendation, "review");
    assert_eq!(inv.repositories[0].target_ref, None);
}
#[test]
fn symlink_targets_are_not_counted_as_directory_contents_and_build_mtime_is_excluded() {
    let f = Fixture::new();
    let outside = f._temp.path().join("outside");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("large.bin"), vec![0u8; 100_000]).unwrap();
    symlink(&outside, f.linked.join("external")).unwrap();
    fs::create_dir(f.linked.join("node_modules")).unwrap();
    fs::write(f.linked.join("node_modules/generated"), "dependency").unwrap();
    let inv = f.scan();
    let row = linked(&inv);
    assert!(row.logical_bytes < 100_000);
    assert!(row.source_modified_at.is_some());
}
#[test]
fn scan_disables_clean_filters_and_does_not_refresh_index() {
    let f = Fixture::new();
    let marker = f._temp.path().join("must-not-run");
    command(
        &f.linked,
        &[
            "config",
            "filter.sentinel.clean",
            &format!("touch '{}'; cat", marker.display()),
        ],
    );
    fs::write(f.linked.join(".gitattributes"), "*.txt filter=sentinel\n").unwrap();
    fs::write(f.linked.join("source.txt"), "changed after index\n").unwrap();
    let before = snapshot(f._temp.path());
    let inv = f.scan();
    assert!(!marker.exists());
    assert!(linked(&inv).modified_count.unwrap() > 0);
    assert_eq!(snapshot(f._temp.path()), before);
}
#[test]
fn discover_deduplicates_common_repositories_and_does_not_walk_symlinked_roots() {
    let f = Fixture::new();
    let codex = f._temp.path().join("codex");
    fs::create_dir_all(codex.join("worktrees/group")).unwrap();
    symlink(&f.linked, codex.join("worktrees/group/linked")).unwrap();
    let empty = inspect_at(Request::default(), &codex).unwrap();
    assert!(empty.repositories.is_empty());
    let inv = inspect_at(
        Request {
            projects: vec![display(&f.root), display(&f.linked)],
            ..Default::default()
        },
        &codex,
    )
    .unwrap();
    assert_eq!(inv.repositories.len(), 1);
}
#[test]
fn invalid_roots_and_malformed_porcelain_are_never_clean_results() {
    let f = Fixture::new();
    assert!(
        inspect_at(
            Request {
                repository: Some("relative".into()),
                ..Default::default()
            },
            f._temp.path()
        )
        .is_err()
    );
    assert!(parse_status(b"R  new\0").is_err());
    assert!(parse_worktrees(b"worktree relative\0\0").is_err());
    assert_eq!(
        parse_status(b"R  new\0old\0?? untracked\nname\0").unwrap(),
        (1, 1)
    );
    let git = Git {
        deadline: Instant::now() - Duration::from_secs(1),
    };
    assert!(git.run(&f.root, &["status"]).is_err());
}

#[test]
fn source_time_excludes_build_files_and_never_follows_replaced_parent_symlinks() {
    let f = Fixture::new();
    let stamp = UNIX_EPOCH + Duration::from_secs(1609459200);
    fs::File::open(f.linked.join("source.txt"))
        .unwrap()
        .set_times(fs::FileTimes::new().set_modified(stamp))
        .unwrap();
    fs::create_dir(f.linked.join("dist")).unwrap();
    fs::write(f.linked.join("dist/bundle.js"), "build").unwrap();
    let inv = f.scan();
    assert_eq!(linked(&inv).source_modified_at, Some(1609459200));
    let outside = f._temp.path().join("outside-source");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("secret"), "secret").unwrap();
    symlink(&outside, f.linked.join("linked-dir")).unwrap();
    let root = nofollow_directory(&f.linked).unwrap();
    assert!(source_modified(&root, Path::new("linked-dir/secret")).is_err());
}
#[test]
fn missing_registration_and_bare_object_storage_are_never_cleanup_candidates() {
    let f = Fixture::new();
    fs::rename(&f.linked, f._temp.path().join("moved")).unwrap();
    let inv = f.scan();
    let row = linked(&inv);
    assert!(!row.complete);
    assert_eq!(row.recommendation, "keep");
    assert!(row.reasons.contains(&"worktreeMissing".into()));
    let bare = f._temp.path().join("bare.git");
    command(
        &f.root,
        &[
            "clone",
            "--bare",
            f.root.to_str().unwrap(),
            bare.to_str().unwrap(),
        ],
    );
    let inv = inspect_at(
        Request {
            repository: Some(display(&bare)),
            ..Default::default()
        },
        f._temp.path(),
    )
    .unwrap();
    let row = &inv.repositories[0].worktrees[0];
    assert_eq!(row.logical_bytes, 0);
    assert_eq!(row.recommendation, "keep");
}

#[test]
fn index_flags_that_hide_local_changes_require_review_without_index_mutation() {
    let f = Fixture::new();
    command(
        &f.linked,
        &["update-index", "--assume-unchanged", "source.txt"],
    );
    fs::write(f.linked.join("source.txt"), "locally changed but hidden").unwrap();
    let before = snapshot(f._temp.path());
    let inv = f.scan();
    let row = linked(&inv);
    assert_eq!(row.recommendation, "review");
    assert_eq!(row.modified_count, None);
    assert!(row.reasons.contains(&"worktreeIndexHidden".into()));
    assert_eq!(snapshot(f._temp.path()), before);
    command(
        &f.linked,
        &["update-index", "--no-assume-unchanged", "source.txt"],
    );
    command(
        &f.linked,
        &["update-index", "--skip-worktree", "source.txt"],
    );
    let inv = f.scan();
    assert_eq!(linked(&inv).recommendation, "review");
}
