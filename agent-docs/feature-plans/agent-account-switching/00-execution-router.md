# Agent 账号切换（切号）：执行路由

> 面向 Luna / Flash 及其他执行型 Agent。本执行包落地「AionUi 多 Agent 协作 + Cockpit 账号管理」合并功能：
> 在已有的 Agent 工作台（单聊 + Team 多 Lane）之上，把「账号（Account Profile）」提升为**一等公民领域对象**，
> 让每个智能体成员/单聊会话绑定独立的 Provider 账号凭据，实现免网关的沙箱化切号与并发隔离。

## 1. 结论先行：从哪里入手

**不要从 UI 入手，从「可行性 Spike + 数据契约」入手。**

理由（代码取证，见第 2 节）：

1. 前端的「切号 UI」**已经做完了**并仍在仓库里：`AccountPill`、`AccountManagerModal`、`TeamCreateModal` 的账号列、
   `SingleChatHeader`、`TeamParallelLanes`、`AgentWorkspacePage` 均已落地并挂进了路由与菜单。
2. 但它是**空壳**：账号数据写在 localStorage（`assetiweave_agent_accounts_v1`），配额是硬编码假数据，切号只改 UI 状态，
   不会真正影响任何子进程。
3. 后端**零**账号概念：`src-tauri` 全库检索无 `agent_accounts` / `account_id` / 凭据存储。
4. 因此真正要补的是两件事：**账号域的持久化与凭据安全**，以及**执行接缝的隔离注入**。两件事都压在后端，
   且都依赖一个尚未在本机验证的前提——*各 Provider CLI 是否真的支持目录级凭据隔离*。

所以第一步是 **P0 Spike**：用半天把隔离矩阵验证掉，再决定数据模型与注入方式，避免把方案建在假设上。

## 2. 现状盘点（事实，非推断）

| 维度 | 现状 | 证据 |
|---|---|---|
| 前端页面 | 已存在并已挂载路由/菜单 | `frontend/src/pages/agent-workspace/AgentWorkspacePage.tsx`；`router/routes.ts` 有 `agent-workspace`；`router/menu.ts` 有 `agent.workspace` |
| 单聊 / Team UI | 已存在 | `components/agent-workspace/single/*`、`components/agent-workspace/team/*`（TeamHeader / TeamParallelLanes / TeamBroadcastBar / TeamCreateModal / TeamViewToggle） |
| 切号 UI | 已存在 | `components/common/AccountPill.tsx`、`components/common/AccountManagerModal.tsx` |
| 前端契约 | 已存在 | `types/agentWorkspace.ts`（`AgentAccount`、`AgentProvider`）、`services/agentAccountService.ts`、`services/agentTeamService.ts` |
| 账号持久化 | **无**，localStorage Mock | `agentAccountService.ts` 的 `STORAGE_KEY = "assetiweave_agent_accounts_v1"`，`quotaSummary` 为硬编码 |
| 后端账号域 | **零** | `src-tauri/migrations/` 无账号表；全库检索无 `agent_account` / `account_id` / `credential` / `keychain` |
| Team 领域模型 | 已存在（无账号维度） | `202608310001_team_roster.sql`（`teams` / `team_members`：role、agent_id、model、execution_context_key）、`202608310002_team_execution.sql`（`team_runs` / `agent_execution_bindings`） |
| 执行环境注入接缝 | **已存在** | `domain/agents/definition.rs` 的 `AgentDefinition.env: Vec<AgentEnvEntry>`；`agent_execution/managed_process.rs` 的 `command.envs(definition.env…)` |
| 定义组装点 | 已存在 | `infrastructure/agent_market/runtime/definition.rs`（账号 env 的最佳注入点） |
| 上游隔离可行性 | Codex ✅ / Claude ✅（源码取证）/ 其余待验 | `~/fork-code/codex/codex-rs` 测试注释：`Credential storage resolves CODEX_HOME from the process environment`；`~/fork-code/claude-code-source/src/memdir/paths.ts` 引用 `CLAUDE_CONFIG_DIR` |

