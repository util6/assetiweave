# Execution Playbook：Lunna / Flash / 代码执行模型

| 字段 | 值 |
|---|---|
| 用途 | 将本 SPEC 以最小、确定上下文逐 Task 实施 |
| 执行单位 | `08-implementation-plan.md` 中唯一一个 Task ID |
| 默认方法 | 测试先行、小 diff、文件白名单、完成即停 |

## 1. 调度原则

1. 不向执行模型发出“实现整个 Agent Market”的请求。
2. 每轮只包含一个 Task ID，最多一个 checkpoint 验证。
3. 模型只加载该 Task 的指定分册/章节和直接代码文件。
4. 模型先报告现有代码事实，再写失败测试。
5. 模型不得自行选择未冻结产品决策。
6. Task 完成后停止，不执行“下一步顺便处理”。
7. 评审者检查 diff/测试/不变量后，才调度依赖图下一项。

## 2. 固定基础上下文

每轮必须提供：

```text
1. 仓库根 AGENTS.md
2. agent-docs/feature-plans/SPEC_ Agent Marketplace 与动态运行时.md
3. agent-docs/feature-plans/agent-marketplace-dynamic-runtime/08-implementation-plan.md 中目标 Task 全节
4. agent-docs/feature-plans/agent-marketplace-dynamic-runtime/10-progress.md 当前状态
5. git status --short
```

不要默认一次加载 01–07 全文；按下表补充。

## 3. Task 到最小 SPEC 上下文

| Task | 必读分册/章节 |
|---|---|
| T00–T01 | 主索引 §3–7；`05` §1 |
| T02–T03 | `01` §4–6；`04` §1/14；`06` §1–2 |
| T04–T06 | `03` §2–12；`07` CAT/DST tests |
| T07 | `03` §5.1/8–10；`04` §5；`07` System controls |
| T08 | `03` §5.2；`07` Binary/archive controls |
| T09 | `03` §5.3；`07` Npx/npm controls |
| T10 | `03` §5.4；`07` Uvx/uv controls |
| T11 | `04` §10；`07` protocol/process tests；前置 ACP process spec |
| T12–T15A | `02` §5–8；`04` §12；`06` current file map |
| T16–T19 | `04` §2–13；`07` LIFE tests |
| T20–T23 | `06` §1–5；`07` API tests |
| T24 | `06` §6；CLI conventions；`07` CLI tests |
| T25–T26 | `05` 全文；`06` §9–10；`07` MIG tests |
| T27–T31 | `06` §7–9；`01` FR-UX/FR-HLT；`07` UI tests |
| T32 | 主索引 §4/7；`05` §5；`06` §12 |
| T33 | `07` 全文；`10` 全文 |

## 4. 通用执行 Prompt

复制模板并替换大写变量：

```text
执行 Agent Market 与动态 Runtime 的 TASK_ID：TASK_TITLE。

你必须先读取：
1. 仓库根 AGENTS.md。
2. 主 SPEC 索引。
3. 08-implementation-plan.md 中 TASK_ID 全节。
4. SPEC_SECTIONS。
5. SOURCE_FILES，只读与本任务直接相关 symbol。
6. 10-progress.md 当前状态和 git status --short。

文件白名单：
FILE_ALLOWLIST

工作协议：
- 首先输出 3–8 条“现有代码事实”和 3–8 条“本任务完成条件”。
- 核对依赖 Task 已 Complete；未完成则停止。
- 先写本任务列出的失败测试，运行并记录预期 FAIL 原因。
- 再做最小实现，直到本任务测试 PASS。
- 不修改白名单外文件；确需修改时停止并报告，不自行继续。
- 不实现后续 Task，不做顺手重构，不修改用户已有未提交改动。
- 不手工编辑 generated contract。
- 不引入执行期网络、npx -y、临时 uvx、shell string、用户任意 env/command。
- 不复制 Conversation package 的 recursive hash/trust/edit/history。
- System distribution 永不拥有或删除外部 executable。
- OpenCode ACP 失败永不转为 CLI execution fallback 或 connected=true。
- 日志/错误不得包含 prompt/result/raw secret/完整 env/raw stderr。
- 每个 process terminal path 和 task cancellation path 必须清理。

必须运行：
VERIFY_COMMANDS

Stop Conditions：
- 采用 08 §2 全局 Stop Conditions。
- TASK_SPECIFIC_STOP_CONDITIONS

完成后只输出：
1. TASK STATUS
2. EXISTING FACTS VERIFIED
3. CHANGES MADE（逐文件）
4. TESTS FIRST（FAIL -> PASS）
5. VERIFICATION（逐命令 + exit code）
6. SPEC ACCEPTANCE（逐项 [x]/[ ]）
7. INVARIANTS CHECKED
8. NOT TOUCHED
9. DEVIATIONS / OPEN ISSUES
10. PROGRESS UPDATE
11. NEXT TASK（只写，不执行）
```

