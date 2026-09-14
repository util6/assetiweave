# Memory Q1–Q36：产品合同

本文件定义用户可观察行为与稳定 Contract ID。实现细节只能在不改变这些结果的前提下调整。

## 1. 产品目标

用户打开「近期」时，应能回答两个问题：

1. 上一个工作阶段，各项目做了什么？
2. 基于现有证据，各项目下一步适合做什么？

产品采用低频稳定快照，而不是实时活动流。结构化事实存 SQLite；只读 Markdown 供人和外部 Agent 阅读；长期知识从近期事实逐层晋升。

## 2. 核心合同

### Authority

- **M35-AUTH-01**：SQLite 是 Memory 唯一 Authority；UI、Markdown、Context、Recall、Engine 和 CLI 不形成独立事实源。
- **M35-AUTH-02**：Conversation 是来源事实。Memory 只消费 canonical Session → Question membership → Turn → Part → Content Node，不回读 Provider 私有日志补造缺失内容。
- **M35-AUTH-03**：AppService 是唯一业务编排边界。Tauri、Engine、CLI、MCP 和 frontend service 只适配。
- **M35-AUTH-04**：Memory Item、revision、source reference、Snapshot、Job 和投影均 tenant-scoped；跨 tenant 标识表现为不可见或统一拒绝。
- **M35-AUTH-05**：页面和 Context 读取 last-success；运行中、失败、取消或投影损坏不产生半成品可见状态。

### L1 Recent Memory

- **M35-L1-01**：近期窗口可选 24/48/72 小时，默认 48 小时；设置入口只在统一设置页。
- **M35-L1-02**：默认每日本地时间 02:00、14:00 两个生成水位；两者可配置且必须不同。
- **M35-L1-03**：窗口固定为 `[target_watermark - window, target_watermark]`，两次成功水位之间不随当前时钟漂移。
- **M35-L1-04**：候选 Session 以 `last_activity_at` 判断；导入时间、创建时间和当前打开状态不替代活动时间。
- **M35-L1-05**：应用离线错过多个水位时只处理最新到期水位；同一目标任务幂等合并。
- **M35-L1-06**：语义输入无变化时发布 reused Snapshot、推进水位并跳过 Agent；UI 显示“内容复用”。
- **M35-L1-07**：每个项目输出窗口摘要、0–3 条有证据的下一步和合格 Memory Item；无证据建议保持为空。
- **M35-L1-08**：active/blocked/waiting 可跨原窗口续接，最长 7 天；新证据可刷新。
- **M35-L1-09**：completed/verified/abandoned/superseded 在形成终态的首个成功 Snapshot 展示一次，之后退出 L1。
- **M35-L1-10**：上一轮输入是结构化事实、状态与引用；旧 Markdown 不进入递归总结。
- **M35-L1-11**：长 Session 只提供变化事实、必要 outline 和有界补读，不默认重复完整历史。
- **M35-L1-12**：未归属项目进入统一分组，获得项目身份前不得晋升 L2。

### L2 Project Memory

- **M35-L2-01**：L2 保存项目未来仍需知道的决定、约束、验证结论、失败原因、持续阻塞和长期待办；普通流水进展不进入。
- **M35-L2-02**：Agent 只提名；应用验证 tenant、project、类别、状态、引用、revision、重复和冲突。
- **M35-L2-03**：有有效用户引用的明确项目决定或约束可以立即晋升。
- **M35-L2-04**：普通 blocker/todo/research 必须在两个有新证据的成功 generated Snapshot 中连续出现才具备候选资格；reused Snapshot 不增加观察次数，也不打断连续性。
- **M35-L2-05**：completion 和普通 progress 不能仅因重复而晋升。
- **M35-L2-06**：Project Consolidation 仅在出现合格候选或既有 L2 输入变化时运行；水位本身不触发空转。

### L3 Permanent Memory

- **M35-L3-01**：L3 只保存明确全局规则、长期偏好、跨项目稳定工作方式、通用约束与轻量项目索引。
- **M35-L3-02**：候选需满足明确的用户全局声明，或在至少两个不同项目中有独立有效证据。
- **M35-L3-03**：Global Consolidation 只消费当前有效 L2，按周级或候选阈值低频运行；不得随每个 L1 水位空转。
- **M35-L3-04**：来源删除、缺失或排除后，已晋升 L2/L3 继续存在；引用转为 unavailable。
- **M35-L3-05**：后续证据通过新 revision 纠正或取代；旧 revision 标记 superseded，历史不可静默改写。
- **M35-L3-06**：Context、普通 Recall 与 Markdown 默认只读当前有效 revision。

### Generation Skill 与 ACP

- **M35-SKILL-01**：后台生成使用独立 `assetiweave-memory-generation`，现有 `assetiweave-memory` 继续服务独立深度回忆。
- **M35-SKILL-02**：系统提供应用管理的默认模板；用户编辑普通 Skill Library 副本，首版只支持 tenant 级选择。
- **M35-SKILL-03**：任务固定 asset ID、asset revision、Skill content hash、合同、预算、投影策略、来源 revision 与目标水位。
- **M35-SKILL-04**：ACP 请求由固定执行信封、用户 Skill 和有界证据工具组成；Skill 只控制提取与表达策略。
- **M35-SKILL-05**：Skill 不能扩大 tenant、Session、工具、网络、协作 Agent、预算、Schema、引用准入或持久化权限。
- **M35-SKILL-06**：Skill 无效时不调用 Agent、不发布部分结果、不静默回退旧用户 Skill；last-success 保持可读。
- **M35-SKILL-07**：生成全程无人值守；未知、缺失与冲突通过结构化结果或失败表达，不向用户提问。

