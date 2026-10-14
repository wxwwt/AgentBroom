# AgentBroom

Codex 本地存储，清晰可见。

**启动后直接只读盘点真实本地 Codex 数据，已移除演示模式、样本预览与样本报告。** 数据来源为 `CODEX_HOME`（未设置则使用 `$HOME/.codex`），扫描失败时明确报错，不回退到样本。不会写入或删除真实数据，不建立 SQLite 连接；所有真实记录均受保护。

已实现分类占用、主对话/子 Agent/fork 与共享历史关联、保护原因、搜索筛选和分页。保留标记只存在本次会话的内存中，没有删除或清理功能。桌面与本机网页共用 Rust 只读盘点器。

支持简体中文与英语，右上角「语言 / Language」可即时切换，并保存本应用的语言偏好。首次启动跟随系统的受支持语言，无法匹配时使用英语。页面和保护原因已翻译，对话标题、路径与 ID 保持原样。

技术栈为 Tauri 2、Rust、React/TypeScript 与 Vite。首个原生目标是 macOS Apple silicon。

开发需要 Node 24+、稳定 Rust 和 Tauri 的平台依赖。安装依赖并运行核心测试后，可选择本机网页或原生桌面入口：

```sh
npm install
cargo test -p agentbroom-core -p agentbroom-cli
npm run test:fixtures  # 仅生成测试资产
npm run dev
# 或停止上述服务后运行：
npm run desktop:dev
```

[启动与验证](docs/live-use.md) · [详细设计](docs/detail-design.md) · [验证结果](docs/verification.md) · [路线图](docs/roadmap.md) · [贡献说明](CONTRIBUTING.md)

欢迎贡献界面、无障碍、文档和合成测试。请勿在公开 issue 中上传真实对话、路径或凭据。MIT 开源；未来清理能力需要独立设计和验证。

真实模式只解析有界 session_meta 首行、session_index 标题和桌面置顶 ID。首行可能含指令等字段，未使用字段会被丢弃；不解析对话正文。数据库/WAL/SHM、凭据、配置、图片和其他文件只统计大小。外置 sqlite_home 和远端数据不包含在内。

[只读适配器设计与限制](docs/codex-readonly-design.md)。文件长度和分配块都不等于可回收空间。

[语言切换说明](docs/i18n.md) · [额外语言的公开搜索调研](docs/language-search-research.md)
