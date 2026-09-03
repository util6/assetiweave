# T02：建立语义 Session 能力与 Team 准入

## Outcome

Agent Market、安装记录、Runtime 和 Team 校验使用 Resume、History Replay、Live Events 语义能力，而不是协议名决定成员资格。

## Blocked by

T01。Live Events capability 必须指向已存在的通用事件契约。

## Read

- Contracts：TW-A01～TW-A03、TW-A08、TW-D04、TW-B01。
- Seams：S01、S03、S07、S12；Tests TS01、TS02、TS05。
- Gates：G0、G2、G3。

## Red test first

构造一个 Native/Direct-CLI definition，声明 Resume、text History Replay 和 Live Events 后应通过 Team Leader/Teammate 准入；任意缺失一项都应返回结构化缺失能力，而不是按 protocol 拒绝。

## Execution steps

1. 扩展 capability declaration 和持久化/reload，区分基础 replay 与 thought/tool rich fidelity。完成标准：catalog → install → database reopen → runtime definition 值不丢失。
2. 收口 Team create/update 校验，使所有成员使用同一语义准入；返回可定位缺失项。完成标准：ACP 和 Native 输入执行相同规则。
3. 更新公开 Agent Market DTO/schema 和生成契约所需表面。完成标准：前后端读取同一 capability facts，无第二套硬编码列表。
4. 保持未完成 Adapter 的 Agent 不宣告能力。完成标准：Antigravity 在 T04/T05 完成前不会被错误标为 Team-ready。

## Acceptance

- [ ] Team 所有成员至少需要 Resume、user/assistant text Replay 和 Live Events。
- [ ] thought/tool rich fidelity 独立声明，不影响基础准入但可被 UI 读取。
- [ ] 准入不检查 protocol、Vendor 或 Agent ID。
- [ ] capability 在 catalog、安装持久化、reload、AppService 与前端 DTO 中一致。
- [ ] 缺失能力返回稳定错误码/字段，便于后续 UI 解释。

## Non-goals

实现 ACP/Antigravity event translation、启用 Antigravity、聊天页面或 history UI。

## Ticket-specific stop

如果需要在 Team 校验中添加 `antigravity`/`native` 特判，或 frontend 维护独立 eligibility 表，按 Stop protocol 报告。
