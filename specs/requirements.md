# 需求文档：AssetIWeave

## 0. 产品定位

AssetIWeave 是一个本地优先的 AI 文件资产挂载管理桌面应用。它管理的不是某一种固定格式，而是 AI 工具共同演进出的文件资产形态，包括 prompt、rules、memory、skills、MCP 配置、agent 定义、slash command、workflow 等。

核心目标是把分散在不同目录、仓库和工具配置中的 AI 文件资产集中编目、分类、标记、分组，并按用户选择的挂载关系部署出去。目标目录只是由 AssetIWeave 生成的投影结果，不是唯一真实来源。

当前产品决策：AssetIWeave 默认不把源仓库资产复制到本软件目录，也不建立中间集中 symlink 池。默认部署方式是将目标 App 目录中的条目直接软链接到源仓库中的真实资产。资产集中整理作为单独的导出功能提供，由用户显式选择目标目录后复制真实文件。

本项目是一个独立 Tauri App，不依赖现有 `skill-link-manager` 脚本，也不依赖 cc-switch 运行时。cc-switch 只作为对照组和可选迁移来源。

## 1. 目标

### 1.1 必须实现的目标

- 集中管理多个本地文件资产源。
- 支持源内不同层级和不同目录结构的资产发现。
- 支持多种 AI 文件资产类型，而不是只管理 skill。
- 支持给资产设置类型、标签、分组、启用状态和兼容性。
- 支持自定义 CLI/App 目标，不把目标工具固定为某几个内置选项。
- 支持按目标 Profile 生成部署计划，并在执行前预览差异。
- 支持以安全、可解释、可回滚思路部署文件资产。
- 支持从 UI 管理配置，不要求用户直接编辑 JSON。

### 1.2 不做的事情

- 不作为 prompt/skill 在线市场。
- 当前阶段不实现远程账号、云同步、多设备协同。
- 当前阶段不编辑源资产正文，只管理元数据、挂载关系和部署策略。
- 当前阶段不直接修改第三方工具数据库。
- 当前阶段不依赖现有 Python 同步脚本或 launchd 任务。
- 当前阶段不做后台实时同步、云同步或内置 AI API；自动化入口通过本地 CLI/Engine 提供。
- 当前阶段不托管在线市场；允许通过 provider 搜索互联网候选 Skill，并把用户确认的远程 `SKILL.md` 目录下载导入为本地受管资产。

## 2. 用户故事

### 2.1 多源资产管理

作为用户，我希望添加多个本地目录作为资产源，并为每个源设置扫描规则，这样我可以把自己、社区和工具生成的 AI 资产统一纳入管理。

验收标准：

- 用户可以添加、编辑、禁用和删除本地目录源。
- 每个源可以配置多个 include glob 和 exclude glob。
- 删除源只取消注册，不删除源目录中的文件。
- 源扫描结果能显示发现数量、失败原因和最近扫描时间。

### 2.2 多类型资产发现

作为用户，我希望 AssetIWeave 能识别 prompt、rule、memory、skill、MCP、agent、command、workflow 等资产，这样它不会被限制在当前某一种 AI 工具形态上。

验收标准：

- 系统为每个发现项生成稳定资产 ID。
- 系统识别资产 kind：`prompt`、`rule`、`memory`、`skill`、`mcp`、`agent`、`command`、`workflow`、`profile`、`custom`。
- 系统识别资产 format：`markdown`、`json`、`yaml`、`toml`、`directory`、`script`、`sqlite`、`unknown`。
- 系统优先根据显式配置和路径规则分类，再使用文件名、扩展名和内容特征辅助分类。
- 无法确定类型的资产归入 `custom` 或 `unclassified`，默认不自动部署。

### 2.3 元数据覆盖层

作为用户，我希望给资产设置标签、分组、类型和启用状态，但不污染源仓库，这样我可以安全管理来自第三方的资产。

验收标准：

