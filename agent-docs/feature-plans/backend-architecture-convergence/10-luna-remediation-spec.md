# SPEC-BA-10：Luna 后端架构收口纠偏施工规范

- 状态：Ready for Agent
- 日期：2026-08-21
- 施工对象：Luna 或同等级执行模型
- 审计基线：`f1cd7c6` 与 2026-08-21 当前工作区
- 上位约束：SPEC-BA-00 至 SPEC-BA-09、仓库架构约束、现有 ADR
- 执行原则：高级模型定义不变量和验收，Luna 按单一垂直切片施工
- 施工 Issue：<https://github.com/util6/assetiweave/issues/1>

## Problem Statement

AssetIWeave 已经完成一轮大规模后端架构迁移，AppRuntime、TaskRuntime、Catalog Task
Provider、TargetCatalog、AgentExecutionRuntime 和 typed error 等结构已经出现，但多个新结构尚未接管
真实生产调用链。当前状态表现为“类型和接口存在，端到端语义仍由旧路径或双轨路径决定”。

从用户视角看，这会产生以下问题：切换租户后当前进程仍可能操作旧租户；搜索索引重建首次启动即与
自身冲突；Engine 方法报告成功但没有执行工作；扫描和批量挂载仍阻塞页面；设置迁移会反复读取旧
字段；后台任务终态结果持续占用内存；扩展错误在边界处丢失分类；动态 Target Provider 只在孤立
测试中成立。与此同时，架构里程碑被提前标记为完成，现有绿色测试没有覆盖这些真实组合故障。

Agent Catalog 和 ACP 还需要遵守已经确定的产品裁决：AssetIWeave 核心版本范围和 Agent 版本只作
观测记录，不作为安装、更新、重装或运行的门禁；版本变化不维护独立变更历史。生命周期门禁只由
可选择的分发、制品完整性、平台适配和 ACP conformance 决定。测试证据应绑定可验证制品身份，而
不是依赖展示版本字符串。

## Solution

通过一组严格排序、每项独立提交的垂直切片，把已经存在的新架构骨架变成唯一生产 Authority。
每个切片必须先增加能够在旧实现上失败的高层行为测试，再修改最小范围代码，最后删除或委托旧
路径。任何任务都不能仅以新增类型、Provider、Registry、getter、局部单元测试或边界脚本通过作为
完成证据。

本轮优先恢复三个 P1 运行时不变量：租户上下文原子切换、搜索索引任务单生命周期、Engine 契约不
得成功 no-op。随后完成 Settings v3 单轨迁移、长任务前端解耦、统一 BatchMountWorkflow、统一
TaskRuntime retention、结构化错误贯通和 TargetCatalog 动态刷新。最后校正 Agent Catalog 证据语义
与里程碑完成状态，并运行完整 verification matrix。

主要测试接缝采用现有最高层边界：ResidentHost 场景通过真实 AppRuntime 与 Tauri command 组合
进行验证；OneShot 场景通过 Engine registry 的公开方法验证；改变持久化状态的领域行为通过共享
AppService workflow 验证；前端只验证公开 service/provider 产生的用户可见状态。只有纯转换逻辑才
使用低层单元测试。

## User Stories

