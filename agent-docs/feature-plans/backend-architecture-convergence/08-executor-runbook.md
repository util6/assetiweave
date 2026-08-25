# SPEC-BA-08：后续代码模型执行手册

- 状态：Proposed v1
- 适用对象：上下文窗口较小、需要逐任务执行的代码模型
- 目标：避免模型重新设计架构、复制 legacy 路径或用局部测试误报完成

## 1. 每次执行前的读取顺序

只加载与当前任务有关的最小上下文：

1. 根目录 `AGENTS.md`。
2. `00-overview.md` 的 Authority 表、边界和完成定义。
3. `06-implementation-tasks.md` 中当前 Task ID。
4. 当前任务对应领域 SPEC。
5. `07-verification-matrix.md` 对应 requirement 行。
6. 当前实现文件、测试和最近相关 Git history。

不得把整套 SPEC 和整个大型源文件一次性塞入上下文后凭印象修改。

## 2. 标准执行循环

```text
定位 Task ID
→ 重新读取当前源码（不信任旧行号）
→ 写/运行失败测试
→ 实现最小完整 vertical slice
→ 删除或委托旧路径
→ 运行目标测试
→ 运行边界/生成物检查
→ 审查 diff
→ 更新任务证据
→ 中文 commit
```

“最小完整”指完成一个 Authority 切换，不是只新增 interface/type。

## 3. 开始任务时必须输出的工作卡

```markdown
Task ID: BA-___
Objective:
Canonical authority:
Legacy path to remove/delegate:
Files expected:
Failing test to add first:
Verification commands:
Out of scope:
```

文件列表超过 5 个时，先拆分任务；不得直接进行大范围机械替换。

## 4. 代码修改规则

### Always

- 先读代码和 Git，再信任 SPEC 中的基线描述。
- 为行为变更新增回归测试。
- 保留 Tauri/Engine 对同一 workflow 的复用关系。
- 使用 strict TypeScript、两空格、双引号、分号；Rust 使用 rustfmt。
- UI 色彩、边框、阴影使用 semantic theme token/foundation component。
- 修改 Engine contract 后运行 `pnpm cli:contract`。
- 只禁用与当前后台任务冲突的操作。
- 错误保留 typed category，不先转 String。
- 保护工作区中任务开始前已有的 staged/unstaged 用户修改。

### Ask first

- 新增外部依赖。
- 修改 SQLite schema 或已发布 migration。
- 改变公开 IPC 参数/返回结构且无法兼容。
- 修改 Catalog 的真实 upstream package/version/证据但缺少可验证来源。
- 改变 `asset_mounts`、OneShot/ResidentHost、半开兼容区间等既定裁决。

### Never

- 手改生成的 `cli/internal/schema/contract.json`。
- 用删除本地 cache 作为产品修复。
- 放宽/删除 core compatibility 检查以让测试变绿。
- 把 fixture URL/hash 放入 production Catalog。
- 新增另一套 task registry、process runner、Agent executor 或 target path table。
- 在 Application 新增 `Result<T, String>`、裸 Command 或直接 Tauri依赖。
- 用 `allow(dead_code)` 掩盖未接线的新架构。
- 删除失败测试而不证明测试已被更强覆盖替代。
- 未执行完成审计就把 GitHub Issues（已取代文件版任务总册） 标成完成。

## 5. 常见误修复与正确动作

| 症状 | 禁止修法 | 正确动作 |
|---|---|---|
| `core_incompatible` | 只改 Catalog range | 删除 ACP core/version 门禁，版本仅观测；保留完整性与 ACP conformance |
| 安装按钮点击报错 | catch 后吞掉 | UI 保留 compatibility 并在请求前门禁 |
| task getter 显示 Running | finish 时多写一次 map | getter 从 TaskRuntime 组合 lifecycle |
| probe 卡住 | 外层再套 sleep/poll | HostProcess deadline + cancellation + process tree cleanup |
| TargetCatalog 未使用 | 加一个新 getter test | 迁移 defaults/detection/planner/mount consumer |
| legacy warning | `#[allow(dead_code)]` | 迁移调用者并删除 legacy code |
| AppError 改造困难 | 全部 `AppError::Legacy(e.to_string())` | 按垂直切片实现 typed conversion |

## 6. 测试策略

### 6.1 测试金字塔

1. 纯函数：semver、catalog revision、action migration、progress mapping。
2. Repository/service：事务、cache selection、mount compensation。
3. Runtime integration：task cancel、process tree、registry snapshot。
4. Surface contract：Tauri/Engine DTO/error parity。
5. Frontend：按钮状态、progress/event/poll fallback。
6. E2E：本地 ACP fixture install/update/recovery。

### 6.2 测试必须证明失败原因

测试名称应描述不变量，例如：

```text
newer_cache_is_selected_even_when_core_range_is_only_observational
runtime_cancellation_cannot_be_overwritten_by_domain_finish
failed_update_preserves_previous_active_installation
```

禁止只断言 `is_err()`；必须断言稳定错误码、状态和副作用。

## 7. Diff 自审清单

提交前逐项检查：

- [ ] 新 Authority 是否有至少一个生产 consumer。
- [ ] 旧 Authority 是否删除或只委托新路径。
- [ ] 同一状态是否仍被双写。
- [ ] 错误是否在某层退化成 String。
- [ ] Tauri 与 Engine 是否共享 workflow。
- [ ] 长 I/O 是否持锁或阻塞 async worker。
- [ ] cancellation 是否到达文件循环/子进程。
- [ ] 日志/DTO 是否泄露 env、prompt、stderr、token、用户绝对路径。
- [ ] 生成契约是否需要更新。
- [ ] 新测试在旧实现上是否确实失败。
- [ ] 是否误改任务开始前的用户文件。

## 8. PR/交接模板

```markdown
## Task
BA-___

## Authority change
- Before:
- After:

## Behavior
- User-visible:
- Compatibility:

## Legacy removal
- Removed:
- Temporary seam and removal task:

## Verification
- [ ] target unit tests
- [ ] integration tests
- [ ] pnpm check:boundaries
- [ ] pnpm check:surface-matrix（如适用）
- [ ] typecheck/test/build（如适用）
- [ ] cargo fmt/test

## Evidence matrix
- Requirement IDs:

## Rollback
- Safe rollback point:
- Persistent state implications:
```

## 9. 停止条件

遇到以下情况必须停止当前实现并记录，而不是猜测：

- 真实 Agent distribution URL/version/evidence 无法从权威来源验证。
- 需要修改已发布 migration checksum。
- 目标 IPC 破坏兼容且没有 migration/alias 方案。
- 当前工作区包含与任务文件重叠的未知用户修改。
- SPEC 间存在互相矛盾的 MUST。

停止不等于标记任务完成；必须保留失败测试、证据和下一步所需输入。

## 10. 完成声明规则

一个 Task 只有同时满足以下条件才可勾选：

1. Acceptance 全部成立。
2. 目标测试和规定命令通过。
3. 负向搜索没有旧路径，或存在精确、带删除任务的 allowlist。
4. 当前 production consumer 已切换。
5. 文档/contract 与实现一致。
6. diff 自审无未解释项。

整个目标只有 SPEC-BA-07 所有 MUST requirement 均为 `achieved` 才能声明完成。
