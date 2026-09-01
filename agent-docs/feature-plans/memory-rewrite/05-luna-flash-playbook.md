# Memory 重写：Luna / Flash 执行手册

## 1. 执行单位

一轮只完成一张 `tickets/TNN-*.md`。Luna 与 Flash 使用相同契约和门禁。

建议硬边界：

- 一张卡预计最多 10 个修改文件；超过时先按用户可演示结果报告拆分点。
- production diff 建议小于 500 行；总 diff 超过 900 行时停止评估拆卡。
- 一次只新增一个 Authority 或一个公共垂直切片。
- 一个提交只表达当前卡的 Outcome。

Flash 在每个 Execution Step 后立即跑该步目标测试；Luna 在每个步骤结束前核对 Completion criterion，不把多步压成一次大改。

## 2. 开工工作卡

修改前先输出并填写：

```text
TICKET: TNN — title / GitHub #N
BLOCKERS: 每个 blocker 的关闭证据
CONTRACTS: 当前卡 Contract IDs
SEAMS: 当前卡生产与测试 Seam IDs
CALL CHAIN: public entry → AppService → repository/runtime → projection
AUTHORITY CHANGED: 本轮唯一新增/改变的事实来源
FILES EXPECTED: 文件及单一职责
RED TEST: 旧行为上失败的最高层测试与预期失败
GATES: 当前卡 Gate IDs
NON-GOALS: 后续卡片行为
PRE-EXISTING CHANGES: 工作区已有变更与排除办法
```

工作卡解释不了的文件不进入 diff。调用链尚未逐跳确认时停留在 Locate。

## 3. 执行循环

1. **Locate**：按 Seam IDs 搜索生产入口、测试 harness、migration 和生成物。完成标准：调用链与每个 Authority 可逐跳列出。
2. **Baseline**：运行 G0 与目标模块现有测试。完成标准：记录命令、测试数、PASS/FAIL 和既有失败。
3. **Red**：添加当前 Acceptance 的最高层失败测试。完成标准：测试因目标行为缺失失败，而非 fixture/编译错误。
4. **Minimal**：实现最小公开垂直路径。完成标准：Red 转绿，不创建后续 Ticket 占位接口、表、页面或兼容层。
5. **Converge**：补足当前卡的错误、tenant、幂等、取消、redaction 和重启分支。完成标准：每条 Acceptance 有自动化证据。
6. **Verify**：运行卡片 Gate IDs。完成标准：命令与关键结果可复现。
7. **Review**：按 G7 审查 Authority、Provider 分支、Conversation 写入、秘密、generated diff 和 scope。
8. **Commit**：中文 Conventional Commit。完成标准：提交可单独回滚且不包含用户既有修改。
9. **Handoff**：按 `06-handoff-template.md` 输出。完成标准：下一 Agent 无需猜测代码状态、证据和 frontier。

## 4. Stop protocol

出现任一条件时停止写代码：

- blocker 未完成或当前分支缺少 blocker 产物；
- Issue、Contract、ADR、当前子 Issue之间有实质冲突；
- 需要 Memory 直接解析 Codex/AntiGravity/OpenCode/Claude Code 私有日志；
- 需要新建第二套 Conversation、Card、Task lifecycle 或 Markdown Authority；
- 需要把 Memory 文件写入来源项目目录；
- Consumer 只能先 ack 或只能创建内存任务；
- 自动测试只能依赖真实 Provider、网络或用户真实数据库；
- 需要记录 prompt、正文、原始 tool input、凭据或环境变量值；
- 需要直接从前端 `invoke`、从 Go CLI 写 SQLite 或手工编辑 generated contract；
- 需要兼容旧 Dream/candidate/Evidence 产品语义；
- Recall 实现开始建设通用 Team、多 Agent 编排或通用聊天 UI；
- 当前卡预计超过 10 文件、900 行或跨入下一卡；
- 发现无法确认所有权的未提交修改；
- migration 需要修改已发布文件或猜测用户数据含义。

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

## 5. 通用执行 Prompt

