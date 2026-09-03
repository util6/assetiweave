# T04：实现 Antigravity Direct-CLI Session 与 live events

## Outcome

Antigravity 每轮启动一个 `agy` CLI 进程，捕获真实 Conversation ID 作为 Resume Anchor，并把 `stream-json` 转为通用 Session Event。

## Blocked by

T01、T02。

## Read

- Contracts：TW-E01～TW-E05、TW-A03、TW-A05、TW-A08、TW-B05。
- Seams：S03、S06、S07、S17、S18；Tests TS02、TS04、TS08。
- Gates：G0、G2、G6。

## Red test first

fake `agy` 首轮 init 返回 `conversation_id=REAL_ID`，第二轮必须带 Provider resume 参数；旧 Native backend 保存 synthetic ID，无法满足断言。

## Execution steps

1. 在 Provider 基础设施内建立 Antigravity Direct-CLI Adapter，复用 host process、limits、cancel 和 cleanup。完成标准：Team/AppService 无 Agent ID 分支，每轮进程结束后被 bounded reap。
2. 增量解析 `stream-json`，从非空 init 或 result 捕获真实 Conversation ID。完成标准：空 ID 不创建/覆盖 binding，已有有效 anchor 在失败 turn 后保持。
3. 使用真实 anchor 构造后续 `agy --conversation` turn；新会话使用 Provider 的 new-conversation 语义。完成标准：fake argv 精确证明首轮和 resume 轮差异。
4. 把 text、step/tool、result、notice/error 和可用 usage/processing 元数据翻译为通用事件。完成标准：step identity 由 Conversation ID 命名空间隔离，unknown/malformed 输入产生受控结果。
5. 保持现有 Antigravity model discovery 和 OneShot 使用者。完成标准：既有 model discovery/Native tests 全绿。

## Acceptance

- [ ] binding 只保存 Provider 返回的非空真实 Conversation ID，不保存 synthetic ID。
- [ ] 每轮一个 CLI 进程；resume turn 使用已保存 ID，进程生命周期不等于 Session 生命周期。
- [ ] text、tool step/result、terminal/error 生成稳定通用事件。
- [ ] 空 ID、authentication failure、unknown event、malformed line、cancel、timeout 都有 deterministic tests。
- [ ] Adapter 差异不泄漏到 Team、transport 或 frontend。

## Non-goals

Provider transcript history、启用 Team eligibility、chat UI、真实登录 smoke。

## Ticket-specific stop

如果无法从 Provider 输出取得真实 ID、需要让 Team 保存 `agy` 参数，或测试依赖真实账号，按 Stop protocol 报告。
