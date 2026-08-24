# 数据、API、前端与 CLI 集成 (Data, API, Frontend and CLI Integration)

| 字段 | 值 |
|---|---|
| 状态 | Proposed |
| 持久化 | SQLite 当前安装 + app settings capability assignment + JSON catalog cache |
| API 策略 | Additive migration；Tauri/Engine 共享 AppService |

## 1. SQLite 设计

### 1.1 单表原则

MVP 新增一个业务表 `agent_installations`。不新增：

- Agent catalog release/version history 表；
- installation version history/rollback 表；
- content hash/trust 表；
- 用户自定义 package/source 表。

Catalog 使用 bundled/cache JSON；active task 使用现有进程内 task registry；capability assignment 继续使用 app settings。

### 1.2 规范性 DDL

实际 migration 编号按仓库最新 migration 递增生成，不手工复用下列占位编号：

```sql
CREATE TABLE agent_installations (
  tenant_id TEXT NOT NULL,
  agent_id TEXT NOT NULL,
  installation_id TEXT NOT NULL,
  display_name TEXT NOT NULL,
  catalog_item_version TEXT NOT NULL,
  agent_version TEXT NOT NULL,
  protocol TEXT NOT NULL CHECK (protocol IN ('acp', 'native')),
  distribution_id TEXT NOT NULL,
  distribution_type TEXT NOT NULL
    CHECK (distribution_type IN ('system', 'binary', 'npx', 'uvx')),
  ownership TEXT NOT NULL CHECK (ownership IN ('system', 'managed')),
  install_dir TEXT,
  resolved_program TEXT NOT NULL,
  args_json TEXT NOT NULL,
  definition_json TEXT NOT NULL,
  integrity_json TEXT,
  source_registry TEXT NOT NULL,
  catalog_version TEXT NOT NULL,
  enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
  installation_status TEXT NOT NULL
    CHECK (installation_status IN ('ready', 'incompatible', 'broken')),
  runtime_status TEXT NOT NULL
    CHECK (runtime_status IN ('unchecked', 'ready', 'runtime_missing', 'entry_missing', 'failed')),
  runtime_error_code TEXT,
  runtime_error_message TEXT,
  runtime_checked_at TEXT,
  protocol_status TEXT NOT NULL
    CHECK (protocol_status IN ('unchecked', 'ready', 'auth_required', 'failed', 'unsupported')),
  protocol_error_code TEXT,
  protocol_error_message TEXT,
  protocol_checked_at TEXT,
  model_status TEXT
    CHECK (model_status IS NULL OR model_status IN ('unchecked', 'ready', 'failed', 'unsupported')),
  model_error_code TEXT,
  model_checked_at TEXT,
  installed_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  PRIMARY KEY (tenant_id, agent_id),
  FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE,
  CHECK (
    (ownership = 'system' AND install_dir IS NULL)
    OR (ownership = 'managed' AND install_dir IS NOT NULL)
  )
);

CREATE INDEX idx_agent_installations_ready
  ON agent_installations (tenant_id, enabled, installation_status, protocol_status);

CREATE UNIQUE INDEX idx_agent_installations_identity
  ON agent_installations (tenant_id, installation_id);
```

说明：

- 公共 API 的 `installation_status=disabled` 是 `enabled=0` 的派生视图；DB 不冗余保存 disabled。
- `catalog_item_version` 是生成启动定义的精选 item 版本；`agent_version` 是实际解析版本。managed 通常一致，System 可是满足支持范围的其他检测版本。
- `installation_id` 由本地生成，标识一次激活实例；同版本重装也必须变化，并用于 active execution/lifecycle 冲突检查。
- `execution_ready` 是列组合 + capability 的派生值，不持久化，避免漂移。
- `definition_json` 是 resolved local execution definition，不含 download/package 信息或 raw secret。
- `integrity_json` 仅存分发生态证据，不存 recursive directory hash。
- error message 必须是脱敏摘要；原始 package manager stderr 不入库。

### 1.3 Repository 方法

