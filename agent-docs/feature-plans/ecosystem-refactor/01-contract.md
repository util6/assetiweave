# 任务一：执行契约

只加载当前卡列出的 ID。下述为本次施工接口，目标与范围仍以 Issue #22 为准。

## C-BASE — 业务与所有权

SQLite 为持久状态权威；来源仓库默认只读；`asset_mounts` 为挂载意图权威；保持直接指向真实源资产的单层软链接。前端 services 是唯一 IPC 边界；Tauri 和 Engine 共享 AppService；Go 不写数据库或挂载链接。移除通用机制，不移除领域规则、回放投影、批处理去重或审计事实。所有状态测试使用临时数据库和临时源/目标目录。

## C-FRONTEND — 数据、导航和客户端状态

| 状态 | 唯一 owner | 对外方式 |
|---|---|---|
| 后端读取结果、请求/错误/刷新状态 | TanStack Query | services + queryOptions/useQuery/useMutation |
| 页面位置、路由匹配/加载/历史 | TanStack Router | 一棵 code-based route tree、memory history |
| 跨页面共享的纯 UI 状态 | Zustand | selector；不镜像 Query cache |
| 表单草稿/校验 | React Hook Form + Zod，或简单局部 state | 复杂表单用 resolver；即时设置仍是 settings mutation |
| Team seq/replay/unread、流式会话投影 | 既有领域算法 | 不作为“通用 Store”删除 |

`app/query/queryClient.ts` 导出 `createAppQueryClient(): QueryClient`。本地 IPC query/mutation 设置 `networkMode: "always"`；自动重试默认关闭，由明确可重试的只读业务单独开启。取消前端查询不等于取消后端任务。生产应用仅一份 QueryClient，每个测试创建独立实例并清理。

`app/query/QueryScopeProvider.tsx` 管理 `{ tenantId: string; epoch: number }` 的查询作用域，不存业务数据。租户查询 key 至少包含 tenantId；同租户重入的旧事件用 epoch/订阅代际过滤。切租户时取消旧读请求、解绑旧事件并重建作用域；丢弃旧作用域响应，不把新租户数据写入旧 key。不假设当前 invoke 支持 AbortSignal。

`app/query/catalogQueries.ts` 集中定义 Catalog keys/options。全局 settings key 固定 `['app-settings']`，不按租户复制。业务写操作通过 mutation；终态后精确失效受影响 key，批处理只触发一次业务刷新。Query options 不再包一层通用 Repository/QueryManager。TanStack 的 observer 轮询不是全局单例，任务轮询遵守 C-TASK。

Router 仅拥有导航机制；菜单业务、选择目标、任务结果通知移到对应业务 hook。已存在的可达页面均需有迁移映射，不能以兜底页删除功能。

## C-TASK — 后台任务

- Rust TaskRuntime 仍拥有任务 ID、租户、dedupe/conflict、进度、状态转换、保留与关闭策略；tokio-util 仅接管通用取消/追踪原语。
- A-R06 将 `TaskRuntime::shutdown_with_grace(grace)` 改为返回同一 ShutdownReport 的 async 方法；AppRuntime 同名同步入口通过既有 runtime bridge 等待，不新增运行时。
- 使用 `TaskTracker` 时另保留接纳闸门：`close()` 本身不拒绝新任务，drop 不取消任务。先停止接纳，再取消/等待；可继续清理子任务，不能提前报告退出完成。
- ResidentHost 的长任务快速返回快照；OneShotEngine 根据既有命令契约等待结果再退出，不能返回一个退出后无人执行的 task ID。
- 前端每个任务资源只有根部一个事件订阅与 polling owner；页面/全局指标只观察缓存。活动任务默认1秒轮询，终态/空闲降为10秒，确保外部启动且开始事件丢失时也能发现；不在每个观察者启用 interval。查询响应和事件共用领域新旧合并，旧 poll 不覆盖新终态。事件失败、丢失、重连、卸载均有确定行为，旧任务终态不得倒退回 running。
- 成功/失败/取消的终态去重，重复事件不重复 toast 或 catalog refresh；租户切换不能串任务。
- 不把 updater 原生资源句柄放入 Query/Zustand/JSON。进程组清理与终端状态仍属于领域/宿主边界。

## C-SETTINGS — 设置与语言

- SQLite `app_settings` 的 `settings_id = "global"` 是唯一设置权威。`config` 不读写 UI 偏好；localStorage 只做启动缓存/一次迁移输入。
- A-C01 新增 TS `AppLocale = "zh" | "en"`，`AppSettings.locale: AppLocale | null`；null 仅代表尚未初始化。Rust 同名 enum 位于 `backend/app_settings.rs`。settings 文档 schema 3 → 4，不更改 SQLite 表结构或历史 migration。
- 新 Tauri 命令 `initialize_app_locale_if_unset`、Engine 方法 `settings.locale.initialize`、AppService 方法 `initialize_app_locale_if_unset(locale: AppLocale)`，统一返回既有 `AppSettingsFile`。前端 `services/appSettings.ts` 导出 `initializeAppLocaleIfUnset(locale: AppLocale): Promise<AppSettingsFile>`。
- 首次读取已有合法 SQLite locale，直接采用；否则从旧 `assetiweave.locale` 的合法 zh/en、浏览器语言（zh 前缀为 zh，否则 en）、无 navigator 时 zh 的顺序提出候选，原子初始化。返回数据库最终值为准；成功后删除旧 locale key。跨窗口竞争时后来的初始化不覆盖前值。
- 普通 save 缺失/null locale 时原子保留数据库已有显式值；明确 zh/en 保存可改变语言。非法显式值返回 validation_error。旧文档缺失字段规范化为 null，不擅自用默认 zh 抢占一次迁移。
- 设置 reset 保留当前 locale，兼容原来 reset 不管理独立 i18n 的行为。启动缓存不作为回写依据；读取失败不可用默认设置覆盖数据库。
- 同版 settings 增加 `columnLayouts: Record<string, number[]>`，默认 `{}`。key 沿用 ResizableColumns 的 storageKey；数组是 2–16 个有限正权重，非匹配列数用默认布局。A-F13 在后端设置读取成功且缺 key 时导入旧 localStorage 权重，通过正常设置 mutation 保存，成功删除旧 key；持久化只在拖动提交时发生。无 storageKey 的实例保持局部状态。
- 同一窗口全量设置保存按 mutation scope 串行、使用 Query MutationCache 的跨 hook 最新草稿投影；旧失败/成功不回滚较新变更。跨窗口其他设置的完整冲突合并不在本卡新增，但 locale 初始化/保留必须在 SQLite 内原子实现。