## 3. 架构决策（与 2026-09-20 旧会话不同，需显式说明理由）

1. **不新建独立 crate。** 旧会话建议 `crates/agent-team` 承载移植代码；但 ADR-0016 已确立
   `Adapters -> Application(AppService) -> Domain <- Store / Infrastructure` 五层单向架构与架构守卫，
   独立 crate 会绕开守卫、契约同步（`pnpm cli:contract`）与单文件规模治理。
   **决定**：账号域落在 `domain/agents`（账号为 agent 的子域）+ `store/agents`，由 `AppService` 收口，
   Tauri/Engine 命令经 `adapters/` 暴露。
2. **账号是一等公民领域对象，不是 UI 配置。** 它参与执行绑定、审计与配额状态，必须进 SQLite（元数据），
   凭据密文不放 SQLite 明文。
3. **不做网关。** 明确排除 Sub2API 类反向代理：会丢失原生 ACP 双向流、权限确认与客户端工具调用。
4. **隔离手段优先级**：Provider 原生目录环境变量 > 覆写 `HOME`/`XDG_*` > 全局覆写 + 串行锁（降级）。
   每个 Provider 的最终手段由 P0 Spike 定稿，不得假设。
5. **换号语义**：账号切换对**新会话/新执行**生效；运行中的会话不中途换号（避免半途串号与状态撕裂）。
   UI 需显式表达这一语义。

## 4. 分阶段路线

### P0 · 隔离可行性 Spike（0.5–1 天，先做，做完才动代码）

目标：产出一张**经过本机验证**的 Provider 隔离矩阵，作为后续所有设计的地基。

| Provider | 首选手段 | 待验证点 | 状态 |
|---|---|---|---|
| Codex | `CODEX_HOME=<profile>/codex` | 并行两进程各读各自 `auth.json`，互不干扰 | 源码取证支持，需本机复验 |
| Claude Code | `CLAUDE_CONFIG_DIR=<profile>` | 凭据是否随该目录走（存在只影响 Markdown 配置的风险） | 待验；不通过则降级到覆写 `HOME` |
| Antigravity | 未知（查 `~/.gemini/antigravity-ide` 结构） | 是否存在 app-data 目录变量 | 待验；不通过则标记「仅单账号 + 串行」 |
| OpenCode | `XDG_DATA_HOME` / `HOME` 覆写 | opencode.db 是否随目录走 | 待验 |
| 通用兜底 | 子进程 `HOME` + `XDG_CONFIG_HOME` + `XDG_DATA_HOME` 指向 profile 目录 | 需确认 workspace 路径必须传绝对路径 | 待验 |

验收标准：
- 至少 Codex 能做到「两个账号并行两个进程，各自读到自己的凭据，且**不污染** `~/.codex`」；
- 每个 Provider 在矩阵里都有明确结论（可用手段 / 降级方案 / 不支持）；
- 产出物：本目录下的 `01-isolation-matrix.md`（P0 完成时补写）。

### P1 · 数据契约与迁移（后端）

- 新增迁移 `agent_accounts`（tenant 作用域）：`id / provider / display_name / email / auth_type / state /
  quota_summary_json / created_at / last_used_at`；**凭据密文不落此表**，只存 `credential_ref`。
- `team_members` 增补可空 `account_id`；`agent_execution_bindings` 增补可空 `account_id`（记录「这次执行用的哪个号」）。
- 凭据存储决策（需拍板，建议新增 ADR 或决策记录）：
  方案 A macOS Keychain（安全性最好，跨平台成本高）；
  方案 B 应用数据目录下加密文件 + SQLite 存引用（跨平台、可控）；
  **建议 B**，Keychain 作为 macOS 增强后置。
- 凭据导入：第一版只做「从现有 `~/.codex/auth.json` 等导入」+「粘贴 Token/API Key」，OAuth 唤起后置。

验收：`cargo test --workspace` 通过；迁移可重复执行；账号 CRUD 经 AppService 且有单测（测试拆到 `*_tests.rs`）。

### P2 · 执行接缝注入（后端）

