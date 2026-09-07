# B2-00：冻结真实基线并归属残余机制

**Objective:** 建立 Issue #24 的可重放基线，让后续执行者不依赖过期计划状态。

**Contracts:** 全部 Contract 只读取，不实现。

**Files:**

- Modify only when evidence requires correction: `agent-docs/feature-plans/backend-infrastructure-convergence-v2/02-current-baseline.md`
- Modify only when an unowned match is found: `agent-docs/feature-plans/backend-infrastructure-convergence-v2/03-ticket-map.md`
- No production code changes.

## Preflight

```bash
git status --short --branch
git rev-parse HEAD
gh issue view 24 --comments
```

- [ ] 记录所有开始前 dirty path；本卡不触碰这些文件。
- [ ] 运行 `02-current-baseline.md` 的基础设施计数、Runtime 分布、依赖和测试基线命令。
- [ ] 将每个 `block_on/run_sync` 文件归入 R03–R08；将每个 dispatcher thread、HostProcess、日志 façade、Settings get、path conversion、try_get 命中归入对应 owner 卡。
- [ ] 若存在未归属命中，只调整 ticket map，不设计实现。
- [ ] 检查 #1、#2、#22 最新评论；只记录已被当前代码和测试证明的成果，不把历史 “PLANNED/COMPLETE” 文本当事实。
- [ ] 向 Issue #24 发布 baseline 评论，包含 HEAD、dirty paths、测试结果、warning 数和各机制计数。

## Gate G-B2-00

```bash
rg -l '\.(block_on|run_sync)\(' src-tauri/src --glob '*.rs' | sort
rg -l 'Condvar|std::thread|thread::' src-tauri/src/backend/events --glob '*.rs' | sort
rg -l '\b(log_info|log_warn|log_error|record_info|record_warn|record_error)\b' src-tauri/src --glob '*.rs' | sort
rg -l 'try_get' src-tauri/src/backend/store --glob '*.rs' | sort
```

通过：每个命中都在 baseline 评论中有 owner 卡；完整基线命令有真实输出。

**Commit:** 仅当本卡修订了执行文档时提交 `docs(agent): 冻结后端基础设施收口基线`；没有文件变更时只评论 Issue。