### Markdown、UI 与设置

- **M35-PROJ-01**：每个 tenant 只发布 `~/.assetiweave/memories/<tenant>/memory_summary.md` 和 `MEMORY.md`。
- **M35-PROJ-02**：`memory_summary.md` 投影最新成功 L1；`MEMORY.md` 投影当前有效 L2/L3，L2 在文件内按项目分组。
- **M35-PROJ-03**：两份文件只读、原子发布、可从 SQLite 重建；不生成逐 Session 或逐项目 Markdown。
- **M35-PROJ-04**：Recent Snapshot 历史默认保留 30 天；磁盘只保留最新成功投影；长期 revision 链继续保留。
- **M35-UI-01**：Recent 默认时间视图：日期轨道 → 日期内项目；项目视图是同一 Snapshot 的客户端分组。
- **M35-UI-02**：切换投影不调用 Agent，两个视图的 Item 与 Session reference 集合完全一致。
- **M35-UI-03**：条目可展开理由、水位和 Session；点击 Session 进入对话记录详情并选中对应 Session。
- **M35-UI-04**：引用 unavailable 时显示状态且不跳转。
- **M35-UI-05**：页面只允许展开/收起、投影切换、Session 导航；窗口、刷新、生成、完成、忽略、置顶、采纳、编辑、删除不出现在页面。
- **M35-UI-06**：失败时显示 last-success 与安静的“更新未完成”；详情、取消和重试进入任务中心。
- **M35-UI-07**：统一设置页提供窗口、水位、生成 Skill 选择/打开/恢复默认和维护重建；设置保存不阻塞等待 Agent。

### 运行与治理

- **M35-OPS-01**：Session Memory Phase 1 可以继续在 Session 稳定后事件驱动；它只更新 SQLite 来源事实，不直接刷新用户可见 L1 Snapshot。
- **M35-OPS-02**：Recent、Project、Global 使用持久 Job、lease、heartbeat、retry、cancel 与晚到结果保护；Agent 调用期间不持有全局应用锁。
- **M35-OPS-03**：内部 Worker 直接调用 AppService/ACP 能力，不启动 AIWC/CLI 子进程；AIWC 共享读取语义但不成为 Authority。
- **M35-OPS-04**：日志、任务详情与遥测只记录 ID、hash、计数、耗时和错误码，不保存完整 Prompt、Conversation 正文、工具正文或秘密。
- **M35-OPS-05**：数据库通过追加 migration 演进；新旧行为采用 expand–migrate–contract 切换，不长期双轨。

## 3. Q1–Q36 追踪

| Q | Contract |
|---|---|
| Q1 | M35-L1-01、M35-L2-01、M35-L3-01、M35-PROJ-01 |
| Q2 | M35-L1-03、M35-L1-04、M35-UI-01–02 |
| Q3 | M35-UI-05、M35-SKILL-01、M35-AUTH-01 |
| Q4 | M35-L1-01、M35-L1-03–04 |
| Q5 | M35-AUTH-01、M35-L1-09、M35-L2-01、M35-L3-01 |
| Q6 | M35-UI-01–02、M35-PROJ-02 |
| Q7 | M35-AUTH-01、M35-L1-07、M35-SKILL-04 |
| Q8 | M35-L1-08–09 |
| Q9 | M35-L2-01–06、M35-L3-01–03 |
| Q10 | M35-PROJ-01–03 |
| Q11 | M35-L1-10–11 |
| Q12 | M35-L1-02、M35-OPS-01 |
| Q13 | M35-PROJ-03、M35-SKILL-02 |
| Q14 | M35-L3-04 |
| Q15 | M35-SKILL-07、M35-UI-05 |
| Q16 | M35-L1-02 |
| Q17 | M35-L1-03、M35-AUTH-05 |
| Q18 | M35-L1-02、M35-L2-06、M35-L3-03 |
| Q19 | M35-L1-08–09、M35-L3-04 |
| Q20 | M35-L1-02、M35-UI-07 |
| Q21 | M35-L1-05、M35-L1-10 |
| Q22 | M35-L1-06、M35-UI-06 |
| Q23 | M35-L1-02、M35-L2-06、M35-L3-03 |
| Q24 | M35-L1-10 |
| Q25 | M35-PROJ-01–03、M35-AUTH-01 |
| Q26 | M35-PROJ-04、M35-OPS-04 |
| Q27 | M35-UI-01、M35-UI-03、M35-UI-05 |
| Q28 | M35-PROJ-01–03、M35-L3-06 |
| Q29 | M35-SKILL-01–04 |
| Q30 | M35-L1-03 |
| Q31 | M35-L2-02–05、M35-L3-02 |
| Q32 | M35-L3-04–06 |
| Q33 | M35-L1-07、M35-L1-12 |
| Q34 | M35-AUTH-05、M35-UI-06 |
| Q35 | M35-SKILL-01–07 |
| Q36 | M35-L1-01、M35-SKILL-01、M35-UI-03、M35-UI-05–07 |

## 4. 明确非目标

- 本规格不重写深度回忆的多轮查询能力。
- 本规格不冻结 Recent 页最终 CSS、颜色、动效或像素。
- 本规格不增加实时或每 Session 的用户可见总结。
- 本规格不增加建议反馈、完成、采纳或编辑状态。
- 本规格不增加逐项目生成 Skill、远程 Memory Pipeline 市场、云同步或团队 Memory。
