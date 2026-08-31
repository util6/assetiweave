# Team 多 Agent：验证矩阵

## Gate 分层

### G0 — 每轮基线

```bash
git status --short
git diff --check
```

记录已有未提交文件并排除其所有权。当前仓库任何用户改动都不得被覆盖、还原或顺手整理。

### G1 — Rust 目标测试

按当前 Ticket 新增测试名称过滤，随后运行受影响模块：

```bash
cargo test --manifest-path src-tauri/Cargo.toml <new_test_name>
cargo test --manifest-path src-tauri/Cargo.toml team
cargo test --manifest-path src-tauri/Cargo.toml ai_execution
```

不存在的 filter 不算通过；交付证据必须显示实际运行的测试数量。

### G2 — 前端目标测试

```bash
pnpm test -- frontend/src/services/team.test.ts
pnpm test -- frontend/src/pages/team
pnpm typecheck
```

文件尚未创建时只运行当前 Ticket 已存在的目标。服务测试必须断言 Tauri command 边界；组件测试必须通过 mock service 驱动用户行为。

### G3 — Engine/CLI contract

公开 Engine 方法或 DTO 变化时：

```bash
pnpm cli:contract
go vet -C cli ./...
go test -C cli -race ./...
```

生成契约由命令更新。交付前检查生成差异只包含当前 Ticket 的方法和 DTO。

### G4 — 架构与生成物

```bash
pnpm check:boundaries
pnpm test:boundaries
pnpm check:surface-matrix
pnpm artifacts:guard
git diff --check
```

公开 surface 未变化时可省略 surface matrix，但交付中必须写明理由。

### G5 — 全仓门禁

```bash
pnpm typecheck
pnpm test
pnpm build
cargo fmt --all -- --check
cargo test --workspace
go vet -C cli ./...
go test -C cli -race ./...
```

T14 追加：

```bash
pnpm cli:test:e2e
pnpm agent-catalog:check
```

## 行为矩阵

| ID | 行为 | 主证据接缝 | 首次负责 Ticket |
|---|---|---|---|
| V01 | 一个 Team 仅一个 Leader，至少一个 Teammate | TS01 | T01 |
| V02 | 活跃 TeamRun 冻结 roster | TS01 | T01/T04 |
| V03 | 同 key resume，不同 execution ID | TS01+TS02 | T02 |
| V04 | Persistent 保留 Session；OneShot 删除 | TS02 | T02 |
| V05 | Leader 重开后正文来自 Provider replay | TS01+TS02+TS05 | T03 |
| V06 | Team 流程对 Conversation 零写入 | TS01 | T03/T14 |
| V07 | awaiting-review 前零 Teammate 执行 | TS01 | T04 |
| V08 | 用户确认映射覆盖推荐映射 | TS01 | T05 |
| V09 | mailbox/task board 同一 SQLite Authority | TS01 | T06 |
| V10 | Team MCP 跨成员写入被拒绝 | TS01+工具测试 | T07 |
| V11 | CLI fallback 不直连 SQLite | TS04 | T08 |
| V12 | 失败、取消、断线不移交 | TS01 | T09 |
| V13 | replay 不产生工具或 mailbox 副作用 | TS01+TS02 | T10 |
| V14 | partial restore 保留原任务 owner | TS01 | T10 |
| V15 | outbox 重复投递不重复 dispatch | TS06 | T11 |
| V16 | 页面切换和漏事件后进度可恢复 | TS05+TS06 | T12 |
| V17 | Leader 需要 Resume+Replay；Teammate 只需 Resume | TS03+TS05 | T13 |
| V18 | 日志与 snapshot 无正文、凭据、resume token | negative assertions | 每张卡/T14 |

## Red test 标准

一个合格的首个失败测试必须满足：

1. 在未实现目标行为的当前基线上失败。
2. 通过公开 AppService、Runtime、Engine 或 UI service 接缝观察结果。
3. 失败原因指向当前 acceptance，而不是缺少 mock、测试编译错误或硬编码调用次数。
4. 使用临时数据库和本地 fake；不依赖真实 Provider、用户目录数据或网络。

## 交付证据

每条命令记录：命令、退出码、实际测试数量或关键摘要。不得只写“tests pass”。手工 smoke 记录 Provider 版本、使用的 capability、结果和 cleanup，不记录 prompt 或正文。