## 5. 执行前检查

执行模型在改文件前必须运行或读取：

```bash
git status --short
git diff -- TASK_FILE_1 TASK_FILE_2
rg -n "TARGET_SYMBOL" TARGET_DIRECTORIES
```

处理规则：

- 白名单文件有他人未提交改动：先理解 diff；无法无损叠加则停止。
- 白名单外有未提交改动：忽略，不清理、不 stash、不 reset。
- 发现计划中的 future file 已存在：先读内容，不覆盖式重建。
- migration `NEXT` 必须基于当前最新文件名生成，禁止猜编号。

## 6. 测试先行标准

有效的 tests-first 证据包含：

1. 新测试 ID/名称。
2. 实现前运行命令。
3. 测试失败是因为目标能力缺失，而不是编译器拼写错误。
4. 实现后同一测试 PASS。
5. 相关回归 suite PASS。

无效证据：

- 先写实现后补测试。
- 测试从未出现预期失败。
- 用 `#[ignore]`、条件跳过或真实网络隐藏失败。
- 只跑新 test，不跑 Task 指定回归。
- 把环境未满足记录成 PASS。

## 7. 领域不变量检查清单

### 7.1 Catalog/Distribution Task

- [ ] Protocol 与 Distribution 未混合。
- [ ] version 固定，无 latest/range。
- [ ] 无任意 URL/path/Git/shell/env。
- [ ] System/Binary/Npx/Uvx 选择确定。
- [ ] 标准 ACP item 无 Vendor code 分支。

### 7.2 Installer Task

- [ ] installer 只返回 MaterializedRuntime，不写 DB/Registry。
- [ ] 取消和 timeout 收敛。
- [ ] managed program 位于 staging/active root。
- [ ] System 不修改外部文件。
- [ ] Runtime definition 不含 package manager download invocation。

### 7.3 Lifecycle Task

- [ ] staging 不进入 Registry。
- [ ] update failure preserves old。
- [ ] active execution blocks switch/delete。
- [ ] DB/Registry failure 有补偿。
- [ ] I/O 时不持有 global app lock。

### 7.4 API/UI Task

- [ ] 业务逻辑在 AppService/backend。
- [ ] frontend invoke 只在 service。
- [ ] CLI 只调用 Engine。
- [ ] installed/connected/execution_ready 分离。
- [ ] no probe-all。
- [ ] current unavailable assignment 被保留。

## 8. Task 专用 Prompt 示例

### 8.1 T09 Npx Installer

```text
执行 T09：实现 Npx Materializing Installer。

读取：
- 03-catalog-and-distribution-contract.md §5.3、§8–10
- 07-security-testing-acceptance.md §2.3、DST-07..09
- 08-implementation-plan.md T09
- 当前 host_process.rs 的 bounded command runner

白名单：
- src-tauri/src/backend/agent_market/installers/npx.rs
- src-tauri/src/backend/agent_market/installers/mod.rs
- src-tauri/src/backend/agent_market/types.rs

先写 fake npm 测试，断言：
1. exact PACKAGE@VERSION；
2. --ignore-scripts/--omit=dev/--no-audit/--no-fund/--save-exact；
3. lock version/integrity mismatch fail；
4. resolved bin 在 staging；
5. result program/args 不含 npx -y；
6. timeout/cancel 清理。

禁止真实 npm 网络。完成后只报告 T09，不执行 T10。
```

