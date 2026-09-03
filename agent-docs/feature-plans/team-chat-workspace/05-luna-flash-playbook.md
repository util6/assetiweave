# Team 聊天工作台：Luna / Flash 执行手册

## 1. 执行单位

一轮只完成一张 `tickets/TNN-*.md`。Luna 与 Flash 使用相同契约；Flash 每个 Execution Step 后立即运行目标测试，Luna 也不得跨 Ticket 扩展。

控制范围：

- 一张卡建议不超过 8 个 production/test 文件；预计超过 10 个先停止并给切分点。
- production diff 建议小于 500 行；总 diff 超过 900 行先评估拆卡。
- 一个提交只表达当前卡 Outcome。
- 当前代码已经满足某项 Acceptance 时，用测试证明并保留实现，不为“做出改动”而重写。

## 2. 开工工作卡

修改前必须输出：

```text
TICKET: TNN — title / Issue #21
BLOCKERS: 逐项证据
CONTRACTS: 本轮 Contract IDs
SEAMS: 本轮生产与测试 Seam IDs
AUTHORITY: Provider正文 / Team facts / Agent binding / transient projection 中本卡涉及者
CALL PATH: 公开入口 → service/transport → AppService/runtime → Provider/store
FILES EXPECTED: 每个文件的一句话职责
RED TEST: 在旧行为上失败的最高层测试及失败原因
GATES: 本轮 Gate IDs
NON-GOALS: 当前卡明确排除项
PRE-EXISTING CHANGES: 已有修改及隔离方法
```

工作卡解释不了的文件不进入 diff。

## 3. 执行循环

1. **Locate**：沿 Seam IDs 列出真实调用链和当前行为。完成标准：每一跳有 symbol 证据，未凭文件名猜测。
2. **Baseline**：运行 G0 和目标模块现有测试。完成标准：记录 PASS/FAIL 并区分既有失败。
3. **Red**：只添加当前卡最高层失败测试。完成标准：测试因目标行为缺失而红，不因 fixture/类型错误而红。
4. **Minimal**：实现最小垂直路径。完成标准：Red 变绿，没有后续卡 DTO、组件、表或 Vendor switch。
5. **Converge**：补足当前卡列出的错误、取消、幂等、权限、乱序或 partial 分支。完成标准：全部 Acceptance 有自动化证据。
6. **Verify**：运行 Gate IDs。完成标准：每条命令记录退出码和关键结果。
7. **Review**：逐个检查 Contract IDs、Authority、依赖方向、隐私和 diff 范围。
8. **Commit**：中文 Conventional Commit。完成标准：提交只含本卡，用户已有修改原样保留。
9. **Handoff**：按 `06-handoff-template.md` 输出。完成标准：下一 Agent 无需猜测已完成事实和剩余风险。

## 4. 四条 Authority 快检

每次 Review 用以下四问定位泄漏：

1. **正文在哪？** Provider Session 或有界内存投影。
2. **任务事实在哪？** Team SQLite + AppService。
3. **恢复锚点在哪？** Agent Execution binding。
4. **活动进度在哪？** TaskRuntime/transient stream projection，持久业务终态仍在 SQLite。

任一事实出现第二个可写 Authority，按 Stop protocol 报告。

## 5. Stop protocol

出现以下任一情况停止写代码：

- blocker 未满足，或当前分支缺少 blocker 的公开产物；
- Issue、ADR、Contract 与生产事实存在实质冲突；
- Team、transport 或 frontend 需要按 Agent ID/Vendor/协议写业务分支；
- 需要 Team/Conversation 持久化正文，或把 event body 放入 durable task snapshot/log；
- Antigravity 只能靠 synthetic ID resume，或必须调用 Conversation application workflow 读历史；
- replay 无法与 live 区分，可能重复工具或 Team mutation；
- migration 需要猜测/覆盖用户数据；
- 自动测试只能依赖真实 Provider、网络、登录态或用户数据库；
- generated contract 只能靠手工编辑通过；
- 预计跨入下一 Ticket、超过 10 个文件或碰到无法确认所有权的已有修改。

