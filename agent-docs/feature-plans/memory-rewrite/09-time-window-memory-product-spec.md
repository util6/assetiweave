# Memory 完整产品规格：Q1–Q36 时间窗口、三层漏斗与用户可编辑生成 Skill

- 日期：2026-09-14
- 状态：待用户评审
- 归属：AssetIWeave Memory 重写，补充 Issue #20 与 Issue #30
- 性质：产品与执行契约；不包含本轮生产实现
- 事实源：本轮访谈 Q1–Q36 的最终答复、当前代码、`01-contract.md`、`08-bounded-evidence-execution-spec.md`

## 1. 目的

AssetIWeave 需要把分散的 Session 现场转化成一套低频、可追溯、可持续演进的 Memory：用户打开「近期」即可知道最近做过什么、各项目下一步适合做什么；Agent 在后续工作中可以读取项目级和永久级长期知识；用户能够修改后台生成策略，同时不接管证据范围、权限和持久化规则。

本规格把这一目标收敛为四个产品结果：

1. SQLite 保存结构化 Memory、版本、引用、状态与任务，是唯一 Authority。
2. 近期、项目长期、永久记忆构成三层漏斗，各层具有不同时间语义和晋升规则。
3. 应用只发布两份租户级只读 Markdown，取消逐 Session 和逐项目 Markdown。
4. 发给后台 Agent 的总结策略以用户可编辑的生成 Skill 表达；应用继续掌握固定执行合同。

## 2. 与现有规格的关系

除本节明确修订的内容外，`01-contract.md` 和 `08-bounded-evidence-execution-spec.md` 的 Authority、ACP 隔离、有界证据、短引用、预算、Durable Job、last-success、脱敏、租户隔离和 AppService 边界继续成立。

本规格作出以下显式修订：

| 修订 | 现有行为 | 新契约 |
|---|---|---|
| R1：近期窗口 | C-D04 固定滚动 72 小时 | 用户在统一设置页选择 24/48/72 小时，默认 48 小时；窗口固定在最近一次生成水位 |
| R2：调度 | 主要由 Session 完成或 idle 触发下游生成 | 面向近期产品的聚合快照默认每日 02:00、14:00 两次生成，两处水位均可配置 |
| R3：Markdown | 逐项目 `MEMORY.md` 加全局 `memory_summary.md`、`MEMORY.md` | 只发布租户级 `memory_summary.md` 与 `MEMORY.md`；项目仅作为条目分组，不形成文件 |
| R4：删除传播 | C-P08 会让删除或排除向 Project/Global 级联失效 | 近期条目立即退出；已经晋升的长期条目继续存在，引用转为不可用，后续证据可将其取代 |
| R5：Recipe | 结构化 Memory Recipe | 后台生成提示词以真正的生成 Skill 表达，`SKILL.md` 可由用户修改并按资产版本绑定执行 |
| R6：近期交互 | 现有表面允许投影参数或重建入口 | 时间窗口只在统一设置页配置；近期页面不提供刷新、窗口设置或 Memory 状态修改 |

这些修订进入实现前必须同步回稳定 Contract ID、GitHub Issue 和执行卡，避免 Agent 同时读取到相互冲突的 72 小时、逐项目文件或删除级联语义。

### 2.1 Q1–Q36 决策追踪

本表是访谈决策的完整追踪索引，不以早期建议覆盖用户后续答复。发生冲突时采用“用户明确答复优先、后答复优先”的规则；正文各节是实现契约，本表用于证明每个问题均已被纳入。

