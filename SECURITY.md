# Security

The supported milestone is a demo-only, read-only prototype. It does not support real agent stores or deletion. Missing metadata is uncertainty, not evidence that a record is orphaned or disposable.

Do not upload private transcripts or credentials in public issues. Prefer GitHub's private vulnerability reporting if enabled, or contact the maintainer through GitHub for a private channel.

The app reads bundled synthetic files and transient in-memory choices. There is no backend account, telemetry, SQL connection, agent subprocess, custom root chooser or automatic cleanup. Security boundaries are covered by the test plan; a successful build does not prove future destructive operations safe.
