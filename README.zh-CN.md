# AgentBroom

本地 Agent 存储，清晰可见。

**当前是只读桌面原型，只检查仓库自带的合成样本。不会发现、扫描或修改真实 Agent 历史，也没有删除、数据库回收或恢复功能。**

![原生 macOS 只读原型](assets/screenshot.png)

已实现：分类占用、分段合并、子 Agent 与独立 fork 区分、受保护记录、搜索筛选、仅本次会话有效的保留选择、不可执行的预览和样本 JSON 报告。原生桌面与内部 CLI 共用 Rust 核心；网页预览使用该核心生成的样本目录。

技术栈为 Tauri 2、Rust、React/TypeScript 与 Vite。首个原生目标是 macOS Apple silicon。

开发需要 Node 24+、稳定 Rust 和 Tauri 的平台依赖。安装 Node 依赖后运行核心测试，生成样本目录，再选择网页或原生开发入口：

```sh
npm install
cargo test -p agentbroom-core -p agentbroom-cli
npm run demo:generate
npm run dev
# 或停止上述服务后运行：
npm run desktop:dev
```

[详细设计](docs/detail-design.md) · [验证结果](docs/verification.md) · [路线图](docs/roadmap.md) · [贡献说明](CONTRIBUTING.md)

欢迎贡献界面、无障碍、文档和合成测试。请勿在公开 issue 中上传真实对话、路径或凭据。MIT 开源；未来真实数据访问及清理能力需要独立验证。
