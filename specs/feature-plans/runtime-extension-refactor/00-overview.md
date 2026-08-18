# SPEC-00:运行时与扩展体系重构总纲(Runtime & Extension Refactor Overview)

- 状态:Draft v3(2026-08-18:吸收首轮审计 15+1 项、v2 复审 8+5 项修订;各分册 P1 关闭并复审通过后方可标记 Approved)
- 适用范围:`src-tauri/src/` Rust 后端、`adapters/`(Tauri + Engine)、`cli/` 契约、`builtin-assets/`
- 基线版本:`main@190bb0e`。**文档中的行号与计数均采集自该基线,执行时 MUST 以符号名搜索定位,不得盲信行号。**
- 本文档集读者:后续执行编码的模型与工程师。执行前必须先读完本篇。

---

## 1. 规范用语(RFC 2119)

- **MUST / MUST NOT**:强制,违反即验收失败。
- **SHOULD / SHOULD NOT**:强烈建议,偏离需在 PR 描述中说明理由。
- **MAY**:可选。

与 `AGENTS.md` 冲突时,以 `AGENTS.md` 为准并停止执行、上报冲突;各分册与本总纲冲突时,以分册(更具体)为准。

## 2. 背景与动机

AssetIWeave 当前存在两类问题:

**A. 运行时问题(最紧迫)**
1. Tauri 侧约 136 处、Engine 侧 dispatch 路径(`adapters/engine/registry.rs` 中 `AppService::open_for_engine()`)每次请求都重新执行完整的 `AppService` 打开链路:`Database::open_initialized`(**新建 tokio Runtime + 连接池 + 跑 migration**)→ `load_local_request_context_sqlx` → `seed_tenant_defaults_sqlx` → Agent runtime `recover_startup` → `migrate_legacy_assignments` → registry `reload`(见 `backend/application/system.rs` 的 `open_with_db_path*` 系列)。
2. `adapters/tauri/` 中 79 处获取全局 `AppState.lock`(`Arc<Mutex<()>>`),将不相关操作串行化。
3. 全仓约 304 处同步 `block_on` 散布在 45 个文件。
4. `AppResult<T> = Result<T, String>`(`backend/dto/types.rs`),无法建立稳定错误契约。

**B. 边界与扩展问题**
1. `backend/store/` 越界执行文件系统副作用与 manifest 校验(`store/conversation_repo.rs` 调 `ensure_official_conversation_adapters` → `conversations/official.rs` 中 `create_dir_all`/`write`/`chmod 0o755`)。
2. 会话卡片 projection 被 store 反向调用(`store/search_index_repo.rs` 调 `conversations::cards::project_persisted_content_card`)。
3. Conversation Adapter 与 Agent Market 是**两套平行生长的扩展系统**,重复实现包身份、版本、信任、安装/升级、运行时启动、注册表、后台任务。
4. 消费者直连具体 provider(`check_opencode_translation_availability`,`backend/card_translation.rs`);目标 App 知识硬编码(`AppKind` 12 变体 + `app_paths.rs`/`defaults.rs` 共 27 处 match);资产分类为硬编码字符串匹配(`scanner/classifier.rs`)。
5. 派生数据(搜索索引、Memory 证据)各自手搓 revision 游标追赶逻辑,无统一变更传播机制。

## 3. 架构总原则(全体 SPEC 的最高约束)

> 1. **可替换的外部实现通过稳定 seam 注入;决定资产身份、挂载意图、安全边界和持久化语义的规则留在 Core。Composition 只选择实现,不改变语义。**
> 2. **AppRuntime 是共享资源宿主,不是全局串行化边界。**
> 3. **Domain event 表达已提交事实,不承担 UI 进度通知;已提交变更的传播只有一条脊柱——outbox 与 revision 游标是同一机制的两个视图,不是两套系统。**
> 4. **Extension Kernel 是"一套共享基础设施 + 多个强类型能力契约",不是一种万能插件 manifest。**

每新增一个功能,用三问裁定归属:替换实现会不会改变已有资产的含义?(会→Core)删掉它已有资产还能否无歧义读取?(不能→Core)它在定义事实还是处理事实?(定义→Core,处理→Capability)。

## 3a. 进程模型(全体分册的共同假设)

本仓库存在两类进程,**每份分册的接口承诺 MUST 声明自己假设哪一类**:

| 角色 | 进程 | 生命周期 | 允许持有 |
|---|---|---|---|
| **ResidentHost(常驻宿主)** | Tauri 桌面进程(以及未来可能的 daemon) | 长生命周期 | 完整 AppRuntime:跨调用 TaskRuntime、outbox dispatcher、注册表快照 |
| **OneShot(一次性进程)** | `assetiweave-engine`:每次 CLI Call 由 `exec.CommandContext` 新起进程,`run_stdio` 读取**单个**请求、应答后退出(`cli/internal/client/engine.go`、`adapters/engine/transport.rs`) | 单请求 | 精简 AppRuntime:MUST NOT 启动 dispatcher;TaskRuntime 仅进程内;长操作前台同步执行 |

