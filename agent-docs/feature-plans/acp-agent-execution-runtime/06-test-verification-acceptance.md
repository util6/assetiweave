# 验证计划：ACP Agent 执行运行时 (Test Verification Acceptance)

| 字段 | 值 |
|---|---|
| 状态 | Verified |
| 对应需求 | `01-product-requirements.md` |
| 实施任务 | `07-implementation-plan.md` |

## 1. 测试原则

1. 先用 fake ACP 和 fixture process 冻结行为，再连接真实 OpenCode。
2. 业务单测不得依赖用户机器的 OpenCode、网络或认证。
3. Process tree cleanup 必须用真实子进程验证，不能只 mock kill function。
4. Engine、Tauri、Frontend 各验证自己的 adapter，不复制 core protocol 测试。
5. 每个 implementation task 都必须有定向验证；最终再跑全量质量门。

## 2. 测试层次

```text
Pure unit
  -> Process fixture integration
  -> Fake ACP protocol integration
  -> AgentExecutor integration
  -> AppService/Engine contract
  -> Tauri task registry
  -> Frontend provider/component
  -> Real OpenCode smoke
```

## 3. Requirements Traceability Matrix

| Requirement | Test ID | 层次 |
|---|---|---|
| FR-REG-001/002 | REG-01..05 | unit |
| FR-REG-003..005 | REG-06..10 | unit/integration |
| FR-EXE-001..005 | EXE-01..10 | unit/integration |
| FR-PROC-001..004 | PROC-01..15 | real process fixture |
| FR-ACP-001..009 | ACP-01..20 | fake ACP integration |
| FR-EVT-001..004 | EVT-01..12 | unit/fake ACP |
| FR-LIFE-001..004 | LIFE-01..12 | integration |
| FR-TASK-001..005 | TASK-01..15 | Rust/frontend |
| FR-TR-001..005 | TR-01..12 | AppService/frontend |
| FR-API-001..003 | API-01..08 | Engine/contract |
| NFR-001 | PROC-03..15、LIFE-01..12、TAURI-07/08、Real Smoke 7/10/11 | process/lifecycle |
| NFR-002 | REG lookup、EXE-05..08、TAURI-01/02 | timing/concurrency |
| NFR-003 | SEC-01..10、ACP-04/07、Real Smoke 12 | policy/redaction |
| NFR-004 | PROC platform helpers、workspace tests、目标平台构建矩阵 | portability |
| NFR-005 | REG-07、模块边界审查、Translation adapter tests | architecture |
| NFR-006 | phase/process/output/cleanup safe metadata、SEC-01..10、Real Smoke 12 | observability |

## 4. Registry Tests

建议位置：

```text
src-tauri/src/backend/agents/registry.rs
src-tauri/src/backend/agents/types.rs
```

| ID | 场景 | 断言 |
|---|---|---|
| REG-01 | builtin registry 构造 | 存在 `opencode` |
| REG-02 | OpenCode definition | protocol=ACP, command=opencode, args=[acp] |
| REG-03 | 重复 ID | 构造失败，错误稳定 |
| REG-04 | 空 command | definition validation 失败 |
| REG-05 | 非法 agent id | parse 失败 |
| REG-06 | get unknown | None / AgentNotFound |
| REG-07 | route | 只读取 protocol，不读取 ID |
| REG-08 | executable missing | unavailable reason=not_found |
| REG-09 | probe timeout/fail | reason 正确分类 |
| REG-10 | model discovery | args 来自 definition，不由业务拼接 |

关键防回归测试：构造一个 id=`not-opencode`、protocol=ACP 的 fake definition，证明 executor 仍走 ACP。

## 5. Process Fixture Tests

建议位置：

```text
src-tauri/src/backend/agents/process.rs
src-tauri/src/backend/host_process.rs
```

Fixture 可复用当前 test binary，以环境变量选择模式。

| ID | Mode | 断言 |
|---|---|---|
| PROC-01 | echo stdio | 首次 `take_stdio` 可读写 |
| PROC-02 | echo stdio | 第二次 `take_stdio` 返回错误 |
| PROC-03 | stderr burst | 全程不 deadlock，tail cap 生效 |
| PROC-04 | stderr UTF-8 broken | lossy diagnostic 不 panic |
| PROC-05 | immediate exit | exit watch 收到 status |
| PROC-06 | long sleep | graceful deadline 后 force kill |
| PROC-07 | ignore SIGTERM | SIGKILL group 收敛 |
| PROC-08 | grandchild | kill 后 parent/child/grandchild 均不存在 |
| PROC-09 | launcher exits | 记录的 group 仍被清理 |
| PROC-10 | repeated terminate | 幂等，无二次 wait panic |
| PROC-11 | missing stdout | spawn error path cleanup |
| PROC-12 | cwd missing | classified error |
| PROC-13 | env overlay | 指定 key 可见，log 不含 value |
| PROC-14 | safe preview | 不含 arg body/env value |
| PROC-15 | wait timeout diagnostic | 返回 cleanup report，不挂死测试 |