```text
执行 AssetIWeave Memory 重写的 {{TICKET_ID}}：{{TITLE}}。

先读取：
1. AGENTS.md。
2. 父 Issue #20、当前子 Issue、评论与 blocker。
3. agent-docs/feature-plans/memory-rewrite/00-execution-router.md。
4. 当前执行卡指定的 Contract IDs、Seam IDs、Gate IDs。
5. 05-luna-flash-playbook.md 和当前唯一执行卡。

先输出强制工作卡。随后严格执行 Locate→Baseline→Red→Minimal→Converge→Verify→Review→Commit→Handoff。

约束：
- 只完成当前 Ticket。
- Conversation 是来源事实，Memory 是可重建派生层。
- AppService 是 mutation Authority；SQLite 是 Memory 结构化 Authority。
- Outbox 必须 durable enqueue 后再推进 offset。
- TaskRuntime 只投影活动状态；阻塞 I/O 与 Agent 调用不持有全局 app lock。
- Memory 只消费 canonical Conversation，不出现 provider-specific 业务分支。
- Recall 只读、结构化、Persistent，但不扩展为通用多 Agent 产品。
- frontend 只走 services，CLI 只走 Engine，不手工编辑 generated contract。
- 保留用户所有既有未提交修改。
- 触发 Stop protocol 时停止并报告。

交付格式使用 06-handoff-template.md。
```

## 6. Luna 校正

Luna 开始实现前把每条 Acceptance 写成「输入 → 公开动作 → 持久结果/可见结果」。每完成一步立即对照该步 completion criterion；若一次修改覆盖两个步骤，回退并缩小到当前步骤。Luna 的 Handoff 必须列出所有未覆盖分支，不能用“基本完成”替代矩阵。

## 7. Flash 校正

Flash 每一步只做一个 Red→Green 小循环：

1. 修改一个目标测试；
2. 运行该单测并记录 Red；
3. 修改最少生产代码；
4. 运行同一单测并记录 Green；
5. 运行相邻回归；
6. 再进入下一步。

Flash 不批量新建 DTO、migration、commands 和 UI 后统一测试；这种顺序会隐藏 Authority 偏差。

## 8. Checkpoint Review Prompt

在新上下文只读审查：

```text
审查 Memory 重写 Checkpoint {{CP_ID}} 的提交 {{BASE}}..{{HEAD}}。

读取父 Issue #20、01-contract.md、03-ticket-map.md、04-verification-matrix.md，以及本 Checkpoint 已完成执行卡。

按顺序检查：
1. 每条 Acceptance 是否有公开行为证据；
2. Conversation、Memory SQLite、Markdown/index、Durable Job、TaskRuntime 的 Authority 是否泄漏；
3. project directory、72h 和 30m idle 边界是否使用正确时间/路径；
4. Outbox ack、幂等、lease、retry、watermark 和 restart 是否成立；
5. last-success、revision、token budget 和失效传播是否成立；
6. Recall 是否只读、tenant-scoped、结构化、Persistent，且未复用 Translation OneShot；
7. UI 是否隐藏内部 ID/locator/Evidence/raw Markdown，导航是否精确；
8. Engine/CLI/Skill/frontend 是否与 Rust contract 一致；
9. 是否提前实现后续 Ticket、保留旧并行路径或写入第三方目录；
10. redaction 与日志最小化是否覆盖模型输入输出和失败路径。

只输出按 P0/P1/P2 排序的 findings，包含文件与行、破坏的 Contract ID、复现步骤和修复验收。无 finding 时写“未发现阻止进入下一 Checkpoint 的问题”，并列剩余盲区。
```

## 9. 常见偏差校正

- **工作线实体**：回到规范化项目目录；项目/时间只是同一 Recent Event 的投影。
- **创建/导入时间当近期**：使用 Conversation last activity 和可控时钟。
- **Outbox 直接跑 Agent**：先持久化 Job，再 ack，再由 worker 取得 lease。
- **TaskRuntime 当数据库**：Job/last-success 在 SQLite；TaskRuntime 只投影活动执行。
- **Markdown 当 Authority**：从 SQLite 原子重建；损坏时读 last-success。
- **Provider switch**：把差异移回 Conversation Adapter 或 Agent Definition/capability。
- **Card 当实体**：持久化 locator，前端把引用渲染为 Card。
- **Recall 当搜索框**：使用持久 Agent Session、只读工具和结构化输出。
- **Recall 当 Team 功能入口**：只实现 Memory Recall 所需共享 runtime seam。
- **UI 直连**：所有页面通过 `frontend/src/services/memory.ts`。
- **CLI 双实现**：Go 只调用 Engine。
- **mock 调用次数当验收**：最终从 AppService/Engine 断言可观察结果与重启状态。