### 8.2 T12 Dynamic Registry

```text
执行 T12：将 AgentRegistry 改为不可变动态快照。

读取：
- 02-architecture-design.md §6
- 04-installation-lifecycle-and-runtime-registry.md §12
- 08-implementation-plan.md T12
- 当前 agents/registry.rs、types.rs 和 installation repository

白名单按 T12。

先写：fresh empty、one ready row、failed reload keeps old、concurrent readers old-or-new、duplicate fail。
不得保留 builtin hardcoded definitions，不得在 lookup 查询 DB，不得在 snapshot 中保存 catalog download fields。
```

### 8.3 T17 Update

```text
执行 T17：Update 与 Reinstall。

测试必须逐个注入：download fail、integrity fail、conformance fail、DB fail、Registry publish fail、old cleanup fail。
前五项都断言 old row/path/definition 不变；cleanup fail 断言 new active + warning。
激活前和激活临界点各检查 active execution。
不新增版本历史表或 rollback UI。
```

### 8.4 T26 OpenCode

```text
执行 T26：修正 OpenCode connection 语义。

先证明当前 cli_fallback 只在 connection probe，而 Translation 不走 opencode run。
测试 version success + ACP fail 必须得到 installed=true、connected=false、execution_ready=false。
删除 cli_fallback 字段/分支，不增加 execution fallback route。
用 fake process 断言失败时没有 opencode run spawn。
```

## 9. 评审 Prompt

实现模型完成后，使用独立评审轮：

```text
审查 TASK_ID 的 diff，不修改代码。

输入：
- TASK SPEC 全节
- git diff -- TASK FILES
- 测试输出
- progress 记录

按优先级检查：
P0 数据丢失、外部文件删除、任意命令/路径、secret 泄露、update 破坏旧版本
P1 状态语义错误、执行期联网、Registry partial、active execution race、取消不收敛
P2 API/边界重复、probe-all、测试不足、错误不稳定
P3 命名/文档/局部可读性

每个 finding 必须给：priority、文件、精确行、违反 Requirement、复现/测试建议。
没有 actionable finding 时明确写“无阻塞问题”，不要虚构建议。
```

## 10. Checkpoint 交付格式

Checkpoint 不写新功能，只验证前一 Phase：

```text
CHECKPOINT: A/B/C/D/E/F
TASKS INCLUDED:
COMMANDS:
- command -> PASS/FAIL
CROSS-TASK INVARIANTS:
- [x]/[ ]
REGRESSIONS:
- none / exact issue
UNCOMMITTED CONFLICTS:
- none / paths
DECISION:
- PASS: next task ID
- FAIL: return to task ID and reason
```

## 11. Progress 更新规则

执行模型只更新自己完成的 Task 行和证据；禁止：

- 把依赖任务批量标 Complete。
- 因代码“似乎存在”跳过验证。
- 把未运行的跨平台 smoke 标 PASS。
- 修改冻结决策来适配实现捷径。
- 把目标架构提前写入 已淘汰的全局设计总册（以代码、测试与 ADR 为准） 当作当前事实。

## 12. 最终发布审查

T33 前独立检查：

```bash
git status --short
git diff --check
rg -n "npx.*-y|cli_fallback|opencode run" src-tauri/src frontend/src cli
rg -n "trusted_hash|installed_content_hash|untrusted|changed" src-tauri/src/backend/agent_market
rg -n "invoke\(" frontend/src --glob '!services/**'
```

任何命中必须分类：合法测试/文档/Conversation 既有域，或 Agent Market 违规。不得只因 `rg` 有输出就机械删除其他域代码。