| 问题 | 最终决策 | 正文落点 |
|---|---|---|
| Q1 | Memory 采用三层漏斗：L1 近期、L2 按项目归属的长期条目、L3 永久记忆；时间窗口是上游入口，不生成逐项目文件。 | §3.4、§4、§5、§6、§10 |
| Q2 | 先按统一时间窗口选取 Session，再在窗口内按项目组织；时间视图是默认入口，项目视图是同一数据集的另一投影，不重复调用 Agent。 | §4.2、§4.6、§11.2 |
| Q3 | Memory 一级入口只保留「近期」与独立的「深度回忆」，不新增 Inspector；Markdown 是只读外部投影，不是页面或查询 Authority。 | §10.1、§11.1、§17 |
| Q4 | 窗口可选 24/48/72 小时，默认 48 小时；Session 以活动时间进入候选集，窗口最终固定锚定成功生成的目标水位。 | §4.1–§4.3 |
| Q5 | 条目离开 L1 窗口不等于删除事实；Session Memory 留在 SQLite，有长期价值的内容进入 L2/L3。 | §4.5、§5、§6、§10.1 |
| Q6 | 近期支持“按项目”和“按时间”两种确定性投影；两者共享 Snapshot、Memory Item 与 Session references，不产生两套总结。 | §4.6、§11.2–§11.3 |
| Q7 | Agent 必须返回结构化 Memory Item，包括稳定身份、层级、项目、状态、摘要、建议、时间和引用；应用据此投影 UI/Markdown。 | §3.2、§10.1、§14 |
| Q8 | `active`、`blocked`、`waiting` 可续接；终态展示一次后退出；续接最长 7 天，新证据可刷新状态。 | §4.5 |
| Q9 | L1→L2 保存长期决定、约束、验证结论、反复出现的阻塞等；L2→L3 仅保存明确全局规则或跨项目稳定知识；Q31 细化准入。 | §5、§6、§15 M09–M12 |
| Q10 | 只发布租户级 `memory_summary.md` 和 `MEMORY.md`；项目是文档内分组，不按项目分文件。 | §10.2–§10.4 |
| Q11 | 长 Session 只消费窗口内新增或变化的事实及必要上下文，不重复灌入完整历史；未完成事项由结构化历史续接。 | §4.3、§8.3、§14 |
| Q12 | 产品目标是低频提供“昨天/上一阶段做了什么、下一步做什么”的建议，不采用每 Session 或高频即时更新。 | §1、§4.4、§17 |
| Q13 | `MEMORY.md` 由应用生成且只读；同理 `memory_summary.md` 也是只读投影，用户修改入口位于生成 Skill 而非结果文件。 | §8、§10.2 |
| Q14 | 原 Session 被删除、缺失或排除后，已晋升 L2/L3 的长期条目继续存在；只改变引用可用性。 | §7、§15 M13 |
| Q15 | Memory 管线不与用户对话、不提问、不等待确认；近期页面不提供完成、忽略、置顶、采纳、编辑或删除状态。 | §8.3、§11.3–§11.4 |
| Q16 | L1 一天生成两次，而不是自然日仅一次或随 Session 高频生成。 | §4.2、§4.4 |
| Q17 | 每次成功生成形成固定 Snapshot；两次水位之间页面保持稳定，仅显示最近成功结果。 | §3.3、§4.2、§11.4 |
| Q18 | L1 每日检查两次；L2 只在出现合格晋升候选时 Consolidate；L3 按周级或候选阈值低频 Consolidate。 | §5.2、§6.2 |
| Q19 | 未完成条目在没有用户反馈的情况下按证据状态续接，最长 7 天；终态只展示一次；L2/L3 不受该短期退出规则影响。 | §4.5、§7 |
| Q20 | 两个默认水位为本地时间 02:00 与 14:00，且均可在统一设置页修改。 | §4.2、§12 |
| Q21 | 应用离线错过多个水位时只补最新已到期水位；每次生成基于结构化历史和变化集合，不补跑全部历史水位。 | §4.2–§4.3、§15 M03 |
| Q22 | 当前目标与上一成功结果无有效变化时，按 fingerprint 复用结果并推进水位，不调用 Agent；UI 标识内容复用。 | §4.4、§11.4、§15 M04 |
| Q23 | 分层运行频率固定为 L1 双水位、L2 候选驱动、L3 周级/阈值驱动，避免每层随每个水位空转。 | §4.2、§5.2、§6.2 |
| Q24 | 新一轮读取上一轮的结构化事实、状态和引用，不把旧 Markdown 作为递归总结输入。 | §4.3、§10.1 |
| Q25 | 每个 tenant 只在 `~/.assetiweave/memories/<tenant>/` 发布两份统一 Markdown；SQLite 是 Authority，文件原子替换。 | §10.1–§10.4 |
| Q26 | Recent Snapshot 历史在 SQLite 默认保留 30 天；Markdown 只保留最新成功投影；L2/L3 revision 链长期保留；不保存完整 Prompt 日志。 | §10.5、§17 |
| Q27 | 近期视觉以 Capacities 的对象化卡片密度和 Linear Timeline 的清晰时间轨为方向；采用日期轨道、日期内项目分组、可展开条目和 Session 卡片，但不冻结最终 CSS。 | §11.1–§11.2、§17 |
| Q28 | 最终拓扑取消逐项目 Markdown：`memory_summary.md` 投影 L1，`MEMORY.md` 投影 L2/L3；Context Resolver 直接读 SQLite。 | §10、§13、§16 |
| Q29 | 发送给 Agent 的 Memory generation prompt 必须以用户可编辑 Skill 表达，不是仅由系统硬编码的一大段 Prompt。 | §8.1–§8.4 |
| Q30 | 24/48/72 小时窗口锚定目标生成水位，而不是随页面当前时间滚动；例如 14:00 的 48h Snapshot 固定覆盖前 48 小时。 | §4.2、§15 M02 |
| Q31 | Agent 负责提名、应用负责准入：明确项目决定可立即晋升；普通 blocker/todo/research 需连续两个成功 Snapshot；完成/普通进展不晋升；L3 需明确全局声明或至少两个项目独立支持。 | §5.2、§6.2、§15 M09–M12 |
| Q32 | 长期条目保留 revision、source availability 与 supersedes 链；来源失效后禁止跳转，后续有效证据可纠正或取代，Context 默认读当前 revision。 | §7、§13、§15 M13–M14 |
| Q33 | 每个项目展示“窗口内做了什么”、0–3 条证据支持的下一步，以及紧凑的决定/验证/阻塞等条目；没有下一步时明确为空；未归属项目不晋升 L2。 | §4.6–§4.7、§15 M07–M08 |
| Q34 | 生成失败时继续显示 last-success、生成时间和覆盖窗口，并安静标注“更新未完成”；不弹窗，详情进入任务中心。 | §8.6、§11.4、§14、§15 M21 |
| Q35 | 新增独立 `assetiweave-memory-generation`；内置模板由系统管理，用户编辑普通 Skill 资产副本；任务固定 asset/revision/hash；无效 Skill 不调用 Agent、不静默回退、不扩权；首版仅 tenant 级。 | §8、§12、§15 M17–M19 |
| Q36 | 48h 默认值与水位只在统一设置页配置；近期页无刷新/窗口设置，除展开与 Session 导航外只读；Session 跳到对话记录详情；「深度回忆」是现有 `assetiweave-memory` 的独立应用功能；外部前端稿只冻结信息架构。 | §8.1、§11、§12、§17 |

