# Contributing

AgentBroom supports metadata inventory and explicitly confirmed native conversation cleanup.

1. Keep inspection policy in `crates/core`; desktop and CLI must not duplicate it.
2. Use synthetic fixtures only. Do not commit original conversations, credentials, machine paths or operational logs.
3. Keep native home discovery restricted to the documented CODEX_HOME/HOME adapter. Do not add renderer-supplied roots, shell/fs/SQL plugins, direct real-file unlinking or hand-written native SQL mutations. Real deletion must use the version-bound native owner after explicit user confirmation.
4. Cover protected/unknown/segment/fork behavior with meaningful tests. Label native smoke and browser catalog tests accurately.
5. Run default and synthetic-cleanup core tests, native generated-store integrations, cross-language fixture expectations, frontend/IPC checks and builds before a pull request.
6. Discuss additional writable surfaces first; preserve version-bound contracts, generated-only automated mutation tests, durable unknown outcomes and no automatic retry.

Contributions to the UI, accessibility, documentation and fixture coverage are welcome.

## Documentation publication boundary

Keep internal design proposals, research, review reports and verification notes in the local `docs/` directory. This directory is ignored by Git. Do not force-add it, copy its contents into tracked files, or publish it in pull request bodies, comments, releases or external review packages without explicit user authorization. Public usage and contribution/security guidance belongs in the root README, CONTRIBUTING and SECURITY files.
