# AgentBroom

**看清 Codex 本地存储，手动确认后清理。** · [English](README.md)

AgentBroom 是一个早期桌面工具，展示真实 Codex 占用、会话分段、主对话、子 Agent、独立 fork 与共享历史关系，支持中英文切换。

## 使用

打开桌面应用会只读盘点 `CODEX_HOME`（未配置则为 `$HOME/.codex`），不会自动选择或删除任何记录。

对话**默认不上锁**。在对话详情中点击「锁定对话」可禁止删除，点击「解锁对话」恢复。锁定保存在本机，刷新、重启后仍然有效，由后端强制校验。锁住主对话会保护其原生子 Agent；锁住子 Agent 也会阻止包含它的删除计划。Codex 的置顶不会自动变成 AgentBroom 锁定。

进入「清理预览」，可选择一条对话，包括原生索引可识别的子 Agent 和缺父关系记录，检查全部原生子树、文件、保留对话和提示。年代、最近写入、fork/共享历史引用不会自动上锁；独立 fork 不会自动加入删除。链接文件、无法识别的路径、版本/数据库不兼容及计划变化仍可能导致执行失败，会与手动锁定分别说明。

后端验证通过后，查看永久删除说明、勾选声明并输入 `DELETE`。只有你的最终确认才会启动官方原生删除；它也会更新 Codex 索引并清理这些对话的原生附属元数据。执行记录不是备份，AgentBroom 不提供撤销或恢复。

执行前会重新核对原生/观测范围、文件内容与身份、源目录、运行时和数据库；执行后核对删除通知、目标文件/状态与未选记录。部分失败、未知结果和并发变化会明确显示。文件长度和分配块不代表实际回收空间；没有 vacuum、压缩、缓存或项目/worktree 清理。

## 运行边界

初始桌面目标为 macOS Apple silicon，元数据盘点也支持 Linux。原生执行仅支持经过生成数据测试的精确版本 `0.162.0-alpha.17.2` 和 `0.162.1`。macOS 自动发现 ChatGPT/Codex 内的官方程序；开发者可通过 `AGENTBROOM_CODEX_BIN` 显式指定可信官方可执行文件，不接受前端传入目录或程序路径，不升级 Codex，不迁移已有数据库。

盘点解析有界元数据，支持 Zstandard 压缩会话头，并只读核对原生索引与子树关系；扫描不完整时用原生索引恢复可识别的身份。清理预览还会校验选中文件内容，不展示或导出正文。实际删除交由官方存储实现及其写入锁、引用保护；这不是跨进程原子快照。最近写入只提示，由你判断是否需要保留；官方原生接口仍可能因写入或引用等原因拒绝删除。

执行意图和结果原子保存到 `~/.agentbroom/cleanup-receipts/`，不含正文。手动锁按数据源保存到其中私有的 `locks/` 子目录，不修改 Codex 数据。结果未知时即使重启也不会自动重试。同一用户的多个应用实例串行执行；正常退出会等待操作结束，强制结束进程不等于恢复，应先核对执行记录和数据。

浏览器开发入口仍只读，没有写 HTTP 接口或样本回退。原生清理须使用桌面应用。

## 开发与测试

需要 Node.js 24+、稳定版 Rust 和 [Tauri 2 平台依赖](https://v2.tauri.app/start/prerequisites/)。

```sh
npm ci
npm run desktop:dev
npm run desktop:build
npm test
cargo test --locked -p agentbroom-core -p agentbroom-cli -p agentbroom-native
cargo test --locked -p agentbroom-core --features synthetic-cleanup
AGENTBROOM_TEST_CODEX_BIN=/absolute/path/to/official/codex cargo test --locked -p agentbroom-native -- --ignored --test-threads=1
```

所有自动化写入/删除测试只使用后端新生成的数据。专用测试桌面构建 `npm run desktop:test:build` 不读取真实数据；普通应用不启用该构建。该测试构建每次启动重建数据，不提供手动锁定入口；锁定策略由真实 owner 的生成数据测试验证。Python 仅用于独立原生契约测试，完整命令见英文说明。

## 隐私与文档

没有 AgentBroom 账号、云端盘点、遥测或 LLM 调用。原生执行的非源目录状态隔离，使用离线模型提供方。会话头只作为不可信数据解析，不能触发指令，不提供正文导出。CLI 完整盘点输出可能包含隐私，不应公开。

内部设计、调研、审核和验证日志只放本地 `docs/` 并由 Git 忽略。远端仅保留代码、合成测试资产及必要公开使用说明。另见 [贡献说明](CONTRIBUTING.md) 与 [安全说明](SECURITY.md)。

MIT 许可；项目独立于 OpenAI 等 Agent 厂商。
