# 后端基础设施生态收口第二阶段 Contract

本文件只保存稳定决策。代码位置与当前计数见 `02-current-baseline.md`；任务顺序见 `03-ticket-map.md`。

## Authority Contracts

### C-RUNTIME-01 — 进程运行时所有权

- `AppRuntime` 是 ResidentHost/OneShot 的进程级资源与关闭协调 Authority。
- backend 领域、application、store、event consumer 不创建 Tokio Runtime，不调用 `block_on`/`run_sync`。
- async-capable 入口直接 `.await`；确实是同步的进程最外层可建立或阻塞 runtime，一旦进入 `AppRuntime`/`AppService` 即保持 async-first。
- `Database` 最终只封装 `SqlitePool`；测试也复用同一 async 构造路径。
- 删除证据：backend 生产代码中 `tokio::runtime::Runtime`、`build_runtime`、`Database::block_on`、`Database::run_sync`、`AppRuntime::block_on`、`AppRuntime::run_sync` 为零。

### C-RUNTIME-02 — ResidentHost / OneShot 对等

- 两种角色共享 bootstrap、请求上下文和 AppService workflow。
- ResidentHost 启动长期 TaskRuntime、dispatcher 与 coordinator；OneShot 不启动 resident dispatcher。
- Engine 成功必须对应真实业务效果；兼容 alias 只能委托 canonical method。
- 角色差异只影响宿主生命周期，不复制持久化或文件系统业务规则。

### C-EVENT-01 — Durable Event 语义

- `domain_event_outbox`、consumer offset、initial position、backfill、per-consumer failure isolation、tenant isolation 和 retention 保持不变。
- 派发循环使用 Tokio task、`Notify`、`CancellationToken`、Tokio timer 与 `TaskTracker`/`JoinSet`。
- `notify` 立即打断 idle/backoff 等待；取消立即打断等待并进入 drain。
- 一个 consumer 失败不移动它的 offset，也不阻断其他 consumer。
- 删除证据：dispatcher 生产实现不含 `std::thread`、`Condvar`、自制 completion condition 或 detach join handle。

### C-SHUTDOWN-01 — 单一绝对 deadline

- 关闭顺序：停止接纳新工作 → 取消并等待 worker/coordinator → 持久化终态 → drain dispatcher → 关闭 pool。
- 所有阶段接收同一绝对 deadline，只使用剩余时长；任何阶段不得重新获得完整 grace period。
- pool close 也受 deadline 限制。
- deadline 到达返回结构化 `ShutdownReport`，明确未完成任务、dispatcher 剩余事件和未完成资源阶段。
- 关闭返回后没有被静默 detach 的 AssetIWeave worker。

### C-PROCESS-01 — 跨平台进程树

- `process-wrap = 10.0.0` 是首选 owner crate，启用 Tokio API；Unix 使用 process group 或 session，Windows 使用 Job Object，drop safety 使用 KillOnDrop。
- 采用前必须用本地 fixture 证明：正常退出、父进程先退出、后代持有管道、忽略终止、超时、显式取消、超大输出和非零退出。
- `which = 8.0.6` 接管标准 PATH 查找；桌面 fallback 只在标准查询失败后执行。
- AssetIWeave 薄策略层保留命令参数、工作目录、环境、总 deadline、stdout/stderr 上限和结构化错误。
- 删除证据：手工 Unix process-group FFI、Windows `taskkill` 进程树实现、取消轮询线程和重复 reader lifecycle 退出生产路径。

### C-SETTINGS-01 — 原始文档 + 类型化切片

- SQLite 保存完整原始 JSON，未知字段 round-trip 不丢失。
- 后端类型化切片仅覆盖实际拥有的 Memory、Conversation、AI runtime、locale、column layout 及执行中发现的后端读取字段。
- `serde(default, rename_all = "camelCase")`、显式 validation 和 schema migration 决定默认值与兼容。
- 业务代码读取类型字段；链式 `Value::get/as_*` 只允许存在于一次性迁移、未知字段合并和边界解析。
- 保存时将已知切片合并回原始文档；重复 canonicalize/migrate 结果不变。

