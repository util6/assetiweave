# Memory Q1–Q36：单上下文执行切片

每张切片是一个可演示、可独立回滚的 tracer bullet。编号是稳定规划身份；GitHub 子 Issue 发布后在标题/正文保留编号。

## 依赖图

```text
N01 -> N02 -> N03 ┬-> N04 ─┐
                   └-> N05 ─┴-> N06 -> N07 -> N08 ─┐
                            └-> N09 ────────────────┴-> N10 -> N11
```

## N01：扩展合同与 Snapshot 兼容骨架

**Blocked by**：无。
**Contract**：M35-AUTH-01–05、M35-OPS-05。
**Outcome**：新表、类型和 Snapshot read API 可在旧 Recent 继续运行时写入/读取 fixture Snapshot。

验收：

- [ ] 新 migration 只追加，旧 schema 升级成功。
- [ ] tenant/unique/CHECK/source non-cascade 约束有数据库测试。
- [ ] `memory.recent.snapshot.get` 返回 empty state 或包含完整 fixture last-success 的 state。
- [ ] 旧 `memory.recent.list` 在 expand 阶段仍通过。
- [ ] 路由、合同和验证矩阵引用 Issue #35，无旧语义冲突。

## N02：用户可编辑 Generation Skill 垂直切片

**Blocked by**：N01。
**Contract**：M35-SKILL-01–06、M35-OPS-01。
**Outcome**：用户可编辑技能目录、生成新 Work Order；损坏时安全回退，内置定义保持只读。

验收：

- [x] 内置与用户目录复制。
- [x] 校验失败时阻止重置覆盖。
- [x] Work Order 包含白名单工具。
- [x] 重置到系统默认。

## N03：固定水位 L1 Snapshot 垂直切片

**Blocked by**：N01、N02。
**Contract**：M35-L1-01、M35-L1-07、M35-L1-12、M35-L1-13、M35-OPS-04。
**Outcome**：单次显式触发能够根据候选生成严格校验的 L1 Snapshot，覆盖所有候选且事务提交。

验收：

- [x] 48h 默认窗口以 last activity 筛选。
- [x] 输出项目摘要、0–3 建议、Item 与 Session refs。
- [x] coverage/Schema/reference 任一不合格时不发布。
- [x] 未归属项目可见且不晋升。
- [x] SQLite 事务和 last-success 原子。

## N04：双水位调度与 reuse 垂直切片

**Blocked by**：N03。
**Contract**：M35-L1-01–06、M35-OPS-02。
**Outcome**：设置 24/48/72 和两个水位后，系统自动处理最新到期目标；无变化时不调用 Agent。

验收：

- [x] 默认 02:00/14:00，自定义值验证和持久化。
- [x] 可控时钟覆盖跨日、DST 和 missed-watermark。
- [x] target/content fingerprint 分工正确。
- [x] reused Snapshot 推进水位、保留 contentGeneratedAt。
- [x] 页面/任务状态显示内容复用。

## N05：L1 续接与增量历史垂直切片

**Blocked by**：N03。
**Contract**：M35-L1-08–11、M35-L3-04。
**Outcome**：未完成事项跨窗口延续，终态一次退出；长 Session 和上一轮只提交结构化变化。

验收：

- [x] active/blocked/waiting 延续最长 7 天。
- [x] reused 不刷新 7 天或晋升观察。
- [x] completed/verified/abandoned/superseded 展示一次。
- [x] 上一轮输入不读取 Markdown。
- [x] 来源失效使未晋升 L1 退出。

## N06：L2 项目长期记忆垂直切片

**Blocked by**：N04、N05。
**Contract**：M35-L2-01–06。
**Outcome**：近期候选按明确决定或两次新证据规则晋升，Context 能读当前项目 L2。

验收：

- [ ] 用户明确决定一次观察可候选。
- [ ] blocker/todo/research 需要两个 generated Snapshot。
- [ ] completion/progress 不晋升。
- [ ] unassigned 不晋升。
- [ ] 无候选时 Project Agent 调用 0 次。
- [ ] 同项目串行，失败保留 current L2。