Unix 可通过 `kill(pid, 0)` 或 `/proc`/`ps` 的平台 helper 验证进程不存在；测试本身必须有总 timeout。

Windows test 使用 `taskkill`/process query 兼容 helper，避免 Unix-only shell 脚本。

## 6. Fake ACP Agent

### 6.1 目标

Fake agent 必须验证真实 stdio framing、SDK schema、request/notification 顺序和 responder。它不能只是 mock `AcpProtocol` trait。

### 6.2 建议实现

测试 binary 模式：

```rust
#[test]
fn fake_acp_process_entry() {
  if let Ok(mode) = env::var("ASSETIWEAVE_FAKE_ACP_MODE") {
    run_fake_agent(mode);
    return;
  }
}
```

父测试通过 `current_exe --exact ... --nocapture` 启动。

若 SDK 提供 agent-side test helper，优先用 typed helper；否则 fixture 可实现最小 NDJSON，但每个 frame 必须由 ACP schema fixtures 校验。

### 6.3 Modes

| Mode | 行为 |
|---|---|
| happy | init/new/text chunks/prompt response/close |
| chunked | 多个 assistant text chunk，含 Unicode |
| initialize_timeout | 不回应 initialize |
| initialize_error | 返回 protocol error |
| new_error | initialize 成功，new 失败 |
| model_ok | 记录 model 后成功 |
| model_reject | 返回 invalid model |
| model_timeout | 不回应 model |
| permission | prompt 中请求 permission |
| tool_call | 发送 tool call update |
| thinking | thought + assistant text |
| wrong_session | 先发其他 session chunk，再正确 chunk |
| empty | prompt 完成但无 assistant text |
| oversized | 超过 1 MiB chunk |
| disconnect | prompt 中断开 stdout |
| exit_during_prompt | child 非零退出 |
| close_error | prompt 成功但 close error |
| close_hang | close 不回应 |
| cancel_wait | prompt 等待 cancel，收到后确认 |
| late_chunk | completion 边界附近发送最后 chunk |

## 7. ACP Protocol Tests

| ID | 场景 | 断言 |
|---|---|---|
| ACP-01 | connect happy | initialize response 缓存 |
| ACP-02 | client info | name/version 非空 |
| ACP-03 | capability declaration | 不宣称 terminal/MCP/FS |
| ACP-04 | init timeout | 10s 可配置测试值后返回 HandshakeTimeout |
| ACP-05 | new session | cwd 与空 MCP 正确发送 |
| ACP-06 | prompt | typed text block 正确 |
| ACP-07 | cancel during prompt | prompt 未完成时 cancel frame 仍发送 |
| ACP-08 | close capability true | 发送 close |
| ACP-09 | close capability false | 跳过 close |
| ACP-10 | shutdown | actor 收敛、alive=false |
| ACP-11 | permission | responder 选择 reject |
| ACP-12 | log redaction | prompt 不出现在 captured log |
| ACP-13 | model ok | prompt 前收到 model |
| ACP-14 | model reject | 不发送 prompt |
| ACP-15 | model timeout | 不发送 prompt |
| ACP-16 | disconnect | classified error |
| ACP-17 | actor channel close | 不死锁 |
| ACP-18 | repeated shutdown | 幂等 |
| ACP-19 | unknown standard event | ignore，不 crash |
| ACP-20 | no dialect shim | 标准 frame 直接进入 SDK |

测试 timeout 用 50–500 ms 的 override，避免 suite 真实等待 10/180 秒。

## 8. Aggregator Tests

| ID | 输入 | 断言 |
|---|---|---|
| EVT-01 | one text chunk | exact text |
| EVT-02 | multiple chunks | 顺序拼接 |
| EVT-03 | Unicode 分块 | 无乱码 |
| EVT-04 | thinking + text | 只返回 text |
| EVT-05 | wrong session | 丢弃 wrong chunk |
| EVT-06 | permission | CancelAndFail(PermissionDenied) |
| EVT-07 | tool call | CancelAndFail(ToolUseDenied) |
| EVT-08 | empty | EmptyOutput |
| EVT-09 | whitespace only | EmptyOutput |
| EVT-10 | exactly cap | 成功 |
| EVT-11 | cap + 1 | OutputLimit |
| EVT-12 | late final chunk | completion boundary 包含该 chunk |

## 9. Executor/Lifecycle Tests

