# SPEC-01:AppRuntime 资源模型、锁粒度与结构化错误(P0)

- 状态:Draft v3(v1 审计 #1/#4/#10;v2 复审 #3/#4/#11 修订)
- 进程模型假设(SPEC-00 §3a):本篇同时覆盖 ResidentHost 与 OneShot 两种角色,差异见 §4。
- 前置:无(本文档是全程的地基)
- 交付物:`backend/runtime/` 新模块、`AppService` 改造、`AppError`、两个 surface 的接入、锁模型替换
- 关联 ADR:实施前 MUST 先落一份 `docs/decisions/ADR-008-app-runtime-and-lock-granularity.md`,内容即本文档第 4、5、6 节的裁决,格式仿 ADR-001。

---

## 1. 目标

1. 进程内只存在一个长生命周期 `AppRuntime`,持有连接池与各注册表快照;消除按命令重建 `Database`(含 tokio Runtime)、重复 seed、重复 registry 恢复。
2. 全局互斥锁 `AppState.lock: Arc<Mutex<()>>` 退役,替换为按冲突域收窄的并发控制。
3. `AppResult<T> = Result<T, String>` 演进为结构化 `AppError`,同时保持对外序列化兼容。
4. Tauri 与 Engine 两个 surface 同步接入,MUST NOT 只改一边。

### 非目标

- 不改变任何 command/method 的对外行为与参数;不拆 `application/` 的领域服务(那是后续独立工作);不引入 async trait 重写浪潮——`block_on` 收敛而非消灭。

## 2. 现状(证据)

- `backend/application/system.rs` — `open_with_db_path` 每次执行:`agent_runtime_manager(&db_path)`(内部 `Database::open_initialized` + `load_local_request_context_sqlx` + `recover_startup`)→ 再次 `Database::open_initialized` → `load_local_request_context_sqlx` → `seed_tenant_defaults_sqlx` → `recover_startup` → `migrate_legacy_assignments` → `reload`。
- `backend/store/database.rs` — `struct Database { pool: SqlitePool, runtime: Runtime }`:**每次 open 新建一个 tokio Runtime**。
- `adapters/app_state.rs` — `AppState` 已持有共享的 `agent_runtime_manager`/`agent_runtime`(`lib.rs` 启动时创建一次),但 commands 仍普遍走 `AppService::open_with_db_path(state.db_path.clone())`(136 处),丢弃了共享实例。
- `adapters/engine/registry.rs` — dispatch 内 `AppService::open_for_engine()`,Engine 每个请求同样全量重开。
- 79 处 `state.lock` 将不相关写操作串行化;`AGENTS.md` 明确要求长任务不得持全局锁。

## 3. 目标结构

新模块 `src-tauri/src/backend/runtime/`(`mod.rs`, `app_runtime.rs`, `error.rs`, `locks.rs`):

```rust
/// 进程级共享资源宿主。整个进程(桌面或 Engine)只创建一次。
pub(crate) struct AppRuntime {
    db_path: PathBuf,
    /// 唯一的 SQLite 连接池。所有常规请求复用;长任务可另开独立连接(见 §5)。
    pool: SqlitePool,
    /// 唯一的 tokio Runtime(多线程)。
    tokio: tokio::runtime::Runtime,
    /// 请求上下文快照(tenant 等),读多写少,原子整体替换。
    context: arc_swap::ArcSwap<RequestContextSnapshot>,
    /// Agent 注册表所有者(现有 AgentRuntimeManager,内部已是原子 Registry 持有者)。
    agent_runtime_manager: Arc<AgentRuntimeManager>,
    agent_runtime: Arc<dyn AgentExecutionRuntime>,
    /// 按冲突域的锁表(见 §5)。
    locks: RuntimeLocks,
    /// SPEC-03 接入点(P1 前为空壳字段占位,不实现)。
    // task_runtime: TaskRuntime,
    // outbox_dispatcher: OutboxDispatcher,   // SPEC-04
    /// 关闭协调:取消令牌 + drain 状态。
    shutdown: ShutdownState,
}

pub(crate) struct RequestContextSnapshot {
    pub tenant: Tenant,
    pub generation: u64,   // tenant 切换时 +1,用于检测跨切换的陈旧请求
}
```

`AppService` 改造(保持名字与全部现有方法签名):

```rust
pub(crate) struct AppService {
    runtime: Arc<AppRuntime>,
    /// 本次请求绑定的上下文快照(load 一次,不再随用随查)。
    context: Arc<RequestContextSnapshot>,
}
impl AppService {
    /// 唯一的新构造方式。O(1),无 IO。
    pub(crate) fn from_runtime(rt: &Arc<AppRuntime>) -> Self { ... }
}
```

依赖注入约定:`arc-swap` 加入 `Cargo.toml`(若未有)。`Database` 类型保留用于测试与迁移工具,但生产路径 MUST NOT 再经 `Database::open_initialized` 创建第二个 Runtime。

## 4. 启动序列(一次性 bootstrap)

`AppRuntime::bootstrap(db_path, role: RuntimeRole) -> Result<Arc<AppRuntime>, AppError>`(`RuntimeRole::{ResidentHost, OneShot}`,SPEC-00 §3a),顺序固定:

1. 建 tokio Runtime(多线程,线程名前缀 `aiw-rt`)。
2. 打开池并跑 SQLx migration(复用现有 `open_migrated_pool` 逻辑)。
3. `load_local_request_context_sqlx` → 构造 snapshot(generation=0)。
4. 通用 tenant defaults seed(修订,v2 复审 #4):现 `seed_tenant_defaults_sqlx`(`store/database.rs`)**内部直接调用 adapter seed**——MUST 先拆分:通用 seed 不含 adapter;其全部调用点(bootstrap、新建 tenant、system reset 等,基线 4 处)同步改为"通用 seed → 步骤 5"两段式(拆分规范见 SPEC-02 §3 步骤 2)。
5. Adapter 物化 → 校验 → adapter seed:await `application::bootstrap::materialize_and_seed_builtin_adapters(&pool, tenant_id)`(SPEC-02 修订版签名:async、仅收 `&SqlitePool`;内部顺序 = 物化文件 → 校验 → 调用只接收已备数据的 store adapter seed)。
6. `AgentRuntimeManager::recover_startup` → `migrate_legacy_assignments` → `reload`,发布 registry 快照。
7. 安装关闭钩子:注册 `ShutdownState`,与现有 `AppState.allow_exit`/`shutdown_sync_done` 标志衔接。

角色差异(修订,审计 #2/#4):`ResidentHost` 执行全部步骤并启动跨调用 TaskRuntime 与 outbox dispatcher(SPEC-04);`OneShot` 执行步骤 1-7 但 **MUST NOT 启动 dispatcher**,TaskRuntime 仅进程内。一次性 Engine 每次调用付一次 bootstrap 成本——与现状(每 dispatch 全量重开)等价,不构成回退;其优化属 daemon RFC(SPEC-00 非目标 9)。

两个 surface 的接线:
- **Tauri**:`lib.rs` 启动时 `AppRuntime::bootstrap(db_path, ResidentHost)`,`AppState` 增加 `runtime: Arc<AppRuntime>` 字段;`AppState.db_path` 保留(路径展示用),`AppState.lock` 进入退役流程(§5)。
- **Engine**:`bin/assetiweave-engine.rs` 进程启动时 `bootstrap(db_path, OneShot)`;`open_for_engine()` 改为 `AppService::from_runtime`。Engine 与 Tauri MUST 走同一个 `bootstrap` 函数,禁止两份初始化逻辑。

## 5. 并发模型:按冲突域收窄

`AppState.lock` 的 79 个获取点逐类替换,替换表:

| 冲突域 | 机制 | 说明 |
|---|---|---|
| SQLite 写一致性 | 事务 | 已有 sqlx 事务;不需要进程内互斥 |
| 同一目标路径/Profile 的 mount/unmount/批量执行 | **keyed lock,键 = DeploymentPlan 触及的路径与 profile 集合** | 从 plan 输出取键集合,按字典序排序后依次获取,防死锁。单点实现:`RuntimeLocks::acquire_plan_scope(keys: BTreeSet<String>) -> PlanScopeGuard` |
| 同一 source 的扫描 | 任务去重(SPEC-03 dedup key),而非互斥 | 重复请求返回进行中任务快照 |
| Registry reload(agent/adapter) | 锁外构建完整新快照 → `ArcSwap::store` 原子替换 | MUST NOT 持有任何写锁执行 probe/网络/文件 IO |
| Tenant 切换 / db 路径切换 | 窄状态锁 + `generation` 自增 | 切换期间新请求取到新快照;跨代请求由 generation 比对拒绝 |
| 长任务(扫描/备份/同步/导入导出) | 独立连接(`SqliteConnectOptions` 新连接或独立小池)+ cancellation token + TaskRuntime 登记 | 遵守 AGENTS.md:不持全局锁做阻塞 IO |
| 普通查询、导航、设置读取 | **无业务锁** | 直接走池 |

**反模式(MUST NOT)**:
1. `RwLock`/`Mutex` guard 跨 `.await` 持有;
2. 持 registry 写锁执行 probe/网络/文件操作;
3. 长任务持 tenant/global guard;
4. 把现有 `state.lock` 原样搬进 `AppRuntime`(哪怕改个名字)。

## 6. async 姿态(block_on 收敛)

裁决:**不做 async 端到端重写**。基线现状(修订,v2 复审 #3):`adapters/tauri/commands.rs` 有 **38 个 `async fn` command、35 处 `tauri::async_runtime::spawn_blocking`**(agent_market.rs 另有 12 处)——"命令全同步"不成立,规则据此制定:

1. `AppRuntime` 提供唯一同步桥 `pub(crate) fn block_on<F: Future>(&self, f: F) -> F::Output`(委托内部 Runtime)。**MUST NOT 在任何 async 上下文(Tauri executor、tokio worker 线程)内调用**——嵌套 Runtime 会 panic 并阻塞 executor。
2. 现有 async command 与其 `spawn_blocking` 边界**保持不变**:把 `Arc<AppRuntime>` clone 进 blocking 闭包,在闭包内 `AppService::from_runtime` 构造服务;闭包运行于阻塞线程,可安全使用 `AppRuntime::block_on`。
3. 同步签名的 command 与 Engine 的同步入口,内部经 `service.runtime.block_on`。
4. `backend/` 内部代码 MUST NOT 新增 `block_on` 调用;既有 304 处随触碰的文件顺带改为经 `AppRuntime::block_on` 或上提为 async fn,不强制一次清零。
5. 新增 CI 检查(并入 SPEC-02 守卫脚本):`backend/store`、`backend/conversations`、`backend/capabilities` 中 `block_on` 出现次数只许减不许增(记录基线数进脚本)。
6. 回归测试:至少覆盖一个 async Tauri command 路径的运行时测试,证明改造后无嵌套 Runtime panic(`runtime::tests::async_command_path_does_not_nest_runtime`)。

## 7. 结构化错误 AppError

`backend/runtime/error.rs`:

```rust
#[derive(Debug, thiserror::Error)]
pub(crate) enum AppError {
    #[error("{0}")] Validation(String),          // 用户可修复的输入问题
    #[error("{0}")] NotFound(String),
    #[error("{0}")] Conflict(String),            // 并发/状态冲突,可重试
    #[error("{0}")] Io(#[from] std::io::Error),
    #[error("{0}")] Db(#[from] sqlx::Error),
    #[error("{0}")] Extension(String),           // SPEC-05 细化为 ExtensionError
    #[error("{0}")] Canceled(String),
    /// 过渡变体:包装尚未分类的历史字符串错误。目标是逐步归零。
    #[error("{0}")] Legacy(String),
}
pub(crate) type AppResult<T> = Result<T, AppError>;
```

迁移策略(修订,审计 #1:backend 现存约 427 处直接 `Err(String)`,`From` 只在 `?` 处生效,因此 MUST NOT 先翻转全局别名。分四步):
1. **并存引入**:落 `AppError` 与双向 `From`;`dto/types.rs` 的 `AppResult<T> = Result<T, String>` **保持不动**。新代码与被重构模块内部改用 `Result<T, AppError>`,在与旧签名交界处经 `From<AppError> for String` 收敛。此步全仓编译绿是真实的,因为没有任何现有签名被改。
2. **边界 wire 契约**:定义 `WireError { code: &'static str, message: String }`;Tauri command 返回错误与 Engine `EngineError`/`DispatchFailure` 增加错误码字段,前端与 CLI 先行容忍未知字段;契约再生走 `pnpm cli:contract`(AGENTS.md 契约条款)。
3. **逐模块迁移**:按模块把内部返回类型换成 `AppError`,直接 `Err(String)` 构造点改为具体变体或 `Legacy`;CI 基线(SPEC-02 脚本)记录 `Err(format!`/`Err("` 与 `Legacy(` 计数只减不增。
4. **最后翻转别名**:当剩余直接构造点降至可一次清扫的规模,翻转 `AppResult` 指向 `AppError`,以编译错误驱动清完尾部。

## 8. 实施步骤(有序)

1. 落 ADR-008(本文档 §4-§6 摘要)。
2. 新建 `backend/runtime/`,实现 `AppRuntime::bootstrap` 与 `from_runtime`(先不动任何调用方);单测:bootstrap 在临时 `ASSETIWEAVE_DB_PATH` 上幂等、可并发读。
3. `AppError` 第 1 步落地(全仓编译绿)。
4. Tauri 接线:`lib.rs` bootstrap → `AppState.runtime`;写一个适配函数 `fn service(state: &AppState) -> AppService`,**机械替换** 136 处 `AppService::open_with_db_path(state.db_path.clone())?` → `service(state)`。分多个 PR、按文件分批,每批跑全量测试。
5. Engine 接线:`open_for_engine` 改从进程级 `OnceLock<Arc<AppRuntime>>` 取。
6. 删除 `open_with_db_path` 的调用后,将其降级为 `#[cfg(test)]`(测试仍可用独立 DB 全量打开);`open_with_db_path_and_manager`/`_and_runtime` 同理。
7. 锁替换:按 §5 表逐域替换 79 处 `state.lock`;每域一个 PR;`RuntimeLocks` 先实现 plan-scope keyed lock 与 tenant generation 两种。
8. 全部替换完成后删除 `AppState.lock` 字段(编译器兜底找残留)。
9. CI 基线脚本更新(与 SPEC-02 共用)。

## 9. 验收标准

- `grep -rn "open_with_db_path" src-tauri/src/adapters/ | wc -l` 结果为 **0**(测试代码除外)。
- `grep -rn "state\.lock" src-tauri/src/adapters/tauri/ | wc -l` 结果为 **0**,且 `AppState` 无 `lock` 字段。
- 计数断言(修订,v2 复审 #11;bootstrap 直接复用 `open_migrated_pool`,不经 `Database`):`AppRuntime::bootstrap` 1 次;池创建 1 次;migration/seed/recovery 各 1 次;生产路径 `Database::open_initialized` **0 次**——分别以测试探针证明。
- `bootstrap(OneShot)` 不启动 dispatcher(测试断言:OneShot 角色下无派发任务注册)。
- 新增回归测试:
  - `runtime::tests::bootstrap_runs_seed_and_recovery_exactly_once`(以计数探针/表内标记验证);
  - `runtime::tests::plan_scope_lock_orders_keys_and_blocks_conflicts`;
  - `runtime::tests::tenant_generation_rejects_stale_requests`;
  - 并发烟雾测试:同时发起 8 个只读命令 + 1 个批量 mount,断言只读命令不被阻塞(时间上界断言放宽到 CI 抖动可容忍)。
- `pnpm cli:contract` 产物 diff 仅包含声明过的错误码字段变更。
- 桌面应用手工冒烟:启动、扫描、挂载、会话同步、Agent 市场刷新各走一遍,行为与基线一致。

## 10. 风险与回滚

- 风险:某些 command 隐式依赖"每次重开=隐式刷新上下文"。对策:替换批次小步走,发现依赖后改为显式 `runtime.refresh_context()`。
- 风险:独立长任务连接与主池的 SQLite busy 竞争。对策:统一 `busy_timeout` 配置进 `AppRuntime`,长任务写路径走小事务。
- 回滚:`from_runtime` 与 `open_with_db_path` 在过渡期并存,任一批次可单独 revert。