1. 作为使用多个租户的用户，我希望切换租户后下一次操作立即使用新租户，以免数据写入旧租户。
2. 作为桌面端用户，我希望租户切换不依赖重启应用，以便连续完成跨租户工作。
3. 作为后台任务使用者，我希望搜索索引重建能够首次启动成功，以便会话搜索得到最新索引。
4. 作为后台任务使用者，我希望相同搜索重建请求被正确去重，以免产生重复 worker 或僵尸任务。
5. 作为任务观察者，我希望任务的 Running、Succeeded、Failed 和 Cancelled 状态只有一个 Authority，以免 UI 与真实执行状态不一致。
6. 作为 CLI 用户，我希望 Engine 方法返回成功时确实执行了对应工作，以便自动化脚本可以相信结果。
7. 作为 CLI 用户，我希望不受 OneShot 支持的跨调用任务方法明确不可用，而不是返回空成功。
8. 作为桌面端用户，我希望启动扫描后仍能导航、筛选和打开设置，以便长任务不冻结工作区。
9. 作为桌面端用户，我希望只禁用与当前任务冲突的操作，以便无关功能保持可用。
10. 作为桌面端用户，我希望扫描失败显示真实错误，而不是被 mock 数据伪装成成功。
11. 作为批量挂载用户，我希望显式、分组和排他挂载具有一致的进度、取消和部分失败语义。
12. 作为批量挂载用户，我希望批量任务只加载一次共享数据并只做一次最终刷新，以便大批量操作保持高效。
13. 作为自动化调用者，我希望 Tauri 和 Engine 使用同一个挂载 workflow，以便两个入口行为一致。
14. 作为设置用户，我希望 Agent action assignment 只有一个事实源，以免模型和 Agent 选择互相漂移。
15. 作为升级用户，我希望旧执行设置只迁移一次，以免旧字段在后续读取时重新覆盖新配置。
16. 作为备份用户，我希望持久设置进入 SQLite 权威路径，以便备份和恢复覆盖真实执行配置。
17. 作为长期运行桌面应用的用户，我希望终态后台任务按统一策略回收，以免内存和 IPC 列表持续增长。
18. 作为诊断故障的用户，我希望超时、取消、程序缺失、输出超限和非零退出具有稳定错误分类，以便界面提供准确修复建议。
19. 作为 CLI 集成者，我希望 Tauri 和 Engine 对同一错误给出一致 code 和 retryable 语义，以便调用方无需解析错误文本。
20. 作为 Target Provider 维护者，我希望新 Provider 能在不新增 Rust enum 分支的情况下参与 seed、detect、plan 和 mount，以便扩展目标应用。
21. 作为运行中刷新 Provider 的用户，我希望无效 catalog 刷新失败时保留旧快照，以免现有挂载能力被破坏。
22. 作为 ACP 用户，我希望 AssetIWeave 或 Agent 展示版本变化不会阻止安装、更新、重装和运行，以便低频 ACP 目录保持宽松兼容。
23. 作为 ACP 用户，我希望无有效分发、制品校验失败或 ACP conformance 失败时生命周期操作被阻止，以便宽松版本策略不削弱完整性。
24. 作为发布维护者，我希望测试证据绑定分发制品身份和校验信息，以便展示版本变化不要求重建无意义的版本历史。
25. 作为项目维护者，我希望 verification matrix 中有直接证据后才能标记任务完成，以免文档领先于真实实现。
26. 作为高级审计模型，我希望每个 Luna 提交只改变一个 Authority，以便快速定位执行偏移。
27. 作为 Luna 执行模型，我希望每个任务包含失败测试、旧路径、验收命令和非目标，以便不需要重新设计架构。
28. 作为代码审查者，我希望每个提交说明生产 consumer 已切换且旧 Authority 已删除或委托，以便判断迁移是否真正完成。
29. 作为回归测试维护者，我希望测试验证用户可见行为和副作用，而不是只断言类型存在或返回错误，以便门禁能捕获真实组合故障。
30. 作为发布负责人，我希望所有 P1 行为测试、边界检查、前端测试、Rust 测试和 CLI 测试共同通过后再关闭本轮施工，以便发布状态可信。

## Implementation Decisions

### 1. 执行协议

- 本规范拆成十个顺序工作包，工作包之间不得合并提交。
- 每个工作包开始时必须输出工作卡，包含目标 Authority、要删除或委托的旧路径、预计模块、先写的失败测试、验证命令和非目标。
- 每个工作包先提交或至少先运行失败测试，记录它在旧实现上的失败原因，再进行实现。
- 一个提交默认不超过五个实现或测试文件；超过时按 backend workflow、adapter、frontend integration 再拆分。
- Luna 不重新设计 Authority，不新增 `legacy`、`new`、`v2` 平行目录，不新增第二套 registry、runner、settings resolver 或 target path table。
- 每个工作包完成后暂停，由高级模型审计 diff 和行为证据；未签署前不得开始下一个工作包。

### 2. 工作包 LUNA-01：恢复里程碑真实性

- 将当前架构里程碑中被现有证据反驳的项目恢复为未完成状态。
- 为每个未完成 requirement 记录 `incomplete`、`contradicted` 或 `missing evidence`，不得用“代码已存在”替代行为证据。
- 校正 Agent Catalog release 测试中的陈旧 revision 断言，使测试验证规则而不是绑定过期常量。
- 该工作包只修正文档和测试基线，不修改运行时行为。