| ID | 场景 | 断言 |
|---|---|---|
| EXE-01 | invalid request | 未调用 resolver/spawn |
| EXE-02 | unknown agent | AgentNotFound |
| EXE-03 | protocol ACP | fake backend 被调用 |
| EXE-04 | protocol Native | UnsupportedProtocol |
| EXE-05 | queue concurrency=2 | 同时最多两个 process |
| EXE-06 | queued cancel | 从未 spawn |
| EXE-07 | queue full policy | stable QueueFull 或 bounded wait |
| EXE-08 | total timeout | cancellation 被触发 |
| EXE-09 | success | result metadata 正确 |
| EXE-10 | requested model | 不误标为 confirmed model_used |

| ID | 场景 | 断言 |
|---|---|---|
| LIFE-01 | success | close/shutdown/wait/remove workspace |
| LIFE-02 | init fail | kill/wait/remove workspace |
| LIFE-03 | new fail | shutdown/kill/wait |
| LIFE-04 | model fail | 不 prompt；close/kill |
| LIFE-05 | prompt fail | cancel/close/kill |
| LIFE-06 | tool | cancel + ToolUseDenied |
| LIFE-07 | user cancel | terminal=cancelled after cleanup |
| LIFE-08 | timeout | terminal=failed/timeout after cleanup |
| LIFE-09 | close fail | process 仍被 kill/reaped |
| LIFE-10 | cleanup repeat | 幂等 |
| LIFE-11 | result ready + cleanup fail | 不返回 success |
| LIFE-12 | panic boundary | task 收敛 failed，child 被清理或 high-priority diagnostic |

## 10. BackgroundTaskRegistry Tests

建议扩展现有：

```text
src-tauri/src/adapters/tauri/background_tasks.rs
```

| ID | 场景 | 断言 |
|---|---|---|
| TASK-01 | begin | snapshot queued，含 cancellation |
| TASK-02 | phase update | updated_at 与 phase 更新 |
| TASK-03 | finish success | result 有值，cleanup 已完成 |
| TASK-04 | finish fail | stable error view |
| TASK-05 | cancel queued | 最终 cancelled，不 spawn |
| TASK-06 | cancel running | cancellation flag=true |
| TASK-07 | cancel terminal | 幂等 |
| TASK-08 | unknown id | not found |
| TASK-09 | list | 不含 prompt |
| TASK-10 | retention time | terminal 过期被 prune |
| TASK-11 | retention count | 只保留最近 100 terminal |
| TASK-12 | running retention | 不淘汰 running |
| TASK-13 | has_running_tasks | AI task 被计入 |
| TASK-14 | cancel all on exit | 所有 running token 被设置 |
| TASK-15 | poisoned lock handling | 返回 error，不 panic |

## 11. Translation/AppService Tests

| ID | 场景 | 断言 |
|---|---|---|
| TR-01 | opencode mapping | agent_id=opencode |
| TR-02 | opencode execution | args 不含 `run` |
| TR-03 | result mapping | text -> translated_text |
| TR-04 | prompt max | 200k 业务上限保持 |
| TR-05 | model validation | invalid 在 spawn 前拒绝 |
| TR-06 | connection test | 使用 AgentExecutor |
| TR-07 | availability | 使用 Registry probe |
| TR-08 | model list | 使用 Definition discovery args |
| TR-09 | Gemini | 现有参数行为不回归 |
| TR-10 | Google/Apple | reserved error 不变 |
| TR-11 | Conversation save | Runtime 不直接写 DB |
| TR-12 | injected fake runtime | AppService tests 不需真实 CLI |

## 12. Tauri Adapter Tests

| ID | 场景 | 断言 |
|---|---|---|
| TAURI-01 | start | 快速返回 snapshot |
| TAURI-02 | start | 不获取 global app lock |
| TAURI-03 | phase | 每次 emit full snapshot |
| TAURI-04 | terminal | executor cleanup 后 emit |
| TAURI-05 | get/list | snapshot 正确 |
| TAURI-06 | cancel | registry token 被触发 |
| TAURI-07 | app close | running task 触发 close prompt |
| TAURI-08 | confirmed exit | cancel all + bounded wait |

若 Tauri command 难以纯单测，应把大部分逻辑抽到 adapter helper，command 只做参数/State wiring。

## 13. Engine/CLI Tests

### 13.1 Contract

- method `conversation.card.translation.run` 仍存在；
- request fields provider/cli/model/prompt 保持；
- result `translated_text` 保持；
- generated contract 无手工 diff。

### 13.2 Engine integration

通过 fake AgentDefinition 覆盖：

- execute success；
- unavailable；
- timeout；
- model fail；
- process interruption cleanup。

