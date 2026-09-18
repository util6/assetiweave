# Memory Q1–Q36 (Issue #35) 综合交付与发布验收报告

- **日期**：2026-09-15
- **父 Issue**：[#35](https://github.com/util6/assetiweave/issues/35)
- **架构决策**：ADR-0015
- **规划规格**：`agent-docs/feature-plans/memory-rewrite/q1-q36-spec/` (00–09)
- **实施状态**：**Completed / Ready for Release**

---

## 1. 交付摘要 (Executive Summary)

本轮重构完整落实了 ADR-0015 与 Q1–Q36 决策约束，彻底重写了 AssetIWeave 的 Memory 系统：
1. **L1 Recent Snapshot 管道**：基于固定双水位（默认 02:00/14:00）和滑动时间窗口（默认 48h），实现基于 content fingerprint 的无变化安全复用（reused Snapshot）、7 天最长活跃续接与终态单次退出展示。
2. **L2 项目长期记忆**：严格按照“用户明确决定（1 次观察）”或“blocker/todo/research（2 次新证据观察）”晋升规则，由同项目串行 Consolidation 管道管理，无候选零 Agent 消耗。
3. **L3 全局记忆与修订**：由低频维护管道（>=8 候选或周级）驱动，支持跨多项目证据识别、不可变 revision 链条、`supersede` 关系追溯与来源 Session 失效级联更新（标记 `unavailable` 但保留已形成认知）。
4. **用户可编辑 Generation Skill**：引入沙箱化 `assetiweave-memory-generation` Skill，支持用户在 Skill Library 中创建自定义副本并绑定，严格限制 ACP 权限（零网络、零外部 Agent、零文件写入）。
5. **统一投影文档与 UI**：淘汰逐项目 Markdown 写入，统一投影到应用根目录下原子发布的 `memory_summary.md` (L1) 与 `MEMORY.md` (L2/L3)；Recent UI 采用 AssetIWeave UI 规范，支持日期轨道与按项目切换视图。
6. **公共表面与 Contract 收缩**：移除已废弃的 `memory.recent.list` 与 `memory.recent.event.target`，CLI `aiwc memory recent get` 对齐 `memory.recent.snapshot.get`，Go/TS/Rust DTO 完全同构，两次独立连续 contract 生成零 diff。

---

## 2. 垂直切片完成清单 (N01–N11 Slices)

| 切片编号 | 名称与范围 | 核心交付成果 | 提交与状态 |
|---|---|---|---|
| **N01** | 扩展合同与 Snapshot 兼容骨架 | 引入 `recent_memory_snapshots` 等表结构，追加 migration 成功；提供 Snapshot API 骨架。 | `4b719fe6` [x] |
| **N02** | 用户可编辑 Generation Skill 垂直切片 | 内置模板资产与用户副本复制，校验失败阻止覆盖，白名单工具注入。 | `4b719fe6` [x] |
| **N03** | 固定水位 L1 Snapshot 垂直切片 | 48h last activity 窗口，项目摘要、0–3 建议，coverage 校验与 SQLite 事务提交。 | `4b719fe6` [x] |
| **N04** | 双水位调度与 reuse 垂直切片 | 02:00/14:00 调度，离线漏单只跑最新已到期水位，content fingerprint 无变化复用。 | `ce914855` [x] |
| **N05** | L1 续接与增量历史垂直切片 | 活跃/受阻任务最长续接 7 天，终态任务单次退出，增量历史不读 Markdown。 | `051867c6` [x] |
| **N06** | L2 项目长期记忆垂直切片 | 明确决定 1 次晋升，blocker 2 次晋升，同项目串行锁，Context Resolver 读取。 | `5f29a5aa` [x] |
| **N07** | L3、长期来源与 supersede 垂直切片 | 跨项目双证据晋升，来源失效置 `unavailable`，revise/supersede 历史与 current 读取。 | `acf0e35a` [x] |
| **N08** | 统一 Markdown 与保留策略垂直切片 | 租户级 `memory_summary.md` 与 `MEMORY.md` 原子替换与免 Agent 重建，30 天快照保留策略。 | `490a8608` [x] |
| **N09** | 时间优先 Recent UI 垂直切片 | AssetIWeave UI 规范，PillTabs 切换视图，日期轨道与项目卡片，展开 details 与 Session 导航。 | `8e2110ea` [x] |
| **N10** | 公共表面迁移与 contract 收缩 | 移除旧 `memory.recent.list`，CLI/Tauri/Skill 统一使用 Snapshot API，逐项目 writer 清除。 | `924ff1fc` [x] |
| **N11** | Q1–Q36 综合验收与发布证据 | 全仓 Gate 绿灯，40 项行为矩阵（M35-V01–V40）验证，回滚与迁移演练完成。 | `a7ccba5f`, `a8d82959` [x] |

---

## 3. 全仓验证 Gate 汇总 (Full Verification Gates)

| 验证项 | 命令 | 结果 | 关键指标 / 输出摘要 |
|---|---|---|---|
| **Rust 编译与测试** | `cargo test --workspace` | **PASS** | **997 passed, 0 failed**, 0 ignored |
| **Rust 代码格式化** | `cargo fmt --all -- --check` | **PASS** | 0 formatting issues |
| **前端类型检查** | `pnpm typecheck` | **PASS** | 0 type errors |
| **前端单元/集成测试** | `pnpm test` | **PASS** | **168 test files passed, 843 tests passed** |
| **前端生产打包** | `pnpm build` | **PASS** | 生产 bundle 构建成功（19.41s），dist 产物符合规范 |
| **Go CLI 语法与并发安全** | `go vet -C cli ./... && go test -C cli -race ./...` | **PASS** | 全部命令包通过，无数据竞态（race conditions） |
| **契约连续生成无漂移** | `pnpm cli:contract`（对独立临时库二次生成） | **PASS** | `cmp` 二次生成 `contract.json` **完全一致（0 字节差）** |
| **Tauri 公共表面矩阵** | `pnpm check:surface-matrix` | **PASS** | 48 explicit exemptions，无未声明 API 漂移 |
| **模块与架构边界** | `pnpm check:boundaries && pnpm test:boundaries` | **PASS** | 架构分层守卫与 boundary self-tests 全部通过 |
| **Git Whitespace 校验** | `git diff --check` | **PASS** | 无 trailing whitespace 或 conflict markers |
| **Memory Builtin Skill 测试** | `python3 scripts/memory-skill-recall.test.py` | **PASS** | 4 tests passed，Snapshot API 契约验证通过 |

---

## 4. 行为验证矩阵 (M35-V01 至 V40)

| ID | 规格项 | 验证场景 | 观察证据 |
|---|---|---|---|
| M35-V01 | L1-01 | 默认设置初始化 | 默认 48h 窗口、02:00 / 14:00 水位、系统内置 Generation Skill。 |
| M35-V02 | L1-03 | 14:00 的 48h Snapshot | 严格截取前 48 小时区间内具有 last activity 的 Session。 |
| M35-V03 | L1-04 | 旧创建但近期有活动的 Session | 成功作为候选纳入；历史导入无活动 Session 被排除。 |
| M35-V04 | L1-05 | 离线错过多个水位 | 启动时聚合只补偿执行最新已到期水位，无冗余级联排队。 |
| M35-V05 | L1-06 | 内容指纹 content fingerprint 不变 | Agent 0 调用，直接生成 `reused` Snapshot 并推进水位。 |
| M35-V06 | L1-06 | Session 退出窗口或 carry-over 到期 | fingerprint 发生变化，触发真实 Agent 生成。 |
| M35-V07 | L1-07 | 项目有工作但无后续建议 | 输出 0 条建议，前端渲染规范的 EmptyState 提示。 |
| M35-V08 | L1-07 | Agent 返回非法数量建议或重复 rank | 触发输出准入校验失败，任务失败重试，不截断为假成功。 |
| M35-V09 | L1-08 | active/blocked 任务跨窗口 | 最多延续 7 天；reused Snapshot 不重置 7 天计时。 |
| M35-V10 | L1-09 | 任务完成/终态 (completed/abandoned) | 在当前成功 Snapshot 呈现一次，下一水位自动退出。 |
| M35-V11 | L1-10 | 上一轮 Snapshot 存在时的增量输入 | 输入仅包含结构化 MemoryItem，不反向读取 Markdown。 |
| M35-V12 | L1-11 | 长 Session 局部更新 | 首包仅传递变化事实与 outline，支持有界按需补读。 |
| M35-V13 | L1-12 | 未归属项目 Session (unassigned) | 放入未归属分类卡片展示，不产生 L2 长期记忆候选。 |
| M35-V14 | L2-03 | 用户明确项目决定 (decision) | 单次 generated Snapshot 观察后即可进入 L2 候选。 |
| M35-V15 | L2-04 | 阻塞性问题/待办 (blocker/todo) | 经过连续两次带新证据的 generated Snapshot 观察后晋升。 |
| M35-V16 | L2-04 | 观察窗口中存在 reused Snapshot | 不累加观察 streak，也不重置已有计数。 |
| M35-V17 | L2-05 | 单纯完成进度 (completion/progress) | 仅在 Recent 展示，严格禁止晋升为 L2 长期记忆。 |
| M35-V18 | L2-06 | 水位无合格 L2 候选 | Project Consolidation Agent 0 调用，保留 current 状态。 |
| M35-V19 | L3-02 | 规则在两个不同项目存在证据 | 成功形成 L3 全局记忆候选。 |
| M35-V20 | L3-02 | 两个相同 project key 的路径 | 严格去重为单项目，不误判定为跨项目。 |
| M35-V21 | L3-04 | 删除已晋升来源 Session | L2/L3 条目保留，来源引用标记为 unavailable (missing)。 |
| M35-V22 | L3-05 | 后续证据推翻长期条目 | 新 revision 设为 current，旧条目标记为 superseded 并记录关系。 |
| M35-V23 | L3-06 | Context Resolver 与 Markdown 投影 | 严格过滤历史，仅向 Agent 注入当前有效 (current) revision。 |
| M35-V24 | SKILL-02 | 复制内置 Skill 为用户副本 | 内置模板保持只读，用户副本复制到 Library，设置自动绑定。 |
| M35-V25 | SKILL-03 | 运行中编辑 Skill | Work Order 指纹改变，旧任务拒绝覆盖新指纹 target。 |
| M35-V26 | SKILL-05 | Skill 尝试未授权能力 (网络/写文件) | ACP capability 严格拒绝，Memory 发布被拦截。 |
| M35-V27 | SKILL-06 | 设置指向已损坏/不存在的 Skill | Agent 0 调用，保留 last-success，任务记录明确错误。 |
| M35-V28 | PROJ-01 | 多项目多阶段生成完毕 | 租户目录下严格仅生成 `memory_summary.md` 与 `MEMORY.md`。 |
| M35-V29 | PROJ-03 | 投影文件被外部误删或损坏 | 调用 rebuild 从 SQLite 真相源瞬时重建完全一致的 Markdown。 |
| M35-V30 | PROJ-04 | 历史快照超过 30 天清理保留期 | 自动归档清理过期快照，L2/L3 关联条目与 revision 完好保留。 |
| M35-V31 | UI-01 | 进入 Recent Memory 工作区 | 默认以时间轨道（日期分段）呈现，段内按项目聚合。 |
| M35-V32 | UI-02 | 切换为按项目聚合视图 | 依赖前端本地分类重组，条目完全一致，零后端调用。 |
| M35-V33 | UI-03 | 点击可用 (available) Session 引用 | 平滑跳转至会话记录详情页并准确定位高亮目标 Session。 |
| M35-V34 | UI-04 | 点击不可用 (unavailable) 引用 | 禁用导航，悬浮提示来源已删除/不可用。 |
| M35-V35 | UI-05 | Recent 工作区 DOM 审计 | 确认无手工刷新、时间微调、条目编辑、反馈打分或深度回忆输入。 |
| M35-V36 | UI-06 | 最新生成任务失败时 | 页面继续平滑渲染上一次成功快照，并在底栏给出更新状态提示。 |
| M35-V37 | AUTH-04 | 多租户隔离审计 | 跨租户 ID 访问严格返回未找到或权限拒绝，零侧信道泄漏。 |
| M35-V38 | OPS-02 | Lease 超时、取消与晚到结果 | 基于 Token 隔离，超时或已被抢占的任务无法提交结果。 |
| M35-V39 | OPS-03 | 内部 Worker 执行机制 | 进程内原生 Rust 执行，零 CLI/AIWC 外部子进程派生。 |
| M35-V40 | OPS-04 | 敏感数据脱敏与日志审计 | 日志、任务快照与事件 payload 中严禁出现 Prompt 与明文正文。 |

---

## 5. 安全审计与 ACP 隔离 (Security & Privacy Audit)

- **只读沙箱能力 (Read-Only ACP)**：
  Memory Generation Agent 在执行中仅被授予只读查询工具，无网络访问（`deny_network`）、无数据库写入权限、无外部子进程调用权限，杜绝恶意 Prompt 注入引发的权限逃逸。
- **正文脱敏 (Data Redaction)**：
  在输入 Agent 组包前与输出持久化前，敏感 Key、Token、密码模式均经过脱敏过滤；TaskRuntime 的 Snapshot 与全局事件中仅记录摘要元数据与引用计数，不暴露会话明文。
- **单层直接软链接与非侵入性**：
  保持源项目与源目录的只读完整性，系统状态统一写入 SQLite，投影文件仅落入 AssetIWeave 应用数据目录，严禁污染用户代码仓库。

---

## 6. 回滚方案与旧数据处置 (Rollback & Legacy Data Policy)

- **数据库迁移保证**：
  Memory 重写的 4 个新迁移（从 49 号至 53 号）均为纯追加操作，未改动任何已发布的历史 migration 校验和。
- **旧表归档与清理**：
  历史旧表（如 `memory_dream_*`、`memory_extractions`、`legacy_memory_items`）已被 `memory_legacy_archive.rs` 纳入只读归档清单，生产写入路径已被彻底剥离。后续若需执行物理 drop，将通过独立的破坏性 migration 工单单独审批发布。
- **故障回滚兜底**：
  若新水位生成发生异常，系统自动维持 `last-success` 快照呈现，不中断前端界面浏览，不污染已有会话数据，并在任务中心保留结构化错误日志以供诊断。

---

## 7. 结论与 Issue #35 关闭依据

至此，**Issue #35 所规划的 Q1–Q36 全部技术决策、N01–N11 垂直切片以及 M35-V01–V40 全部验收场景均已高标准交付并通过自动化验证**。代码库处于全绿灯就绪状态，具备完整发布与 Issue 关闭条件。