`src-tauri/src/backend/agent_market/repository.rs` 的最小接口：

```text
get(tenant_id, agent_id)
list(tenant_id)
list_registry_candidates(tenant_id)
upsert_active(installation)
update_enabled(tenant_id, agent_id, enabled)
update_health(tenant_id, agent_id, health)
delete(tenant_id, agent_id)
```

所有 SQL 通过项目 `Database/sqlx` 模式执行。Application/CLI 不拼 SQL。

## 2. 核心 DTO

### 2.1 Market View

```text
AgentMarketListRequest {
  query?
  protocol?
  installed_only?
  include_incompatible? = true
}

AgentMarketItemView {
  id
  display_name
  description
  protocol
  version
  core_compatible
  capabilities
  verification
  distributions: DistributionCandidateView[]
  recommended_distribution_id?
  installed?: AgentInstallationSummary
  update_available
}
```

### 2.2 Preview

```text
AgentInstallPreviewRequest {
  agent_id
  catalog_version?
  agent_version?
  distribution_id?
  action                 # install/update/reinstall
}

AgentInstallPreview {
  agent_id
  action
  selected_distribution
  alternatives[]
  current_installation?
  target_version
  ownership
  target_path?
  download_size?
  runtime_requirements[]
  conflicts[]
  warnings[]
  confirmation_required
}
```

### 2.3 Start Request

```text
AgentInstallStartRequest {
  agent_id
  catalog_version
  agent_version
  distribution_id
  preview_token
}
```

`preview_token` 是后端根据非敏感计划字段计算的短期 identity，用于发现 UI 预览后的 catalog/环境变化；它不是安全 token。后端仍重新校验所有输入。

卸载 request：

```text
AgentUninstallStartRequest {
  agent_id
  clear_capability_assignments: string[]
  preview_token
}
```

前端永远不提交 `resolved_program`、args、env、install_dir、download URL 或 hash。

### 2.4 Installed/Health View

```text
AgentInstallationView {
  agent_id
  display_name
  version
  protocol
  distribution_id
  distribution_type
  ownership
  display_install_path?
  enabled
  installed
  installation_status     # installing/ready/disabled/incompatible/broken
  runtime_status
  protocol_status
  connected
  execution_ready
  health_stale
  selected_model_id?
  model_status?
  update_available
  operation?              # installing/updating/reinstalling/uninstalling
  last_checked_at?
  error?: AgentMarketErrorView
  warnings[]
}
```

路径返回 UI 前使用项目 portable display path 规则归一化；用户目录显示为 `~`。

## 3. Engine 方法

### 3.1 新增方法

当前 `cli/internal/client/engine.go` 的每次 `Call` 都启动一个 one-shot `assetiweave-engine` 并等待退出。所以下表是 **Engine contract**，生命周期写操作使用同步 `*.run`；Tauri/Desktop 的后台 start/get/list/cancel 见第 4 节，二者共享同一 lifecycle service，不共享传输期 task registry。

| Engine method | 类型 | 行为 |
|---|---|---|
| `agent.market.list` | read | bundled/cache catalog + installed summary |
| `agent.market.inspect` | read/probe | 单 item 分发候选和 Runtime preflight |
| `agent.market.refresh.run` | synchronous workflow | 在当前 Engine 请求内刷新 curated catalog并返回终态 |
| `agent.install.preview` | read/probe | 生成确定安装计划和 preview token |
| `agent.install.run` | synchronous workflow | 在当前 Engine 请求内首装并返回终态 |
| `agent.update.preview` | read/probe | 更新计划 |
| `agent.update.run` | synchronous workflow | 手动更新并返回终态 |
| `agent.reinstall.run` | synchronous workflow | 重装当前固定版本并返回终态 |
| `agent.uninstall.preview` | read | 引用、活跃 execution、ownership 和删除范围 |
| `agent.uninstall.run` | synchronous workflow | 卸载/解除绑定并返回终态 |
| `agent.enable` | write | 启用并 reload Registry |
| `agent.disable` | write | 停用并 reload Registry |
| `agent.installed.list` | read | 当前租户安装 |
| `agent.installed.get` | read | 单 installation 详情 |
| `agent.runtime.check` | bounded probe | 单 Agent 入口/Runtime 检查 |
| `agent.connection.check` | bounded probe | 单 Agent 协议检查 |
| `agent.models.list` | bounded probe | 已安装 Agent model discovery |

