# T03：把 ACP runtime events 接入通用 Session Event

## Outcome

ACP Agent 在保持现有 final-text 与权限策略的同时，实时输出通用 text、processing/thinking、tool 和 terminal events。

## Blocked by

T01、T02。

## Read

- Contracts：TW-E01～TW-E05、TW-A04、TW-A08、TW-B05。
- Seams：S03、S04、S05、S18；Tests TS02、TS03、TS08。
- Gates：G0、G2、G5。

## Red test first

fake ACP process 发出 text chunks、thought update、tool activity 和 terminal result；旧 aggregator 只累积 final text/计数，event sink 收不到可稳定合并的 typed events。

## Execution steps

1. 将 ACP SessionUpdate 翻译为通用 Session Event，并生成 session/turn/item 范围内稳定 identity。完成标准：连续 text chunk 更新同一 response item，tool result 更新原 tool item。
2. 在 ACP execution loop 同时馈送 event sink 与现有 final-text aggregator。完成标准：旧 caller 的结果文本、错误和 cleanup 不变。
3. 标记 ACP replay 输出并阻断 live 副作用。完成标准：replay tool history 只显示，不触发权限请求、执行或 Team mutation。
4. 保持现有权限策略。完成标准：本卡不因为“显示 tool activity”扩大可执行工具范围。

## Acceptance

- [ ] fake ACP 的 text/thought/tool/terminal 顺序形成预期通用事件。
- [ ] text delta 与 tool lifecycle 使用稳定 item identity，无重复卡片。
- [ ] replay events 全部标记 replay 且无工具、mailbox、TeamTask 副作用。
- [ ] 没有 event consumer 时 final text、错误、取消、timeout 和 cleanup 与基线一致。
- [ ] Provider 不提供 thought 正文时不合成隐藏推理。

## Non-goals

改变 ACP 工具权限、Team API、Antigravity、frontend store 或 chat UI。

## Ticket-specific stop

如果必须修改 Team 代码理解 ACP event，或为了 rich UI 放宽现有 tool permission，按 Stop protocol 报告。
