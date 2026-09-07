# 当前错误链基线

## 历史审计记录 (Audit Baseline)

- Revision：`b39932fe124781973fadfbd9f99ddfc5dc1efdcf`
- `AppResult<T>`：`Result<T, AppError>` 标准类型别名。
- `AppError`：已使用 `thiserror::Error`，并保留 `sqlx::Error`、`io::Error` source。
- `WireError`：已有稳定 `code/message/retryable/details` 与公开脱敏测试。

### 历史计数

```text
Result<..., String> signature lines in backend/adapters: 83
map_err(AppError::external) in backend: 984
AppError::External(...to_string()) in backend: 77
map_err(...to_string()) in backend: 301
```

## B2-G03 后真实基线 (Active Baseline)

- Revision：`9387443c3cde0bd042c4031d990aa948513225db` (B2-G03 收口提交)
- 日期：2026-09-07
- 状态：Issue #24 全部 10 张卡已闭环并通过 CI 5/5 jobs 验收。

### 真实计数

```text
Result<..., String> signature lines in backend/adapters: 80 (79 处真实 Result<T, String>，1 处 BTreeMap 假阳性)
map_err(AppError::external) in backend: 850
AppError::External(...to_string()) in backend: 77
map_err(...to_string()) in backend: 273
```

### 重放命令

```bash
rg -n --pcre2 '(?:std::result::)?Result<[^\n]*,\s*String\s*>' src-tauri/src/backend src-tauri/src/adapters --glob '*.rs'
rg -n -F 'map_err(AppError::external)' src-tauri/src/backend --glob '*.rs'
rg -n --pcre2 'AppError::External\([^\n]*\.to_string\(\)' src-tauri/src/backend --glob '*.rs'
rg -n --pcre2 'map_err\([^\n]*\.to_string\(\)' src-tauri/src/backend --glob '*.rs'
```

## 全量 Q1 命中逐条分类表 (80 Hits Breakdown)