### 13.3 Commands

```bash
pnpm cli:contract
go vet -C cli ./...
go test -C cli -race ./...
pnpm cli:test:e2e
```

## 14. Frontend Tests

### 14.1 Service

文件：

```text
frontend/src/services/cardTranslation.test.ts
```

- start invoke command/params；
- get/list/cancel invoke；
- browser fallback；
- request/result types；
- 原同步 command 不再被正常 translation helper 调用。

### 14.2 Provider

建议：

```text
frontend/src/app/backgroundTasks/AiExecutionTaskProvider.test.tsx
```

- mount 时 list；
- event merge；
- duplicate event idempotent；
- running 时 poll；
- terminal 后停止不必要 polling；
- missed event 由 poll 恢复；
- unmount 清理 listener/timer；
- cancel action。

### 14.3 ConversationContentCards

- start 后当前 block 显示 phase；
- 其他 block/button 可点击；
- cancel；
- success 保存 translation；
- save failure 与 AI task success 分开显示；
- task failed 显示 error；
- cancelled 不显示 failure banner；
- page unmount 不自动 cancel。

### 14.4 Global indicator

- running count；
- phase；
- cancel；
- unrelated navigation enabled；
- app close test 能看到 background task。

## 15. Security/Redaction Tests

| ID | Secret marker | 搜索位置 | 断言 |
|---|---|---|---|
| SEC-01 | prompt secret | captured logs | 不存在 |
| SEC-02 | result secret | captured logs | 不存在 |
| SEC-03 | env value secret | spawn preview/log | 不存在 |
| SEC-04 | arg secret | spawn preview/log | 不存在 |
| SEC-05 | stderr secret | public error JSON | 不存在 |
| SEC-06 | auth token | task snapshot | 不存在 |
| SEC-07 | raw tool input | runtime event/log | 不存在 |
| SEC-08 | prompt | task snapshot | 无该字段 |
| SEC-09 | temp path | public DTO | 不存在 |
| SEC-10 | full ACP JSON | info log | 不存在 |

建议使用唯一 marker，如 `DO_NOT_LOG_7f12...`，测试结束对 captured output 做全文搜索。

## 16. 真实 OpenCode 冒烟测试 (Real OpenCode Smoke Test)

### 前置

```bash
opencode --version
opencode auth login   # 仅当本机尚未认证
```

### 步骤

1. `pnpm tauri:dev`。
2. Settings 检查 availability。
3. 刷新 model list。
4. 不指定 model 翻译短文本。
5. 指定有效 model 翻译短文本。
6. 指定无效 model，确认不静默回退。
7. 翻译长文本并中途 cancel。
8. 同时启动 3 个 block，确认最多 2 个 running。
9. 运行中切换页面，确认 global indicator。
10. 运行中关闭 app，确认提示和 cleanup。
11. 使用 `ps`/Activity Monitor 确认无残留 `opencode acp`。
12. 检查应用日志无输入/输出内容。

### 证据记录

记录：

- app commit；
- OpenCode version；
- OS；
- model（可脱敏）；
- 每步 pass/fail；
- process cleanup 观察；
- 日志 redaction 观察。

不要把真实 prompt/result 放入提交。

## 17. 质量门命令

### 定向开发

```bash
cargo test --manifest-path src-tauri/Cargo.toml backend::agents
cargo test --manifest-path src-tauri/Cargo.toml backend::ai_execution
cargo test --manifest-path src-tauri/Cargo.toml backend::card_translation
```

### Rust 全量

```bash
cargo fmt --all -- --check
cargo test --workspace
```

### Frontend 全量

```bash
pnpm typecheck
pnpm test
pnpm build
```

### Go/Engine

```bash
pnpm cli:contract
go vet -C cli ./...
go test -C cli -race ./...
pnpm cli:test:e2e
```

## 18. 完成证据模板

每个任务完成时写：

```markdown
### Task X verification

- Changed files: ...
- Tests added: TEST-ID ...
- Commands:
  - `...` -> PASS
- Behavioral evidence:
  - ...
- Known limitations:
  - ...
- Spec deviations:
  - None / link to approved decision
```

## 19. 最终验收禁止项

出现以下任一项不得标记完成：

- 只有真实 OpenCode 手测，没有 fake ACP 自动测试。
- 只有 mock process，没有真实 process tree test。
- cancel 只 drop future，没有 protocol cancel 与 kill/wait。
- cleanup 失败仍返回 success。
- frontend 在页面卸载时丢失 task。
- Engine 与 Tauri 使用不同 execution 实现。
- OpenCode 正常 Translation 仍调用 `opencode run`。
- 日志包含 prompt、result、raw stderr 或 env value。
