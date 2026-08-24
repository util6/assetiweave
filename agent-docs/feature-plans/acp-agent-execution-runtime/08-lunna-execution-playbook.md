# Lunna Execution Playbook

| 字段 | 值 |
|---|---|
| 用途 | 将本 Spec 文档集交给低成本执行模型逐任务实施 |
| 执行单位 | `07-implementation-plan.md` 中一个 Task ID |
| 默认策略 | 小上下文、测试先行、禁止跨任务扩展 |

## 1. 核心原则

Lunna 每次只执行一个 Task。不要一次要求“完成整个 Agent Runtime”。该功能的高风险在协议顺序、进程清理和跨层边界；大批量生成代码会掩盖错误。

每轮执行必须有：

1. 唯一 Task ID。
2. 明确文件白名单。
3. 最小 Spec 段落。
4. 必须先写的 tests。
5. 验证命令。
6. Stop conditions。
7. 固定交付格式。

## 2. 首轮上下文

给 Lunna 的固定基础上下文：

```text
- 仓库根 AGENTS.md
- agent-docs/feature-plans/SPEC_ ACP Agent Execution Runtime.md
- agent-docs/feature-plans/acp-agent-execution-runtime/07-implementation-plan.md 中当前 Task
```

再按任务补充，不要一次塞入所有文档。

## 3. 按任务选择上下文

| Task | 附加 Spec | 目标项目参考 |
|---|---|---|
| T01 | 04 §4.1 | AionCore Cargo.toml |
| T02–T03 | 02 §5–6；03 §3–4 | AionCore registry 指定 symbol |
| T04 | 01 FR-EXE；02 §5/10 | 当前 ai_execution.rs |
| T05–T07 | 04 §3；06 §5 | AionCore cli_process 指定 symbol |
| T08–T09 | 04 §4–5；06 §7 | AionCore protocol/acp.rs 指定 symbol |
| T10 | 04 §5–6；06 §8 | AionCore events/translate.rs |
| T11 | 04 §7–11；06 §6/9 | OpenCode ACP service 指定 symbol |
| T12 | 02 §7；01 FR-EXE | 无需额外参考项目 |
| T13–T15 | 05 §1–4 | 当前 AppService/Translation |
| T16–T18 | 05 §5–7；06 §10–13 | 当前 background task/Engine 模式 |
| T19–T21 | 05 §8–14；06 §14 | 当前 frontend providers/components |
| T22 | 06 全文 | 无 |

## 4. 通用执行 Prompt

复制以下模板并替换变量：

```text
执行 ACP Agent Execution Runtime 的任务 {{TASK_ID}}：{{TASK_TITLE}}。

必须先读取：
1. 仓库根 AGENTS.md。
2. agent-docs/feature-plans/SPEC_ ACP Agent Execution Runtime.md。
3. agent-docs/feature-plans/acp-agent-execution-runtime/07-implementation-plan.md 中 {{TASK_ID}} 全节。
4. {{SPEC_SECTIONS}}。
5. {{SOURCE_FILES}}。
6. {{REFERENCE_SYMBOLS}}，只读指定 symbol 附近，不复制完整目标项目结构。

文件白名单：
{{FILE_ALLOWLIST}}

工作方式：
- 先说明你对该任务的 3–6 条具体理解。
- 先添加或修改该任务要求的失败测试，再实现最小代码使其通过。
- 不修改白名单外文件；确需修改时停止并报告原因，不自行继续。
- 不实现后续 Task。
- 不做顺手重构。
- 不手工编辑 generated contract。
- 日志不得包含 prompt、result、raw stderr、完整 args 或 env value。
- 所有 process terminal path 必须清理；本任务尚未负责 cleanup 时不得伪造已支持。

必须运行：
{{VERIFY_COMMANDS}}

Stop conditions：
{{STOP_CONDITIONS}}

交付时严格输出：
1. CHANGES MADE
2. TESTS ADDED/UPDATED
3. VERIFICATION（逐条命令 + PASS/FAIL）
4. SPEC COMPLIANCE（逐条 acceptance）
5. THINGS NOT TOUCHED
6. OPEN ISSUES / DEVIATIONS
7. NEXT TASK（只写依赖图中的下一项，不执行）
```