- 标签、分组、备注、启用状态、兼容性配置保存在 App 自己的数据目录。
- 修改元数据不会改写源资产文件。
- 同一资产重新扫描后能保留已有元数据。
- 支持批量设置标签、分组、启用状态。

### 2.4 目标 Profile 管理

作为用户，我希望为 Codex、Claude、Cursor、OpenCode、Gemini、OpenClaw 或任意自定义工具创建目标 Profile，这样我可以控制每个工具使用哪些资产。

验收标准：

- 用户可以创建任意数量的目标 Profile。
- Profile 包含名称、工具类型、目标路径、支持资产类型、部署策略和启用状态。
- 系统提供常见工具的 Profile 模板，但用户可以修改或新增自定义 Profile。
- Profile 不使用固定数据库列，例如 `enabled_codex`、`enabled_cursor`，而是通过通用配置表达。

### 2.4.1 资产挂载关系

作为用户，我希望在每条资产右侧通过 App 小图标快速打开或关闭该资产对某个 App/Profile 的挂载，这样我可以像开关矩阵一样管理哪些工具使用哪些资产。

验收标准：

- 系统使用通用 `asset_mounts` 模型记录 asset/profile 挂载关系。
- 用户点击右侧 App 快捷图标后，挂载状态写入 SQLite，而不是只保存在前端状态。
- 用户在展开面板中选择挂载卡片后，也写入同一份挂载关系。
- 部署计划只基于已启用的挂载关系生成动作。
- App 快捷图标展示哪些 Profile、顺序和样式来自可配置数据，而不是写死在前端。
- 禁用挂载只影响目标 App 投影，不删除源仓库文件。

### 2.4.2 源级 Skill 批量挂载

作为用户，我希望在技能源管理页面按来源一次性选择该来源下的全部 Skill 并挂载到某个 App/Profile，这样我不需要逐条资产重复点击。

验收标准：

- 技能源管理页显示每个来源下可批量操作的 Skill 数量。
- 用户可以按来源对某个 Profile 执行全选挂载或取消全选。
- 批量挂载写入与单个资产相同的 `asset_mounts` 模型。
- 批量挂载只作用于当前来源下的 `skill` 资产。
- App 专属来源和 App 本地来源默认禁止直接跨 App 批量挂载，避免错误地把目标目录重新挂载到其他目标。
- 批量操作完成后，部署计划仍然基于启用的挂载关系生成。

### 2.4.3 Skill 场景分组管理

作为用户，我希望在已有 Skills > Groups 标签页中按工作场景维护 Skill 分组，并把某个分组批量挂载或卸载到指定 App/Profile，这样我可以按当前任务减少 AI App 的上下文负载。

验收标准：

- 分组管理在现有 `skills.groups` 子导航下实现，不新增顶部标签、二级标签或新的导航入口。
- 第一版只管理 `skill` 资产。
- 用户可以创建、编辑、删除场景分组。
- 分组成员由手动成员和实时规则匹配成员共同组成。
- 第一版规则支持 Source、relative path glob、名称包含。
- 批量挂载/卸载只影响当前分组解析出的 Skill，不清空同一 Profile 下其他已挂载 Skill。
- 批量挂载/卸载写入并复用 `asset_mounts`，沿用当前即时挂载/卸载链路。
- 批量操作允许部分成功，并返回成功数量和失败原因。

### 2.4.4 Skill 互联网发现和导入

作为用户，我希望能用自然语言关键词从互联网 provider 搜索可能有用的 Skill，并在确认后下载导入 AssetIWeave 管理，这样我不需要先手工克隆仓库再整理目录。

验收标准：

