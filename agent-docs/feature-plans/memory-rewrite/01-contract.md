# Memory 重写：不可变执行契约

> **Issue #35 提示**：处理时间窗口、三层漏斗、Generation Skill、统一 Markdown 或新版 Recent UI 时，以 `q1-q36-spec/00-index.md` 与 `01-product-contract.md` 的 `M35-*` 合同为目标 Authority。本文件中的固定 72h、逐项目 Markdown、来源级联删除和 Memory Recipe 语义只代表旧实现基线。

本文件只保存跨 Ticket 稳定约束。DTO、表名和组件名由实现确定；执行卡通过 Contract ID 引用。

## 领域与产品边界

### C-D01 — Conversation 是来源事实

Session、QuestionTurn、Turn、Part 与 Content Node locator 来自 provider-neutral canonical Conversation。Memory 只消费这套合同，不读取宿主 Agent 私有日志，也不写回 Conversation 事实。

### C-D02 — Memory 是可重建派生层

Session Memory、Project Memory、Global Memory、Recent Event、检索索引与 Markdown 文档均由 Conversation 和版本化 Memory 合同派生。它们可失效、重建，不成为第二套 Conversation。

### C-D03 — 项目目录是聚合边界

项目目录解析顺序为：已登记项目根目录 → Git worktree 根目录 → 规范化原始 cwd。解析符号链接和平台路径大小写；同一 worktree 的子目录合并，不同 worktree 保持分离，不按 Git remote 合并。

### C-D04 — 近期是同一事件集的投影

Recent Work 使用 Session `last_activity_at` 的滚动 72 小时窗口。项目视图和时间视图投影同一组 Recent Event；类别固定为 `progress`、`decision`、`research`、`verification`、`blocker`、`follow_up`。

### C-D05 — locator 是身份，Card 是表现

内部引用可包含 record kind、Session、Question、Turn、Part、node order 和 node ID。Question ID 用于语义路由；Content Node locator 用于精确定位；Card 只在前端渲染，不新增 Card 持久化实体。

### C-D06 — 两个用户页面

Memory 一级入口只提供「近期」与「回忆」。Evidence、candidate、Dream、Library、原始 JSON、内部 ID、locator 字段与未渲染 Markdown 不形成用户页面。

## Authority 与存储

### C-A01 — AppService 权威

所有持久状态转换进入 AppService。Tauri、Engine、Go CLI、ACP 工具和前端 service 仅做适配；Go CLI 不访问 SQLite，页面不直接 `invoke(...)`。

### C-A02 — SQLite 是结构化权威

SQLite 保存 Memory 记录、版本、source reference、usage、job、last-success 和 Recall workflow/binding 元数据。Markdown 与语义索引是可重建投影。

### C-A03 — 应用自有 workspace

Memory 文档只写入 AssetIWeave 自有目录。来源目录和第三方项目目录保持只读；文档通过临时文件、验证和原子替换发布。

### C-A04 — Durable Job 与 TaskRuntime 分工

SQLite Job 保存可恢复状态、ownership token、lease、heartbeat、retry、watermark 和错误；TaskRuntime 只管理活动执行、dedup/conflict、进度、取消、shutdown 和有限终态投影。

### C-A05 — Tenant 全链路隔离

Memory 记录、Job、检索索引、文档路径、usage 和 Recall Session 的读取与修改都绑定 tenant；跨 tenant 标识返回统一不可见或拒绝结果。

## 后台管线

### C-P01 — Durable enqueue before ack

Conversation 提交事务写 Outbox。Memory Consumer 先幂等写入持久 Job，再推进 consumer offset；Consumer 不直接调用 Agent，也不只创建内存 Task。

### C-P02 — Backfill 与 cutoff

新 Consumer 注册时记录 cutoff，并补齐此前合格 Conversation。大批提交通过 `sync_run_id` 和 delta 查询恢复 Session 集合，不依赖 Outbox payload 永久容纳全部 ID。

### C-P03 — 稳定 Session 门

明确完成的 Session 可立即归纳；缺少完成信号的 Session 只有在最后活动至少空闲 30 分钟后才可运行 Phase 1。导入时间和创建时间不替代最后活动时间。

### C-P04 — revision 幂等

一个 tenant + Session + source revision/fingerprint + Memory contract version 至多产生一个有效 Phase 1 结果。重复事件、重复扫描和应用重启不增加重复 Job、Memory 或 Recent Event。

### C-P05 — 两阶段并发

Phase 1 跨 Session 有界并发，同一 Session 不并发；Project Consolidation 按规范化项目目录串行，不同项目可并行；Global Consolidation 轻量串行，只消费成功 Project Memory。

### C-P06 — 主动恢复和重试

应用启动执行漏单扫描、过期 lease 恢复和到期 retry 调度。运行中 Job 定期 heartbeat；进程中断后依持久状态恢复，不等待无关用户操作触发。

### C-P07 — 水位合并

新输入到达已排队/运行中的同一 scope 时合并目标水位或安排一个后继 Job，不创建无界重复任务；成功结果记录实际消费的 source watermark。

### C-P08 — 失效传播

Conversation 修订、删除、来源缺失、项目目录变化、排除设置变化和 Memory contract 升级会失效 Session Memory，并推进相应 Project、Global、Recent Event、检索索引和文档投影更新。

### C-P09 — 受限 Consolidation

Phase 1、Project、Global 使用独立 action assignment。生成运行无网络、无协作 Agent、无递归 Memory、无无关插件，只写应用自有 Memory workspace；阻塞 I/O 和 Agent 调用不持有全局应用锁。

## Memory 层级与上下文

### C-M01 — Session Memory

Session Memory 至少保存目标、结果、决定、验证、阻塞、待办、主题、Recent Event、项目目录、生成版本和 source references；身份绑定 Session source revision/fingerprint。