| 序号 | 文件路径与行号 | 签名 / 内容 | 分类 | 说明 / 归属 Ticket |
|---|---|---|---|---|
| 1 | `src-tauri/src/backend/projection/conversation_cards.rs:81` | `) -> Result<Option<ConversationCard>, String> {` | `cross-module migrate (ERR-C01)` | Conversation card projection query |
| 2 | `src-tauri/src/backend/projection/conversation_cards.rs:137` | `) -> Result<Vec<ConversationCard>, String> {` | `cross-module migrate (ERR-C01)` | Conversation card projection query |
| 3 | `src-tauri/src/backend/projection/conversation_cards.rs:148` | `) -> Result<Option<ResolvedConversationContentCard>, String> {` | `cross-module migrate (ERR-C01)` | Conversation card projection query |
| 4 | `src-tauri/src/backend/projection/conversation_cards.rs:177` | `) -> Result<Option<ResolvedConversationContentCard>, String> {` | `cross-module migrate (ERR-C01)` | Conversation card projection query |
| 5 | `src-tauri/src/backend/projection/conversation_cards.rs:192` | `) -> Result<Option<ResolvedConversationContentCard>, String> {` | `cross-module migrate (ERR-C01)` | Conversation card projection query |
| 6 | `src-tauri/src/backend/projection/conversation_cards.rs:201` | `) -> Result<(), String> {` | `cross-module migrate (ERR-C01)` | Conversation card projection query |
| 7 | `src-tauri/src/backend/projection/conversation_cards.rs:271` | `) -> Result<bool, String> {` | `cross-module migrate (ERR-C01)` | Conversation card projection query |
| 8 | `src-tauri/src/backend/projection/conversation_cards.rs:350` | `) -> Result<(), String> {` | `cross-module migrate (ERR-C01)` | Conversation card projection query |
| 9 | `src-tauri/src/backend/projection/conversation_cards.rs:425` | `) -> Result<Option<ResolvedConversationContentCard>, String> {` | `cross-module migrate (ERR-C01)` | Conversation card projection query |
| 10 | `src-tauri/src/backend/projection/conversation_cards.rs:565` | `fn validate_schema_version(card: &Map<String, Value>) -> Result<(), String> {` | `private parser retain` | Internal JSON/schema validation |
| 11 | `src-tauri/src/backend/projection/conversation_cards.rs:580` | `fn resolve_card_kind(card: &Map<String, Value>) -> Result<String, String> {` | `private parser retain` | Internal JSON/schema validation |
| 12 | `src-tauri/src/backend/projection/conversation_cards.rs:594` | `fn validate_card_kind(kind: &str) -> Result<(), String> {` | `private parser retain` | Internal JSON/schema validation |
| 13 | `src-tauri/src/backend/projection/conversation_cards.rs:619` | `) -> Result<ConversationCardRenderer, String> {` | `cross-module migrate (ERR-C01)` | Conversation card projection query |
| 14 | `src-tauri/src/backend/projection/conversation_cards.rs:654` | `fn parse_renderer(value: &str) -> Result<ConversationCardRenderer, String> {` | `private parser retain` | Internal JSON/schema validation |
| 15 | `src-tauri/src/backend/projection/conversation_cards.rs:672` | `) -> Result<ConversationCardRenderer, String> {` | `cross-module migrate (ERR-C01)` | Conversation card projection query |
| 16 | `src-tauri/src/adapters/engine/transport.rs:90` | `pub(crate) async fn run_stdio() -> Result<(), String> {` | `outer transport retain` | Stdio runner outer loop |
| 17 | `src-tauri/src/adapters/engine/transport.rs:147` | `fn write_response(response: EngineResponse) -> Result<(), String> {` | `outer transport retain` | Stdio runner outer loop |
| 18 | `src-tauri/src/adapters/engine/transport.rs:1470` | `fn lock(&self) -> Result<TestEnvGuard<'_>, String> {` | `test-only retain` | Test lock helper |
| 19 | `src-tauri/src/backend/projection/conversation_content_nodes.rs:109` | `) -> Result<Vec<ConversationContentNode>, String>` | `cross-module migrate (ERR-C01)` | Conversation card projection query |
| 20 | `src-tauri/src/backend/projection/conversation_content_nodes.rs:111` | `F: FnMut(&ConversationPart) -> Result<Vec<ConversationContentNodeCandidate>, String>,` | `cross-module migrate (ERR-C01)` | Conversation card projection query |
| 21 | `src-tauri/src/adapters/engine/registry.rs:80` | `validate_typed_params: fn(&Value) -> Result<(), String>,` | `outer transport retain` | Engine RPC typed params validator |
| 22 | `src-tauri/src/adapters/engine/registry.rs:4876` | `fn validate_typed_params<T: DeserializeOwned>(params: &Value) -> Result<(), String> {` | `outer transport retain` | Engine RPC typed params validator |
| 23 | `src-tauri/src/backend/capabilities/groups.rs:496` | `) -> Result<(), String> {` | `cross-module migrate` | Groups capability helper |
| 24 | `src-tauri/src/adapters/tauri/app_icon.rs:3` | `pub(crate) fn set_application_icon(app: AppHandle, icon: Vec<u8>) -> Result<(), String> {` | `outer transport retain` | Tauri window icon setup |
| 25 | `src-tauri/src/adapters/tauri/app_icon.rs:27` | `fn set_macos_application_icon(icon: &[u8]) -> Result<(), String> {` | `outer transport retain` | Tauri window icon setup |
| 26 | `src-tauri/src/backend/ai_execution/backends/antigravity.rs:457` | `fn parse_line(line: &[u8]) -> Result<Option<AgyEvent>, String> {` | `private parser retain` | Stream event line parser |
| 27 | `src-tauri/src/backend/agents/registry.rs:74` | `pub(crate) fn publish(&self, definitions: Vec<AgentDefinition>) -> Result<u64, String> {` | `cross-module migrate` | Agent registry publish |
| 28 | `src-tauri/src/backend/models/memory.rs:187` | `pub fn fingerprint(&self) -> Result<String, String> {` | `private parser retain` | Memory fingerprint hash/json |
| 29 | `src-tauri/src/backend/executor/deployment.rs:244` | `fn target_can_be_replaced_with_asset(asset: &Asset, target_path: &Path) -> Result<bool, String> {` | `private parser retain` | Internal asset target check |
| 30 | `src-tauri/src/backend/logs.rs:45` | `pub(crate) fn write_startup_log() -> Result<(), String> {` | `cross-module migrate (ERR-C01)` | LogSnapshot and log management API |
| 31 | `src-tauri/src/backend/logs.rs:73` | `) -> Result<LogSnapshot, String> {` | `cross-module migrate (ERR-C01)` | LogSnapshot and log management API |
| 32 | `src-tauri/src/backend/logs.rs:99` | `pub(crate) fn logs_open_log_directory() -> Result<(), String> {` | `cross-module migrate (ERR-C01)` | LogSnapshot and log management API |
| 33 | `src-tauri/src/backend/logs.rs:125` | `) -> Result<(), String> {` | `cross-module migrate (ERR-C01)` | LogSnapshot and log management API |
| 34 | `src-tauri/src/backend/logs.rs:164` | `fn get_log_dir() -> Result<PathBuf, String> {` | `cross-module migrate (ERR-C01)` | Logs internal/public helper |
| 35 | `src-tauri/src/backend/logs.rs:173` | `fn ensure_default_log_file() -> Result<(), String> {` | `cross-module migrate (ERR-C01)` | LogSnapshot and log management API |
| 36 | `src-tauri/src/backend/logs.rs:195` | `fn list_managed_log_files() -> Result<Vec<PathBuf>, String> {` | `cross-module migrate (ERR-C01)` | LogSnapshot and log management API |
| 37 | `src-tauri/src/backend/logs.rs:215` | `fn resolve_managed_log_file(file_name: Option<&str>) -> Result<PathBuf, String> {` | `cross-module migrate (ERR-C01)` | LogSnapshot and log management API |
| 38 | `src-tauri/src/backend/logs.rs:234` | `fn read_log_tail_lines(log_file: &Path, line_limit: usize) -> Result<String, String> {` | `cross-module migrate (ERR-C01)` | Logs internal/public helper |
| 39 | `src-tauri/src/backend/logs.rs:289` | `fn build_managed_log_file(path: &Path) -> Result<ManagedLogFile, String> {` | `cross-module migrate (ERR-C01)` | LogSnapshot and log management API |
| 40 | `src-tauri/src/backend/logs.rs:305` | `fn build_available_log_files(paths: Vec<PathBuf>) -> Result<Vec<ManagedLogFile>, String> {` | `cross-module migrate (ERR-C01)` | LogSnapshot and log management API |
| 41 | `src-tauri/src/backend/logs.rs:332` | `fn write_fatal_panic_log(paths: &[PathBuf], message: &str) -> Result<(), String> {` | `cross-module migrate (ERR-C01)` | Logs internal/public helper |
| 42 | `src-tauri/src/backend/logs.rs:397` | `fn open_directory(path: &Path) -> Result<(), String> {` | `cross-module migrate (ERR-C01)` | Logs internal/public helper |
| 43 | `src-tauri/src/backend/store/conversation_repo.rs:8878` | `) -> AppResult<std::collections::BTreeMap<String, String>> {` | `false positive` | Not Result<T, String> |
| 44 | `src-tauri/src/backend/agent_market/distribution.rs:42` | `) -> Result<Vec<DistributionCandidate>, String> {` | `cross-module migrate (ERR-A01)` | Agent Market repository/runtime/cache/migration/distribution |
| 45 | `src-tauri/src/backend/agent_market/cache.rs:51` | `pub(crate) fn read(&self) -> Result<Option<(Catalog, Option<String>)>, String> {` | `cross-module migrate (ERR-A01)` | Agent Market repository/runtime/cache/migration/distribution |
| 46 | `src-tauri/src/backend/agent_market/cache.rs:81` | `pub(crate) fn write_atomic(&self, bytes: &[u8], etag: Option<&str>) -> Result<Catalog, String> {` | `cross-module migrate (ERR-A01)` | Agent Market repository/runtime/cache/migration/distribution |
| 47 | `src-tauri/src/backend/agent_market/cache.rs:143` | `pub(crate) fn best_available() -> Result<CatalogService, String> {` | `cross-module migrate (ERR-A01)` | Agent Market repository/runtime/cache/migration/distribution |
| 48 | `src-tauri/src/backend/agent_market/cache.rs:156` | `pub(crate) fn refresh_default() -> Result<CatalogRefreshOutcome, String> {` | `cross-module migrate (ERR-A01)` | Agent Market repository/runtime/cache/migration/distribution |
| 49 | `src-tauri/src/backend/agent_market/cache.rs:160` | `pub(crate) fn refresh_from_url(url: &str) -> Result<CatalogRefreshOutcome, String> {` | `cross-module migrate (ERR-A01)` | Agent Market repository/runtime/cache/migration/distribution |
| 50 | `src-tauri/src/backend/agent_market/cache.rs:169` | `) -> Result<CatalogRefreshOutcome, String> {` | `cross-module migrate (ERR-A01)` | Agent Market repository/runtime/cache/migration/distribution |
| 51 | `src-tauri/src/backend/agent_market/migration.rs:17` | `) -> Result<Vec<String>, String> {` | `cross-module migrate (ERR-A01)` | Agent Market repository/runtime/cache/migration/distribution |
| 52 | `src-tauri/src/backend/agent_market/repository.rs:20` | `pub(crate) async fn get(&self, agent_id: &str) -> Result<Option<AgentInstallation>, String> {` | `cross-module migrate (ERR-A01)` | Agent Market repository/runtime/cache/migration/distribution |
| 53 | `src-tauri/src/backend/agent_market/repository.rs:29` | `pub(crate) async fn list(&self) -> Result<Vec<AgentInstallation>, String> {` | `cross-module migrate (ERR-A01)` | Agent Market repository/runtime/cache/migration/distribution |
| 54 | `src-tauri/src/backend/agent_market/repository.rs:37` | `pub(crate) async fn list_registry_candidates(&self) -> Result<Vec<AgentInstallation>, String> {` | `cross-module migrate (ERR-A01)` | Agent Market repository/runtime/cache/migration/distribution |
| 55 | `src-tauri/src/backend/agent_market/repository.rs:46` | `) -> Result<(), String> {` | `cross-module migrate (ERR-A01)` | Agent Market repository/runtime/cache/migration/distribution |
| 56 | `src-tauri/src/backend/agent_market/repository.rs:77` | `) -> Result<(), String> {` | `cross-module migrate (ERR-A01)` | Agent Market repository/runtime/cache/migration/distribution |
| 57 | `src-tauri/src/backend/agent_market/repository.rs:93` | `) -> Result<(), String> {` | `cross-module migrate (ERR-A01)` | Agent Market repository/runtime/cache/migration/distribution |
| 58 | `src-tauri/src/backend/agent_market/repository.rs:103` | `pub(crate) async fn mark_health_unchecked(&self, updated_at: &str) -> Result<u64, String> {` | `cross-module migrate (ERR-A01)` | Agent Market repository/runtime/cache/migration/distribution |
| 59 | `src-tauri/src/backend/agent_market/repository.rs:119` | `) -> Result<(), String> {` | `cross-module migrate (ERR-A01)` | Agent Market repository/runtime/cache/migration/distribution |
| 60 | `src-tauri/src/backend/agent_market/repository.rs:133` | `pub(crate) async fn delete(&self, agent_id: &str) -> Result<(), String> {` | `cross-module migrate (ERR-A01)` | Agent Market repository/runtime/cache/migration/distribution |
| 61 | `src-tauri/src/backend/agent_market/repository.rs:143` | `fn row_to_installation(row: sqlx::sqlite::SqliteRow) -> Result<AgentInstallation, String> {` | `cross-module migrate (ERR-A01)` | SQL row codec |
| 62 | `src-tauri/src/backend/agent_market/repository.rs:305` | `fn parse_distribution(value: &str) -> Result<DistributionType, String> {` | `private parser retain` | Internal enum parser |
| 63 | `src-tauri/src/backend/agent_market/runtime.rs:46` | `pub(crate) fn from_installation(installation: &AgentInstallation) -> Result<Self, String> {` | `cross-module migrate (ERR-A01)` | Agent Market repository/runtime/cache/migration/distribution |
| 64 | `src-tauri/src/backend/agent_market/runtime.rs:142` | `pub(crate) async fn reload(&self) -> Result<u64, String> {` | `cross-module migrate (ERR-A01)` | Agent Market repository/runtime/cache/migration/distribution |
| 65 | `src-tauri/src/backend/agent_market/runtime.rs:178` | `pub(crate) async fn recover_startup(&self, runtime_root: &Path) -> Result<Vec<String>, String> {` | `cross-module migrate (ERR-A01)` | Agent Market repository/runtime/cache/migration/distribution |
| 66 | `src-tauri/src/backend/agent_market/runtime.rs:251` | `pub(crate) async fn prepare_startup_health_refresh(&self) -> Result<u64, String> {` | `cross-module migrate (ERR-A01)` | Agent Market repository/runtime/cache/migration/distribution |
| 67 | `src-tauri/src/backend/agent_market/runtime.rs:262` | `) -> Result<AgentHealthRefreshSummary, String> {` | `cross-module migrate (ERR-A01)` | Agent Market repository/runtime/cache/migration/distribution |
| 68 | `src-tauri/src/backend/agent_market/runtime.rs:303` | `) -> Result<AgentConnectionResult, String> {` | `cross-module migrate (ERR-A01)` | Agent Market repository/runtime/cache/migration/distribution |
| 69 | `src-tauri/src/backend/agent_market/runtime.rs:312` | `) -> Result<AgentModelsResult, String> {` | `cross-module migrate (ERR-A01)` | Agent Market repository/runtime/cache/migration/distribution |
| 70 | `src-tauri/src/backend/agent_market/runtime.rs:372` | `async fn probe_native_health(&self, agent_id: &str) -> Result<AgentConnectionResult, String> {` | `cross-module migrate (ERR-A01)` | Health probe |
| 71 | `src-tauri/src/backend/agent_market/runtime.rs:482` | `) -> Result<AgentModelsResult, String> {` | `cross-module migrate (ERR-A01)` | Agent Market repository/runtime/cache/migration/distribution |
| 72 | `src-tauri/src/backend/agent_market/runtime.rs:488` | `async fn probe_acp_health(&self, agent_id: &str) -> Result<AgentModelsResult, String> {` | `cross-module migrate (ERR-A01)` | Health probe |
| 73 | `src-tauri/src/backend/agent_market/runtime.rs:748` | `) -> Result<AgentDefinition, String> {` | `cross-module migrate (ERR-A01)` | Agent Market repository/runtime/cache/migration/distribution |
| 74 | `src-tauri/src/backend/store/menu_repo.rs:13` | `) -> Result<(), String> {` | `cross-module migrate (ERR-D01)` | MenuRepo DB operations |
| 75 | `src-tauri/src/backend/store/menu_repo.rs:21` | `) -> Result<(), String> {` | `cross-module migrate (ERR-D01)` | MenuRepo DB operations |
| 76 | `src-tauri/src/backend/store/menu_repo.rs:78` | `) -> Result<(), String> {` | `cross-module migrate (ERR-D01)` | MenuRepo DB operations |
| 77 | `src-tauri/src/backend/store/menu_repo.rs:188` | `) -> Result<NavigationModel, String> {` | `cross-module migrate (ERR-D01)` | MenuRepo DB operations |
| 78 | `src-tauri/src/backend/agent_market/types.rs:682` | `) -> Result<crate::backend::extension_kernel::PackageIdentity, String> {` | `cross-module migrate (ERR-A01)` | Agent Market model validation/manifest |
| 79 | `src-tauri/src/backend/agent_market/types.rs:726` | `pub(crate) fn package_manifest(&self) -> Result<AgentPackageManifest, String> {` | `cross-module migrate (ERR-A01)` | Agent Market model validation/manifest |
| 80 | `src-tauri/src/backend/agent_market/types.rs:981` | `pub(crate) fn validate_basic(&self) -> Result<(), String> {` | `cross-module migrate (ERR-A01)` | Agent Market model validation/manifest |

## 统计汇总 (Classification Summary)

| 分类类别 | 命中数 | 目标卡片 / 处理方案 |
|---|---|---|
| `cross-module migrate (ERR-D01)` | 4 | ERR-D01 收口 MenuRepo 的 SQLite/JSON 操作错误 |
| `cross-module migrate (ERR-A01)` | 32 | ERR-A01 引入 AgentMarketError 统一收口 repository/runtime/cache/migration/distribution |
| `cross-module migrate (ERR-C01)` | 26 | ERR-C01 引入 LogSnapshotError / ProjectionError 收口日志和卡片投影 |
| `cross-module migrate (other)` | 2 | 后续 ERR-E01 / 专项收口 groups capability 与 agent registry |
| `private parser retain` | 8 | 模块私有单文件解析/哈希，不穿透公开契约，保留精确签名 |
| `outer transport retain` | 6 | 最外层 stdio/Tauri window 宿主交互边界，序列化为输出协议，保留精确签名 |
| `test-only retain` | 1 | 测试环境互斥锁助手，保留精确签名 |
| `false positive` | 1 | `conversation_repo.rs:8878` 为 `AppResult<BTreeMap<String, String>>`，非错误类型 |
| **总计** | **80** | **全部命中分类覆盖率 100%** |
