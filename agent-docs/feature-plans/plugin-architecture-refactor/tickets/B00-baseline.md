# B00：接收任务一验收基线

> **Status: WAITING_FOR_TASK_1**。只做事实核对，使用 `superpowers:executing-plans`。

**Goal:** 证明第二任务从真实已采用成熟依赖、统一契约的基线开始。
**Depends:** Issue #22 / A-G01 的接受记录。
**Read:** 本目录入口、P-INPUT、Issue #22 最终交接；不重读第一任务全部卡。
**Files:** 只读源码/锁文件/测试；输出为 Issue #23 交接评论。

- [ ] 读取 `gh issue view 22 --comments` 与 `gh issue view 23 --comments`；核对 A-G01 每项证据。缺少时停在 WAITING_FOR_TASK_1，不实施插件代码。
- [ ] 运行以下命令；确认当前 revision 包含 accepted revision 及用户后续成果，没有把旧 HEAD 当成新基线。

```sh
git rev-parse HEAD
git status --short
pnpm list --depth 0
cargo tree -p assetiweave --depth 1
rg -n 'PackageIdentity|RegistrySnapshot|LifecycleTaskCoordinator' src-tauri/src/backend/extension_kernel
rg -n 'AssetKind|AssetFormat|SourceScannerKind' src-tauri/src/backend/models src-tauri/src/backend/scanner
rg -n 'AgentExecutionRuntime|team_tools' src-tauri/src/backend
```

- [ ] 核查 `02-work-packages.md` 每条路径与当前生产入口，标出已在任务一合并/删除/迁移的符号。复制地址索引即可，不复制源码形成新文档快照。
- [ ] 运行当前相关模块测试，记录是否匹配真实测试而非 0 tests：

```sh
cargo test -p assetiweave extension_kernel
cargo test -p assetiweave conversations
cargo test -p assetiweave scanner
pnpm conversation-adapters:test
```

- [ ] Issue 交接包含 revision、锁版本、合同版本、数据恢复证据索引、现有插件接缝/限制和 B01 可用输入。仅证据齐全才标 B00 VERIFIED。

**完成条件：** 下一位研究者能在同一已验收基线上复现接口与测试；本卡没有顺便创建新的 PluginManager/Manifest/运行时。