- CLI 和桌面 App 共用同一套 Rust Engine 搜索、预览和导入规则。
- 第一版 provider 支持 GitHub repo search 和 GitHub code search，后续可以增加其他 provider 或外部 Agent/插件。
- 搜索结果优先返回具体包含 `SKILL.md` 的目录 URL；无法解析时允许降级为仓库级候选。
- GitHub API 支持匿名请求，也支持通过 `GITHUB_TOKEN` 或 `GH_TOKEN` 提高速率限制。
- 导入前支持 dry-run，展示仓库、分支、目录、暂存路径、最终 Skill 名称和目标路径。
- 确认导入必须显式 `--yes` 或等价 UI 确认；导入后写入 AssetIWeave 备份库并重新扫描。
- 确认导入后记录远程来源、分支、目录、获取时 tree SHA 和本地内容 hash，并支持检查远程 tree SHA 是否变化。
- 导入流程必须展示或返回远程 Skill 安全提示，并有隔离目录测试覆盖 clone、导入、重扫和远程来源记录。
- 该能力不是托管 marketplace，不内置 LLM API，也不自动信任远程代码；远程来源只成为用户确认后的本地受管 Skill。

### 2.5 规则与策略

作为用户，我希望按资产类型、标签、分组、来源和路径规则决定部署策略，这样新增资产可以自动进入合适的工具。

验收标准：

- Profile 支持 include/exclude 规则：kind、tag、group、source、path pattern。
- 单个资产支持显式 include/exclude 覆盖 Profile 规则。
- 决策优先级清晰：未启用挂载 > 显式禁用 > Profile 能力/规则 > 目标目录冲突 > 生成部署动作。
- 系统能解释每个资产为什么被部署或为什么被跳过。

### 2.6 部署计划预览

作为用户，我希望执行同步前看到将要发生的变化，这样我可以避免误删或误覆盖。

验收标准：

- 系统生成部署计划，包含 create、update、remove、skip、conflict。
- 每个动作显示资产、目标 Profile、目标路径、原因和风险提示。
- 真实目录或非本应用管理的文件默认不覆盖、不删除，只报告冲突。
- 用户可以只执行选中的 Profile 或选中的动作。

### 2.7 部署执行

作为用户，我希望 AssetIWeave 能把已挂载的资产部署到各个 AI 工具目标目录，这样工具可以直接使用这些资产。

验收标准：

- 当前默认部署策略是 `symlink_to_source`：目标 App 目录直接软链接到源仓库中的真实资产。
- 保留 `copy_to_target` 作为兼容策略，但不作为默认路径。
- 后续可以扩展 `render`、`append`、`config-merge`。
- 系统只清理自己管理的部署结果。
- 每次执行记录部署状态，便于后续识别 stale asset。
- 执行失败时给出明确错误，不继续执行高风险动作。
- 系统不使用“两跳 symlink”：不创建 `源仓库 -> AssetIWeave 中间池 -> App` 的默认链路。

### 2.7.1 资产导出

作为用户，我希望在需要集中整理或归档时，可以把选中的真实资产复制到指定目录，这样我可以得到一份独立的集中资产包。

验收标准：

- 用户可以选择导出全部资产、按类型导出、按 Source 导出或按 Profile/挂载状态导出。
- 导出行为复制真实文件或目录，不移动、不删除、不改写源仓库。
- 导出目录可由用户指定。
- 导出可选择保持源目录结构或按 AssetKind 分组。
- 导出生成 manifest，记录来源、hash、类型、描述和导出时间。

### 2.8 自动同步

作为用户，我希望源资产变化后目标目录可以自动更新，这样新增、删除或移动资产不需要手动同步。

验收标准：

- 当前阶段支持 App 内手动刷新和一键同步。
- 后续支持后台 watcher 或系统启动项。
- 自动同步必须复用部署计划机制，不能绕过预览和安全规则。
- 自动同步的破坏性动作需要可配置，例如默认允许创建和更新，删除可单独开关。

### 2.9 cc-switch 对照和迁移

作为用户，我希望能参考 cc-switch 已经管理的 skill 状态，但不让新 App 依赖 cc-switch。

验收标准：

- 当前阶段可以只读扫描 `~/.cc-switch/skills` 作为普通本地源。
- 后续可以读取 cc-switch 数据库进行一次性迁移。
- 迁移结果写入 AssetIWeave 自己的数据模型。
- 运行时不依赖 cc-switch 数据库或服务。

### 2.10 数据展示视图模式