## 3. 术语与 Authority

### 3.1 来源事实

Conversation、Session、Question、Turn、Part 和 Content Node locator 是来源事实。Memory 不读取 Provider 私有历史来补造已经缺失的内容。

### 3.2 Memory Item

Memory Item 是从来源事实提取的结构化语义单元。至少具有：

- 稳定逻辑 ID 与 revision；
- tenant、project key 和适用范围；
- `progress`、`decision`、`research`、`verification`、`blocker`、`follow_up` 等语义类别；
- 标题、摘要、理由、状态和时间；
- source references 及其可用性；
- 所属层级、晋升来源和取代关系；
- 生成 Skill、合同、预算策略和输入 fingerprint。

同一事实跨窗口继续存在时更新其 revision，不创建多个互不相关的重复事实。

### 3.3 Snapshot

Snapshot 是某个生成水位上，依据一个固定时间窗口得到的完整近期视图。Snapshot 引用 Memory Item，不复制出另一套事实 Authority。

### 3.4 三层命名

| 层级 | 产品名称 | 主要用途 | 时间属性 |
|---|---|---|---|
| L1 | 近期记忆 | 回顾最近工作并推荐下一步 | 24/48/72 小时窗口，允许有限续接 |
| L2 | 项目长期记忆 | 保存项目内长期有价值的决定、约束、阻塞和结论 | 无固定窗口，按项目归属 |
| L3 | 永久记忆 | 保存跨项目稳定偏好、工作方式和全局规则 | 无时间到期，但允许被修订 |

SQLite 分层存储这些结构化数据。Markdown 只是最新成功状态的确定性投影。

## 4. 第一层：近期记忆

### 4.1 窗口设置

- 可选值固定为 24、48、72 小时。
- 新用户和未迁移用户默认 48 小时。
- 设置入口只位于统一设置页。
- 近期页面只显示当前有效窗口，不提供窗口切换控件。
- 修改窗口设置会产生新的目标 fingerprint，并在下一次合格运行中生成对应 Snapshot。

### 4.2 双水位调度

- 默认水位为用户本地时区的 02:00 和 14:00。
- 两个水位均可在统一设置页修改，必须是两个不同的本地时间。
- Snapshot 的结束时间固定为实际目标水位；窗口开始时间为结束时间减去配置时长。
- 页面在下一次成功 Snapshot 发布前保持稳定，不随当前时钟持续漂移。
- 应用关闭期间错过多个水位时，恢复后只生成最新一个已经到期的水位，不补造每个历史水位。
- 新输入到达正在排队或运行的同一水位时继续采用目标水位合并，不制造无界重复任务。