推论(MUST):跨调用的任务查询/取消只对 ResidentHost 有意义;领域事件可由任何进程在事务内追加,但只由 ResidentHost 派发;OneShot 进程的读新鲜度由读路径的常设 pull 兜底保证(SPEC-07)。

## 4. 文档地图与执行顺序

| 编号 | 文档 | 阶段 | 前置依赖 |
|---|---|---|---|
| SPEC-01 | `01-app-runtime.md` AppRuntime 资源模型、锁粒度、AppError | P0 | 无 |
| SPEC-02 | `02-boundary-repairs.md` store 边界修复、projection 中立化、依赖守卫 | P0 | 可与 01 并行 |
| SPEC-03 | `03-task-runtime.md` 统一后台 TaskRuntime 与进度通道 | P1 | 01 |
| SPEC-04 | `04-domain-events-outbox.md` 领域事件与 Outbox 单脊柱 | P1 末 | 01、03 |
| SPEC-05 | `05-extension-kernel.md` 扩展内核共享底座 | P1 | 01、03 |
| SPEC-06 | `06-capability-seams.md` ActionId/策略类、Availability、Detector、TargetProvider | P2 | 01;TargetProvider 部分依赖 05 |
| SPEC-07 | `07-event-consumers.md` 首批事件消费者(索引推进、Memory stale) | P2 | 04 |
| SPEC-08 | `08-interface-coverage.md` 覆盖矩阵与命令元数据统一 | P3 | 01 |

执行模型 MUST 按阶段顺序推进;同阶段内可并行的已在表中标注。**MUST NOT 跨阶段抢跑**(例:在 SPEC-01 未验收前实现 SPEC-04 的 dispatcher)。

## 5. 全局非目标(所有分册共同遵守)

以下事项 MUST NOT 在本轮实施,即使看起来"顺手":

1. **拆分 workspace crate。** 违背 `docs/repository-structure.md`(单一 Rust package 决策)。边界靠模块可见性 + SPEC-02 的依赖守卫。
2. **进程内热重载 / 动态加载插件**(dylib/WASM)。运行时扩展性由进程外 Node 适配器承担。
3. **万能 PackageManifest。** 领域 manifest(Conversation / Agent)保持强类型、各自演进。
4. **存储插件化。** SQLite + SQLx migration 是唯一事实源(ADR-001)。
5. **通用权限 DSL、泛型 `topic + serde_json::Value` 事件总线。**
6. **Memory 域的功能性重做。** 用户已决定后续全部重做;本轮只铺轨道(事件、seam、TaskRuntime),SPEC-07 的 stale 标记是轨道验证,不是 Memory 设计。
7. **市场 UI 合并。** 底座统一后 UI 是否合并是独立产品决策。
8. 树形会话模型、轻量 hook 脚本层等来自其他项目调研的候选项,**不在本套 SPEC 范围**,另行立项。
9. **CLI 后台任务与常驻 daemon。** Engine 是一次性进程(§3a),CLI 长操作保持前台同步;跨进程任务与 dispatcher lease 属于未来 daemon RFC,MUST NOT 在本轮顺手实现。

## 6. 术语表

| 术语 | 定义 |
|---|---|
| **AppRuntime** | 进程级长生命周期资源宿主:连接池、上下文/注册表快照、TaskRuntime、Outbox dispatcher、关闭状态。见 SPEC-01。 |
| **AppService** | 轻量请求级门面:`Arc<AppRuntime>` + 本次请求的上下文快照。保留现有 `impl AppService` 方法签名。 |
| **Seam** | 一项可替换能力的三角色组合:Service Definition(接口)+ Provider(实现)+ Consumer(使用方)。 |
| **Extension Kernel** | 从两套市场系统抽取的共享底座六件套 + 信任模型。见 SPEC-05。 |
| **包 kind** | 扩展包的领域类型,当前两种:`conversation-adapter`、`agent`。 |
| **DomainEvent** | 已提交业务事实,同事务写入 outbox,不得静默丢失。 |
| **Progress event** | 任务进度提示,可丢失,轮询兜底。MUST NOT 与 DomainEvent 混用通道。 |
| **Transport event** | Tauri emit / Engine 通知等传输形态,不进入业务语义。 |
| **Revision 脊柱** | `source_revision` 单调序 + outbox + 统一消费者 offset 构成的唯一变更传播机制。 |
| **ActionId** | AI 能力消费点的标识(如 translation、memory-extraction),与安全策略解耦。 |
| **ExecutionPolicyClass** | 受控的执行策略类别(超时、并发、预算),由 Core 定义,未知 action 关闭失败。 |
| **TargetProviderId / TargetProfileDescriptor** | 目标 App 的开放标识与声明式描述文件,逐步取代 `AppKind` 硬编码知识。 |
| **Detector** | 资产类型识别器,带优先级/置信度/稳定序,注册进 scanner 引擎。 |

