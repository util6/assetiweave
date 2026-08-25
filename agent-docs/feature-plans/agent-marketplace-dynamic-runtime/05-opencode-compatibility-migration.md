# OpenCode Compatibility Migration

| 字段 | 值 |
|---|---|
| 状态 | Proposed |
| 逻辑 Agent ID | `opencode` |
| 执行协议 | ACP only |
| 首版分发 | compatible System CLI、official managed Binary |

## 1. 当前事实

当前 Rust definition：

```text
program = opencode
args = ["acp"]
availability = opencode --version
cli_fallback = true
```

当前 `AgentExecutor::check_connection` 的行为：

1. 先执行 CLI availability/version probe。
2. availability 成功后执行 ACP connection probe。
3. ACP probe 失败且 `cli_fallback=true` 时，仍返回：
   - `available=true`
   - `connected=true`
   - `connection_method=cli_fallback`

但实际 Translation 路径已经是：

```text
Translation -> AgentExecutionRuntime -> AgentExecutor -> ACP Backend
```

它不会在 ACP 失败后调用 `opencode run`。因此 `cli_fallback` 当前只是连接检查的错误语义，不是执行双路由。

## 2. 必须拆开的三类“兜底”

| 维度 | 定义 | MVP |
|---|---|---|
| Distribution fallback | 同一 Agent 可在 System、Binary、Npx、Uvx 中选择可用分发 | 支持；OpenCode 为 System/Binary |
| Installation/probe fallback | managed 不可选时可绑定 System；或 CLI 存在但协议失败时保留诊断状态 | 支持，但不伪造 connected |
| Execution fallback | ACP 执行失败后切换为 `opencode run` 等另一套执行协议 | 不支持，明确禁止 |

任何实现、UI 或文档都不得再用一个 `cli_fallback: bool` 表示以上三件事。

## 3. 目标 OpenCode Market Item

```text
AgentMarketItem {
  id: opencode
  protocol: acp
  version: PINNED_VERSION
  distributions: [
    System {
      command_candidates: [opencode]
      version_args: [--version]
      version_range: SUPPORTED_RANGE
      launch_args: [acp]
    },
    Binary { per-platform official archive + sha256 + launch_args: [acp] }
  ]
}
```

UI 中只能有一个 OpenCode card。详情页展示两个安装候选，而不是两个 Agent。

## 4. 状态真值表

| System/managed 入口 | Version probe | ACP probe | installed | connected | execution_ready | 用户状态 |
|---|---|---|---:|---:|---:|---|
| 不存在 | 失败/未运行 | 未运行 | false | false | false | 未安装 |
| 存在 | 不兼容 | 未运行 | 可有诊断 binding | false | false | 版本不兼容 |
| 存在 | 成功 | 成功 | true | true | true | 可用 |
| 存在 | 成功 | 失败 | true | false | false | 已安装，ACP 连接失败 |
| 存在 | 成功 | auth required | true | false | false | 已安装，需要认证 |
| managed 入口缺失 | 失败 | 未运行 | true | false | false | 安装损坏，建议重装 |

禁止结果：`ACP probe 失败 && connected=true`。

## 5. 代码迁移要求

### 5.1 类型

从 `AgentDefinition` 删除：

```rust
cli_fallback: bool
```

不要替换成另一个模糊布尔值。分发候选存在 catalog；health 结果使用显式枚举；未来执行路由使用独立 route 类型。

### 5.2 Connection Check

`AgentRuntimeManager`/AppService connection workflow 的目标算法：

```text
1. 从 installation repository 解析 Agent；connection 检查不能只依赖 execution-ready Registry，否则无法复查 failed diagnostic binding。
2. installation 不存在 -> agent_not_installed。
3. runtime/entry 不可用 -> installed=true, connected=false, runtime error。
4. ACP probe 成功 -> connected=true, connection_method=acp。
5. ACP probe 失败 -> connected=false, connection_method=acp, error_code=acp_connection_failed。
6. CLI version probe 结果可作为 runtime_status/version，不覆盖 connected。
```

现有 `AgentExecutor::check_connection` 可在迁移期委托该 workflow，或将连接检查职责移出 Executor；不得继续用 execution Registry 是否有条目推断 `installed`。兼容 DTO 中 `available` 建议映射为 runtime available，不映射为 execution ready。新 UI 必须读取新字段而不是推断。

### 5.3 Translation

保持前置 ACP SPEC 的 D-004：

- actual translation 使用 `opencode acp`。
- 禁止在 Translation 新增 `opencode run` 分支。
- ACP timeout、取消、协议错误、权限请求或空输出都不触发 CLI execution fallback。
- assignment 指向 OpenCode 但未安装/未 ready 时返回明确错误和安装 CTA。

### 5.4 旧 CLI 代码

仓库中可能仍有 `AiCliRuntime::Opencode` 的参数构造 seam。迁移时：

