# T01：建立通用 Session Event 与 transient projection

## Outcome

Agent Execution 可以用一套协议无关事件表达增量文字、processing/thinking、工具、终态和错误，并在有界内存投影中稳定合并、去重和读取。

## Blocked by

None。以 Issue #21 和当前 `AiExecutionProgressSink`/TaskRuntime 为输入。

## Read

- Contracts：TW-D02、TW-E01～TW-E06、TW-A03、TW-B05。
- Seams：S03、S08、S18；Tests TS02、TS07、TS08。
- Gates：G0、G2、G5。

## Red test first

向 projection 依次送入重复 text delta、out-of-order sequence、tool start/result、replay item 和 live item；旧实现无法形成一个有序、无重复、可读取且有界的 Session snapshot。

## Execution steps

1. 定义协议无关事件 envelope、item identity、sequence、event family 和 replay/live 标记。完成标准：类型不引用 TeamTask、ACP 或 Antigravity 原生类型。
2. 建立 thread-safe transient projection/cache，支持 append/update、snapshot、dedup 和有界 eviction。完成标准：重复事件幂等，delta 更新原 item，超过上限按明确策略释放。
3. 将事件 sink 接入现有 execution progress 链，但保持 phase/cleanup 和 final result 兼容。完成标准：未提供 event sink 的现有调用行为不变。
4. 对 Debug/log/snapshot 做 redaction 审核。完成标准：正文和 payload 不进入 Debug、operation log 或 durable task detail/result。

## Acceptance

- [ ] 通用 event contract 覆盖 TW-E03 事件家族并显式区分 replay/live。
- [ ] event identity/sequence 能隔离并发 execution，重复和乱序输入不产生重复 logical item。
- [ ] transient snapshot 可供订阅重连读取，缓存有明确上限且应用退出不持久化。
- [ ] 现有 final-text、phase、cleanup consumer 无行为回归。
- [ ] 测试 marker 不出现在日志、durable task snapshot 或数据库。

## Non-goals

ACP/Antigravity 翻译、Provider history、Team member turn API、Tauri events 和前端组件。

## Ticket-specific stop

如果通用事件必须引用 Vendor 类型、必须持久化正文才能去重，或需要替换 TaskRuntime Authority，按 Stop protocol 报告。
