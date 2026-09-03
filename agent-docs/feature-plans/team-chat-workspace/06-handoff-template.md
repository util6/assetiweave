# Team 聊天工作台：交接模板

每张执行卡完成、停止或部分完成时使用。不要粘贴大段测试日志；给出可复现命令和关键结果。

```markdown
## Ticket

- ID：TNN
- Outcome：
- Status：DONE / STOPPED / PARTIAL
- Commit：<hash 或未提交原因>

## Scope delivered

- 用户可观察结果：
- 新增/改变的公开契约：
- Authority 变化：无 / 精确说明

## Acceptance evidence

| Acceptance | Test/command | Result | Evidence |
|---|---|---|---|
| A1 | `...` | PASS/FAIL | 测试名/截图/摘要 |

## Gates

| Gate | Command | Exit | Key result |
|---|---|---:|---|
| G0 | `...` | 0 | ... |

## Diff map

| 文件 | 职责 | 为什么属于本卡 |
|---|---|---|
| `path` | ... | ... |

## Contract review

- Contracts checked：
- Provider正文 Authority：
- Team facts Authority：
- Agent binding Authority：
- Transient projection Authority：
- Conversation zero-write evidence：
- Sensitive-data evidence：

## Existing changes preserved

- 开工前已有修改：
- 隔离/保留方式：

## Remaining risks

- 未覆盖分支：
- 手工验证缺口：
- 后续卡必须知道的事实：

## Next frontier

- Ready Ticket：
- Blockers verified：
- 下一卡禁止假设：
```

## STOPPED 附加段

```markdown
## Stop evidence

- Exact conflict：
- Affected acceptance：
- Current code evidence：
- Options and trade-offs：
- Recommendation：
- Files left unchanged：
```

## 完成交接标准

- 每条 Acceptance 都能从表中定位证据。
- 每个修改文件都能映射到本卡 Outcome。
- 已有修改与本卡修改可区分。
- 风险写成具体未验证行为，不写“可能有问题”。
- 下一 Agent 只需读取 router、当前下一卡和本交接即可继续。