### 3. 工作包 LUNA-02：租户上下文原子切换

- `AppRuntime` 是当前进程请求上下文唯一 Authority。
- 切换租户和创建并激活租户必须在数据库更新成功后构造新 RuntimeContext，并通过运行时快照原子替换。
- 新上下文构造失败时不得留下数据库 active tenant 与 RuntimeContext 长期分裂；需要明确事务、补偿或重新加载策略。
- 后续 AppService 请求必须从替换后的 runtime snapshot 获取租户，不允许 adapter 或页面维护第二份 active tenant。
- 失败测试必须在同一常驻进程中完成“切换租户后立即 CRUD”，并验证旧租户未被写入。

### 4. 工作包 LUNA-03：搜索索引任务单生命周期

- `TaskRuntime` 是搜索索引任务生命周期唯一 Authority。
- 搜索索引启动只能注册一次；已经注册的 external task 必须启动对应 worker，或者由单次 spawn 同时完成注册和启动。
- Projection 只组合领域进度和 TaskRuntime snapshot，不独立决定 Running 或 terminal 状态。
- 相同 dedup key 的第二次请求返回同一活动任务；不得返回由自身重复注册造成的 conflict。
- worker 启动失败、取消和 panic 都必须进入 terminal 状态，不能遗留 Running 僵尸任务。

### 5. 工作包 LUNA-04：清除 Engine 成功 no-op

- Engine registry 继续作为 method、risk、confirmation 和 exposure 元数据 Authority。
- 每个公开方法必须委托真实 AppService workflow，或明确标记为 OneShot 不支持并从公开 exposure 移除。
- OneShot 不新增无意义的跨调用 get/list/cancel 任务契约；可以同步执行的操作应调用 canonical workflow 并返回真实业务结果。
- compatibility alias 可以保留，但只能委托 canonical method，不能返回固定 null 或空数组。
- Engine contract 发生变化时必须重新生成契约和 surface matrix，不得手工编辑生成物。

### 6. 工作包 LUNA-05：Settings v3 单轨迁移

- SQLite settings repository 成为持久设置 Authority；配置文件只允许作为明确的导入或迁移来源。
- 增加按 schema/document version 门控的一次性迁移；迁移成功后删除旧 execution keys。
- 正常读取、保存、normalize 和 resolver 只读写 canonical action assignments。
- Agent、模型或 action 的切换必须作为一个 canonical assignment 原子更新，不再双写旧 map。
- migration 必须幂等：第一次迁移产生预期 canonical 数据，第二次运行不改变数据。
- Action registration 必须提供 resolver 所需的默认 Agent、required capability 和 policy，默认值不再散落于 legacy 配置。

### 7. 工作包 LUNA-06：长任务前端解耦与真实失败传播

- source scan、skill source scan 和 catalog 长任务的 start 调用必须快速返回 task snapshot。
- Catalog Task Provider 负责全局任务订阅、事件更新和轮询补偿；service 不得无限等待 terminal 状态。
- 发起页面只维护冲突操作状态，不使用页面级 busy 阻塞导航、筛选、设置和无关 CRUD。
- Tauri 运行时错误必须向上返回；mock fallback 只允许浏览器预览环境使用。
- 事件丢失时轮询必须最终收敛，事件正常时应降低轮询频率。

### 8. 工作包 LUNA-07：统一 BatchMountWorkflow

- 在 Application workflow 边界统一 explicit、group 和 exclusive 三种批量模式。
- Tauri ResidentHost worker 和 Engine OneShot 都调用同一 workflow；差别只在任务宿主和返回方式。
- workflow 一次加载 Profile、assets、mount intent 和其他共享数据，先完成 preview，再执行物理动作。
- 每个物理动作前检查取消；取消后不得开始下一个 item。
- 定义成功、部分失败、取消和补偿语义，并向前端保留 item-level error。
- 整个批次默认只进行一次 catalog/status refresh，不允许普通批量继续前端逐项串行调用。

### 9. 工作包 LUNA-08：TaskRuntime 统一 retention

