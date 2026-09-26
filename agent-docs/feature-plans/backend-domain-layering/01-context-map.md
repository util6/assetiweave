# 后端领域分层 Context Map

> 状态：Issue #44 已完成。四大领域分层（application, domain, infrastructure, store）已全面收口，11 个旧根清空物理删除，500 行质量门禁已建立。

## 1. 分层对应关系

| 熟悉的分层 | AssetIWeave 目录 | 职责 | 禁止事项 |
| --- | --- | --- | --- |
| Controller | `src-tauri/src/adapters/` | Tauri、Engine、stdio/MCP 等协议适配；参数与 wire error 转换 | 不写 SQL、不直接操作挂载、不复制业务校验 |
| Service / Use Case | `src-tauri/src/backend/application/` | AppService 门面、用例编排、事务边界、任务与副作用协调 | 不直接构造进程、不成为模型或工具垃圾桶 |
| Domain | `src-tauri/src/backend/domain/` | 实体、值对象、Policy、状态转换、执行计划、领域错误 | 不依赖 SQLx、Tauri、Runtime、文件系统、网络或进程 |
| Mapper / Repository | `src-tauri/src/backend/store/` | SQLx 查询、事务内持久化、Codec、历史数据兼容 | 不依赖 Application、Adapter 或 UI DTO |
| Infrastructure | `src-tauri/src/backend/infrastructure/` | Runtime、Tasks、Events 投递、Extensions、Host、AgentExecution、HTTP、日志、Backup | Runtime/Tasks/Events/Extensions 不依赖 Application |

稳定调用方向：

```text
Adapters -> Application(AppService) -> Domain
                         |              ^
                         +-> Store -----+
                         +-> Infrastructure
```

Application 是编排层，因此可以调用 Store 与 Infrastructure；Store 与 Infrastructure 可以使用 Domain 类型，但不得反向调用 Application。

## 2. 上下文所有权

| Context | Application | Domain | Store | Infrastructure |
| --- | --- | --- | --- | --- |
| Catalog | `application/catalog` | `domain/catalog` | `store/catalog` | `infrastructure/filesystem`、`infrastructure/http_client` |
| Mounting | `application/mounting` | `domain/mounting` | `store/mounting` | `infrastructure/filesystem`、`infrastructure/tasks` |
| Conversations | `application/conversations` | `domain/conversations` | `store/conversations` | `infrastructure/extensions`、`infrastructure/tasks` |
| Memory | `application/memory` | `domain/memory` | `store/memory` | `infrastructure/tasks` |
| Agents | `application/agents` | `domain/agents` | 需要持久化的系统级配置暂归 `store/system` | `infrastructure/extensions`、`infrastructure/agent_execution` |
| System / Operations | `application/system` | `domain/tenant` | `store/system` | `infrastructure/runtime`、`infrastructure/events`、`infrastructure/backup` |

## 3. 统一语言与边界

- **Catalog**：Asset、Source、Skill、Skill Library、Built-in Skill、Remote Skill、发现、导入与扫描。Skill 是 Catalog 中的资产类型，不是独立上下文。
- **Mounting**：Profile、Asset Group、Asset Mount、Deployment Strategy、Target Catalog、Planner、Executor 与物理挂载检查。
- **Conversations**：Conversation Session、Turn、Part、Question、Content Node、Conversation Source、Conversation Adapter、Package、同步、搜索与 Usage。
- **Memory**：Recent Work、Session/Project/Global Memory、Recall、Generation、Consolidation 与 Evidence。Recent Work 不建立独立上下文。
- **Agents**：Agent Market、Agent Package、Agent Catalog、Agent Session 与执行能力的业务接入；协议进程与通用执行机制属于 Infrastructure。
- **System / Operations**：Tenant、Settings、Navigation、Diagnostics 与公共 Task View；不得承接无法归类的业务代码。

## 4. 横切模块所有权

### Events

| 内容 | Owner |
| --- | --- |
| Conversation Source committed 等业务事实 | 对应 `domain/<context>` |
| Durable Outbox、Consumer Offset、Retention SQL | `store/system` |
| Dispatcher、重试、投递、关闭、Consumer 注册 | `infrastructure/events` |
| 具体消费后的业务动作 | 消费方 `application/<context>` |

事件链必须继续使用 durable outbox，不引入内存 EventBus。

### Errors

| 错误 | Owner |
| --- | --- |
| 业务不变量失败 | `domain/<context>` |
| SQL/Codec | `store` |
| 进程、网络、路径、扩展、任务 | 对应 `infrastructure/<module>` |
| 稳定应用错误映射 | `application` 的 `AppError/AppResult` |
| `code/message/retryable/details` | Adapter/DTO 的 `WireError` |

迁移期间 `backend/error` 已拆分并物理删除；业务与系统错误按层归位，统一由 `application::error` 对外映射。

### Extension Kernel

`infrastructure/extensions` 是 Agent 与 Conversation Adapter 共用的深模块，拥有 Package Identity、Compatibility、Trust Gate、Launcher、Probe、Registry Snapshot 与 Lifecycle Coordination。领域 Manifest 与 Package 解释仍归各自上下文；本重构不修改插件协议或 ABI。

## 5. 稳定入口

- Tauri：`adapters/tauri -> application::AppService`
- Engine/CLI：`adapters/engine -> application::AppService`
- AppService：仅持有进程级依赖快照并编排用例，不吸收所有业务实现。
- SQLite：所有 schema 变更只通过 `src-tauri/migrations/`；包含 Recent Memory 状态错误重试列兼容迁移（`202609260001_recent_memory_state_error_retryable.sql`）。
- Mount：`asset_mounts` 继续保存挂载意图；目标目录使用单层直接软链接。

## 6. 迁移完成判据与验收结果

1. `application` 只保留 Catalog、Mounting、Conversations、Memory、Agents、System 六个业务命名空间和 AppService 门面。（已满足）
2. `backend/mod.rs` 的生产模块声明只包括 `application`、`domain`、`infrastructure`、`store`；测试辅助模块仅允许作为 `#[cfg(test)] test_support`。（已满足）
3. `capabilities`、顶层 `runtime`、顶层 `events`、顶层 `extension_kernel`、顶层 `data_backup` 兼容门面删除。（已满足）
4. Domain、Store、Infrastructure 的反向依赖由 `scripts/check-module-boundaries.sh` 零容忍阻止（当前全部为 0）。（已满足）
5. 跨层依赖显式化与跨层 glob 清理：生产代码严禁跨层 glob 导入（`use crate::backend::{domain,store,infrastructure}::...::*` 经架构守卫严格封禁）；禁止 infrastructure/conversations 替 Domain/Store 桥接转导出；Domain 严禁动态读取环境变量；Application 内部私有子模块与测试文件局部 `use super::*` 保持受控；反向依赖由架构守卫强制为 0。（已满足）
6. Engine contract、Tauri/CLI surface、SQLite 行为、错误 wire shape 与用户行为保持兼容。（已满足）
7. `backend/` 不再包含与四层并列的混合业务模块；全部 411 个生产 Rust 实现文件规模门禁（<= 500 行）实测通过；CI/脚本默认严格执行四根检查；Rust 工作区全量测试（1071 项）实测通过；`cargo check` 实测记录基线为 642 条存量告警（本轮未新增告警，澄清非全部清零）；前端门禁因存在用户独立未提交改动本轮未重跑。（已满足）
