# AgentBroom repository guidance

- Keep internal designs, research, review reports and verification notes in local `docs/`. The whole directory is Git-ignored and must not be force-added or uploaded.
- Publish only project code, synthetic test assets, and necessary public usage/contribution/security documentation. Do not copy local `docs/` content into tracked files, pull request bodies, comments, releases, or external review packages without explicit user authorization.
- Check the staged file list before every commit/push for internal documents and private data. `.gitignore` does not untrack previously committed files.
- Keep real Agent data read-only unless the user explicitly authorizes a concrete operation. Tests use synthetic data only.
