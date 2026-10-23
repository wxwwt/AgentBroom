# AgentBroom

**Codex local storage, clearly.** · [简体中文](README.zh-CN.md)

AgentBroom is an early desktop tool for inspecting local Codex storage and explicitly confirmed native conversation cleanup. It distinguishes logical conversations, physical segments, spawned Agents, independent forks and shared-history references.

## Use

The desktop opens your configured `CODEX_HOME` (otherwise `$HOME/.codex`) for read-only inventory and native identity lookup. Nothing is selected or deleted automatically. English and Simplified Chinese are available. The minimal interface uses a quiet top navigation with Overview, Conversations and Execution log. Light/dark appearance can be switched in the header and is remembered locally; first launch uses the system appearance.

Conversations are **unlocked by default**. In conversation details, choose **Lock conversation** to prevent deletion, or **Unlock conversation** to allow it again. Locks are saved locally, survive rescans/restarts, and are enforced by the backend. A locked parent protects its native spawned descendants; a locked descendant blocks a plan containing it. Codex pins are separate from AgentBroom locks.

The **Conversations** table supports search, activity/size/title sorting, pagination, individual row actions, batch locking/unlocking and selection of up to 128 conversations. Choose **Clean…** or **Clean selected** to review their native spawned scope, measured files, retained records and notices. Parent/child selections are deduplicated; independent roots run sequentially, stop on the first failed/unknown request, and share a single confirmation and overall result verification. A lock conflict blocks the entire selection. Age, recent writes and fork/history references do not automatically lock records. Independent forks are never automatically included. Linked files, unidentified paths, unsupported runtimes/databases and changed plans can still prevent execution; these are execution errors, not manual protection.

Selecting Delete opens one confirmation dialog with the backend-validated scope, associated Sub Agents and measured size. Clicking Permanently delete in that dialog starts official native `thread/delete`; no typing or extra checkbox is required. Canceling or merely opening the dialog never executes deletion. It can also update Codex indexes and remove native metadata belonging to those conversations. Receipts are **not backups**. Deletion cannot be undone through AgentBroom.

The backend checks native/observed scopes, file content/identity, source identity, runtime and database compatibility again before execution. It verifies deleted notifications, selected files/state and retained records afterwards. Partial failure, unknown outcomes and concurrent changes are reported. File lengths and allocated blocks are accounting, **not reclaimed-space estimates**. No vacuum, compression, cache cleanup or project/worktree removal is implemented.

**Worktrees** is a separate read-only desktop page. It discovers Codex worktrees and conversation project candidates, or inspects an explicit repository path. Choose a target branch (automatic: locally cached `origin/HEAD`, `main`, `master`) and compare current HEAD containment, commits outside the target, uncommitted/untracked/ignored entries, Git locks, last commit, observed source-file modification time, and measured file lengths. Search, assessment filters and sorting help review old or large directories. No fetch, checkout, worktree removal, branch deletion or pruning is performed. Shared Git storage is excluded from worktree sizes; source timestamps exclude dependencies/build output and do not prove last use. Squash/cherry-pick history can require manual review. Ignored entries may contain private docs or configuration even when Git is clean. Missing paths, shallow history, scan limits and incomplete inspection are shown explicitly.

## Runtime and safety boundaries

The initial native target is macOS Apple silicon; metadata inspection also supports Linux. Native compatibility is based on validated storage contracts (state migration histories 59 and 61), rather than an exact executable-version allowlist. Compatible runtime updates need no version-list update. Unknown or modified storage structures still pause cleanup. Generated-data regressions cover `0.162.1` and `0.163.0-alpha.10.2`; the earlier bundled `0.162.0-alpha.17.2` uses the same state-59 contract. On macOS the bundled official Codex executable is discovered inside ChatGPT/Codex. A developer can explicitly set `AGENTBROOM_CODEX_BIN` to a trusted official executable. AgentBroom does not upgrade Codex or migrate existing databases; incompatible native schemas remain blocked. Source roots and executable paths cannot be supplied by the renderer. Runtime/binary changes after preview still require a new preview. Cleanup also verifies thread-owned attachments, tools, read receipts/review feedback when present, goals, logs, memory jobs/outputs and queue content. Queue invalidation revisions are retained and advanced by Codex when queued content is removed; they are not leftover queue content. Codex may expire logs at startup; such changes are reported separately, and deletion is checked against the log snapshot after startup. All other associated data keeps its original verification baseline.

