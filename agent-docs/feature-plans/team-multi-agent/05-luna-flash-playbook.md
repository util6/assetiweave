# Team 多 Agent：Luna / Flash 执行手册

## 1. 执行单位

一轮只完成一张 `tickets/TNN-*.md`。Luna 和 Flash 使用相同契约；Flash 在每个 Execution Step 后立即运行对应目标测试，Luna 也不得跨 Ticket 扩展。

建议范围：

- 一张卡最多 10 个修改文件；预计超过时先报告切分点。
- production diff 建议小于 500 行；总 diff 超过 900 行时停止评估拆卡。
- 一个提交只表达当前卡的一个用户可验证结果。

## 2. 开工协议

修改前输出工作卡：

```text
TICKET: TNN — title / GitHub #N
BLOCKERS: 已核对全部关闭
CONTRACTS: 本轮 Contract IDs
SEAMS: 本轮生产与测试 Seam IDs
AUTHORITY CHANGED: 本轮新增或改变的唯一事实来源
FILES EXPECTED: 文件及每个文件的职责
RED TEST: 将在旧行为上失败的最高层测试
GATES: 本轮 Gate IDs
NON-GOALS: 明确排除的后续卡片行为
PRE-EXISTING CHANGES: 工作区已有变更及排除方式
```

工作卡解释不了的文件不进入 diff。

## 3. 执行循环

1. **Locate**：沿执行卡 Seam IDs 定位当前生产调用链。完成标准：从公开入口到持久化/Runtime 的调用路径可以逐跳列出。
2. **Baseline**：运行 G0 和目标模块现有测试。完成标准：记录基线 PASS/FAIL，区分既有失败。
3. **Red**：只添加当前 acceptance 的最高层失败测试。完成标准：测试因缺少目标行为失败。
4. **Minimal**：实现使 Red 变绿的最小垂直路径。完成标准：没有后续 Ticket 的接口、表或 UI 占位实现。
5. **Converge**：补足错误、取消、幂等和权限分支。完成标准：当前卡全部 acceptance 有自动化证据。
6. **Verify**：运行执行卡 Gate IDs。完成标准：每条命令有可核对结果。
7. **Review**：检查 diff 是否泄漏 Authority、Provider 分支、Conversation 写入、秘密或后续 scope。
8. **Commit**：中文 Conventional Commit。完成标准：提交只包含当前卡；用户已有修改仍原样存在。
9. **Handoff**：按 `06-handoff-template.md` 评论/输出。完成标准：下一 Agent 不需要猜测当前事实。

## 4. Stop protocol

遇到以下情况停止写代码：

- blocker 未关闭或当前分支缺少 blocker 产物；
- Issue、Contract、ADR 与生产事实存在实质冲突；
- 需要 Team 或通用 Runtime 按 Agent ID/Vendor 名称分支；
- 需要建立第二套 Conversation、Team transcript、task board 或 session binding Authority；
- migration 需要猜测或覆盖用户数据；
- 自动测试只能依赖真实 Provider、用户真实数据库或网络；
- 需要把 prompt、正文、原始 tool input、凭据或 resume token 写入日志/快照；
- 预计修改超过 10 个文件或跨入下一 Ticket；
- 发现无法确认所有权的未提交修改；
- generated contract 只能靠手工编辑才能通过。

停止报告固定格式：

```text
STOPPED
EXACT CONFLICT:
AFFECTED ACCEPTANCE:
CURRENT CODE EVIDENCE:
OPTIONS AND TRADE-OFFS:
RECOMMENDATION:
FILES LEFT UNCHANGED:
```

## 5. 通用执行 Prompt

```text
执行 AssetIWeave Team 多 Agent 的 {{TICKET_ID}}：{{TITLE}}。

先读取：
1. AGENTS.md。
2. 父 Issue #19、当前子 Issue、评论与 blocker。
3. agent-docs/feature-plans/team-multi-agent/00-execution-router.md。
4. 当前执行卡指定的 Contract IDs、Seam IDs、Gate IDs。
5. 05-luna-flash-playbook.md 和当前唯一执行卡。

先输出强制工作卡。随后严格执行 Locate→Baseline→Red→Minimal→Converge→Verify→Review→Commit→Handoff。

约束：
- 只完成当前 Ticket。
- 使用 TS01 作为跨领域主证据；低层 mock 不替代公开行为。
- AppService 是 Team mutation Authority。
- Team 与 Conversation 零写入隔离。
- 复用 AgentExecutionRuntime；Provider 差异来自能力/Definition。
- 不手工编辑 generated contract。
- 保留所有用户已有未提交修改。
- 触发 Stop protocol 时停止并报告，不自行扩大范围。

交付格式使用 06-handoff-template.md。
```

## 6. Checkpoint Review Prompt

使用新上下文，只读不改代码：

```text
审查 Team 多 Agent Checkpoint {{CP_ID}} 的提交 {{BASE}}..{{HEAD}}。

读取父 Issue #19、01-contract.md、03-ticket-map.md、04-verification-matrix.md，以及本 Checkpoint 已完成执行卡。

按顺序检查：
1. 每条 acceptance 是否有公开行为证据；
2. Team、Provider Session、TaskRuntime、Conversation 的 Authority 是否泄漏；
3. 人工审核门和固定任务 owner 是否可被绕过；
4. resume/replay 是否重复执行工具或写入正文；
5. MCP/CLI 权限和 AppService 收口是否成立；
6. cancellation、restart、outbox 和轮询是否幂等；
7. 日志、错误和 snapshot 是否泄漏正文或凭据；
8. Engine/CLI/frontend 是否与 Rust contract 一致；
9. 是否提前实现后续 Ticket 或保留平行旧路径。

只输出按 P0/P1/P2 排序的 findings，包含文件与行、破坏的 Contract ID 和修复验收。无 finding 时写“未发现阻止进入下一 Checkpoint 的问题”，并列剩余盲区。
```

## 7. 常见偏差校正

- **Provider switch**：把 `if agent_id == ...` 移回能力/Definition 和 Adapter。
- **Session 等于进程**：Persistent 保留 Provider binding；进程仍可 bounded reap。
- **Resume 等于 Replay**：Teammate 可只 Resume；Leader 必须额外 Replay/Read。
- **回放当 live**：恢复事件只构造临时 Leader 时间线，不触发 Team mutation。
- **顺序当 fallback**：排序只帮助推荐/展示，失败仍保留原 owner。
- **TaskRuntime 当数据库**：活动投影来自 TaskRuntime，恢复事实来自 SQLite。
- **Frontend 直连**：所有页面调用进入 `frontend/src/services`。
- **CLI 直写 SQLite**：Go CLI 只调用 Engine。
- **测试只断言 mock 次数**：最终通过 TS01 断言持久状态和外部行为。