### C-LOG-01 — tracing 生产链路

- `tracing` event/span 是生产观测 Authority；subscriber 负责 filter、format、non-blocking writer 和 rolling。
- task、tenant、operation 与资源身份优先由 span 继承；事件只记录本事件新增字段。
- Engine/MCP stdout 只承载协议；日志进入文件或 stderr。
- 现有 LogSnapshot、打开日志目录和 panic 紧急落盘保持用户可见行为。
- 敏感字段脱敏规则保持：token、secret、password、prompt、environment 与用户绝对路径不进入公开输出。
- 删除证据：手工 `Vec<(&str, String)>` 字段 façade 和逐调用转发函数无生产 consumer。

### C-PATH-01 — 路径分层

- OS/file-system 层使用 `Path`/`PathBuf`，保留非 UTF-8。
- 持久化与 IPC 层可使用 `camino = 1.2.5` 的 UTF-8 类型，转换失败产生明确错误，不使用 lossy 结果参与权限、比较或路径选择。
- `directories = 6.0.0` 接管应用 config/data/cache 目录解析。
- portable anchors、Stored/Resolved/Display 分层、Windows 保留名、case 规则和最长路径匹配保持不变。
- `path-clean`/`dunce` 只有在 fixture 证明能删除现有实现且不改变契约时才加入。

### C-SQLX-01 — Typed Row

- 继续直接使用 SQLx，不引入 ORM。
- 稳定、多列、重复映射的结果使用 `FromRow`/`query_as`；标量、动态投影和确实不稳定的查询保留 `query_scalar`/手工读取。
- 每次迁移保持 null、JSON 解码、排序、tenant filter 和历史数据语义。
- 验收按 repository 外部行为与 `try_get` 删除量，不按新增 row struct 数量。

### C-ERROR-01 — 取消与边界错误

- `Canceled`/`Cancelled` 合并为一个 Rust variant，wire code 继续为 `cancelled`。
- 进程、存储、超时、取消和 extension 错误不通过 `to_string()` 往返后再分类。
- 公共 message/details 保持安全；内部 source 和 tracing 字段保留诊断上下文。
- 本轮触及的基础设施模块编译 warning 归零；全仓 warning 建立单调下降基线。

## Product Invariants

### C-PRODUCT-01 — 保留的领域事实

- `asset_mounts` 是资产与目标 Profile 挂载意图的唯一事实。
- Source 默认只读；元数据进入 SQLite 或 app-owned 路径。
- TaskState、TaskProgress、dedup、conflict、tenant ownership、retention 是产品语义。
- durable outbox/offset 是可靠性语义；portable path anchors 是持久化协议。
- AppService 继续是 Tauri 与 Engine 共享的应用 workflow 边界。

### C-SCOPE-01 — 不进入本轮

- 插件宿主、市场协议、ORM、通用 EventBus、跨进程持久任务队列、UI 改版不进入本轮。
- 不改变 OneShot 执行后退出，不改变直接 symlink 默认部署策略。
- 不批量重写已有路径数据，不复制完整前端 Settings 类型。

## Dependency Ownership

| 依赖 | 锁定候选 | Owner 卡 | 保留条件 |
|---|---:|---|---|
| process-wrap | 10.0.0 | B2-P01 | P01 fixture 全绿且 P02 删除手工进程树 |
| which | 8.0.6 | B2-P01 | 标准 PATH consumer 切换，桌面 fallback 保真 |
| directories | 6.0.0 | B2-F01 | 替换应用目录拼装并通过三平台 fixture |
| camino | 1.2.5 | B2-F01 | 仅用于 UTF-8 契约边界且减少字符串转换 |
| tokio/tokio-util | 现有版本 | B2-R01/B2-R02 | 统一 runtime、task、cancel、notify、tracking |
| tracing 三件套 | 现有锁定版本 | B2-L01 | 生产 consumer 与 rolling/span 接管 |
| serde/thiserror/sqlx | 现有版本 | B2-S01/B2-D01/B2-Q01 | 充分使用现有能力，不加平行框架 |

