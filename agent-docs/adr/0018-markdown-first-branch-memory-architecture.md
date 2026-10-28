# 0018: Markdown 优先、思维分支寻回与双模式轻量 Memory 知识流水线架构

> **状态**：已接受
> **决策日期**：2026-10-27
> **产品规格**：[Issue #135](https://github.com/util6/assetiweave/issues/135)
> **取代**：[ADR-0015](./0015-memory-hybrid-authority-and-generation-skill.md)（废弃 SQLite 多表状态机与单向只读投影）

---

## 背景

在 ADR-0015 实施后，AssetIWeave 将 Memory 体系构建在复杂的 SQLite 关系表群（`session_memory_jobs`、`recent_memory_jobs`、`memory_maintenance_jobs`、`memory_item_evidence` 等）与巨型 JSON 打包器（`pack_builder`）之上。

然而随着实际使用深入，该设计暴露了显著架构瓶颈：
1. **状态机与锁竞争过度**：引入了租约竞争（Lease Claim）、心跳保活、看门狗以及阶段重试等繁重机制，在会话高频同步或并发重启时极易出现死锁或孤儿任务。
2. **LLM 调用成本与上下文爆炸**：推模式打包将大量原始节点打包成巨型 JSON 推送给外部模型，耗费巨额 Token 且容易由于超出模型上下文而失败。
3. **断点寻回体验割裂**：开发者在切换项目或恢复工作时，最核心的诉求是快速了解“之前想到了哪里、哪些路径还悬空未决（🍂 悬空叶子节点）、哪些任务已完成闭环（✅）”。旧架构缺乏对思维分叉（Thought Branches）与悬空叶子消融的一等公民支持。
4. **人类阅读与 CLI 直读困难**：事实全部锁在关系数据库深层字段中，CLI 与普通文本工具无法直接开箱即用。

---

## 决策

全面重构 Memory 架构为 **“Markdown 优先 + 薄 SQLite 水位 + 思维分支寻回 + 双模式（GUI/CLI）轻量流水线”**。

### 1. 磁盘 Markdown 产物与物理底座
所有知识与记忆事实直接以结构化 Markdown 形式存储于应用私有目录（`~/.assetiweave/memory/<tenant_id>/`）：
- **单会话分叉记忆**：`sessions/<session_id>.md`
  - 记录会话核心意图目标、思维分支拓扑（显著区分 🍂 待探索悬空叶子节点与 ✅ 已完成闭环任务）、关键决策与技术结论。
  - 包含精确到轮次的协议锚点 `[short#T<turn_id>](aiwc://session/<session_id>/turn/<turn_id>)`，支持跨会话点击跳转。
- **项目记忆与叶子消融池**：`projects/<slug>.md`
  - 持续维护当前项目尚未闭环的悬空叶子池。新会话识别出已完成动作时，在下次流水线执行时自动将对应悬空叶子消融转移到已完成归纳区。
- **近期研发进展快照**：`recent/latest.md`
  - 汇总 72 小时内跨项目的活跃研发脉络，聚焦当前最紧迫的待续工作。
- **全局长期沉淀**：`MEMORY.md`
  - 提炼跨项目工程经验、系统认知与稳定开发者偏好。

### 2. 极薄 SQLite 水位真相源
彻底移除 10 余张状态机表，仅保留单一轻量水位表 `memory_session_watermarks`：
- 字段仅包含 `session_id`, `project_path`, `source_updated_at`, `summary_rel_path`, `has_signal`, `updated_at`。
- 每次会话提交或流水线运行时，仅需比对 `conversation_sessions.imported_at` 与 `source_updated_at` 水位差值；无信号或无变更会话实现 **0 LLM 调用秒级跳过**。

### 3. 三阶段后台流水线 (遵循 ADR-0017)
将复杂的异步任务网精简为单一原子化的三阶段任务流水线：
- **阶段 1：`stages::SESSION_FACTS`（提取会话思维分叉）**
  - 增量扫描未处理会话，提取意图目标、思维分支拓扑（🍂/✅），原子写 `sessions/<session_id>.md` 并更新水位。
- **阶段 2：`stages::RECENT_SNAPSHOT`（近期记忆快照与项目叶子消融）**
  - 读取近期有信号的 Markdown 事实，聚合生成 `recent/latest.md`，并将其中悬空与闭环叶子动态消融到各项目 `projects/<slug>.md`。
- **阶段 3：`stages::GLOBAL_CONSOLIDATION`（全局长期记忆维护）**
  - 提炼跨项目稳定规律，去重增量更新至根目录 `MEMORY.md`。

### 4. 双模式（GUI 与 CLI）无缝消费
- **CLI 原生**：
  - `aiwc memory recent get` 直接返回 `recent/latest.md`；
  - `aiwc memory project get <path>` / `--current-project` 自动映射 project_path 至 slug 并返回 `projects/<slug>.md`；
  - `aiwc memory context resolve --current-project` 统一组装 `MEMORY.md` + 项目 Markdown + 近期 Markdown 注入 LLM 上下文。
- **GUI 交互**：
  - 桌面端直接渲染流体象牙质感 Markdown。
  - `ConversationMarkdown` 识别 `aiwc://session/<session_id>/turn/<turn_id>` 自定义协议链接，拦截后驱动路由直接在会话工作区定位到目标轮次。

### 5. 全面废弃与清理
- 移除 `session_memory_jobs`、`recent_memory_jobs`、`memory_maintenance_jobs` 等旧数据访问层。
- 移除 `pack_builder`、`bounded_evidence_baseline` 等推模式重型打包与归档测试代码。
- 移除复杂的租约认领抢占与看门狗竞争逻辑。

---

## 备选方案权衡

### 方案 A：继续沿用 SQLite 存储思维分支节点树
- **缺点**：分支演化、消融和跨项目汇总需要在关系数据库中编写大量树形递归遍历和多表级联更新，调试极度困难且不透明。
- **结论**：否决。Markdown 天然具备树状列表与人类可读特性，且便于原子写入和 Git/外部工具消费。

---

## 后果

- **性能飞跃**：增量跳过由复杂事务查询降为简单时间戳比对；单次流水线耗时下降 80%+。
- **可靠性提升**：无多表死锁，无孤儿运行任务，磁盘原子写（临时文件 + rename）保障断电/崩溃零损坏。
- **开发体验统一**：CLI 与桌面端消费同一份真实 Markdown 资产，消除数据呈现不一致。