## N07：L3、长期来源与 supersede 垂直切片

**Blocked by**：N06。
**Contract**：M35-L3-01–06。
**Outcome**：明确全局规则或两个项目证据形成 L3；删除来源后知识保留，后续证据可修订。

验收：

- [ ] 两个真实 project key 独立支持才形成 cross-project candidate。
- [ ] Global 按周/8 候选低频触发，无候选不调用 Agent。
- [ ] 删除/缺失/排除只更新 reference availability。
- [ ] unavailable reference 不跳转也不支持新晋升。
- [ ] revise/supersede 保留完整历史，Context 只读 current。

## N08：统一 Markdown 与保留策略垂直切片

**Blocked by**：N07。
**Contract**：M35-PROJ-01–04。
**Outcome**：每个 tenant 只得到两份可重建只读文档，旧逐项目写入停止。

验收：

- [ ] `memory_summary.md` 来自 L1，`MEMORY.md` 来自 L2/L3。
- [ ] 日期/项目/Item 排序确定，reused 元数据正确。
- [ ] 原子替换失败保留旧文件。
- [ ] 删除文件后无需 Agent 即可重建。
- [ ] Snapshot 30 天清理不影响长期 revision。
- [ ] 没有新的逐 Session/逐项目 Markdown。

## N09：时间优先 Recent UI 垂直切片

**Blocked by**：N04、N05。
**Contract**：M35-UI-01–07。
**Outcome**：用户在日期轨道查看项目总结、建议、条目和 Session，并切换同源项目视图。

验收：

- [ ] 默认按时间，日期内按项目；项目视图 identity 集合相同。
- [ ] 使用 PillTabs 和 Auroraqua-UI 分层组件。
- [ ] 条目展开显示 rationale/watermark/Session。
- [ ] available Session 进入对话记录详情；unavailable 不导航。
- [ ] DOM 无刷新、窗口、生成、编辑、反馈或深度回忆输入。
- [ ] last-success、内容复用、更新未完成状态正确。

## N10：公共表面迁移与 contract 收缩

**Blocked by**：N08、N09。
**Contract**：M35-AUTH-03、M35-OPS-03、M35-OPS-05。
**Outcome**：Engine、Tauri、CLI、AIWC Skill、Context 和前端全部使用新 Authority，旧 Recent/Recipe/逐项目写入无 active caller。

验收：

- [ ] CLI contract 连续生成一致，Go/TS/Rust DTO 同构。
- [ ] `assetiweave-memory` 使用 Snapshot API，深度回忆行为不变。
- [ ] 旧 `memory.recent.list`、event target 从 public surface 移除。
- [ ] v2 生成无 Recipe 新任务调用。
- [ ] 逐项目文档 writer 无 active call path。
- [ ] 内部 Worker 未启动 CLI/AIWC 子进程。

## N11：Q1–Q36 综合验收与发布证据

**Blocked by**：N10。
**Contract**：全部 `M35-*`。
**Outcome**：自动化、迁移、桌面和安全证据足以发布并关闭 Issue #35。

验收：

- [ ] M35-V01–V40 全部有可复现证据。
- [ ] migration/backfill/rollback 在临时旧库通过。
- [ ] Rust、Frontend、Go、boundary、surface matrix 全仓 Gate 通过。
- [ ] 日志/Task/event payload 无 Prompt、正文、tool body、secret。
- [ ] 桌面视觉、键盘、窄窗口、Session 导航与设置 smoke 通过。
- [ ] 独立 review 无 P0/P1。
- [ ] Handoff 记录发布、回滚、保留数据和旧表后续处置。

## Frontier

初始仅 N01。N03 完成后 N04 与 N05 可并行；N05 完成后 N09 可与 N06→N07→N08 链并行。N10 等待 N08 与 N09，N11 最后执行。
