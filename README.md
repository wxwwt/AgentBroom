# AgentBroom

**Codex local storage, clearly.**

[简体中文](README.zh-CN.md)

AgentBroom is an early, local-first desktop inspection prototype for coding-agent storage. It makes physical files, logical conversations, spawned agents and independent forks easier to distinguish before anyone considers cleanup.

> **Read-only local inventory.** Launch the regular app to inspect your real `CODEX_HOME` (or `$HOME/.codex`) immediately. There is no demo mode, sample plan or sample report in the regular application. Source failures are shown explicitly. Cleanup candidates, observed impact and a locked confirmation page are available. No real source write, deletion or database connection is available; all real records stay protected.

## Prototype features

- Codex storage overview with file lengths and allocated blocks.
- Logical conversations grouped across physical segments; parent, spawned agent, fork lineage and shared-history evidence stay distinct.
- Explicit protection reasons for pins, references, missing parents, conflicts, unsupported headers and unknown runtime activity.
- English / Simplified Chinese switching, with saved language preference and system-language defaults.
- Search, filters, details and session-local keep choices.
- Cleanup review candidates, observed main/spawn scope, retained records, outside dependencies and explicit blocking reasons.
- Tauri desktop and an internal CLI sharing one Rust core.

Real Codex deletion, database reclamation, compression and recovery are **not implemented**. A feature-gated developer harness really deletes backend-generated synthetic files, updates synthetic JSON state, and verifies removed and preserved objects; it has no path/real-store constructor and is absent from default desktop builds.

## Development

Requires Node.js 24+, a stable Rust toolchain and the platform prerequisites for [Tauri 2](https://v2.tauri.app/start/prerequisites/). The initial native target is macOS Apple silicon.

```sh
npm install
cargo test --locked -p agentbroom-core -p agentbroom-cli
cargo test --locked -p agentbroom-core --features synthetic-cleanup # private generated data only
npm run test:fixtures
npm run dev              # real read-only local web app on 127.0.0.1:1420
npm run desktop:dev      # native Tauri app
```

```sh
npm test
npm run test:native-helper
npm run build
npm run desktop:build
cargo run -p agentbroom-cli -- codex-summary # real aggregate counts/sizes only
```

The localhost development UI uses a loopback-only, same-origin endpoint backed by the same Rust CLI scanner. It does not fall back to synthetic data. The native application uses `scan_codex` IPC and does not host HTTP. Both regular entry points automatically inspect real metadata on startup. Synthetic fixture generation is for tests only; fixtures are not bundled into the regular app.

Choose either the browser or native development command; stop the browser development server before starting native development, as both use port 1420. Rust `cargo` must be available on PATH. Static web builds require the local read-only development bridge or the native app; there is no remote inventory service.

## Native contract tests

Developer-only native integration tests use the official Codex CLI 0.162.1 executable. They create a fresh private HOME/CODEX_HOME and synthetic rollouts/database, then test native deletion, spawned descendants, independent/copied forks, external shared history, multiple segments, writer conflicts and mismatched scopes. They never accept an existing data root and never fall back to your global CLI. Python 3 is needed only for these tests; it is not part of the desktop runtime.

```sh
cargo build --locked -p agentbroom-cli
AGENTBROOM_TEST_CODEX_BIN=/path/to/official-0.162.1/codex npm run test:native
```

The `cleanup-scope <logical-id>...` CLI command is a local, read-only analysis using the shared Rust planner; its output can contain private metadata and must not be published. Comparing observed/native logical-ID sets is evidence only, never permission to delete. Production `cleanup_execute` remains locked. These tests do not establish safety for a live store, complete auxiliary-database effects, atomic scope checks, recovery or reclaimed disk space.

## Documentation

Public usage, development and security guidance is kept in this README, [CONTRIBUTING.md](CONTRIBUTING.md) and [SECURITY.md](SECURITY.md). Internal designs, research, review reports and verification notes are local-only under `docs/`, excluded from Git and not distributed with this repository.

## Contributing

Read [CONTRIBUTING.md](CONTRIBUTING.md). Please use synthetic data, keep read-only boundaries intact, and avoid uploading real transcripts, paths, screenshots or credentials. This prototype does not accept real agent data as a debugging fixture.

## Security and privacy

No account, telemetry, remote network service or LLM call in the regular application. The native Codex adapter reads only bounded session headers, title-index rows and desktop pinned IDs. Auth/config/database/asset contents are never opened. Session headers may contain instructions; unused fields are discarded, and conversation bodies are not parsed. The regular native product never writes source files, opens SQLite, runs maintenance or launches agent processes. `cleanup_execute` unconditionally rejects before source access. The browser development transport invokes only the fixed read-only scanner. Symlinks are never followed. macOS/Linux native inspection only; unsupported platforms report an error and do not fall back to demo. Report vulnerabilities using the repository's private reporting channel when enabled; otherwise contact the maintainer through GitHub before sharing sensitive reproduction material. See [SECURITY.md](SECURITY.md).

## License

MIT. Third-party dependencies retain their own licenses. This project is independent of OpenAI, Anthropic and other agent vendors.

### Isolated native desktop test

The regular desktop build remains read-only. A separate developer build connects the preview, confirmation and receipt UI to official Codex 0.162.1 deletion **only in a newly generated temporary test store**. It never loads your real Codex home. This does not enable production cleanup.

```sh
AGENTBROOM_TEST_CODEX_BIN=/absolute/path/to/official/codex cargo test --locked -p agentbroom-native-test -- --ignored --test-threads=1
npm run desktop:test:build
```

Launch **AgentBroom Native Test** with `AGENTBROOM_TEST_CODEX_BIN` set to your trusted official executable; the binary is not bundled or discovered from PATH. Select one test conversation, preview its native subtree and retained records, acknowledge and type `DELETE`, then review the verified receipt. Closing the result refreshes the inventory. An uncertain response can query the saved receipt without retrying deletion. Receipts last only for this running test session; they are not backups. No live-root argument or browser write endpoint exists.