风险和确认元数据按 Engine registry 现有 contract 机制声明：

- list/get/check/preview：read 或 bounded process risk。
- install/update/reinstall/refresh run：network + app-owned file write，明确 preview/confirmation；Engine context/SIGINT cancellation 必须收敛 lifecycle。
- uninstall managed：destructive app-owned file delete，必须 preview/confirmation。
- System bind/unbind：不删除外部文件。

### 3.2 兼容方法

保留：

- `agent.catalog.list` / `list_agent_catalog`
- `agent.connection.check` / `check_agent_connection`
- `agent.models.list` / `list_agent_models`

`agent.catalog.list` 迁移映射：

- 返回 Market item 的兼容视图，并新增 `installed`、`execution_ready` 字段。
- legacy `command/args/availability_command` 只对已安装 resolved runtime 填充；未安装时为空。
- 这些 legacy 字段仅供展示兼容，禁止新调用者据此启动进程。

`AgentConnectionResult` additive 字段：

```text
installed
installation_status
runtime_status
protocol_status
connected
execution_ready
health_stale
```

迁移后不再产生 `connection_method=cli_fallback`。

## 4. Tauri Commands

建议命名映射：

| Tauri command | AppService |
|---|---|
| `list_agent_market` | `agent.market.list` workflow |
| `inspect_agent_market_item` | inspect |
| `refresh_agent_market` | background start |
| `preview_agent_installation` | install/update preview |
| `start_agent_installation` | install start |
| `start_agent_update` | update start |
| `start_agent_reinstallation` | reinstall start |
| `preview_agent_uninstall` | uninstall preview |
| `start_agent_uninstall` | uninstall start |
| `get_agent_lifecycle_task` | get task |
| `list_agent_lifecycle_tasks` | list tasks |
| `cancel_agent_lifecycle_task` | cancel task |
| `list_installed_agents` | installed list |
| `check_agent_runtime` | runtime check |

Tauri command 只负责：参数反序列化、取 `AppState`、快速 begin task、spawn async/blocking work、emit event。不得在 command 中实现 installer 或 SQL。

Tauri method 不需要与 Engine method 同名：`start_agent_installation` 快速返回 task snapshot，而 `agent.install.run` 阻塞当前 one-shot Engine 请求直到同一 workflow 终态。这是 transport adapter 差异，不是两套业务实现。

## 5. Application Service 边界

新增 `application/agent_market.rs`，由 `AppService` 和显式依赖注入的 `AgentAppService` 共享业务方法。目标依赖：

```text
AgentMarketAppService {
  app_service / db context
  catalog_service
  lifecycle_service
  runtime_manager
}
```

现有 `AgentAppService` 只有 `_service + agent_runtime`，实施时应扩展或重命名为能访问 DB/market 的显式服务；不得重新调用全局 `shared_agent_execution_runtime(db_path)`。

## 6. Go CLI 设计

建议命令：

```text
assetiweave agent market list [--protocol acp|native] [--json]
assetiweave agent market inspect AGENT_ID [--json]
assetiweave agent market refresh [--json]
assetiweave agent installed list [--json]
assetiweave agent install AGENT_ID [--distribution ID] [--yes]
assetiweave agent update AGENT_ID [--distribution ID] [--yes]
assetiweave agent reinstall AGENT_ID [--yes]
assetiweave agent disable AGENT_ID
assetiweave agent enable AGENT_ID
assetiweave agent uninstall AGENT_ID [--clear-assignment CAPABILITY] [--yes]
assetiweave agent check AGENT_ID [--protocol] [--json]
```

CLI 规则：

