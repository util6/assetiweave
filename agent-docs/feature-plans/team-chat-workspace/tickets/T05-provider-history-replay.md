# T05：实现 Provider History Replay 与 Antigravity transcript reader

## Outcome

Agent Execution 可以按 member binding 渐进读取 Provider-owned history；Antigravity 优先完整 transcript、降级简化 transcript，并在能力完整后正式满足 Team 准入。

## Blocked by

T03、T04。

## Read

- Contracts：TW-D01～TW-D04、TW-E04、TW-R03～TW-R06、TW-A01～TW-A07。
- Seams：S03～S07、S16、S17；Tests TS01、TS03、TS04、TS08。
- Gates：G0、G2、G5、G6。

## Red test first

给定真实 anchor 对应的临时 Antigravity Provider store，同时提供 `.system_generated/logs/transcript_full.jsonl` 与 `transcript.jsonl`；History Replay 必须选择完整文件、产生 replay events 且 Conversation/Team 正文表零写入。旧 runtime 无此能力。

## Execution steps

1. 在 Session Adapter 边界定义增量 History Replay、fidelity 和 partial/unavailable 结果。完成标准：Team 不接收 Provider 文件路径或原生记录。
2. 将 ACP replay 接入该 port；沿用 T03 的通用事件和 replay 标记。完成标准：无 live 副作用且 final-text 兼容路径保持。
3. 实现 Antigravity Provider store 定位和只读解析，完整 transcript 优先、简化 transcript fallback。完成标准：以真实 Conversation ID 选择会话，稳定排序并限制读取大小。
4. 对 missing、malformed、部分记录和未知块返回结构化 fidelity/status。完成标准：有效文本仍可显示，损坏不会伪装 ready/full。
5. 更新 Antigravity semantic capabilities 和 Team eligibility。完成标准：只有 Resume、text Replay、Live Events 三项真实可用后才宣告 Team-ready。
6. 锁定依赖方向。完成标准：Agent Execution 不调用 Conversation application/repository；共享 fixtures 可以被两个领域测试使用。

## Acceptance

- [ ] ACP 与 Antigravity 通过同一 History Replay port 输出 replay Session Events。
- [ ] Antigravity 使用真实 anchor，完整 transcript 优先，简化 transcript 正确 fallback。
- [ ] missing/malformed/partial history 返回明确 fidelity/status，不清除有效 binding。
- [ ] replay 不执行工具、不写 mailbox/TeamTask，不创建或修改 Conversation facts。
- [ ] Antigravity 完成后通过语义能力准入，准入代码无 Agent ID 特判。
- [ ] tests 不读取真实用户目录、登录态或网络。

## Non-goals

replay 调度优先级、Tauri transport、frontend merge、chat shell。

## Ticket-specific stop

如果必须从 Conversation repository 读取历史、需要复制 Aion 的 messages 表，或无法在临时 Provider store 中测试，按 Stop protocol 报告。