例如：配置为 48 小时，目标水位为 2026-09-14 14:00，则 Snapshot 覆盖 `[2026-09-12 14:00, 2026-09-14 14:00]`。

### 4.3 输入集合

- 以 Session `last_activity_at` 是否落入窗口作为近期来源筛选基础。
- Session 的完成、idle、排除、缺失、tenant 和内部执行隔离规则继续复用现有合同。
- Snapshot 读取 canonical Conversation、成功 Session Memory、当前长期条目和上一份结构化 Snapshot 状态。
- 上一份 Markdown 不作为递归总结输入。
- 同一运行使用固定来源 revision、Skill revision、合同版本和水位。

### 4.4 低频与无变化策略

- 定时水位是近期聚合的主要触发方式，不因每条 Session 更新立即调用 Agent。
- 如果从上一成功水位到当前目标水位之间没有改变 Snapshot 结果所需的有效变化，复用上一成功结果并推进水位，不调用 Agent。
- 无可记忆内容、预算不足、证据未读完整和执行失败是不同终态。
- 失败、取消或不完整输出不覆盖 last-success。

### 4.5 延续与退出

- `active`、`blocked`、`waiting` 的未完成事项可以跨出原始 24/48/72 小时窗口继续保留。
- 续接最长为 7 天；新的 Session 证据可以刷新状态和续接依据。
- 已完成、已验证、已取消或被取代的事项在形成终态的第一个成功 Snapshot 中展示一次，之后退出近期层。
- 来源被删除、缺失或排除的未晋升近期条目立即退出下一成功 Snapshot。
- 条目退出近期层不等于删除其审计 revision。

### 4.6 项目与日期组织

- 时间窗口是第一筛选维度。
- 近期数据支持两种确定性投影：默认的时间视图采用日期轨道，并在日期内部按项目分组；项目视图按项目汇总同一个窗口。
- 切换投影只改变客户端分组，不创建新 Snapshot，也不再次调用 Agent。
- 项目身份继续采用登记项目根目录、Git worktree 根目录、规范化 cwd 的解析合同；不同 worktree 保持分离。
- 没有项目归属的 Session 进入「未归属项目」分组，仍可参与近期总结。
- 「未归属项目」中的条目在未获得项目归属前不晋升 L2。
- 同一项目可以出现在多个日期段；所有条目仍引用同一个结构化项目身份。
- 项目视图、时间视图和 Markdown 大纲是同一结构化数据集的不同确定性投影；前端按 SQLite 中的时间与 project key 分组，不解析 Agent Markdown 来识别项目。

### 4.7 项目近期内容

每个窗口内有合格 Session 的项目应产生：

1. 一段“这个窗口做了什么”的简明项目摘要；
2. 0–3 条有直接证据的下一步建议；
3. 值得保留在近期的决定、进展、研究、验证、阻塞与待办条目；
4. 每条内容使用到的 Session references。

没有证据支持的下一步保持为空。产品可以显示“本窗口没有形成明确下一步”，Agent 不为填充界面而生成建议。

## 5. 第二层：项目长期记忆

### 5.1 内容边界

L2 保存对项目未来工作仍有价值的内容，例如：

- 用户已经确认的架构或产品决定；
- 项目级不变量、约束和术语；
- 可复用的验证结论与失败原因；
- 持续存在的阻塞和待办；
- 被否决方案及其理由；
- 后续工作必须知道的上下文。

普通进度播报、一次性命令输出和已经完成且没有复用价值的事项不进入 L2。

### 5.2 L1 → L2 晋升

Agent 可以提名，应用负责准入：

- 用户明确确认的项目决定或约束，在引用有效且归因明确时可以立即晋升。
- 普通阻塞、待办和研究结论需要连续存在于两个成功 Snapshot，才具备晋升资格。
- 完成播报和短期进度不能仅因重复出现而晋升。
- 准入检查验证 tenant、project key、语义类别、状态、来源引用、当前 revision 和重复关系。
- 冲突候选进入待合并状态，由 Consolidation 形成一个当前有效条目并保留取代关系。
- Project Consolidation 只在至少出现一个通过准入的 L2 候选或既有 L2 输入发生有效变化时入队；普通近期水位本身不触发空转 Agent 调用。

L2 在 SQLite 中按项目身份组织，但不按项目生成文件。

## 6. 第三层：永久记忆

### 6.1 内容边界

L3 仅保存：

- 用户明确声明的全局规则或长期偏好；
- 跨项目稳定成立的工作方式；
- 多个项目都需要遵循的通用约束和术语；
- 项目长期记忆的轻量索引。

L3 不复制项目详细进度、项目内部待办或单次 Session 的局部偏好。