- 所有 ResidentHost 长任务使用同一个终态保留策略，包括扫描、批量挂载、Memory、同步、搜索、市场和 Agent lifecycle。
- retention 同时受最大数量和最大存活时间约束，并在 list/get/start 的稳定位置执行，不依赖某个页面访问特定领域。
- 大型业务结果只能有一个权威存储位置；Projection detail 不重复保存完整 result。
- prune 后 task getter、list、dedup index 和 domain projection 必须一致，不得返回 orphan Running。
- 正在运行的任务和关闭保护所需信息不得被回收。

### 10. 工作包 LUNA-09：结构化错误贯通

- `AppError` 和领域 typed error 保留稳定分类；禁止通过全局 `From<String>` 把未知错误统一标为 external 且 retryable。
- HostProcess 的 missing program、spawn failure、timeout、cancel、output limit、nonzero exit 和 cleanup failure 必须逐类映射到 Extension Kernel。
- Extension Kernel、Agent lifecycle、Tauri 和 Engine 的转换不得通过 `to_string()` 往返。
- Wire error 必须保持稳定 code、retryable 和安全 details；日志可以包含诊断上下文，但不得泄露 token、prompt、环境变量和用户绝对路径。
- LegacyResult 必须从生产 Rust 源码完全退出；守卫直接拒绝新增别名和引用，删除兼容 allowlist。

### 11. 工作包 LUNA-10：TargetCatalog 动态运行时闭环

- Runtime builder 支持注入 TargetCatalog，生产启动使用经校验的 snapshot。
- catalog 刷新在锁外构造和验证，成功后原子替换；失败时保留旧 snapshot。
- defaults seed、target detect、planner 和 mount executor 在同一请求中使用一致 snapshot。
- 动态 Provider 不要求新增 Rust enum 分支或硬编码路径表。
- 兼容 helper 不得在内部重新调用 builtin catalog；必须显式接收 snapshot 或委托 runtime。

### 12. Agent Catalog 与 ACP 版本裁决

- AssetIWeave core range、Agent version 和 catalog revision 均可记录和展示，但不参与 install、update、reinstall、rollback 或 execute 的允许判断。
- 版本字符串改变不要求新增独立 evidence revision 或版本变更历史。
- selectable distribution、平台匹配、artifact identity、checksum、签名策略和 ACP conformance 仍是强制门禁。
- conformance evidence 应关联 distribution/artifact identity；若展示版本与证据中的观测版本不同，只要制品身份一致，不得据此阻止生命周期操作。
- 若证据无法证明当前制品身份，条目不得宣称 tested；这属于证据完整性问题，不是版本兼容问题。

### 13. 完成和提交规则

- 每个工作包使用中文 Conventional Commit，并保持一个 Authority 变化一个提交。
- 工作包只能在失败测试转绿、production consumer 切换、旧路径删除或委托、目标质量门禁通过后标记完成。
- 整体完成前必须重新审计 verification matrix；任一 P1 为 incomplete、contradicted 或 missing evidence 时，里程碑保持未完成。
- 工作区已有的 Agent Catalog、ACP、翻译设置和审计文档修改必须保留，施工前先记录基线，不得覆盖或重置。

## Testing Decisions

