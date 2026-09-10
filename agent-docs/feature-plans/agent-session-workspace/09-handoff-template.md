# 单卡 Handoff 模板

```markdown
## Ticket

- Stable ID: TNN
- GitHub Issue: #...
- Parent: #31
- Branch/worktree: ...
- Commit: ...
- Status: Implemented | Verified | Blocked

## Outcome

一句话描述用户现在可观察到的端到端行为。

## Contract / Seam / Gate

- Requirements: ...
- Contracts: ...
- Seams changed: ...
- Gates run: ...

## Red Evidence

- Test/command:
- Expected failure:
- Observed failure:

## Implementation

- 新增/改变的公开行为：
- 保持的 Authority：
- 兼容层/迁移状态：

## Verification

| Command/check | Result | Evidence |
|---|---|---|
| ... | PASS/FAIL/NOT RUN | exit code / screenshot / note |

## Files

- Changed for this ticket: ...
- Existing unrelated dirty files preserved: ...

## Risks / Known limits

- None，或列出具体事实与影响。

## Review

- P0/P1 findings: ...
- AionUi intentional differences: ...
- Logs/tracing/transcript audit: ...

## Next frontier

- Ready: T...
- Still blocked: T... by ...
```

规则：

- 不写“全部完成”，只写当前卡 Outcome；
- 未运行命令标记 NOT RUN；
- pre-existing failure 附 baseline 证据；
- Blocked 必须指向 blocker/Stop Protocol；
- GitHub 评论使用相同内容，避免文档与 tracker 两套叙述。