作为用户，我希望不同页面根据数据结构提供合适的视图切换，而不是所有页面套用同一套 Finder 视图，这样页面行为和信息结构保持一致。

验收标准：

- 资产总览目录只提供列表视图和卡片视图。
- 资产总览的卡片视图突出资产名称、类型、来源、描述、路径和 App 快捷挂载入口。
- 技能源管理页只提供列表视图和分栏视图。
- 技能源管理的分栏视图按来源、来源下 Skill、来源级批量挂载区域组织信息。
- Toolbar 使用统一组件实现，但每个页面只暴露符合该页面语义的视图选项。
- 切换视图只影响展示方式，不改变资产、来源、Profile 或挂载关系数据。

### 2.11 对话记录管理

作为用户，我希望对话记录不是只按第三方 App 的 Session 存储，而是能在 Session 内按“我的一个问题”组织，这样后续整理、搜索和导出更贴近真实工作流。

验收标准：

- 对话记录作为独立 Conversation 领域实现，不复用 Asset/Source 扫描模型。
- 系统把来源 Session 标准化为 Session、Turn、Part，再用可编辑 Question Group 表达用户问题。
- Turn 以真实用户消息为边界；工具调用、命令、代码、子 Agent/sidechain 输出作为有序 Part 保留。
- 合并和拆分只修改 Question Group 与 Turn 的归属，不改写导入内容。
- v1 内置 Codex、Claude Code、OpenCode 适配器。
- v1 支持显式注册可信外部适配器脚本；脚本输出标准化 Session/Turn/Part，AssetIWeave 负责分组、搜索、存储和导出。
- 外部脚本启动使用可执行文件和参数数组，不经过 Shell；注册记录路径和内容 hash，并要求用户确认。
- 支持手动同步、Session-first 浏览、搜索、Question 合并/拆分。
- 支持按 Session 导出 Markdown，每个 Session 一个文件，Question Group 作为章节。
- v1 不做后台 watcher、JSON 导出、内置 AI API 或完整原始 Session 复制。

## 3. 当前产品开发阶段

当前已经完成早期闭环，进入具体功能开发阶段。已完成基础能力：

1. 创建 Tauri 2 + React + TypeScript + Rust 桌面应用。
2. 管理本地目录源。
3. 扫描 `skill`、`prompt`、`rule`、`custom` 四类资产。
4. SQLite 存储 Source、Asset、Profile、部署状态、导航、App 快捷入口和资产挂载关系。
5. 管理目标 Profile 模板。
6. 基于启用的 `asset_mounts` 生成部署计划并执行基础部署。
7. 直接 symlink 到源资产真实路径，并展示已挂载、未挂载、冲突、断链状态。
8. 前端 Catalog 页面、Sources 页面、导航、通知出口、App 快捷挂载 UI。
9. 中英文 i18n 基础。
10. 前端组件化架构和全局设置弹窗基础。
11. 统一数据 Toolbar 组件，并支持页面级视图选项配置。
12. Conversation 领域 v1：标准化对话模型、SQLite 存储、内置/外部适配器入口、CLI、Session-first 前端页面和 Markdown Session 导出。
12. 资产总览目录支持列表视图和卡片视图切换。
13. 技能源管理页支持列表视图和分栏视图切换。
14. 技能源管理页支持按来源批量挂载该来源下所有 Skill 到指定 App/Profile。
15. 技能源导入入口和目录选择 UI 已接入。
16. Skill Group 管理已实现 CRUD、规则匹配、手动成员、批量挂载和独占挂载。
17. 技能源管理页已接入互联网 Skill 搜索、预览和导入入口。
18. CLI 已具备 `skill search` / `skill acquire` / `skill remote check`，可从 GitHub 候选中定位 `SKILL.md`、导入备份库并检查远程 drift。
19. 前端目录结构已明确为 `app/components/config/hooks/i18n/layouts/mock/pages/router/schemas/services/store/styles/types/utils`，其中 `services` 和 `pages` 保持当前项目约定。
20. App 快捷入口支持真实应用图标 token 和自定义 SVG path 资源，并可在设置页编辑。
21. 导航菜单支持中英文本地化 label 覆盖，设置页编辑当前语言对应文案。
22. Tauri 后端支持按资产 kind 查询/扫描、取消真实挂载并返回最新挂载状态、通过 dialog plugin 选择导入目录。
23. 当前自动化验证基线包含 `pnpm typecheck`、`pnpm test`、`cargo test`、`pnpm build`。

