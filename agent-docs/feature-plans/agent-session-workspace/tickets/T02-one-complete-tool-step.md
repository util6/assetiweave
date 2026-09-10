# T02：打通一条完整 Tool Step

## Outcome

用户在共享 Agent Session 工作区中展开一条 ACP 工具调用，能够看到真实工具名称、状态、Input 和 Output；同一调用的 start/update/result 只呈现一行。

## Blocked by

T01。

## Scope

- Requirements：R-SES-001、R-SES-002、R-CHAT-005、R-NFR-002、R-NFR-008
- Contracts：C-040–C-043、C-080–C-087、C-100
- Seams：S-BE-01、S-BE-03、S-BE-05–07、S-FE-01–03、S-FE-05、S-TEST-01/02
- Gates：G-01–G-05、G-11

## Preflight

1. 确认 T01 已 verified，并读取其 handoff 和实际提交 diff。
2. 重新定位 ACP Tool event、Session projection、Tauri service/schema 与共享 renderer 接缝。
3. 记录相关 Rust/TypeScript targeted tests 的 baseline；失败必须先归因。
4. 固定一条最小 tool fixture，禁止在本卡顺带实现 thinking group、terminal 或 diff renderer。

## Red tests

Backend fixture：

1. ACP ToolCall start 携带 name + raw input；update 携带 running detail；result 携带 raw output。
2. Session snapshot 只有一个 logical tool item。
3. item 保留 typed input/output，state=succeeded。
4. Debug output 不包含 input/output 文本。

Frontend fixture：

1. service/schema 接受 typed tool payload。
2. shared Workspace 显示 Step row。
3. 展开后存在 Input/Output label 和实际内容。
4. 三个 event/snapshot 更新不产生三行。

## Implementation steps

1. 在 SessionEvent/Item 旁 additive 增加最小 Tool Step typed payload；旧 detail 字段保留兼容。
2. ACP bridge 映射 raw input/output 与工具状态；稳定 `tool_call_id → item_id`。
3. Projection materialize start/update/result 为同一 item；保持 state monotonicity。
4. 保持 Debug redaction；禁止把 payload 加入 Task detail/logging。
5. 扩展 Tauri DTO/TypeScript/Zod/service fixture。
6. 移除当前 shared path 对 tool detail 的清空；旧 Team serializer 如仍需要兼容，仅在旧 adapter 内处理。
7. 在共享 Timeline 中实现一条 Step row + detail，暂不实现多步骤分组。
8. 运行 schema/contract generation（仅当公开 Engine surface 变化）。

## Acceptance criteria

- [ ] ACP input/output 从 Provider 到 UI 端到端可见。
- [ ] start/update/result 合并为一个 logical item。
- [ ] Tool 状态不会从 terminal 回退 running。
- [ ] Provider 缺失 input/output 时不显示空 section。
- [ ] Debug/tracing/task view 不含正文。
- [ ] 旧 Team 操作仍工作。

## Verification

- targeted ACP mapping tests；
- session event projection tests；
- Rust DTO serialization tests；
- TS schema/store/component tests；
- `cargo fmt --all -- --check`；
- `pnpm typecheck`；
- 手工运行一条真实 ACP tool call 并展开 Input/Output。

## Non-goals

- 多 StepGroup；
- Thinking/完整 Turn；
- terminal/file/diff/image 专用 renderer；
- Memory wiring；
- Legacy 删除。

## Handoff

完成后 T03 进入 frontier。列出仍以旧 detail/text 表达的 Provider content 类型。
