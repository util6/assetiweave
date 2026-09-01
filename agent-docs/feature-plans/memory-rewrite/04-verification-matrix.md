# Memory 重写：验证矩阵

每张执行卡只运行其 Gate。T15 运行完整 Gate，并把命令、退出码和关键断言写入 Handoff。

## 1. Gate

### G0 — 工作区与基线

```bash
git status --short
git diff --check
git log -5 --oneline
```

记录所有 pre-existing changes。基线目标测试失败时先确认失败是否由当前修改引起；不覆盖未确认变更。

### G1 — Rust 目标测试

先运行与当前接缝相邻的最小过滤测试，再扩大：

```bash
cargo test -p assetiweave memory
cargo test -p assetiweave events
cargo test -p assetiweave runtime
cargo test -p assetiweave conversation
```

过滤器无匹配不能作为通过证据；必须观察至少一个目标测试运行。T15 使用 `cargo test --workspace`。

### G2 — Rust 格式与静态边界

```bash
cargo fmt --all -- --check
pnpm check:boundaries
pnpm test:boundaries
```

Memory 新模块进入既有 application/store/runtime 分层；adapter 无持久化规则。

### G3 — Frontend 目标测试

```bash
pnpm typecheck
pnpm test -- frontend/src/services/memory.test.ts
pnpm test -- frontend/src/pages/memory/MemoryPage.test.tsx
pnpm test -- frontend/src/app/backgroundTasks/MemoryTaskProvider.test.tsx
```

按卡片补充新增的 component/navigation test。T15 使用 `pnpm test && pnpm build`。

### G4 — Engine 与 CLI contract

```bash
pnpm cli:contract
cp cli/internal/schema/contract.json /tmp/assetiweave-memory-contract.json
pnpm cli:contract
cmp /tmp/assetiweave-memory-contract.json cli/internal/schema/contract.json
git diff --check -- cli/internal/schema/contract.json
go vet -C cli ./...
go test -C cli -race ./...
pnpm check:surface-matrix
```

Handoff 需说明生成命令和实际 contract diff；连续生成结果必须一致。任何情况下都不手工编辑 contract。

### G5 — Skill 与脚本

```bash
python3 scripts/memory-skill-recall.test.py
rg -n "dream|candidate|evidence|memory\\.recall\\.(preview|run)" builtin-assets/skills/assetiweave-memory cli frontend/src src-tauri/src
```

T14 之后第二条只允许 migration、只读归档和明确标记的历史文档命中；每个命中需分类，不以空 grep 猜测完成。

### G6 — Migration 与数据安全

使用临时 `ASSETIWEAVE_DB_PATH` 或现有 migration test harness：

- 旧版 schema → 最新 schema 成功；
- 已发布 migration 未被修改；
- 新表 tenant 约束与唯一 fingerprint 成立；
- 旧 Memory 只读归档可生成且不参与新查询；
- Memory workspace 只落在临时 app-owned 根目录；
- 文档发布失败时 last-success 保持不变。

具体命令以仓库当前 migration harness 为准，Handoff 记录定位到的测试名。

### G7 — 架构与泄漏审查

对本 Ticket diff 逐项核对：

- mutation 是否全部经过 AppService；
- frontend 是否全部经过 `frontend/src/services`；
- CLI 是否只走 Engine；
- Consumer 是否 durable enqueue 后再 ack；
- 阻塞 I/O/Agent 调用是否脱离全局 app lock；
- TaskRuntime 是否只作活动投影；
- Provider 差异是否停留在 Adapter/Definition；
- 用户输出是否隐藏 raw Markdown、JSON、内部 ID 和 locator；
- 日志/task snapshot 是否隐藏正文、prompt、tool input 与 secrets；
- Card 是否仍为前端表现而非新事实实体。

### G8 — 最终全仓

```bash
git diff --check
pnpm typecheck
pnpm test
pnpm build
cargo fmt --all -- --check
cargo test --workspace
go vet -C cli ./...
go test -C cli -race ./...
pnpm check:boundaries
pnpm test:boundaries
pnpm check:surface-matrix
python3 scripts/memory-skill-recall.test.py
```

如果环境 toolchain 与 `AGENTS.md` 不一致，报告精确版本和未运行项，不用降级结果冒充通过。

## 2. 行为矩阵

| ID | 行为 | 最低证据 | 首次负责 Ticket |
|---|---|---|---|
| V01 | 72h 以 last activity 判断，覆盖跨午夜 | 可控时钟 AppService test | T01 |
| V02 | 注册根→worktree→cwd 解析，symlink 归一 | 路径 fixture | T01 |
| V03 | 不同 worktree 不合并，不按 remote 合并 | 路径 fixture | T01 |
| V04 | commit→Outbox→durable Job→Session Memory | TS01+TS02 | T02 |
| V05 | 同 revision 重放不重复 | restart 后计数断言 | T02 |
| V06 | 完成立即、未完成 idle 30 分钟 | 可控时钟 | T02 |
| V07 | lease/retry/heartbeat/restart/cancel 恢复 | TS02 | T03 |
| V08 | 同项目串行、不同项目可并行 | Fake Agent barrier | T04 |
| V09 | Global 只保留稳定跨项目信号 | snapshot/DTO 断言 | T05 |
| V10 | Context last-success、revision、预算和优先级 | TS01 | T05 |
| V11 | 修订/删除/缺失/迁目录/合同升级失效 | TS01+TS04 | T06 |
| V12 | Markdown/索引可从 SQLite 重建 | 临时 workspace | T06 |
| V13 | 近期项目/时间视图使用同一数据 | service/component test | T07 |
| V14 | 近期 UI 无 raw Markdown/ID/locator/Evidence | DOM 负断言 | T07 |
| V15 | Recent 引用精确打开并高亮现场 | navigation test | T07 |
| V16 | filter+lexical+semantic+rerank 与去重 | search fixture | T08 |
| V17 | Recall tool tenant-scoped 且只读 | Fake ACP 权限测试 | T08 |
| V18 | 单轮输出四字段且非法引用被拒绝 | TS03+schema test | T09 |
| V19 | 多轮追问保留范围，取消/恢复/退出可见 | TS03+UI test | T10 |
| V20 | 实际 Context/Recall 使用更新 usage | AppService test | T11 |
| V21 | 生成/使用开关和排除规则独立 | AppService/settings test | T11 |
| V22 | Engine/CLI/Skill 方法与错误一致 | TS05 | T12 |
| V23 | 只剩「近期」「回忆」，后台任务不锁无关 UI | router/provider test + manual | T13 |
| V24 | 旧归档可读，旧公开表面退出，新路径不依赖旧表 | TS07+surface grep | T14 |

## 3. 人工桌面验收

T13 和 T15 在 Tauri 桌面至少执行一次：

1. 启动应用但不进入 Memory，观察漏单任务自动进入全局任务状态。
2. 任务运行时切换 Conversation、筛选和设置，确认无关操作仍可用。
3. 「近期」切换项目/时间视图并跳转 Session/具体内容。
4. 「回忆」发送模糊线索、连续追问、取消一次、恢复后继续。
5. 关闭应用时有运行中 Memory 任务，确认 close guard 提示。
6. 重启后确认 Job/Recall Session 恢复，last-success Context 仍可读。

人工结果只补充视觉与桌面生命周期，不替代自动 Gate。