当前里程碑：

- 2026-05-30：前端工程分层与视图工作流里程碑。当前代码状态确认前端目录职责边界已收敛，保留 `services` 与 `pages`，并明确 `layouts/router/mock/store/styles/types` 等顶层目录职责；资产总览目录为“列表/卡片”切换，技能源管理为“列表/分栏”切换；Toolbar 组件统一但视图选项按页面传入；源级 Skill 批量挂载写入同一份 `asset_mounts` 关系；App 快捷入口支持真实应用图标和自定义 SVG 图标资源。已通过 `pnpm typecheck`、`pnpm test`、`cargo test`、`pnpm build`。构建仍有 Vite 单 chunk 超过 500 kB 的体积提示，暂不影响功能通过。

当前默认 Profile 模板：

- Codex
- Claude
- Cursor
- OpenCode
- Gemini
- Antigravity
- OpenClaw
- Custom

当前阶段后续重点：

- 挂载关系、计划生成和执行链路补充更完整测试。
- Profile include/exclude 规则继续细化到 tag、group、source、path pattern。
- 前端补齐执行确认、执行结果和独立 Plan 页面。
- Source 创建/编辑校验和 metadata overlay 写入。
- 导出资产功能。
- Skill 发现链路继续补 provider 抽象、候选评分解释、远程来源漂移提醒和更完整的导入安全提示。

当前阶段暂不要求：

- Git 源自动 clone/pull。
- MCP 配置合并。
- 完整后台常驻同步。
- 全功能 CLI。
- 在线仓库和插件市场。

### 3.1 技术约束

- App 必须是 Tauri 2 桌面应用。
- 前端使用 React、TypeScript、Vite。
- CSS 使用 Tailwind CSS；设计 token 写入 `frontend/tailwind.config.ts`，避免继续扩展大段手写页面 CSS。
- Rust 侧使用 workspace 结构，核心模型和后续扫描/计划逻辑放在独立 core crate。
- 持久化使用 SQLite，配置导出/导入使用 JSON 作为后续能力。

## 4. 非功能需求

### 4.1 安全性

- 源目录默认只读。
- 禁止路径穿越写入目标目录外部。
- 不覆盖用户真实文件，除非用户明确选择 force。
- 删除动作只作用于本应用记录过的部署结果。

### 4.2 可解释性

- 每个部署决策都必须有原因。
- 每个冲突都必须说明阻塞点和建议处理方式。
- 每个 Profile 的 effective asset 列表可查看。

### 4.3 可扩展性

- 新资产类型通过分类器和部署 adapter 扩展。
- 新 AI 工具通过 Profile 模板扩展。
- 核心逻辑不依赖固定工具列表。

### 4.4 本地优先

- 所有核心功能离线可用。
- 配置和元数据存储在用户本机。
- 支持导出/导入配置备份。

### 4.5 性能

- 1000 个资产内的扫描和计划生成应在交互可接受范围内完成。
- 大目录扫描应可取消。
- 文件 hash 和解析结果应缓存。

## 5. 假设

- 用户主要在 macOS 上使用，但核心模型应尽量跨平台。
- 资产大多是文本文件或包含 `SKILL.md` 等入口文件的目录。
- 目标工具最终消费的是文件或目录。
- 用户希望保留手工控制权，不希望默认自动改写大量目标目录。

## 6. 待确认问题

- App 是否需要菜单栏常驻模式作为第一版能力？
- 默认是否启用 cc-switch 源扫描模板？
- 部署状态记录是否要支持多 workspace 维度？
