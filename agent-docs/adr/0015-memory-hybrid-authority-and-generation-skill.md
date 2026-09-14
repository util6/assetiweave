# 0015: Memory 采用 SQLite Authority、只读 Markdown 投影与用户可编辑生成 Skill

> **状态**：已接受
> **决策日期**：2026-09-15
> **产品规格**：[Issue #35](https://github.com/util6/assetiweave/issues/35)
> **取代**：Issue #20/#30 中固定 72 小时、逐项目 Markdown、长期条目随来源级联删除、Memory Recipe 直接承载用户策略的早期设计

## 背景

AssetIWeave 同时需要机器可查询、可事务更新的结构化 Memory，以及人类和外部 Agent 可以直接阅读的长期文档。早期实现把 Session、Project、Global Memory 分散在 SQLite 与多层 Markdown 中，并让 Project/Global 文档参与下游读取。这样会产生两类风险：同一事实出现多个可修改副本，以及来源 Session 删除后长期决定被连带清除。

后台生成还需要通过 ACP 调用用户选择的 Agent。把完整生成提示词硬编码进请求会增加上下文、降低可维护性，并剥夺用户调整总结策略的能力；仅把它改名为 Recipe 仍没有进入 AssetIWeave 已有的 Skill Library、版本和来源治理体系。

## 决策

1. SQLite 是 Memory Item、revision、source reference、Recent Snapshot、水位、晋升、取代、Skill 绑定、Job 与 last-success 的唯一 Authority。
2. 每个 tenant 只发布两份应用生成的只读 Markdown：
   - `memory_summary.md`：最新成功 L1 Recent Snapshot；
   - `MEMORY.md`：当前有效 L2 Project Memory 与 L3 Permanent Memory。
3. 项目只作为条目的归属、查询条件和文档内分组，不形成逐项目 Memory 文件。Session Memory 只保存在 SQLite。
4. Markdown 是可删除、可重建的确定性投影。页面、Context Resolver、Recall、Engine 和 CLI 读取 SQLite，不解析 Markdown 恢复业务结构。
5. L2/L3 的知识生命周期与来源生命周期分离。来源删除、缺失或排除只把引用标记为不可用；已经晋升的长期条目继续存在，直到后续证据将其取代。
6. 新增 `assetiweave-memory-generation` Skill 表达用户可修改的总结策略。系统模板位于应用管理层，用户编辑普通 Skill Library 资产副本。
7. ACP 的 tenant、范围、工具、预算、Schema、引用校验和写入能力由 AppService 的固定执行信封控制。Skill 只决定“如何总结”，不决定“可以做什么”。
8. Recent 聚合每日默认在本地时间 02:00 与 14:00 运行。时间窗口为 24/48/72 小时，默认 48 小时，并锚定目标水位。

## 备选方案

### 仅使用 SQLite

优点是 Authority 单一、事务简单。缺点是用户和外部 Agent 缺少可直接阅读、可携带的高密度长期摘要。结论：SQLite 保持 Authority，但保留只读文档投影。

### SQLite 与 Markdown 保存等价可写副本

优点是任一副本都能工作。缺点是冲突合并、修改来源和失效传播不可判定，最终形成双重事实源。结论：否决。

### 每个项目生成独立 `MEMORY.md`

优点是单项目文件直观。缺点是两天内项目数量虽小，长期仍会形成目录扩散、重复全局信息和跨项目晋升同步问题。结论：项目在统一 `MEMORY.md` 中分组，不按项目分文件。

### 将生成提示词硬编码或继续使用 Memory Recipe

优点是实现直接。缺点是用户不可治理，且与现有 Skill Library、资产 revision、content hash 和跨 Agent 装配机制重复。结论：以独立 Skill 取代 Recipe 的新任务入口，旧 Recipe 仅保留历史任务审计。

## 后果

- 新实现需要追加 SQLite migration，并以 expand–migrate–contract 方式退出旧 Recent Event、逐项目文档和 Recipe 读取路径。
- Markdown 文件可以被删除或损坏而不丢失 Memory；投影失败不回滚 SQLite last-success。
- 用户修改生成 Skill 后，新任务必须绑定新的 asset revision/hash；运行中的旧任务不得覆盖新目标。
- Context Resolver 的正常读取只采用当前有效 L2/L3 revision，不默认注入整份 L1 Snapshot。
- 页面可以提供时间与项目两种投影，但两者共享同一 Snapshot，不增加 Agent 调用。
- `assetiweave-memory` 的深度回忆职责保持独立，不因新增生成 Skill 改变。