1. 先用 `rg` 确认所有生产调用点。
2. 若只剩测试/未使用兼容代码，单独任务删除或标记 deprecated。
3. 不得为了“保留兜底”重新接回 Translation。
4. Gemini legacy seam 的处理遵循前置 ACP SPEC，不借此任务扩大迁移范围。

## 6. 升级迁移流程

### 6.1 输入

- 旧 `agentCapabilityAssignments`。
- 旧 `aiRuntime.cli/model` 归一化结果。
- 当前 hardcoded Agent IDs。
- 本机 System command probe。
- 新 curated catalog bundled 版本。

### 6.2 规则

1. 只处理现有 capability assignment 实际引用的 Agent；未引用的九个 Agent 只出现在 Market。
2. assignment 是 OpenCode 且本机存在兼容 System CLI：
   - 创建 system installation/binding；
   - 执行 bounded ACP conformance；
   - 成功则进入 Registry；失败则保留 failed health，assignment 不变。
3. assignment 是 OpenCode 但无兼容 CLI：
   - 不下载 Binary；
   - assignment 保留；
   - UI/执行返回 `agent_not_installed` + install CTA。
4. assignment 指向其他 compatible System Agent 时可使用相同绑定规则。
5. assignment 指向旧 `npx -y` Agent 时：
   - 不在升级中联网；
   - 不把临时 npx 能力视为已安装；
   - assignment 保留，提示从 Market 安装固定版本。
6. 不静默把失败 assignment 改成 OpenCode、Gemini 或任意默认 Agent。
7. 迁移必须幂等：重复启动不会重复插入、覆盖用户后续选择或重复 probe 已完成相同 catalog/core 版本的 installation。

Post-upgrade System discovery/conformance 必须作为后台能力运行：数据库 migration 只建表，不在 migration transaction 中启动进程；Desktop 启动后或首次 Agent 管理/执行时调度幂等 migration task。Engine/CLI 使用同一 AppService workflow。migration 未完成时返回可观察的 pending/not-installed 状态，不阻塞全局 app lock。

### 6.3 默认 assignment

旧逻辑在未配置时可能隐式选择 OpenCode。迁移后：

- 可以继续把逻辑默认记录/显示为 OpenCode；
- 但若 OpenCode 未安装，不能假装可执行；
- 首次使用对应 capability 时展示安装流程；
- 不允许通过运行时 `npx -y` 或自动下载消除错误。

## 7. 安装选择 UX

OpenCode install preview：

```text
OpenCode / PINNED_VERSION

推荐：使用现有安装
- program: ~/... 或解析后的路径以 ~ 归一化展示
- detected version: VERSION
- ownership: System（AssetIWeave 不会删除它）
- execution: ACP

备选：安装受管 Binary
- target: ~/.assetiweave/agent-runtimes/opencode/...
- download: SIZE
- checksum: SHA-256 verified
- ownership: Managed
- execution: ACP
```

用户必须看见 ownership 差异。选择 System 后不得在失败时自动下载 Binary；应返回 failed health 并提供“改用受管版本”动作。

## 8. 未来真正 Execution Fallback 的门槛

本节只定义未来评审条件，不属于 MVP 实施任务。

若未来支持 `opencode run`，必须新增显式：

```text
AgentExecutionRoute {
  id
  protocol
  program
  args
  capabilities
  fallback_policy
}
```

并满足：

1. 只在 prompt 尚未发送前的可安全分类错误中 fallback。
2. prompt 已发送、出现 partial output、timeout、cancel、auth、invalid model、tool/permission 事件后绝不 fallback，避免重复副作用。
3. 两条 route 的输出聚合、模型选择、取消和日志语义都有独立测试。
4. UI 清楚显示实际 route，不能把 CLI 执行称为 ACP connected。
5. 通过新 SPEC/ADR 批准后实施。

## 9. 回归测试

| ID | 测试 |
|---|---|
| OC-01 | System OpenCode version + ACP 均成功，execution_ready=true |
| OC-02 | version 成功、ACP 失败，installed=true/connected=false/execution_ready=false |
| OC-03 | System 缺失，预览推荐 managed Binary |
| OC-04 | System 存在但版本不兼容，不可静默选择 |
| OC-05 | 用户显式选择 System 且 conformance 失败，不自动下载 Binary |
| OC-06 | managed Binary 安装后 resolved program 在 app-owned root，args=`["acp"]` |
| OC-07 | Translation ACP 失败时没有 `opencode run` process |
| OC-08 | upgrade 有 assigned compatible System 时幂等创建 binding |
| OC-09 | upgrade 无 System 时保留 assignment 并返回 install CTA，不联网 |
| OC-10 | compatibility `connection_method` 不再返回 `cli_fallback` |

## 10. 完成条件

- `cli_fallback` 从领域模型和所有生产分支删除。
- OpenCode 只有一个 Market item 和一个 logical ID。
- System/Binary 选择可预览且 ownership 清楚。
- 所有实际执行仍为 ACP。
- CLI 存在但 ACP 失败不会被报告为 connected。
- 旧 assignment 迁移幂等且不静默联网/换 Agent。
