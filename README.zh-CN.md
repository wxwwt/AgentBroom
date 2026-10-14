# AgentBroom

Codex 本地存储，清晰可见。

**启动后直接只读盘点真实本地 Codex 数据，已移除演示模式、样本预览与样本报告。** 数据来源为 `CODEX_HOME`（未设置则使用 `$HOME/.codex`），扫描失败时明确报错，不回退到样本。不会写入或删除真实数据，不建立 SQLite 连接；所有真实记录均受保护。

已实现分类占用、主对话/子 Agent/fork 与共享历史关联、保护原因、搜索筛选和分页。保留标记只存在本次会话的内存中，已增加清理候选、主对话/子 Agent 的已观测影响范围、保留项、外部依赖和确认步骤；真实删除始终在后端锁定。桌面与本机网页共用 Rust 只读盘点器。

支持简体中文与英语，右上角「语言 / Language」可即时切换，并保存本应用的语言偏好。首次启动跟随系统的受支持语言，无法匹配时使用英语。页面和保护原因已翻译，对话标题、路径与 ID 保持原样。

技术栈为 Tauri 2、Rust、React/TypeScript 与 Vite。首个原生目标是 macOS Apple silicon。

开发需要 Node 24+、稳定 Rust 和 Tauri 的平台依赖。安装依赖并运行核心测试后，可选择本机网页或原生桌面入口：

```sh
npm install
cargo test --locked -p agentbroom-core -p agentbroom-cli
cargo test --locked -p agentbroom-core --features synthetic-cleanup # 仅后端生成的临时合成数据
npm run test:fixtures  # 仅生成测试资产
npm run dev
# 或停止上述服务后运行：
npm run desktop:dev
```

[贡献说明](CONTRIBUTING.md) · [安全说明](SECURITY.md)

欢迎贡献界面、无障碍、文档和合成测试。请勿在公开 issue 中上传真实对话、路径或凭据。MIT 开源。开发者 synthetic-cleanup feature 已实现合成文件实际删除、JSON 状态更新和双向结果验证；默认桌面构建不包含该删除器，不接收真实目录。原生 Codex 删除、数据库腾空间和中断恢复仍需另外验证。

真实模式只解析有界 session_meta 首行、session_index 标题和桌面置顶 ID。首行可能含指令等字段，未使用字段会被丢弃；不解析对话正文。数据库/WAL/SHM、凭据、配置、图片和其他文件只统计大小。外置 sqlite_home 和远端数据不包含在内。

文件长度和分配块都不等于可回收空间。

公开使用、开发和安全说明保留在仓库根目录。内部设计方案、调研、审核与验证记录仅保存在本地 `docs/`，不由 Git 跟踪或上传。

开发者可用官方 Codex CLI 0.162.1 运行 `npm run test:native`，通过 `AGENTBROOM_TEST_CODEX_BIN` 指定受信任的官方测试可执行文件。测试只在自行生成的临时 HOME/CODEX_HOME、JSONL 和数据库中运行，不接收已有数据目录，也不回退到全局 CLI。Python 3 仅用于测试，不进入桌面运行时。

`agentbroom cleanup-scope <逻辑ID>...` 提供共享 Rust 计划器的本地只读分析，输出可能含私有元数据，不应上传。原生/观测范围一致只是一项证据，真实删除仍锁定；这些合成测试不证明真实库活动状态、所有辅助数据库副作用、原子检查、中断恢复或磁盘回收安全。
