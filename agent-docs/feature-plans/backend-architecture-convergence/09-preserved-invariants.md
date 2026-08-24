# SPEC-BA-09：已落地能力、架构特例与防回退约束

- 状态：Proposed v1
- 目的：收口未完成部分时，不破坏已经正确落地的架构决策
- 关联：原 `runtime-extension-refactor/02/04/07/08`、0010、0011

## 1. AppService 必须始终绑定 AppRuntime

当前 `AppService` 已不再使用 `runtime: Option<Arc<AppRuntime>>`。该成果 MUST 保持：

- production/test builder 都必须构造 Runtime。
- 测试使用 `TestRuntimeBuilder` 或现有等价 builder，不得恢复 DB-only fallback。
- 业务方法不得出现 `if let Some(runtime) ... else DB query`。
- `AppState` 中不再被消费的重复 manager 字段应删除，而不是重新启用为旁路。

防回退检查：

```bash
rg 'runtime:\s*Option<Arc<AppRuntime>>|if let Some\(runtime\)' \
  src-tauri/src/backend/application
```

## 2. Store 与 Projection 边界

以下原则已经建立，后续任务 MUST NOT 回退：

- Store 负责 SQL/transaction，不物化内置文件、不执行扩展进程。
- Projection 位于中立模块；Store 和 Application 都可以调用纯 projection。
- Store 不得反向导入 Application。
- Store 可接收由 Application/bootstrap 准备好的 typed data。

移动 Runtime bootstrap 时，不能把文件物化重新塞回 Store transaction。

## 3. Conversation Catalog v2 与 InstallSpec

Catalog discovery metadata 与 package installation input 必须继续分离：

```text
Catalog v2 release ─┐
Legacy catalog ─────┼→ ConversationAdapterPackageInstallSpec → installer
Local package ──────┘
```

- Catalog v2 MUST NOT 为了安装而转换成完整 legacy catalog item。
- legacy mapper MAY 保留，但输出必须是版本无关 InstallSpec。
- immutable release identity、checksum、version path、安全解压与 rollback 规则保持现有测试。
- Extension Kernel 收口不得合并 Conversation 和 Agent manifest。

## 4. Domain Event / Outbox 单脊柱

本轮不是重新设计事件系统。必须保持以下不变量：

### 4.1 事件分类

| 类型 | 通道 | 约束 |
|---|---|---|
| Progress | TaskRuntime snapshot + Tauri event | 可丢，轮询补偿；不得进 outbox |
| Domain Event | 同事务 outbox + dispatcher | at-least-once；消费者幂等 |
| Transport Event | adapter surface | 不承载业务事实保证 |

### 4.2 写入与派发

- Domain event 必须与业务状态在同一 DB transaction 写入。
- Application 构造业务语义；持有 transaction 的 Store 执行 append。
- 不得 commit 后补写 outbox。
- `ResidentHost` 是唯一 dispatcher 宿主。
- OneShot 只追加 outbox，不启动 dispatcher。
- dispatcher 重启从 consumer offset catch up。

### 4.3 Consumer

- Consumer ID 全局唯一且稳定。
- `handle` 返回 Ok 前业务效果必须已经完成并提交。
- offset 必须在业务成功后推进。
- 新 consumer 必须声明 `InitialPosition`；需要 backfill/cutoff 而未完成注册时拒绝启动。
- 搜索/Memory 的常设 pull 兜底必须保留，CLI-only 场景不能依赖常驻 dispatcher 才读到新数据。

TaskRuntime 重构不得把 Domain Event 改成普通 progress event，也不得把 dispatcher offset 当成
Task lifecycle state。

## 5. Interface coverage 与 Engine Registry

- Engine registry 继续是 method/risk/confirmation/exposure/params schema 的唯一元数据事实源。
- `canonical_method` 是活跃兼容契约，不得删除。
- `SurfaceMapping` 只表达 canonical capability 与 Tauri command 对应关系，不复制 risk metadata。
- `agent-docs/generated/surface-matrix.md` 必须由脚本生成。
- 新增 Tauri command 时必须更新 mapping/coverage；是否暴露 CLI 由 exposure 明确决定。

不得为了 scan/batch 的 Tauri-only task API 给 OneShot Engine 增加无意义的跨调用
`task.get/cancel` 契约。

## 6. Capabilities 层的职责

`backend/capabilities/` 定义为“跨多个 repository 或主机副作用的稳定领域能力”，不是机械中间层：

```text
workflow orchestration       → AppService/Application
跨 repo + filesystem 规则    → Capability
单表 CRUD                    → Store/Repository
OS process/path mechanics    → HostProcess/HostFilesystem
```

- 简单 CRUD 不必为了形式进入 Capability。
- Capability 不得保存请求级 UI 状态或 Task lifecycle。
- Application 可以组合多个 Capability，并负责用户用例级事务/补偿策略。
- Capability 若改变持久化状态，Tauri 与 Engine 必须通过同一个 AppService workflow 调用。

## 7. HostFilesystem 边界

HostFilesystem 只强制覆盖平台敏感和安全敏感操作：

```text
symlink 创建/删除/识别
portable path normalize/resolve/display
path containment/equality
Windows reserved/case/separator 语义
跨平台目录删除差异
```

普通无特殊平台语义的 `fs::read_to_string`、`fs::write` MAY 直接使用标准库。不得为了形式统一
包裹所有文件调用，也不得绕过 HostFilesystem 执行 mount/symlink/containment。

## 8. Database Backup 特例

`data_backup` MAY 保留独立 Runtime/Pool，前提是路径明确标记为：

```text
OneShot / Shutdown Infrastructure Path
```

理由：checkpoint/VACUUM/关闭期备份可能需要在 Resident AppRuntime drain 或关闭后继续运行。

强制约束：

- 该模式不得复制到普通 Application workflow。
- 独立 pool 只连接明确的同一 DB 路径，并有读写时序测试。
- backup 不得与仍在运行的写任务无协调地复制数据库。
- backup 目录使用 app-owned/configured root 和 HostFilesystem 安全校验。
- 注释与 ADR/设计文档必须明确“基础设施特例，不是推荐构造方式”。

## 9. Source read-only 与安装目录

- 第三方 source directory 默认只读。
- metadata、label、mount intent、observation 写入 SQLite/app-owned 路径。
- Remote Skill/Agent/Adapter 下载必须先进入 staging/app-owned library。
- 不得在外部源码目录中写 lock、marker、cache 或安装结果。
- 默认部署仍是 target directory 到真实 source asset 的单层 symlink，不引入中间 symlink pool。

## 10. 数据库和 migration

- SQLite 继续是 catalog state、tenant、source、asset、profile、mount、conversation、settings、
  backup metadata、operation log 和 remote record 的事实源。
- schema 变化必须新增 migration；不得修改已发布 migration 内容/checksum。
- 测试必须使用临时 `ASSETIWEAVE_DB_PATH`，不得污染本机应用数据库。
- Catalog cache 是可重建外部数据，不得成为 installation DB state 的替代事实源。

## 11. 防回退验收

除各专项测试外，最终必须复跑：

```bash
pnpm check:surface-matrix
cargo test --manifest-path src-tauri/Cargo.toml --lib backend::events
cargo test --manifest-path src-tauri/Cargo.toml --lib backend::projection
cargo test --manifest-path src-tauri/Cargo.toml --lib backend::application::conversation_adapter_catalog_v2
cargo test --manifest-path src-tauri/Cargo.toml --lib backend::data_backup
```

若专项收口使上述既有行为测试需要改变，必须先更新对应原 SPEC/ADR，并在 PR 中解释为什么不是
架构回退。
