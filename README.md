# AgentBroom

**Codex local storage, clearly.** · [简体中文](README.zh-CN.md)

AgentBroom is an early desktop tool for inspecting local Codex storage and explicitly confirmed native conversation cleanup. It distinguishes logical conversations, physical segments, spawned Agents, independent forks and shared-history references.

## Use

The desktop opens your configured `CODEX_HOME` (otherwise `$HOME/.codex`) for read-only inventory and native identity lookup. Nothing is selected or deleted automatically. English and Simplified Chinese are available.

Conversations are **unlocked by default**. In conversation details, choose **Lock conversation** to prevent deletion, or **Unlock conversation** to allow it again. Locks are saved locally, survive rescans/restarts, and are enforced by the backend. A locked parent protects its native spawned descendants; a locked descendant blocks a plan containing it. Codex pins are separate from AgentBroom locks.

Open **Cleanup preview**, select a conversation (including a native-indexed sub Agent or orphan), and review its native spawned subtree, measured files, retained records and notices. Age, recent writes and fork/history references do not automatically lock records. Independent forks are never automatically included. Linked files, unidentified paths, unsupported runtimes/databases and changed plans can still prevent execution; these are execution errors, not manual protection.

If the backend can validate the plan, review the permanent-deletion warning, acknowledge the scope and type `DELETE`. Only this confirmation starts official native `thread/delete`. It can also update Codex indexes and remove native metadata belonging to those conversations. Receipts are **not backups**. Deletion cannot be undone through AgentBroom.

The backend checks native/observed scopes, file content/identity, source identity, runtime and database compatibility again before execution. It verifies deleted notifications, selected files/state and retained records afterwards. Partial failure, unknown outcomes and concurrent changes are reported. File lengths and allocated blocks are accounting, **not reclaimed-space estimates**. No vacuum, compression, cache cleanup or project/worktree removal is implemented.

## Runtime and safety boundaries

The initial native target is macOS Apple silicon; metadata inspection also supports Linux. Supported native versions are exactly `0.162.0-alpha.17.2` and `0.162.1`, tested on newly generated stores. On macOS the bundled official Codex executable is discovered inside ChatGPT/Codex. A developer can explicitly set `AGENTBROOM_CODEX_BIN` to a trusted official executable. AgentBroom does not upgrade Codex or migrate existing databases; incompatible native schemas remain blocked. Source roots and executable paths cannot be supplied by the renderer.

Inventory uses bounded Zstandard session headers and read-only native identities/spawn edges, recovering indexed identities when header scanning is incomplete. Cleanup previews additionally hash the selected files; bodies are never displayed or exported. Actual deletion is delegated to the official native store, including its writer/reference checks. This is not an atomic snapshot of a concurrently running Codex instance. Recent writes are shown as notices. Official native writer/reference checks can still reject a requested deletion.

Execution intent and results are atomically persisted under `~/.agentbroom/cleanup-receipts/`, without transcript bodies. Manual locks are source-scoped files in its private `locks/` subdirectory and never change Codex data. Unknown operations cannot automatically retry, including after restart. Other instances are serialized by an application lock. Graceful exit waits for an ongoing operation. Forced OS termination is not recovery; inspect the saved receipt and current data before taking further action.

The loopback development browser remains **read-only**, with no write HTTP endpoint or fixture fallback. Native cleanup requires the desktop app.

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
AGENTBROOM_TEST_CODEX_BIN=/absolute/path/to/official-0.162.1/codex npm run test:native
npm run test:native-helper
npm run test:fixtures
```

`native-cleanup-test` is a separate developer-only desktop build (`npm run desktop:test:build`). It generates its own store, never imports real data and shares the transport and confirmation DTOs with the independently constructed live owner. It is not enabled in the regular app. Its generated store is discarded on restart, and manual lock controls are available only in the regular desktop app. Manual-lock policy is tested through the real owner using generated stores. Python is only a test dependency.

## Privacy and documentation

No AgentBroom account, cloud inventory, telemetry or LLM call. Native deletion runs locally with isolated non-source runtime directories and an offline model provider. Headers are treated as untrusted data, never instructions. No transcript export is provided. Local CLI `scan`/`cleanup-scope` output can contain private metadata; do not publish it.

Internal designs, research, review reports and verification logs stay in local Git-ignored `docs/`. Only code, generated test assets and necessary public guidance are distributed. See [CONTRIBUTING.md](CONTRIBUTING.md) and [SECURITY.md](SECURITY.md).

## License

MIT. This project is independent of OpenAI and other agent vendors. Dependencies retain their own licenses.