### 6.2 L2 → L3 晋升

满足以下任一条件时可以形成候选：

1. 用户明确将其声明为跨项目或全局规则；
2. 同一偏好、约束或工作方式在至少两个项目中得到独立证据支持。

Global Consolidation 只消费成功、当前有效的 L2 输入。Agent 提名后，应用继续验证适用范围、引用和冲突状态。L3 按既有低频策略运行：有候选时进入等待集合，并通过周级或候选数量阈值收口，避免每个近期水位都调用 Global Agent。具体候选数量是版本化资源策略参数，在首张实施卡中随预算基线冻结；它只影响运行频率，不改变本节定义的晋升资格。

## 7. 长期条目的持续存在与修订

- 晋升为 L2/L3 后，原 Session 删除、缺失或被排除不会删除长期条目。
- 相应 source reference 标记为 `unavailable`，产品不再提供无效跳转。
- 后续有效证据可以纠正或取代长期条目。
- 被取代条目标记为 `superseded` 并指向新 revision，不静默改写历史内容。
- Context Resolver、Recall 和 Markdown 默认只采用当前有效 revision。
- 审计读取可以看到 revision 链和引用可用性，但内部 ID 不进入普通用户正文。
- 用户排除规则仍阻止被排除来源产生新的近期条目或新的长期晋升候选。

该规则有意区分“来源生命周期”和“已经形成的长期知识生命周期”。

## 8. 用户可编辑的 Memory Generation Skill

### 8.1 Skill 分工

新增独立的 `assetiweave-memory-generation` Skill，专门表达后台 Agent 的 Memory 总结提示词。

现有 `assetiweave-memory` Skill 继续作为应用内「深度回忆」及 Recent、Context、Project、Recall 查询能力的外部入口。深度回忆是独立功能，不属于近期生成链路，本规格不改变其产品语义。

### 8.2 默认与用户版本

- 软件发布一个经过验证的内置生成 Skill 模板。
- `.system` 中的内置模板保持应用管理，不作为用户直接编辑目标。
- 用户可以在普通 Skill 资产层创建或选择一个可编辑副本。
- 统一设置页保存当前激活的 Memory Generation Skill 资产身份。
- 未配置用户版本时使用内置默认模板。
- 产品提供打开当前 Skill、恢复默认模板和重新选择 Skill 的入口。
- 第一版只支持租户级激活项，不支持逐项目 Skill 覆盖。

### 8.3 ACP 执行组成

后台执行由三部分组成：

1. **固定执行信封**：由应用生成，包含 tenant/scope 绑定、目标水位、预算、工具白名单、输出 Schema、引用和提交规则。
2. **用户可编辑生成 Skill**：定义提取重点、忽略主题、项目术语、表达方式、总结步骤和内容组织。
3. **有界证据工具**：提供 Session outline、范围内 search、Question 读取和 Content Node 读取。

ACP 请求只发送精简启动指令、Work Order 和执行所需的 Skill 引用或受控挂载，不再把整段生成策略及全量 Session JSON 拼接成巨型 Prompt。Agent 按 Skill 工作，并只通过固定工具补读证据。

整个生成过程是无交互后台任务。Agent 遇到未知、缺失或矛盾证据时返回结构化未知、缺口或失败状态，不向用户提问、不请求批准，也不等待用户补充信息。

### 8.4 Skill 版本绑定

任务入队时固定：

- Skill asset ID；
- Skill revision；
- `SKILL.md` content hash；
- Memory contract version；
- projection policy version；
- budget policy version；
- source revision 与目标 watermark。

运行中的任务继续使用已固定快照。Skill 修改形成新的输入 fingerprint；旧运行结果不得覆盖新目标。是否重建历史仍沿用 scope rebuild 与失效策略，不通过修改磁盘文件偷偷改变已经完成任务的含义。

### 8.5 固定权限边界

用户 Skill 可以决定“如何总结”，不能决定：

- 读取哪个 tenant 或越过当前 Session/项目范围；
- 新增工具、网络、协作 Agent 或文件系统权限；
- 修改预算、lease、retry、watermark 或任务状态；
- 绕过输出 Schema、引用验证、脱敏和准入；
- 直接写 SQLite、Markdown 投影或 Conversation；
- 递归调用 Memory 管线。

Skill 文本和来源 Conversation 都按不可信数据处理。能力由 ACP 执行配置和 AppService 固定，而不是依赖 Skill 中的自我声明。

### 8.6 无效 Skill