- 新增 profile 目录物化器（Infrastructure）：按 `account_id` 准备 `<profile>/<provider>/<account_id>/` 并写入凭据文件，
  spawn 后清理敏感临时态；**严禁**改动用户 home 下真实文件。
- 在 `infrastructure/agent_market/runtime/definition.rs` 组装 `AgentDefinition` 时，按账号合并 env 条目
  （复用既有 `AgentEnvEntry`，最小改动）。
- 执行绑定落 `account_id`；明确「换号只对下一次执行生效」。
- 长任务后台化：凭据刷新/导入不得阻塞 UI，遵循 Issue #33 的 Task Center 规范。

验收：Rust 层集成测试——同一 Team 两个 Codex 成员用 A/C 两个账号并行执行，断言各自的 `CODEX_HOME` 不同且
`~/.codex` 未被修改。

### P3 · 前端接线（把空壳接上真后端）

- `services/agentAccountService.ts`：localStorage → Tauri 命令（**保持现有导出类型与组件 API 不变**，
  这样 `AccountPill` / `AccountManagerModal` / `TeamWorkspace` 零改造）。
- `TeamCreateModal`：成员行的「绑定账号」列从本地状态改为落库 `account_id`（UI 已有，只补持久化）。
- `AccountManagerModal`：真实凭据录入 + 连通性检测（参考 cockpit-tools 的「连接检测」交互：发一条最小提示词验活）。
- 配额展示：第一版保留 mock 但**显式标注为未接入**，避免假数据冒充真相（红线：不得伪造状态）。

验收：`pnpm typecheck && pnpm test && pnpm build`；`cargo fmt --all -- --check && cargo test --workspace`；
契约变动必须 `pnpm cli:contract`。

### P4 · 增强（后置，不在首版范围）

- 真实配额/健康探针（复用 `acp_health_probe` 能力），额度耗尽时输入框给出「一键切备用号」；
- 429 / Quota Exceeded 自动 failover 到同 Provider 备用号，并把切号事实回传到会话；
- 远端通道（飞书 / 微信 / Webhook）→ 团队 + 账号路由绑定；
- OAuth 唤起登录。

## 5. 每轮固定读取顺序

1. 根 `AGENTS.md`；
2. 本专项 GitHub Issue 正文与最新评论（含 blocker）；
3. 本文件与 `01-isolation-matrix.md`（P0 后）；
4. 交叉领域：Issue #31（Agent Session 工作区 / `agent-session-workspace` 执行包）、Issue #33（Task Center）、Issue #20（Memory）；
5. ADR-0004（AppService 权威）、ADR-0007（adapter 不注入 UI）、ADR-0009（前端设计系统）、ADR-0011（ACP 市场）、ADR-0016（分层边界）；
6. 代码、测试与 CLI `--help`（最高事实源）。

与 #31 / #33 冲突时按第 5 节顺序裁定；上位来源互相冲突时执行 Stop Protocol，不自行发明产品语义。

## 6. 红线

- SQLite 是「账号元数据」唯一真相源；**凭据密文不得落 SQLite 明文**，也不得写入源资产目录。
- 应用不得破坏用户在 home 下的真实登录态（P0/P2 的硬验收项）。
- 前端不得绕过 `frontend/src/services` 直调 `invoke`；CLI 不得绕过 Engine。
- 单业务文件 ≤ 500 行，测试拆到同级 `*_tests.rs`；契约变动同步 `pnpm cli:contract`。
- 长耗时操作（导入、刷新、探针）必须后台任务化 + 事件/轮询，不在点击里 await 全过程。

## 7. 待你拍板的问题

1. 首版支持哪些 Provider？（建议 Codex + Claude，Antigravity 视 Spike 结果）
2. 凭据存储选 Keychain 还是加密文件？（建议加密文件 + macOS Keychain 后置）
3. 首版是否要做真实配额展示？（建议不做，标注为未接入）
4. 是否需要保留现有 localStorage Mock 作为 `pnpm dev` 纯前端预览能力？（建议保留，用开关切换）