## C-CONFIG — 运行环境

仅集中 `ASSETIWEAVE_HOME`、`ASSETIWEAVE_DB_PATH`、`ASSETIWEAVE_LOG_DIR`、`ASSETIWEAVE_POLICY_PATH`。保留 OsString/非 UTF-8 路径；显式启动参数优先于环境，再为原有默认。HOME 不隐式改变数据库既有默认发现位置。保留每个变量的原空值规则：HOME trim 后空则默认；DB_PATH 仅空 OsString 则默认，非空路径保留空格；LOG_DIR/POLICY_PATH 只要存在就保留原值，空 policy 不能变成未配置而绕开既有失败行为。调用级凭据、PATH、Team/Recall prompt 或子进程 env 不收进全局配置。

A-R01 创建 `runtime/config.rs`：

```rust
pub(crate) struct RuntimeConfig {
    pub(crate) home_dir: std::path::PathBuf,
    pub(crate) db_path: std::path::PathBuf,
    pub(crate) log_dir: std::path::PathBuf,
    pub(crate) policy_path: Option<std::path::PathBuf>,
}
pub(crate) struct RuntimeConfigDefaults {
    pub(crate) home_dir: std::path::PathBuf,
    pub(crate) data_dir: std::path::PathBuf,
}
```

`RuntimeConfig::from_env_map(&BTreeMap<String, OsString>, &RuntimeConfigDefaults) -> AppResult<Self>` 为纯解析；`from_environment() -> AppResult<Self>` 为进程边界。A-R02 提供 `runtime_config() -> AppResult<Arc<RuntimeConfig>>` 与 `AppRuntime::config() -> Arc<RuntimeConfig>`；保留现有 bootstrap 签名，显式 db_path 覆盖该快照。具体路径拼接沿 A-R01 的旧行为测试。

## C-ERROR — 错误与日志

既有 `AppError -> WireError { code, message, retryable, details }` 保持公开边界；内部 thiserror 保留 source，anyhow 仅用于启动/工具等无需 typed match 的终端边界。不把业务错误统统转换为 external_error。公开错误与日志字段继续脱敏。

日志使用 tracing/subscriber/appender；Engine stdout 仅有协议帧。文件 writer guard 活到受追踪任务退出之后；stderr 是诊断通道。现有日志浏览器读取旧格式的能力保留或显式实现兼容读取；不要把 `operation_log.rs` 简单转发误认为独立 SQLite 审计表。

## C-HTTP — 标准客户端与阻塞边界

任务一使用复用连接池的 `reqwest::blocking::Client`，不是全后端 async 重写。`backend/http_client.rs` 导出 `shared_http_client() -> AppResult<reqwest::blocking::Client>`；Clone 共享连接池，创建/使用/释放遵循阻塞 API 的运行时约束。调用只能在普通线程或既有 blocking worker；async 安装流程通过局部 `spawn_blocking` 桥接，覆盖 join error，保持 per-Agent lease，不持全局 AppService 锁等待网络。

兼容原 ureq 2 默认行为：不启用环境/系统代理（ClientBuilder.no_proxy），保留 gzip，重定向最多 5 跳；携带 Authorization 的请求重定向后剥离该 header。具体跟随策略由 A-R08 的薄适配与 fixture 测试保证。

保留各调用点既有超时、ETag/304、缓存回退、来源/重定向策略、凭据传播限制、响应上限、流式写文件、checksum、失败清理。取消最迟在当前有界 read/timeout 后被观察，测试给出上界；不宣称 blocking HTTP 具有瞬时中断。外部网络失败不能切换成假成功的 mock。全部 ureq 生产调用迁出后才删除依赖。

## C-STORAGE — 映射、版本与数据

复用 SQLx FromRow/query_as 与 semver，不引入新 ORM。保留 SQL 的租户过滤、事务、约束和 JSON 历史解码。版本库接管数值比较，不扩大既有包 requirement 接受语法，也不恢复曾移除的 Agent core-version 门禁。

普通增量升级使用已有 SQLx migrations；只新增迁移，不改已发布 SQL 字节。本任务只需要 settings JSON 字段升级，不要求数据库重写。若后续决定整库转换，先按一致快照→新库→integrity_check+foreign_key_check+业务保真→停写切换流程审定专卡；不能复制旧 `_sqlx_migrations` 冒充已执行新迁移。数据库备份不等于源资产/扩展包/链接备份。

## C-UI — 成熟控件替换

保留主题 token、键盘可达、批量操作、定位与关键滚动行为。resizable-panels 用选定 4.x 的 Group/Panel/Separator 和实际单位；横向 overflow 仍是容器布局责任。Markdown 用 AST pipeline，保留可信 Diff/Mermaid/数学渲染和可视区策略，不默认启用 raw HTML 或任意外部 React 组件。表单保留业务 normalize，字段错误与生命周期由 RHF/Zod 管理。
