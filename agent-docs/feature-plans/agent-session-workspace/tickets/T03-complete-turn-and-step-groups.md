# T03：还原完整 Turn、Thinking 与查看步骤

## Outcome

共享 Timeline 按真实顺序展示 user request、assistant、thinking/processing、多个 `查看步骤 · N` 分组和 terminal；live/replay/out-of-order 更新不会重复或重排逻辑内容。

## Blocked by

T02。

## Scope

- Requirements：R-SES-003–006、R-CHAT-001–007、R-NFR-003/004/008
- Contracts：C-020–C-024、C-040–C-073、C-090–C-103
- Seams：S-BE-01–06、S-FE-01–03、S-TEST-01–03
- Gates：G-01–G-05、G-11

## Preflight

1. 确认 T02 已 verified，并用其 fixture 复核一个 logical tool item 的 identity。
2. 读取 `02-public-contract.md` 的 state、merge、group 与 bounds 合同。
3. 记录 projection/reducer/timeline targeted tests baseline。
4. 冻结 `06-verification-matrix.md` 的 canonical complete fixture，Red/Green 使用同一序列。

## Red tests

使用 canonical complete fixture：

1. user → assistant → thinking → tools → assistant → tool → terminal 顺序保持。
2. 连续 tools 形成一个 group；assistant 分隔后形成第二个 group。
3. `N` 等于 logical tools 数。
4. running group 默认展开，completed replay 默认折叠，failed 默认展开。
5. user manual expansion 在 delta 后保持。
6. duplicate replay/live 只显示一次；相同 sequence live 胜出。
7. terminal text 与 assistant 相同不重复。
8. oversized UTF-8 内容产生 truncation metadata 与可见标记。
9. conflict terminal 产生 notice，状态不来回切换。

## Implementation steps

1. 完成 AgentSessionView/item union 所需的 request、assistant、thinking、processing、terminal、notice/error 字段。
2. 为 Session entry 增加 metadata/state/timestamps/retention counters。
3. 实现契约中的 dedupe、order、delivery priority、state monotonicity、head/tail truncation。
4. 让 request 在 execute 前成为真实 item；Team interactive 先接入实际 user input。
5. 建立 frontend 纯 reducer：ordered items → turns → step groups。
6. 实现 Assistant、Thinking、Processing、StepGroup、Terminal/Error renderer。
7. 将 expansion state 绑定 sessionRef + item identity，和 server snapshot 分离。
8. 为 unavailable/partial/truncated 增加 typed UI 状态。
9. 以 canonical fixture 同时跑 backend 与 frontend tests。

## Acceptance criteria

- [ ] canonical fixture 的所有 item 按 sequence 可见。
- [ ] StepGroup 分段与计数符合契约。
- [ ] replay/live/delta/snapshot 合并无重复。
- [ ] terminal 单调且相同正文不重复。
- [ ] Provider missing/partial/truncated 被区分。
- [ ] session bounds 不因丰富 payload 失效。
- [ ] Debug/tracing 仍 redacted。

## Verification

- Rust table/property tests：dedupe/order/state/bounds；
- frontend reducer/component tests；
- canonical fixture snapshot；
- `cargo test --workspace` 的相关 packages；
- `pnpm typecheck` 与 targeted tests；
- 人工逐项核对 canonical sequence。

## Non-goals

- 最终 Chat Shell/Composer 视觉；
- terminal/diff/image 高级 renderer；
- Team parallel；
- Memory Task link。

## Handoff

T04、T05、T06、T09 进入 frontier；T09 开工时按提交 `dcb0fcbf` 之后的当前代码重新验证 #33 接缝。