1. `install/update/uninstall` 默认先调用 preview 并输出精确计划。
2. 非交互模式未传 `--yes` 时不开始有副作用任务。
3. 有副作用命令调用单次 `*.run` Engine method 并等待终态；不得返回 task ID 后另起 Engine 进程轮询。
4. 不调用 npm/uv/curl、不解析 catalog、不读写 SQLite。
5. `--json` 输出 DTO，不混入 progress 文本；progress 写 stderr 或关闭。
6. Ctrl-C 取消当前 Engine context；Engine 必须在退出前向 lifecycle cancellation token 传播并完成 bounded process/staging cleanup。

## 7. Frontend Service

`frontend/src/services/agentRuntime.ts` 是唯一 Tauri boundary。需要：

- 为所有新 DTO 建立 TypeScript 类型和必要的运行时 schema validation。
- 暴露 market/preview/task/installed/health API。
- 保留现有方法作为兼容 wrapper，迁移完成后标记 deprecated。
- 组件、hooks、settings schema 禁止直接 `invoke(...)`。

若 DTO 数量增长，应拆为 `services/agents/types.ts` + `services/agentRuntime.ts`，但仍保持 service boundary。

## 8. Frontend 信息架构

### 8.1 Agent Settings Panel

```text
Agent Settings
├── Capability Assignments
│   ├── Card Translation
│   ├── Memory
│   └── Prompt Optimization
└── Agent Management
    ├── Market
    └── Installed
```

### 8.2 Market

支持：

- query、protocol、installed/update filters；
- dense card/list；
- 协议 badge 与 distribution badges 分开；
- tested/experimental、core compatibility、Runtime requirement；
- install/inspect/update CTA；
- 无网络时显示使用 bundled/cache，而不是空状态。

点击安装先打开 preview dialog，不直接开始下载。

### 8.3 Installed

每行至少：

- icon/name/protocol；
- version + update available；
- ownership + distribution；
- installation/runtime/protocol 三段状态；
- selected model；
- check、update、reinstall、disable/enable、uninstall。

状态文案示例：

- `已安装 · ACP 已连接`
- `已安装 · ACP 连接失败`
- `已安装 · 需要认证`
- `安装损坏 · 建议重装`
- `已停用`

禁止把 `CLI 已检测` 显示为 `已连接`。

### 8.4 Task Provider

新增 `AgentLifecycleTaskProvider`，复用现有 Provider 模式：

1. mount 时 list retained tasks。
2. listen `agent-lifecycle-task-updated`。
3. 活跃任务 polling fallback。
4. merge 依据 task ID + updated_at；terminal 不回退。
5. 发起页面显示 item 级进度。
6. 全局区域显示离开页面后的活跃任务。
7. 只禁用同 Agent 冲突 action。

不要把 Agent lifecycle task 塞入 `AiExecutionTaskProvider`；二者取消、结果和保留语义不同。

## 9. Capability Assignment

### 9.1 保存校验

保存 `agentCapabilityAssignments[service_id] = agent_id` 前，后端必须验证：

- Agent installation 存在；
- enabled；
- execution_ready；
- item/definition 声明支持该 purpose。

失败返回 `agent_not_installed` / `agent_not_ready` / `agent_capability_unsupported`。

### 9.2 执行校验

即使设置保存时有效，每次业务 execution 仍通过动态 Registry lookup；不可用时返回错误，不改 setting。

### 9.3 UI Picker

- 默认只列 execution-ready Agent。
- 当前 assignment 不可用时保留一条 disabled current value，显示原因和安装/修复 CTA。
- 不允许 picker 为了展示而对所有 Market Agent做 connection probe。
- model picker 只在用户展开当前 Agent 时懒加载。

## 10. 旧数据迁移

### 10.1 原则

- 不把所有 hardcoded Agent 写入 `agent_installations`。
- 只 materialize 当前 assignments 实际引用的 Agent。
- 不在 migration/应用启动中联网安装。
- 可自动绑定已存在且兼容的 System CLI，因为旧版本已依赖该本机入口。
- Npx hardcoded command 不等于持久安装；需用户确认 Market 安装。
- assignment 和 model 选择必须保留，即使 Agent 暂时不可用。

