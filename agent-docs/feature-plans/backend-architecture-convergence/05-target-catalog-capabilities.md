# SPEC-BA-05：TargetCatalog、Capability Action 与 Legacy Agent 收口

- 状态：Proposed v1
- 优先级：P1/P2
- 前置：SPEC-BA-01；Agent probe 服从 SPEC-BA-03
- 交付物：运行时 Catalog 注入、canonical ActionId settings、legacy executor 删除

## 1. 当前问题

### 1.1 TargetCatalog 未接线

`AppRuntime` 保存 `RegistrySnapshot<TargetCatalog>`，但生产 workflow 没有消费：

- `app_paths.rs` 重新创建 `TargetCatalog::builtin()`。
- `defaults.rs` 重新创建 `TargetCatalog::builtin()`。
- `path_utils::detect_app_target` 硬编码 AppKind 与目录表。
- profile capability 接受 caller 提供的路径/provider，而不是查询 runtime catalog。

因此“新增 provider 不修改 Rust”只被孤立单元测试证明，没有覆盖 seed、Profile、planner 和
mount executor。

### 1.2 Agent action 配置仍双轨

`resolve_agent_for` 同时读取：

```text
agentCapabilityAssignments[action]
legacy memory
cardTranslation fallback
aiRuntime.cli/model fallback
```

任何 action 最后都可能错误回退到 `cardTranslation`。设置规范还混用 camelCase
`promptOptimization` 与 runtime snake_case `prompt_optimization`。

### 1.3 旧执行栈仍编译

以下已无生产调用，但仍存在：

```text
ai_execution/legacy_gemini.rs
configured_agent_capability
AiCliRuntime
AiStructuredTextRequest
execute_structured_text
run_cli_command
resolve_cli_executable
```

## 2. TargetCatalog Authority

### 2.1 Provider ID

`TargetProviderId(String)` 是 canonical identity。`AppKind` 仅为旧 UI/存储兼容字段，禁止作为
新 provider 的权威枚举。

```rust
pub(crate) struct TargetProfileDescriptor {
    pub id: TargetProviderId,
    pub name: String,
    pub default_targets: Vec<TargetPathDescriptor>,
    pub supported_kinds: Vec<AssetKind>,
    pub deployment_strategy: DeploymentStrategy,
    pub app_kind_compat: Option<AppKind>,
}
```

新 provider MUST NOT 要求修改 `AppKind`。只有确实需要兼容旧前端协议时才设置
`app_kind_compat`。

### 2.2 注入规则

- `AppService` 通过 `self.runtime.target_catalog()` 获取一致快照。
- Application workflow 在开始时加载一次 snapshot，整个请求内保持一致。
- defaults/bootstrap 显式接收 `&TargetCatalog`，不得内部调用 `builtin()`。
- `AppPathCatalog` 改为对 `TargetCatalog` 的查询服务或删除。
- path detection 遍历 descriptor targets，使用 HostPaths 展开 portable anchor，再使用
  HostFilesystem containment。
- planner/executor 从持久化 Profile 的 `target_provider_id` 关联 descriptor；不存在 provider
  时返回 `target_provider_missing`，不得按 AppKind 猜测。

### 2.3 启动与替换

```text
load bundled descriptors
→ validate unique ID/path/schema
→ construct complete TargetCatalog
→ publish RegistrySnapshot
→ seed defaults using the same snapshot
```

动态刷新必须在锁外构造完整 catalog，成功后原子 replace；失败保留旧 snapshot。

### 2.4 Profile seed

- 默认 Profile 来源只能是 active TargetCatalog。
- seed 使用稳定 provider ID，重复启动不得覆盖用户自定义名称、路径、规则或 enabled。
- 新 provider 可新增默认 Profile；被移除 provider 的已有 Profile 保留并标记 unavailable，不能
  静默删除 mount intent。

## 3. ActionId 与配置 schema

### 3.1 Canonical action IDs

唯一命名表：

```text
translation.card
translation.connection_test
translation.model_discovery
memory.extraction
memory.dream
prompt.optimization
```

新增 action 必须先注册：

```rust
pub(crate) struct ActionRegistration {
    pub id: ActionId,
    pub policy: ExecutionPolicyClass,
    pub required_capability: AgentCapability,
    pub default_agent: Option<AgentId>,
}
```

开放字符串并不意味着任意字符串合法；未知 action 必须失败闭合。

### 3.2 Canonical settings

```json
{
  "settingsSchemaVersion": 3,
  "agentAssignments": {
    "translation.card": { "agentId": "opencode", "modelId": null },
    "memory.extraction": { "agentId": "opencode", "modelId": null },
    "memory.dream": { "agentId": "opencode", "modelId": null },
    "prompt.optimization": { "agentId": "opencode", "modelId": null }
  }
}
```

原则：

- agent 与 model 作为同一 assignment 读取，避免跨 map 不一致。
- connection/model-discovery 默认使用其父 action Agent，不单独持久化，除非产品允许独立配置。
- resolver 只读 canonical schema，不读取 legacy key。
- 默认 Agent 来自 ActionRegistration；不得统一回退到 card translation。

