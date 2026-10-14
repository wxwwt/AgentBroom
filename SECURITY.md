# Security

The supported milestone is a read-only prototype with synthetic demo and explicit native local Codex inspection. It does not support deletion. Missing metadata is uncertainty, not evidence that a record is orphaned or disposable.

Do not upload private transcripts or credentials in public issues. Prefer GitHub's private vulnerability reporting if enabled, or contact the maintainer through GitHub for a private channel.

The app starts with bundled synthetic files. Explicit native inspection parses bounded Codex session headers, title index and desktop pinned IDs; other files are stat-only. Metadata and keep choices remain in memory. There is no backend account, telemetry, SQL connection, agent subprocess, custom root chooser or automatic cleanup. Real records are always protected and have no export or preview API. Symlinks are not followed. External database roots, remote data, runtime activity and compressed session contents are excluded. OS read access times and concurrent Codex writes are not an atomic immutability claim. Security boundaries are covered by the test plan; a successful build does not prove future destructive operations safe.
