# Contributing

AgentBroom is at the synthetic, read-only prototype milestone.

1. Keep inspection policy in `crates/core`; desktop and CLI must not duplicate it.
2. Use synthetic fixtures only. Do not commit original conversations, credentials, machine paths or operational logs.
3. Do not add home discovery, custom roots, shell/fs/SQL plugins or mutation commands to this prototype.
4. Cover protected/unknown/segment/fork behavior with meaningful tests. Label native smoke and browser catalog tests accurately.
5. Run core tests, frontend tests and builds before a pull request.
6. Discuss real-store or writable features first; they need separate version-bound contracts and isolated evidence.

Contributions to the UI, accessibility, documentation and fixture coverage are welcome.
