# 交接模板

## 基本信息

- Task ID：
- Commit：
- 执行日期：
- 基线 HEAD：
- 工作区原有修改：

## 1. 修改文件列表

| 文件 | 修改目的 |
|---|---|
| | |

## 2. 测试先行证据

- 新增/修改测试：
- 施工前失败命令：
- 精确失败原因：
- 实现后通过命令：

## 3. 验证结果

| 命令 | PASS/FAIL | 摘要 |
|---|---|---|
| | | |

## 4. 契约核对

- [ ] logical ID 仍为 `antigravity`
- [ ] 无 AntiGravity-specific ACP backend/protocol 分支
- [ ] 无 `agy` fallback
- [ ] model 状态未污染 protocol/readiness
- [ ] 旧 Native installation 不进入 Registry
- [ ] Binary SHA-256 来自真实 archive
- [ ] Conversation Adapter 无改动
- [ ] Target Profile 无改动
- [ ] Native 通用 seam 保留
- [ ] 未验证能力没有写 passed

## 5. 真实 AntiGravity ACP E2E

- 状态：PASS / FAIL / NOT_RUN / BLOCKED
- 平台：
- Registry version/commit：
- initialize：
- session/new：
- model discovery：
- prompt：
- session/update：
- close/delete：
- process/workspace cleanup：
- evidence ID：

## 6. 未完成或偏差

- 未验证事项：
- Stop condition：
- residual risk：

## 7. Native production path 最终审计

- 搜索命令：
- 命中分类：
- 是否存在剩余 `AntiGravity -> NativeExecutionBackend`：是 / 否

## 8. 下一任务

- 仅写直接依赖图中的下一 Task ID，不执行。