## 5. Stop Conditions 标准集

每个任务都包含：

```text
- Spec 与现有代码事实冲突。
- ACP SDK 实际 API 与 Spec 推测不一致。
- 需要修改超过 5 个文件。
- 需要数据库 migration。
- 需要新增 Vendor-specific adapter。
- 需要记录或传递 prompt/result/raw tool input 才能继续。
- 测试只能依赖真实 OpenCode/网络才能通过。
- 现有未提交修改与本任务文件冲突，无法确认所有权。
```

发生 Stop condition 时，只报告：

- exact conflict；
- affected requirement/task；
- 2–3 个方案及 trade-off；
- 推荐方案；
- 等待决策。

不要擅自选高范围方案。

## 6. 任务专用 Prompt 示例

### 6.1 T03 Registry

```text
执行 T03：实现 Builtin AgentRegistry。

读取：
- 02-architecture-design.md §5–6
- 03-aioncore-reference-code-map.md §3–4
- ~/fork-code/AionCore/crates/aionui-ai-agent/src/registry.rs 中 AgentRegistry/get/decode_row
- ~/fork-code/AionCore/crates/aionui-db/migrations/001_initial_schema.sql 中 OpenCode seed

白名单：
- src-tauri/src/backend/agents/registry.rs
- src-tauri/src/backend/agents/mod.rs
- src-tauri/src/backend/agents/types.rs

先写 REG-01、REG-02、REG-06、REG-07。
不要引入 SQLite、async repository、UI metadata 或 handshake persistence。
验证：
cargo test --manifest-path src-tauri/Cargo.toml backend::agents::registry
```

### 6.2 T06 Managed Process

```text
执行 T06：实现 ManagedAgentProcess。

读取：
- 04-acp-process-runtime-design.md §3
- 06-test-verification-acceptance.md §5
- AionCore CliAgentProcess/spawn_for_sdk/take_stdio/kill/wait_for_exit
- 当前 backend/host_process.rs

白名单：
- src-tauri/src/backend/agents/process.rs
- src-tauri/src/backend/agents/mod.rs
- src-tauri/src/backend/host_process.rs

先写 PROC-01..06、PROC-10、PROC-13、PROC-14。
本任务不实现 ACP。
任何 stderr 日志包含原始内容即不合格。
```

### 6.3 T08 ACP connection

```text
执行 T08：建立 ACP connection actor。

读取：
- 04-acp-process-runtime-design.md §4.2–4.5
- AionCore AcpProtocol::connect 与 run_sdk_background

白名单：
- src-tauri/src/backend/agents/protocol/mod.rs
- src-tauri/src/backend/agents/protocol/acp.rs
- src-tauri/src/backend/agents/mod.rs

先写 ACP-01..04、ACP-10、ACP-17、ACP-18。
只做 initialize/connect/shutdown；不要提前实现 session flow、terminal、auth UI、dialect shim。
SDK API 与文档不同时立即停止，贴出准确 type/signature 与建议。
```

### 6.4 T11 ACP Backend

```text
执行 T11：Fake ACP + AcpExecutionBackend。

读取：
- 04-acp-process-runtime-design.md §7–11
- 06-test-verification-acceptance.md §6/9
- OpenCode service.ts 的 initialize/new/cancel/close/model/prompt

白名单按 07 的 T11。

先实现 fake modes，再实现 orchestration。
每个错误注入都必须断言：protocol shutdown、process reaped、workspace removed。
不要用 sleep 解决最后 chunk race。
```

### 6.5 T14 Translation migration

```text
执行 T14：迁移 OpenCode Translation，隔离 Gemini。

读取：
- 05-translation-task-api-integration.md §1–4
- 当前 card_translation.rs/application/card_translation.rs

先写 TR-01..06、TR-09/10/12。
OpenCode 正常 execution 不允许出现参数 run。
Gemini 当前参数和错误行为必须有回归测试。
不要改 frontend DTO。
```