- 测试只验证外部行为、稳定状态和持久副作用，不以私有字段、内部调用次数或类型存在作为主要完成证据。
- ResidentHost 的主要接缝是真实 AppRuntime 加公开 Tauri command 组合测试。这一接缝覆盖租户切换、任务注册、取消、dedup、terminal 状态和关闭保护。
- OneShot 的主要接缝是 Engine registry 黑盒调用。测试必须证明成功结果对应真实业务效果，并证明不支持的方法不会成功返回 null。
- 共享 workflow 通过 AppService 集成测试覆盖，Tauri 和 Engine 只增加薄 adapter parity 测试，不复制领域行为测试。
- Settings v3 使用临时 SQLite 数据库覆盖 legacy 导入、一次迁移、二次幂等、legacy 删除、备份恢复和 resolver 单轨读取。
- 前端使用 service/provider/component 组合测试覆盖 start 快速返回、全局进度、event/poll fallback、导航可用、冲突操作禁用和 desktop error 不被 mock 掩盖。
- Batch mount 测试覆盖 explicit、group、exclusive、取消边界、partial failure、compensation、同 Profile dedup、共享加载和单次最终 refresh。
- TaskRuntime 测试覆盖时间与数量 retention、运行任务保护、大 result 不重复、prune 后 dedup 清理和 projection 一致性。
- Extension 测试覆盖 missing program、timeout、cancel、large output、nonzero exit、cleanup failure，以及 Tauri/Engine wire error parity。
- TargetCatalog 使用虚构 Provider 的 runtime 集成测试覆盖 seed、detect、plan、mount，并覆盖无效刷新保留旧 snapshot。
- Agent Catalog 测试把版本字段作为观测数据，分别验证“版本变化不阻断”和“制品完整性或 conformance 失败会阻断”。
- Catalog release 测试不得绑定会频繁过期的固定 revision；应验证格式、单调性规则、制品身份和证据关系。
- 每个新行为测试必须能够说明它在施工前为何失败；仅在新实现上运行成功不构成回归证据。
- 既有测试先例优先复用 TaskRuntime runtime tests、AppService repository tests、Engine contract tests、Catalog Task Provider tests 和 Agent lifecycle e2e fixture，不新增平行测试框架。
- 每个后端提交至少运行 Rust 格式检查、目标 Rust 测试和模块边界检查；涉及前端时运行 typecheck、frontend tests 和 production build；涉及 Engine 时重新生成 contract 并运行 surface matrix、Go vet、Go race tests 和 CLI e2e。
- 最终验收必须运行完整 verification matrix，并逐项记录 achieved、incomplete、contradicted 或 missing evidence，不能只附一条“全量测试通过”。

## Out of Scope

- 不全面重写 Rust 后端或 React 前端。
- 不替换 SQLite、SQLx、Tauri、Go CLI、ACP SDK 或现有事件脊柱。
- 不引入跨进程持久化任务队列；OneShot Engine 仍然一次调用完成后退出。
- 不把 Conversation Catalog、Agent Catalog 和 TargetCatalog 合并成万能插件 manifest。
- 不改变 `asset_mounts` 作为挂载意图唯一事实源的产品决策。
- 不修改已发布 migration 的内容或 checksum；需要 schema 变化时只新增 migration。
- 不重新引入 vendor 专用翻译 executor、旧 Gemini 执行栈或页面直连 Tauri invoke。
- 不以删除缓存、吞掉异常、放宽制品完整性检查或使用 mock 数据代替生产修复。
- 不在本轮增加新的 Agent 市场功能、UI 视觉改版或无关性能优化。
- 不把 Agent 展示版本或 AssetIWeave core version 恢复为 ACP 生命周期门禁。

## Further Notes

### Luna 每次执行时的强制工作卡

- Task ID：只选择一个 LUNA 工作包。
- Objective：描述用户可见行为，不写“新增一个类型”。
- Canonical authority：明确本次完成后谁拥有事实状态。
- Legacy path：明确删除、委托或保留原因及后续删除任务。
- Failing test：写出施工前预期失败及失败原因。
- Production consumer：指出本次会切换的真实入口。
- Verification：列出目标测试、边界检查和生成物检查。
- Out of scope：列出本次禁止顺手修改的领域。

### 高级模型审计签署标准

- 新 Authority 有真实 production consumer。
- 旧 Authority 已删除，或兼容入口只委托 canonical workflow。
- 测试穿过真实组合边界，能够捕获原始故障。
- Tauri 与 Engine 没有复制改变持久状态的业务 workflow。
- 错误和任务状态没有在 adapter 处降级为字符串或第二份事实。
- 文档完成状态与 verification matrix 证据一致。

### 推荐施工顺序

1. LUNA-01：恢复真实基线。
2. LUNA-02：租户上下文。
3. LUNA-03：搜索索引任务。
4. LUNA-04：Engine no-op。
5. LUNA-05：Settings v3。
6. LUNA-06：长任务前端解耦。
7. LUNA-07：BatchMountWorkflow。
8. LUNA-08：Task retention。
9. LUNA-09：typed errors。
10. LUNA-10：TargetCatalog 动态闭环。
11. 复跑 Agent Catalog/ACP 裁决测试和完整 verification matrix。

前三个行为修复完成并经高级模型签署前，不开始 Settings、Batch Mount 或 TargetCatalog 的并行大范围改造。
