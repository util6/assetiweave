# Memory 重写：Handoff 模板

每张 Ticket 结束时原样复制并填写。省略项视为未完成。

```markdown
## {{TICKET_ID}} — {{TITLE}}

### Result
- GitHub Issue:
- Branch/worktree:
- Commit:
- User-visible outcome:

### Contract evidence
| Contract ID | Evidence |
|---|---|
| C-... | test/command/result |

### Acceptance evidence
| Acceptance | Red evidence | Green evidence |
|---|---|---|
| ... | command + failure | command + pass |

### Authority and call chain
- Public entry:
- AppService workflow:
- SQLite/repository:
- Durable Job/TaskRuntime:
- Projection/UI:
- New or changed authority:
- Explicitly unchanged authorities:

### Files changed
- `path` — responsibility

### Verification
- `command` — PASS/FAIL; tests/count/key assertion
- `command` — PASS/FAIL; tests/count/key assertion

### Generated artifacts
- Command:
- Files changed:
- Diff reviewed:

### Pre-existing changes preserved
- `path` — ownership/status

### Manual verification
- Scenario:
- Result:
- Not run and reason:

### Risks and blind spots
- ...

### Scope audit
- Later Ticket behavior introduced: none / exact item
- Legacy path retained intentionally: none / exact reason
- Third-party project directory writes: none
- Raw IDs/prompt/secrets exposed: none

### Next frontier
- Ready Ticket:
- Blocked Ticket:
- Required checkpoint review:
```

## Failed/Stopped handoff

触发 Stop protocol 时不提交半成品，使用：

```markdown
## {{TICKET_ID}} — STOPPED

- Exact conflict:
- Affected acceptance:
- Current code evidence:
- Commands already run:
- Files changed before stop:
- Files restored/left unchanged:
- Options and trade-offs:
- Recommendation:
- User/pre-existing changes preserved:
```