## 7. Review Prompt

每个 Checkpoint 用新上下文让 Lunna 只做 review，不改代码：

```text
审查 Checkpoint {{NAME}}，只读，不修改代码。

依据：
- 对应 Spec sections
- 已完成 Task 的 acceptance
- git diff
- test output

检查：
1. 架构边界泄漏。
2. Vendor hardcode。
3. process/async cancellation leak。
4. prompt/result/env/stderr 泄漏。
5. 测试是否真的覆盖真实边界而非 mock 掩盖。
6. 是否提前实现后续 scope。
7. 每个 public contract 是否有兼容测试。

输出 findings，按 P0/P1/P2 排序，包含文件与行；无问题时明确写 no findings，并列出 residual risks。
```

## 8. 修复 Prompt

```text
只修复 review finding {{FINDING_ID}}。

允许文件：{{FILES}}
要求：
- 先加能复现 finding 的测试。
- 最小修复。
- 不处理其他 review finding。
- 运行 finding 指定测试和该模块全量测试。
- 输出 root cause、test evidence、diff scope。
```

## 9. 防止低成本模型常见偏差

### 9.1 看见 OpenCode 就写 match

拒绝：

```rust
if agent_id == "opencode" { ... }
```

要求回到 Definition + protocol route。

### 9.2 手写 JSON-RPC

除 fake fixture 的受控 frame 外，生产 Protocol 必须 typed SDK。

### 9.3 只 kill direct child

必须 process group/tree + wait/reap。

### 9.4 timeout 只 drop future

必须触发 cancellation、protocol cancel、cleanup convergence。

### 9.5 把原始 stderr 塞进错误

内部可保留 bounded diagnostic；public error 和日志只输出 safe message/metadata。

### 9.6 为未来建立大 trait hierarchy

Phase 1 不复制完整 SessionBackend/Orchestrator。只有第二个实际 backend 需要时再抽象。

### 9.7 顺手迁移 Gemini

没有决策时不做。Gemini compatibility 是独立范围。

### 9.8 测试用真实 OpenCode

自动测试用 fake ACP；真实 OpenCode 只用于最后 smoke。

## 10. Diff Budget

建议单任务：

- production code <= 300 changed lines；
- test code按需要，但一个 task 不超过 5 files；
- 超过约 600 total changed lines 时应检查是否需拆分。

T11 等协议集成任务可能自然偏大，仍优先拆 fixture 与 backend 两个 commit，保持同一个 Task checkpoint。

## 11. Commit 规则

每个通过验证的 Task 一个聚焦 commit，中文 Conventional Commit：

```text
feat: 建立内置 Agent 注册表
feat: 增加 ACP 托管进程运行时
feat: 接入 ACP 会话执行后端
test: 补充 ACP 进程树清理覆盖
refactor: 将 OpenCode 翻译迁移到 Agent Runtime
```

不要把 docs、依赖、process、protocol、frontend 混成一个 commit。

## 12. 交接状态文件

长周期执行可在 `07-implementation-plan.md` 任务旁记录：

```markdown
Status: complete
Commit: <hash>
Verification:
- `command` PASS
Notes:
- ...
```

若不希望修改计划文档，可在每次会话输出保存相同内容。不得只写“已完成”。

## 13. Final Audit Prompt

```text
对 ACP Agent Execution Runtime Phase 1 做最终只读审计。

读取全部 01–07 Spec、git diff/main range、测试结果。
逐项核对 AC-001..AC-008 和 06 的禁止项。
特别搜索：
- Translation 范围的 opencode run execution call site
- `AiCliRuntime::Opencode` 是否仅存在于已明确排除、等待重写的 Memory legacy 链路
- Vendor match
- raw prompt/result/stderr logging
- child.kill without tree cleanup
- timeout dropping future without cancellation
- Tauri direct invoke outside frontend services
- hand-edited contract

输出：
1. Blocking findings
2. Acceptance matrix PASS/FAIL/EVIDENCE
3. Residual risks
4. Real OpenCode smoke checklist status
5. Go/No-Go conclusion
```
