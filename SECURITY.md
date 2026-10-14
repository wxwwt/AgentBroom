# Security

The supported milestone is a read-only prototype with automatic local Codex inspection. It does not support deletion. Missing metadata is uncertainty, not evidence that a record is orphaned or disposable.

Do not upload private transcripts or credentials in public issues. Prefer GitHub's private vulnerability reporting if enabled, or contact the maintainer through GitHub for a private channel.

The app starts by inspecting real local data. The native IPC and loopback-only development browser adapter parses bounded Codex session headers, title index and desktop pinned IDs; other files are stat-only. Metadata and keep choices remain in memory. There is no backend account, telemetry, SQL connection, agent subprocess, custom root chooser or automatic cleanup. Real records are always protected and have no export or preview API. Symlinks are not followed. External database roots, remote data, runtime activity and compressed session contents are excluded. OS read access times and concurrent Codex writes are not an atomic immutability claim. Security boundaries are covered by the test plan; a successful build does not prove future destructive operations safe.

The localhost development endpoint only accepts GET requests from a loopback socket with an allowed Host, matching Origin/fetch metadata and application request header. It emits no CORS permission or cacheable metadata, accepts no source-root input, uses a fixed own-project CLI command with no shell, and does not log inventory contents. This is a local-owner boundary, not isolation from a malicious process already running as the same OS user.