Inventory uses bounded Zstandard session headers and read-only native identities/spawn edges, recovering indexed identities when header scanning is incomplete. Cleanup previews additionally hash the selected files; bodies are never displayed or exported. Actual deletion is delegated to the official native store, including its writer/reference checks. This is not an atomic snapshot of a concurrently running Codex instance. Recent writes are shown as notices. Official native writer/reference checks can still reject a requested deletion.

**Overview** includes identified Codex/CodexCLI reports from macOS user/system DiagnosticReports, Codex Crashpad pending/completed/new directories, and dedicated crash directories within Codex home. Other applications and Crashpad settings are excluded. External report bytes are added once; unavailable locations are explicitly marked incomplete. Inspect reports, select exact files and confirm once before removal; file identity/content are checked again and removed files verified. Neither measured file lengths nor allocated blocks guarantee reclaimable space.

**Execution log** records scans, lock changes, previews and cleanup outcomes, including affected IDs/titles and verified removed file lengths. History is kept for seven days, with expired AgentBroom log files pruned when history is read or written. This retention does not erase the separate durable at-most-once execution receipts needed for uncertain outcomes. No transcript bodies are logged.

Execution intent and results are atomically persisted under `~/.agentbroom/cleanup-receipts/`, without transcript bodies. Manual locks are source-scoped files in its private `locks/` subdirectory and never change Codex data. Unknown operations cannot automatically retry, including after restart. Other instances are serialized by an application lock. Graceful exit waits for an ongoing operation. Forced OS termination is not recovery; inspect the saved receipt and current data before taking further action.

The loopback development browser remains **read-only** (appearance, navigation and table inspection are available), with no write HTTP endpoint or fixture fallback. Native cleanup requires the desktop app.

## Development and tests

Requires Node.js 24+, stable Rust and [Tauri 2 prerequisites](https://v2.tauri.app/start/prerequisites/).

```sh
npm ci
npm run desktop:dev
npm run desktop:build
npm test
cargo test --locked -p agentbroom-core -p agentbroom-cli -p agentbroom-native
cargo test --locked -p agentbroom-core --features synthetic-cleanup
```

All automated mutation tests use newly generated data. Do not use real conversations as fixtures. Native integration tests require an explicitly selected trusted executable; no global CLI fallback:

```sh
AGENTBROOM_TEST_CODEX_BIN=/absolute/path/to/official/codex cargo test --locked -p agentbroom-native -- --ignored --test-threads=1
AGENTBROOM_TEST_CODEX_BIN=/absolute/path/to/official/codex cargo test --locked -p agentbroom-native-test -- --ignored --test-threads=1
cargo build --locked -p agentbroom-cli
AGENTBROOM_TEST_CODEX_BIN=/absolute/path/to/official/codex npm run test:native
npm run test:native-helper
npm run test:fixtures
```

`native-cleanup-test` is a separate developer-only desktop build (`npm run desktop:test:build`). It generates its own store, never imports real data and shares the transport and confirmation DTOs with the independently constructed live owner. It is not enabled in the regular app. Its generated store is discarded on restart, and manual lock controls are available only in the regular desktop app. Manual-lock policy is tested through the real owner using generated stores. Python is only a test dependency.

## Privacy and documentation

No AgentBroom account, cloud inventory, telemetry or LLM call. Native deletion runs locally with isolated non-source runtime directories and an offline model provider. Headers are treated as untrusted data, never instructions. No transcript export is provided. Local CLI `scan`/`cleanup-scope` output can contain private metadata; do not publish it.

Internal designs, research, review reports and verification logs stay in local Git-ignored `docs/`. Only code, generated test assets and necessary public guidance are distributed. See [CONTRIBUTING.md](CONTRIBUTING.md) and [SECURITY.md](SECURITY.md).

## License

MIT. This project is independent of OpenAI and other agent vendors. Dependencies retain their own licenses.
