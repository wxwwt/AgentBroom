# Contributing

AgentBroom is at the read-only inventory + cleanup preview + isolated synthetic execution milestone.

1. Keep inspection policy in `crates/core`; desktop and CLI must not duplicate it.
2. Use synthetic fixtures only. Do not commit original conversations, credentials, machine paths or operational logs.
3. Keep native home discovery restricted to the documented CODEX_HOME/HOME adapter. Do not add renderer-supplied roots, shell/fs/SQL plugins or real mutation commands. The production cleanup handler remains unconditionally locked.
4. Cover protected/unknown/segment/fork behavior with meaningful tests. Label native smoke and browser catalog tests accurately.
5. Run default and synthetic-cleanup feature core tests, cross-language fixture expectations, frontend tests and builds before a pull request.
6. Discuss expansion of real-store inspection or writable features first; they need separate version-bound contracts and isolated evidence.

Contributions to the UI, accessibility, documentation and fixture coverage are welcome.
