# Conversation 重构：Luna Execution Playbook

| 字段 | 值 |
|---|---|
| 执行模型 | Luna |
| 执行单位 | 一个 GitHub 子 Issue |
| 任务范围 | #4–#16 |
| 调度依据 | GitHub 原生 parent/sub-issue 与 blocker |
| 默认方法 | 单一垂直切片、测试先行、中文提交 |

## 1. 开工协议

Luna 每轮只执行一张未关闭、无未关闭 blocker 的子 Issue。不要把多个“看起来相近”的 Issue
合并施工，也不要提前实现后续 schema contract 或兼容删除。

开工前必须：

1. 读取仓库根 `AGENTS.md`；
2. 读取当前子 Issue 全文及其最新评论；
3. 读取本目录 `00-overview.md` 至 `04-luna-execution-playbook.md`；
4. 检查原生 blocker，确认全部关闭；
5. 检查工作区和当前分支，不覆盖既有未提交修改；
6. 认领当前 Issue；
7. 建立 `codex/` 前缀的独立分支或 worktree；
8. 输出工作卡后再修改代码。

## 2. 强制工作卡

每轮开始先输出：

```text
ISSUE: #N — 标题
AUTHORITY CHANGED: 本轮接管的唯一事实/读取 Authority
OLD PATH: 本轮要委托、兼容或删除的旧路径
FILES EXPECTED: 预计文件与理由
FAILING TEST FIRST: 先建立的行为失败证据
VERIFY: 将运行的命令
NON-GOALS: 明确不做的后续事项
BLOCKERS: 已核对关闭的 blocker
```

如果预计范围不能由当前 Issue 的验收标准解释，停止扩展并在 Issue 留下发现。

## 3. 实施规则

- 先用代码、migration 和现有测试确认当前事实，不按文档猜测实现；
- 先增加能在旧行为上失败的最高层合理测试，再做最小实现；
- 领域逻辑放在共享 AppService/backend seam，Tauri 和 Engine 只做薄适配；
- 不创建 `legacy`、`new`、`v2` 或第二套 Question membership/投影路径；
- 不让 Question 表重新承担正文、搜索缓存、顺序或 grouping origin；
- 不把 Card 作为后端领域实体；
- 不用字符串拼接恢复缺失的原始 Shell Execution；
- 长运行迁移遵循后台任务约束，不用页面级 busy 包住全流程；
- 公开 Engine 契约变化后运行 `pnpm cli:contract`，不手工编辑生成物；
- 每个 commit 只包含当前 Issue 的一个可解释 Authority 变化；
- commit 使用简洁的中文 Conventional Commit；
- 不顺手更新 ADR。最终 ADR 只在 #16 有真实实现和迁移证据后创建。

## 4. Stop conditions

遇到以下任一情况，Luna 停止写代码，在当前 Issue 评论准确事实、影响与建议：

- Issue 与仓库当前 schema/生产调用链存在实质冲突；
- blocker 未关闭或所需契约尚未进入目标分支；
- 必须引入第二套 Authority 才能继续；
- 历史数据缺少可靠来源，修复只能依赖有损猜测；
- migration 无法提供备份、验证或回滚路径；
- 公开 Engine 变化与 CLI 兼容范围超出当前 Issue；
- 工作区有无法确认归属的冲突修改；
- 测试必须依赖用户真实数据库或不可重复的外部状态。

报告格式：exact conflict、受影响 acceptance、可选方案及 trade-off、推荐方案。不要自行扩大范围。

## 5. 通用执行 Prompt

```text
使用 Luna 执行 AssetIWeave Conversation 重构子 Issue #{{ISSUE_NUMBER}}。

必须读取：
1. AGENTS.md。
2. GitHub Issue #{{ISSUE_NUMBER}} 全文、原生 blocker 和最新评论。
3. agent-docs/feature-plans/conversation-semantic-projection-refactor/00-overview.md。
4. 同目录 01-domain-contract.md、02-migration-and-compatibility.md、03-verification-matrix.md。
5. 同目录 04-luna-execution-playbook.md。

执行方式：
- 只执行本 Issue；先输出强制工作卡。
- 先定位当前生产调用链和 Authority，再写能够证明旧行为不满足 acceptance 的测试。
- 实现最小垂直切片，使公开 AppService/Engine/UI 行为成立。
- 不创建平行 membership、Card 持久化或 Question 内容快照。
- 不提前执行被当前 Issue 阻塞的 schema 删除或兼容收缩。
- 运行 Issue 对应测试和验证矩阵中适用门禁。
- 使用中文 Conventional Commit；一个提交只表达一个 Authority 变化。

交付时严格输出：
1. CHANGES MADE
2. TESTS ADDED/UPDATED
3. VERIFICATION（逐条命令与 PASS/FAIL）
4. ACCEPTANCE CRITERIA（逐条证据）
5. MIGRATION / COMPATIBILITY IMPACT
6. THINGS NOT TOUCHED
7. OPEN ISSUES / DEVIATIONS
8. COMMIT(S)
9. NEXT READY ISSUE（只报告，不执行）
```

## 6. Review 协议

代码完成后使用新的 Luna 上下文只做 review，不改代码：

```text
审查 Conversation 重构 Issue #{{ISSUE_NUMBER}} 的提交 {{BASE}}..{{HEAD}}。

按以下顺序检查：
1. Issue acceptance 是否逐条有行为证据；
2. 是否建立了新的平行 Authority 或保留了未说明的旧写路径；
3. Question/Turn/Part/Content Node 身份是否混淆；
4. migration、历史兼容、索引和 evidence 是否可能丢失数据；
5. Tauri、Engine、CLI 和前端是否绕过 AppService；
6. 测试是否只证明类型/内部调用，而未证明公开行为；
7. 验证命令与生成契约是否真实执行。

只输出按严重度排序的 findings；没有 finding 时输出“未发现阻止合并的问题”并列出剩余测试盲区。
```

Review finding 修复后重新运行相关门禁，再向 Issue 写入完成证据。Issue 在 acceptance 全部满足前保持打开。

## 7. 完成评论模板

```text
## Luna execution result

- Branch / commit: ...
- Authority changed: ...
- Old path removed or delegated: ...

### Acceptance evidence
- [x] ... — test/command/result

### Verification
- `command` — PASS

### Migration and rollback
- ...

### Remaining compatibility
- ...（对应后续 Issue；没有则写 None）
```