停止报告：

```text
STOPPED
EXACT CONFLICT:
AFFECTED ACCEPTANCE:
CURRENT CODE EVIDENCE:
OPTIONS AND TRADE-OFFS:
RECOMMENDATION:
FILES LEFT UNCHANGED:
```

## 6. 通用执行 Prompt

```text
执行 AssetIWeave Issue #21 的 {{TICKET_ID}}：{{TITLE}}。

只读取并执行当前卡：
1. AGENTS.md。
2. GitHub Issue #21 全文、最新评论；Issue #19 只作已落地基线。
3. agent-docs/feature-plans/team-chat-workspace/00-execution-router.md。
4. 当前卡列出的 Contract IDs、Seam IDs、Gate IDs。
5. 05-luna-flash-playbook.md。
6. tickets/{{TICKET_FILE}}。

先输出强制工作卡，再执行 Locate→Baseline→Red→Minimal→Converge→Verify→Review→Commit→Handoff。

硬约束：
- 一轮只完成当前 Ticket；不预建后续卡接口或 UI。
- AppService 持有 Team mutation Authority。
- Provider 持有聊天正文；Team/Conversation 零正文写入。
- Agent Execution 持有 Session Adapter 与 Resume Anchor。
- Team/frontend 不按 Vendor、Agent ID 或协议分支。
- replay 只展示，不产生 live 副作用。
- frontend 只通过 services；Go CLI 只通过 Engine。
- generated contract 只用生成命令更新。
- 保留所有用户已有未提交修改。
- 触发 Stop protocol 时立即停止并按固定格式报告。

交付使用 06-handoff-template.md。
```

## 7. Checkpoint Review Prompt

使用新上下文，只读不改代码：

```text
审查 Team 聊天工作台 {{CP_ID}} 的 {{BASE}}..{{HEAD}}。

读取 Issue #21、01-contract.md、03-ticket-map.md、04-verification-matrix.md，以及本 Checkpoint 已完成执行卡。

按顺序检查：
1. 每条 Acceptance 是否有最高层公开行为证据；
2. Provider正文、Team facts、Agent binding、transient projection 是否各自单一 Authority；
3. Vendor/Agent/protocol 分支是否泄漏到 Team、transport 或 frontend；
4. event identity、ordering、dedup 和 replay/live 是否可靠；
5. Antigravity 是否捕获真实 ID、每轮进程、空 ID 保留旧 anchor；
6. Conversation 零写入、日志/快照无正文和 Resume Anchor；
7. AppService、frontend service、Engine/CLI 边界是否闭合；
8. 是否提前实现后续 Ticket 或保留平行 form-first/v2 路径。

只输出 P0/P1/P2 findings。每条包含文件与行、破坏的 Contract ID、外部影响、修复验收和回归测试。无阻断项时明确写出剩余盲区。
```

## 8. 常见偏差校正

- **群聊错觉**：Team 是群组容器；正文仍按成员独立 Session 显示。
- **Persistent 等于常驻进程**：保留 Provider context；Antigravity 仍每轮一个进程。
- **Resume 等于 Replay**：Resume 恢复 Provider context；Replay 构造界面历史。
- **Replay 当 Live**：replay item 只展示，不能写 TeamTask/mailbox 或执行工具。
- **Aion 持久消息复制**：AssetIWeave 使用 Provider history，不建立 Team transcript。
- **Task 卡当聊天正文**：卡片从 TeamTask 事实重建；Agent 执行正文来自 Provider。
- **Provider switch**：差异留在 Session Adapter/capability，不进入 Team 或 UI。
- **订阅等于可靠存储**：live 用订阅；漏事件用 transient snapshot/polling；重启用 Provider replay。
- **只测 reducer**：TS02 之外必须用 TS01/TS06 证明垂直行为。