- Frontmatter、内容、资产身份或挂载不符合合同时时，不调用 Agent。
- 当前目标水位标记为明确的配置失败。
- last-success 继续可读，不发布部分 Snapshot 或空白 Markdown。
- 产品不静默回退到旧用户 Skill，以免用户误以为修改已经生效。
- 错误详情进入任务中心和统一设置页；近期页面仅显示被动的“更新未完成”状态。

## 9. 内置管线与 AIWC 的边界

AIWC + `assetiweave-memory` Skill 继续提供用户主动探索 Session 现场的开放能力。内置 Memory Generation Skill 不把自动管线退化为对 AIWC 命令的简单封装。

内置管线额外承担：

- 按水位确定范围并选择变化集合；
- 准备高密度证据首包和短引用映射；
- 强制补读预算、工具白名单和 tenant 隔离；
- Durable Job、lease、heartbeat、取消、重试和恢复；
- fingerprint 幂等、缓存和无变化跳过；
- 结果 Schema、引用、脱敏和晋升准入；
- L1/L2/L3 生命周期、取代关系和失效传播；
- SQLite 持久化、原子 Markdown 投影和 UI 刷新。

AIWC 和内置管线共享 AppService 下的读取语义。内部 Worker 不启动 AIWC 子进程，也不让 CLI 成为业务 Authority。

## 10. SQLite 与 Markdown 投影

### 10.1 SQLite Authority

SQLite 至少保存以下逻辑实体；正式表名由实现计划和追加 migration 确定：

- Session Memory 事实和引用；
- 稳定 Memory Item 与 revision；
- Recent Snapshot、watermark、窗口和条目成员关系；
- L2 项目长期条目与项目身份；
- L3 永久条目；
- promotion、supersedes 和 source availability；
- Generation Skill 快照与版本身份；
- Durable Job、last-success、预算、coverage 和错误状态。

Card、日期轨道和 Markdown 标题不形成新的持久实体。

近期页面和 Context Resolver 直接读取 SQLite 投影 DTO；两份 Markdown 面向人类检查与外部工具读取，不作为前端分组或业务查询输入。

### 10.2 唯一文档目录

每个 tenant 只发布：

```text
~/.assetiweave/memories/<tenant>/
├── memory_summary.md
└── MEMORY.md
```

两个文件均由应用生成、只读、可由 SQLite 重建。发布采用临时文件、内容验证、fsync 和原子替换；失败不覆盖上一成功版本。

### 10.3 `memory_summary.md`

只包含最新成功 L1 Snapshot：

```markdown
# Recent Memory

- Window: 48h
- From: 2026-09-12 14:00
- To: 2026-09-14 14:00
- Generated: 2026-09-14 14:00

## 2026-09-14

### AssetIWeave

#### What changed
...

#### Suggested next steps
- ...

#### Memory items
- ...
```

日期是第一层，项目是日期内子分组。投影只写人类可读引用，不暴露数据库内部 ID 或原始 locator。

### 10.4 `MEMORY.md`

只包含当前有效 L2/L3：

```markdown
# Memory

## Project Memory

### AssetIWeave
- ...

### util6-agents
- ...

## Permanent Memory
- ...
```

不创建逐项目目录、逐项目 `MEMORY.md`、逐 Session Markdown 或模型原始 Prompt 文件。项目筛选和 Context Resolver 直接查询 SQLite。

### 10.5 保留策略

- SQLite 中的 Recent Snapshot 历史默认保留 30 天。
- 磁盘只保留最新成功 `memory_summary.md`，不创建近期版本文件集合。
- 长期条目的 revision 和取代链按长期知识生命周期保留。
- 旧逐项目 Markdown 的迁移、归档和退出必须通过单独执行卡完成；新管线不继续写双轨。

## 11. 近期页面功能契约

### 11.1 范围

Memory 一级入口继续包含「近期」和独立的「深度回忆」。本节只定义「近期」。

当前外部视觉草稿为 `~/Downloads/assetiweave-recent-memory-craft-date-rail.html`。它是可变的设计输入，不是仓库事实源。当前确认的是其日期轨道、日期内项目分组、可展开 Memory 和 Session 卡片的信息架构；CSS、颜色、尺寸和最终组件拆分等待用户完成视觉微调后另行收口。

### 11.2 页面结构

- 页面标题显示「近期」及当前配置的只读时间范围说明。
- 页面提供“按项目 / 按时间”的视图投影切换；这不是时间窗口设置，也不触发后台生成。
- 默认按时间展示日期轨道，并在每个日期下按项目分组；项目视图按项目汇总同一个窗口。
- 项目区域展示窗口摘要、0–3 条下一步建议和其他合格 Memory Item。
- 条目默认显示类别、时间、标题与简述。
- 展开后显示理由、生成水位、Snapshot 和相关 Session。
- Session 卡片至少展示来源应用、Session 标题和相对时间。
- 点击 Session 跳转到「对话记录」的详细浏览页，并选中对应 Session。
- 来源不可用时显示不可用状态，不提供失效跳转。

