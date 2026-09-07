# B2-G03：最终重新验收

**Authority:** 当前代码、当前测试、当前 SHA 的三平台 CI 和 Issue #24 最新验收矩阵共同决定完成状态。

**Contracts:** v2 全部 Contract。

**Files:**

- Modify: `agent-docs/feature-plans/IMPLEMENTATION-STATUS.md`
- Modify: `agent-docs/feature-plans/backend-infrastructure-convergence-v2/00-execution-router.md`
- Modify stale status only: 本计划目录
- No production changes.

## Steps

- [ ] 逐条读取 `01-contract.md`，每项引用一条行为测试、一条删除/保留证据和适用的平台证据。
- [ ] 运行 `10-remediation-verification-matrix.md` 的 G-FULL；记录每条命令 exit code、测试数和 warning 数。
- [ ] 确认 Issue #1 的 Agent Catalog release gate 两条命令都通过。
- [ ] 确认 Issue #2 仍为 OPEN 且明确 blocked by #24；v2 不把全仓错误链优化伪装成本卡前置成果。
- [ ] 确认 B2-G02 run 的 `headSha` 与当前提交一致，macOS/Linux/Windows jobs 全部 success。
- [ ] 检查所有审计 delete query；regex 必须覆盖 `libc::kill`、任意 reader thread、任意 polling 间隔和通用字段 façade，不依赖旧变量名。
- [ ] 任一项失败时向 Issue #24 评论 `Status: INCOMPLETE` 并保持 OPEN。
- [ ] 全部通过后更新状态索引、评论完整矩阵并关闭 Issue #24；不得仅写“全量测试通过”。

## Verify

```bash
git diff --check
git status --short
gh issue view 1 --json state,comments
gh issue view 2 --json state,body,comments
gh issue view 24 --json state,comments
```

状态文档提交：`docs(agent): 记录后端基础设施纠偏最终验收`