### 3.3 一次性迁移

迁移函数负责读取：

```text
agentCapabilityAssignments.cardTranslation
agentCapabilityAssignments.memory
agentCapabilityAssignments.memory.extraction
agentCapabilityAssignments.memory.dream
agentCapabilityAssignments.promptOptimization
aiRuntime.cli
aiRuntime.model
```

迁移步骤：

1. 如果 canonical assignment 已存在，保留用户值。
2. 展开 legacy `memory` 到缺失的 extraction/dream。
3. 把 cardTranslation 映射到 `translation.card`。
4. 把 promptOptimization 映射到 `prompt.optimization`。
5. 只在缺失时使用 aiRuntime cli/model。
6. 写入 schema v3。
7. 从持久化文档删除 legacy execution keys；UI 非执行配置可保留在其领域对象。
8. 后续启动看到 v3 不再执行迁移。

迁移必须幂等，并保留未知用户字段；未知 action assignment 必须隔离并记录，不得静默绑定。

## 4. AgentExecutionRuntime 唯一入口

所有用户文本执行、连接检查和模型发现必须映射为 Agent runtime 请求：

```rust
pub(crate) trait AgentExecutionRuntime: Send + Sync {
    fn execute(&self, request: AiExecutionRequest) -> Result<AiExecutionResult, AiExecutionError>;
    fn check_connection(&self, agent_id: &AgentId, limits: ExecutionLimits)
        -> Result<AgentConnectionResult, AiExecutionError>;
    fn discover_models(&self, agent_id: &AgentId, limits: ExecutionLimits)
        -> Result<AgentModelsResult, AiExecutionError>;
}
```

Compatibility 方法可以保留原名称，但必须只做：

```text
旧 DTO → ActionId/Agent request → AgentExecutionRuntime → 旧 DTO
```

不得再执行 vendor-specific command。

## 5. Legacy 删除顺序

1. 为所有旧 public method 增加 runtime delegation 行为测试。
2. 把测试 fixture 从 `AiCliRuntime` 迁到 fake `AgentExecutionRuntime`。
3. 删除 `legacy_gemini.rs`。
4. 删除 structured CLI request/runner/argument builder。
5. 删除 `configured_agent_capability`。
6. 删除旧 settings runtime fallback。
7. 增加边界守卫，禁止符号回归。

旧 CLI Agent 本身仍可作为 AgentDefinition 的 native/ACP provider；删除的是旁路 executor，
不是禁止使用 CLI 程序。

## 6. Availability 中立化

UI 不得调用带 provider 名称的 availability API。canonical API：

```text
check_action_availability(actionId)
→ assignment
→ installed Agent definition
→ shared probe
→ ActionAvailability
```

```rust
pub(crate) struct ActionAvailability {
    pub action_id: String,
    pub configured_agent_id: Option<String>,
    pub available: bool,
    pub reason_code: Option<String>,
    pub repair_action: Option<String>,
}
```

`check_opencode_translation_availability` MAY 暂存为 alias，但内部不得硬编码 OpenCode，且契约
必须标 deprecated/since/removal target。

## 7. 边界守卫

完成后以下搜索必须为 0：

```text
legacy_gemini
configured_agent_capability
AiCliRuntime
AiStructuredTextRequest
execute_structured_text
run_cli_command
TargetCatalog::builtin()  # 除 bootstrap/catalog tests
detect_app_target 的硬编码 AppKind path table
```

允许点必须用精确文件 allowlist，而不是全局计数。

## 8. 测试要求

### TargetCatalog

1. `runtime_catalog_drives_default_profiles`
2. `runtime_catalog_drives_target_detection`
3. `new_descriptor_mounts_without_app_kind_or_rust_change`
4. `missing_provider_marks_profile_unavailable_without_deleting_mounts`
5. `failed_catalog_refresh_preserves_previous_snapshot`

### Action settings

1. `settings_v3_migration_is_idempotent`
2. `explicit_canonical_assignment_beats_all_legacy_values`
3. `legacy_memory_fans_out_only_to_missing_actions`
4. `prompt_optimization_never_falls_back_to_card_translation`
5. `unknown_action_fails_before_agent_execution`
6. `model_is_read_from_the_same_assignment_as_agent`

### Runtime delegation

1. `gemini_compat_translation_calls_injected_agent_runtime_once`
2. `opencode_compat_availability_uses_action_availability`
3. `connection_and_model_discovery_use_shared_probe_limits`
4. `legacy_executor_symbols_are_absent`

## 9. 验收标准

- 默认 Profile、target detection、planner 和 mount 使用同一 runtime catalog snapshot。
- 虚构 provider descriptor 无需修改 Rust enum 即完成默认 Profile 与 symlink mount e2e。
- settings resolver 只读取 canonical v3 assignments。
- 任意 action 不再回退到 cardTranslation。
- legacy Gemini/structured CLI executor 从生产代码和测试中删除。
- provider-specific public alias 只剩显式 deprecated 的 DTO adapter，且内部走共享 runtime。