### 11.3 允许的页面操作

- 展开或收起 Memory Item；
- 在同一 Snapshot 上切换“按项目 / 按时间”投影；
- 点击 Session 进入对话记录详细浏览；
- 使用应用既有导航离开页面。

近期页面不提供：

- 24/48/72 小时切换；
- 刷新或立即生成；
- 完成、忽略、置顶、采纳或删除建议；
- 编辑 Markdown 或 Memory Item；
- 深度回忆的对话输入和结果面板。

手动重建属于统一设置或任务中心的维护能力，不是近期页面的主要操作。

### 11.4 状态呈现

- 页面始终读取 last-success。
- 正常状态显示 Snapshot 的生成时间和覆盖区间。
- 当前水位复用上一成功语义结果时，显示安静的“内容复用”状态，避免用户误以为后台遗漏执行。
- 最新水位失败或未完成时，以安静、非阻断的状态提示“更新未完成”。
- 页面不弹窗、不主动通知、不要求用户回答 Memory 问题。
- 详细失败原因、重试和取消进入统一任务中心。

## 12. 统一设置页契约

Memory 设置至少包含：

| 设置 | 类型 | 默认值 |
|---|---|---|
| 近期时间窗口 | 枚举：24h / 48h / 72h | 48h |
| 上午水位 | 本地时间 | 02:00 |
| 下午水位 | 本地时间 | 14:00 |
| Memory Generation Skill | Skill 资产选择 | 内置默认 Skill |

设置页提供当前 Skill 的打开与恢复默认入口。时间窗口和水位改变只影响后续目标，不在近期页面制造即时、阻塞式 Agent 调用。

## 13. Context 与 Recall 读取

- Context Resolver 继续按永久记忆 → 当前项目长期记忆 → 少量相关 Session Memory 编译预算化上下文。
- L1 近期 Snapshot 服务于用户续接和候选晋升，不默认整份注入每个新 Session。
- `assetiweave-memory` 及应用内深度回忆继续通过只读 Memory API 检索结构化 Authority。
- Recall 只返回当前有效 revision；需要解释历史变化时可以读取取代链。
- 无效或不可用 source reference 不妨碍已晋升长期条目被检索，但必须如实呈现引用状态。

## 14. 失败与一致性

- 当前 Snapshot 发布前再次验证 source revision、Skill hash、合同版本和目标 watermark。
- 晚到的旧 Skill、旧来源或旧窗口结果不能覆盖新目标。
- Agent 输出缺少必需结构、引用越界、预算耗尽或证据不完整时不发布成功版本。
- Markdown 发布失败不回滚 SQLite last-success；后台保留可重建状态并重试投影。
- SQLite 提交失败不发布对应 Markdown。
- 页面、Engine、CLI 和 Skill 读取同一 last-success 与 source availability 语义。

## 15. 验收矩阵