### C-M02 — Project Memory

Project Memory 是主要长期知识层，按规范化项目目录合并已成功 Session Memory；保留 last-success、输入 fingerprint、生成状态与引用。

### C-M03 — Light Global Memory

Global Memory 只保存跨项目稳定偏好、通用工作方式和项目索引，不复制各项目详细内容。

### C-M04 — last-success 非阻塞读取

Context Resolver 与用户读路径始终读取最后成功且完整验证的版本。生成中、失败或文档损坏不阻塞调用方，也不替换 last-success。

### C-M05 — 预算化上下文

`memory.context.resolve` 按 Global Summary → 当前 Project Memory → 少量相关 Session Memory 的优先级编译上下文，遵守 token budget，并返回 context text、revision、generated time 与内部 source references。

### C-M06 — 使用反馈
 
 只有实际被新 Session Context 或 Recall 引用的 Memory 才更新 usage count 与 last used time。usage 影响保留/排序，不篡改 Conversation 事实或 Memory 内容。

### C-M07 — 显式修订 A：取消逐 Session Markdown 依赖

Session 详细记忆、语义事实和 source references 保存在 SQLite；取消逐 Session Markdown 投影。Project/Global 长期文档仍为应用自有目录中的原子发布投影，且可由 SQLite 完整重建。

### C-M08 — 显式修订 B：内置 Memory Recipe 与固定执行合同

可编辑 Memory Recipe 纳入内置生成，控制提取重点、忽略主题、项目术语与表达方式。Recipe 不控制租户、工具白名单、预算、输出准入、数据库写入与任务生命周期；不引入第三方 Pipeline 安装或未经验证的远程 Skill。

### C-M09 — 精简证据包与执行内短引用

系统准备精简证据首包（任务边界、意图与修正、结果证据、索引）；模型通过执行内短引用与有界补读工具交互，不重复传输完整数据库 ID 与冗余环境/源码/日志。一次提取事实，程序统一投影至 Session Memory 和 Recent Events。

## Recall Agent

### C-R01 — 产品内置 Recall Profile

Recall 是产品定义的专属 Agent Profile、固定 Workflow、工具白名单和输出合同。`memory.recall` action 只选择 ACP Agent/model；Skill 可以承载提示，不承担权限边界。

### C-R02 — Persistent 多轮

Recall 支持创建、发送、读取、取消和恢复持久多轮 Session。它复用共享 Agent 执行与 ACP primitives，但本规格不建设通用多 Agent 产品。

### C-R03 — 只读工具

Recall Agent 只获得 Conversation 与 Memory 的只读检索、候选读取和 locator 解析工具。工具授权绑定 tenant 与 Recall Session。

### C-R04 — Hybrid 检索

检索按线索解析/范围过滤 → lexical → semantic → 合并重排 → 候选现场读取执行。语义索引是派生层，Conversation 修订和删除后同步失效。

### C-R05 — 结构化输出

Recall 输出为 `answer`、`sessionReferences`、`contentReferences`、`followUpSuggestions`。自然语言回答不混入内部 ID；引用单独校验、去重并渲染。

### C-R06 — 精确跳转

Session reference 打开对应 Session；content reference 使用现有 Conversation 导航链精确定位 Question/Turn/Part/Content Node，并高亮和滚动目标。

### C-R07 — 独立交互执行

Recall 不复用 Translation 的 OneShot 文本聚合器。共享执行层只增加 Recall 需要的持久 Session、工具活动、取消与恢复语义，OneShot 现有行为保持不变。

### C-R08 — 复用 Conversation 运行记录

Recall 的用户消息、Agent 回答和 turn 内容通过既有 Conversation 合同持久化。Memory 表只保存 Recall workflow 状态、provider binding、任务关联和结构化引用元数据，不复制一份 Memory transcript。

## 设置、安全与迁移

### C-S01 — 生成与使用分离

“生成 Memory”和“使用 Memory”是独立持久设置；Session/来源排除是生成输入过滤，不删除 Conversation。

### C-S02 — Secrets redaction 与高精度保留

发给模型前和结果落库前都执行 secrets redaction。以凭据上下文和明确 secret 格式优先，高熵为辅助；正反样本保护 Git SHA (40-hex)、长文件路径、测试名与代码符号不被误伤。日志、Task snapshot 与公开错误不包含 prompt、生成正文、工具原始输入、凭据或环境变量值。

### C-S03 — 确定性测试

自动测试使用临时 SQLite、可控时钟、Conversation fixtures、Fake AgentExecutor 和 Fake ACP Server。真实 Provider、用户数据库和网络只用于可选 smoke。

### C-S04 — 内部执行统一隔离

内部 Agent 执行记录具有显式执行来源标记；普通 Conversation 列表、FTS 搜索、Recent Work、Memory 调度和 Recall 候选统一隔离内部生成记录，避免递归提取与搜索污染。产品持久 Recall 问答正常保留。

### C-X01 — Expand-contract 切换

新结构通过追加 migration 建立。旧自动生成 Memory 不导入新模型；切换前可生成只读归档。新 UI/Engine/CLI/Skill/后台管线完成后删除旧公开语义，不长期维护 v1/v2。

### C-X02 — 公共表面一致

新表面至少包含 Recent list、Context resolve、Project get、scope rebuild、Recall Session 生命周期、Memory Task 查询/取消/重试。Engine contract 由 Rust 生成，Tauri/CLI/Skill/frontend 与其一致。

### C-X03 — Legacy 只作删除清单

旧 Dream、旧 Recall、candidate、Evidence、Library、旧 API 与旧自动产物不作为新实现行为基准。可复用的底层 redaction、TaskRuntime、Agent runtime 和 locator 基础设施需通过新合同验收。