### 10.2 幂等算法

```text
for each distinct assigned agent_id:
  if installation row exists: continue
  if curated item has compatible System distribution:
    probe bounded system command/version
    if present:
      create system binding
      run conformance and record health
      continue
  retain assignment without installation
  surface migration notice/install CTA
```

记录 migration marker 使用现有 settings/schema version 或数据库 migration 事实，不新增每 Agent 历史表。重复执行不得覆盖用户后来选择的 managed distribution。

该算法是 post-upgrade 后台 workflow，不属于 SQL migration transaction。Desktop 与 Engine 都通过 AppService 调度同一幂等逻辑；运行期间只锁定对应 Agent，不阻塞设置、目录读取或其他 Agent。若首次业务执行早于迁移完成，返回 `agent_migration_pending` 或 `agent_not_installed` 的稳定状态，绝不回到旧临时命令。

## 11. 当前文件迁移地图

| 当前文件 | 目标变化 |
|---|---|
| `src-tauri/src/backend/agents/registry.rs` | 删除 hardcoded catalog，改动态 snapshot/definition builder |
| `src-tauri/src/backend/agents/types.rs` | resolved definition + 明确 health DTO；删除 `cli_fallback` |
| `src-tauri/src/backend/ai_execution/mod.rs` | 删除进程级 shared `OnceLock` |
| `src-tauri/src/backend/ai_execution/executor.rs` | registry handle + active agent identity + 正确 connection semantics |
| `src-tauri/src/backend/application/agent.rs` | installed/health/compat reads |
| `src-tauri/src/backend/application/system.rs` | 构建显式 Agent service/runtime ownership |
| `src-tauri/src/adapters/app_state.rs` | 持有 runtime manager |
| `src-tauri/src/adapters/tauri/background_tasks.rs` | 增加 Agent lifecycle registry；后续可按模块拆分 |
| `src-tauri/src/adapters/engine/registry.rs` | 新增 Engine methods/风险元数据 |
| `frontend/src/components/settings/agentCatalog.ts` | 删除静态命令/package 真相；仅保留纯展示 icon metadata 或合并到服务数据 |
| `frontend/src/components/settings/AgentSettingsPanel.tsx` | Market/Installed 管理界面 |
| `frontend/src/components/settings/AgentCapabilityDialog.tsx` | 只使用 ready installations，取消 probe-all |
| `frontend/src/services/agentRuntime.ts` | 新 DTO/API 统一边界 |

## 12. API 兼容与弃用阶段

### Stage A：Additive

- 新 DB、catalog、installer、dynamic Registry。
- 旧 API 存在；新字段 additive。
- 新 UI 使用 Market API。

### Stage B：Compatibility View

- `agent.catalog.list` 从新 catalog/installed 组装。
- hardcoded Rust/TS catalog 不再是 source of truth。
- `cli_fallback` 不再产生。

### Stage C：Cleanup

- 确认所有 internal caller 已迁移。
- 删除未使用 legacy OpenCode CLI execution seam。
- 公开旧方法是否移除另行走 deprecation SPEC；MVP 不做 breaking removal。

## 13. 跨层验收

1. 同一临时 DB 下 Tauri/AppService/Engine 的 installed list 语义一致；Tauri task 与 Engine run 的 terminal result 等价。
2. CLI install 调用 Engine 后，Desktop 重载/下次读取能看见同一 row。
3. Engine method/DTO 变更后 `cli/internal/schema/contract.json` 由生成命令更新。
4. 前端所有 Agent invoke 只存在于 service 层。
5. compatibility API 对未安装 Agent 不泄露可执行的临时 `npx -y` 命令。
6. 旧 assignment 迁移不联网、不改 Agent ID、不丢 model。
7. 用户目录路径在 UI/文档输出为 `~`，Runtime 内部仍使用已规范化实际路径。