| ID | 输入或动作 | 可观察结果 |
|---|---|---|
| M01 | 新用户首次启用 Memory | 设置为 48h、02:00、14:00，近期页面没有窗口切换和刷新按钮 |
| M02 | 在 14:00 生成 48h Snapshot | 覆盖区间严格锚定前 48 小时，页面在下一成功水位前保持相同 Snapshot |
| M03 | 应用关闭期间错过多个水位后启动 | 只排队最新到期水位，不为每个错过水位调用 Agent |
| M04 | 当前水位相对上一成功结果没有有效变化 | 推进水位并复用结果，不调用 Agent，不产生重复条目 |
| M05 | 未完成事项离开原始窗口 | 有证据时最长续接 7 天；终态展示一次后退出近期 |
| M06 | 同一项目在多个日期有 Session | 日期优先、日期内项目分组；条目引用同一 project key |
| M07 | Session 没有项目归属 | 出现在「未归属项目」，不晋升 L2 |
| M08 | 项目有近期进展但没有明确下一步 | 展示项目进展和“没有明确下一步”，不生成填充建议 |
| M09 | 用户确认项目级决定 | Agent 可提名并由应用验证后晋升 L2 |
| M10 | 普通阻塞只出现一次 | 保持 L1，不立即晋升 L2 |
| M11 | 同一阻塞连续两个成功 Snapshot 存在 | 具备 L2 候选资格，仍需引用与状态准入 |
| M12 | 同一偏好在两个项目得到独立支持 | 具备 L3 候选资格；Global Consolidation 不复制项目细节 |
| M13 | 删除已晋升条目的原 Session | L1 来源退出；L2/L3 条目继续存在，引用不可用，Context 仍读取当前有效条目 |
| M14 | 后续证据推翻长期条目 | 新 revision 成为当前版本，旧版本标记 superseded 并可审计 |
| M15 | 成功生成全部层级 | 只存在租户级 `memory_summary.md`、`MEMORY.md`，没有新的逐 Session/逐项目 Markdown |
| M16 | 删除或损坏两份 Markdown | 可从 SQLite 重建出语义一致的最新成功投影 |
| M17 | 用户修改激活的生成 `SKILL.md` | 新任务绑定新的 Skill revision/hash，运行中旧任务不能覆盖新目标 |
| M18 | Skill 请求越权工具、网络或直接写库 | 固定执行合同拒绝扩权，未发布 Memory |
| M19 | Skill 格式或挂载无效 | 不调用 Agent，保留 last-success，任务中心显示配置错误 |
| M20 | 点击近期条目的 Session | 打开对话记录详细浏览并定位对应 Session；不可用引用不触发跳转 |
| M21 | 最新水位生成失败 | 近期继续展示 last-success，并被动标注更新未完成 |
| M22 | AIWC 与内置管线读取同一 Session 范围 | 共享证据读取语义；内部 Worker 未启动 AIWC 子进程 |
| M23 | 同一 Snapshot 切换“按项目 / 按时间” | 只改变客户端分组，不调用 Agent；条目和 Session references 集合完全一致 |
| M24 | 长 Session 跨越多个窗口且只有局部内容变化 | 新任务只消费变化事实和必要上下文，不重复提交完整 Session 历史 |
| M25 | L1 水位到期但没有 L2/L3 候选 | 只处理 L1；Project/Global Consolidation 不空转调用 Agent |
| M26 | 新 Snapshot 需要续接上一轮未完成事项 | 从结构化历史读取事实、状态和引用，不读取旧 Markdown 递归总结 |
| M27 | 用户检查 Memory 文件 | 两份 Markdown 均为应用生成的只读投影；修改总结策略必须编辑激活的生成 Skill |
| M28 | Recent Snapshot 超过 30 天且不再被保留策略引用 | SQLite 可清理 Snapshot 历史；L2/L3 revision 和取代链继续存在 |
| M29 | 内容 fingerprint 未变化并复用上一结果 | 页面显示当前目标水位及“内容复用”状态，语义内容保持不变 |

## 16. 迁移约束

- 数据库变化只使用追加 migration，不改写历史 migration。
- 现有 Session/Project/Global last-success 在 expand 阶段继续可读。
- 新结构化长期条目与统一文档投影通过版本化合同切换。
- 旧逐项目 Markdown 在新 SQLite Authority 和两份统一投影验证成功后停止写入，再按执行卡归档；不长期维持新旧双轨。
- 迁移失败保留旧 last-success 和回滚入口，不删除来源 Conversation。
- Engine DTO 或公开方法改变时运行 `pnpm cli:contract`，Tauri、CLI、Skill 与 frontend service 必须同步。

## 17. 非目标

- 本规格不冻结近期页面的最终 CSS、颜色、动效或像素尺寸。
- 不重写应用内深度回忆、Recall Session 或现有 `assetiweave-memory` 查询 Skill。
- 不提供逐项目 Memory Generation Skill。
- 不提供实时或每 Session 高频近期总结。
- 不增加建议的完成、忽略、采纳、置顶或用户反馈状态。
- 不生成逐 Session、逐项目 Markdown 或原始 Agent Prompt 归档。
- 不让用户 Skill 控制权限、预算、Schema、调度或持久化。
- 不让内置 Worker 通过 CLI/AIWC 回环执行。
- 不把 Markdown 重新提升为读取 Authority。

## 18. 实施前完成条件

进入代码计划前必须完成：

1. 用户评审并接受本文。
2. 将 R1–R6 同步到 `01-contract.md`，为稳定语义分配 Contract ID。
3. 修订 Issue #20/#30 中的 72h、逐项目 Markdown、删除传播和 Recipe 表述。
4. 为新增生成 Skill、设置、Snapshot、长期条目、统一投影和迁移拆分执行卡。
5. 将 M01–M29 映射到 `04-verification-matrix.md` 的自动化与桌面验证门禁。
6. 将用户最终确认的前端稿转化为 Auroraqua-UI 组件契约；在此之前不把外部 HTML 直接复制进生产页面。
