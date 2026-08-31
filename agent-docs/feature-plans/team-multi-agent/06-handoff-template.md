# Team 多 Agent：交接协议

## 工单完成输出

```text
## EXECUTION RESULT

TICKET: TNN / GitHub #N
BRANCH / COMMIT: ...
AUTHORITY CHANGED: ...

### CHANGES MADE
- path — responsibility changed

### TESTS ADDED / UPDATED
- test — behavior proved

### ACCEPTANCE EVIDENCE
- [x] criterion — test/command/result

### VERIFICATION
- `command` — PASS/FAIL — count/summary

### CONTRACT CHECK
- C-... — evidence

### MIGRATION / CONTRACT IMPACT
- migration, generated Engine contract, rollback or None

### THINGS NOT TOUCHED
- excluded area and reason

### PRE-EXISTING USER CHANGES
- path — preserved unchanged

### OPEN ISSUES / DEVIATIONS
- None, or exact blocker with recommendation

### NEXT READY TICKET
- report only; do not execute
```

## 工单 Issue 评论

工单完成后把上面的精简证据写入当前子 Issue。只有 acceptance 全部勾选、验证通过且提交可定位时才关闭工单。父 Issue #19 保持打开，直到 T14 提交完整验收矩阵。

## 跨上下文恢复

新 Agent 不继承上一 Agent 的口头结论。它只相信：

1. 已合并/可定位提交；
2. 当前子 Issue 评论中的测试证据；
3. tracker blocker 状态；
4. 当前代码和测试。

未提交工作通过 `git status` 和 diff 识别，不通过聊天摘要猜测所有权。