## 7. 基线事实速查(采集于 main@190bb0e)

执行模型定位代码时按符号搜索;此表用于理解规模与验证"修复后应归零/收敛"的量。

| 事实 | 量 | 定位符号 |
|---|---|---|
| Tauri 层重复打开 | 136 处 | `AppService::open_with_db_path` |
| Engine 层按次打开 | dispatch 内 1 处调用点 | `AppService::open_for_engine`(`adapters/engine/registry.rs`) |
| 全局锁获取 | 79 处 | `state.lock`(`adapters/tauri/`) |
| 同步桥 | 304 处 / 45 文件 | `block_on` |
| Tauri commands | 166 个 | `#[tauri::command]` |
| Engine 契约方法 | 249 个 | `cli/internal/schema/contract.json` |
| store→conversations 引用 | 21 处(混合:类型/投影/真越界) | `backend::conversations` in `backend/store/` |
| 目标 App 硬编码 | `AppKind` 12 变体;`app_paths.rs` 15 处、`defaults.rs` 12 处 match | `enum AppKind` |
| Engine 进程模型 | 每次 CLI 调用一进程、单请求即退出 | `run_stdio`(`adapters/engine/transport.rs`) |
| 契约 canonical | `canonical_method` 全部填充,存在 alias→canonical 结构,Go 侧读取 | `cli/internal/schema/contract.json` |
| Adapter 运行时 | Node/Python/Bash/Executable 四种,带 args 与版本探测 | `ConversationAdapterRuntimeKind`(`conversations/types.rs`) |
| Adapter 信任态 | BuiltIn/Trusted/**Changed**/Untrusted(Changed=受信内容哈希漂移) | `ConversationAdapterTrustState`(`models/conversation.rs`) |
| 会话导入事务 | 按批分事务提交,**每批 bump 一次 revision**,另有收尾事务 | `chunks(CONVERSATION_IMPORT_BATCH_SIZE)`(`store/conversation_repo.rs`) |
| profiles 存储 | `(tenant_id, id, payload)`,TargetProfile 整体 JSON 入 payload | `migrations/202606270002_tenant_scope_catalog.sql` |
| sync deltas | 无 revision 列;`change_kind` CHECK 仅允许 `new/updated` | `migrations/202607290001_conversation_sync_deltas.sql` |
| Memory 证据主键 | `memory_evidence_snapshots(tenant_id, id)`,自带 record_kind/session_id/block_id/content_hash | `migrations/202607230001_memory_domain.sql` |
| Tauri async 现状 | commands.rs 38 个 `async fn`、35 处 `spawn_blocking` | `adapters/tauri/commands.rs` |

## 8. 全局验收与回归门槛

任何分册的 PR 合入前 MUST 全绿:

```bash
cargo fmt --all -- --check && cargo test --workspace
pnpm typecheck && pnpm test && pnpm build
go vet -C cli ./... && go test -C cli -race ./...
pnpm cli:contract   # Engine 契约变更时必跑;生成物不得手改
```

行为兼容总则:
- 166 个 Tauri command 与 249 个 Engine method 的**对外行为与序列化形态 MUST 保持不变**,除非分册明确声明了变更并同步契约。
- 迁移文件放 `src-tauri/migrations/`,命名 `YYYYMMDDNNNN_snake_case.sql`,只增不改旧文件(ADR-001)。
- 测试使用临时 `ASSETIWEAVE_DB_PATH`,MUST NOT 触碰本机真实数据。

## 9. 交付节奏建议

每个分册按"可独立合入的最小步"拆 PR;每步保持上面四组命令全绿。禁止长寿命大分支。Conventional Commits(`refactor(runtime): ...` 等)。

## 10. 修订纪律

- 分册修订时 MUST 做四项检查:(a) **跨文档一致性**——引用了其他分册机制的段落,与被引方逐条对账(v1 审计 #3/#8/#10 均为跨文档矛盾);(b) **进程模型对账**——接口承诺与 §3a 声明的进程角色相容(v1 审计 #2/#4);(c) **存储模型对账**——凡涉及表结构/存储形态的断言,MUST 先读对应 migration 文件(v2 复审 #6/#7/#8 均因凭想象描述 schema 被推翻);(d) **现状断言可采样**——凡声称"现状是 X",MUST 附可复核的采样命令(v2 复审 #3:async command 现状与断言相反)。
- 审计原文见 `review-comments.md` 与 `review-comments-v2.md`,逐条处理记录见 `review-resolutions.md`。
