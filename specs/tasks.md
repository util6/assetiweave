# 任务清单：AssetIWeave

## Milestone：2026-05-30 前端工程分层与视图工作流

- [x] M1.1 将当前代码状态确认为“资产总览列表/卡片 + 技能源管理列表/分栏”的视图语义里程碑
- [x] M1.2 统一 Catalog 和 Sources 的 Toolbar 组件语言
- [x] M1.3 资产总览目录只暴露列表视图和卡片视图
- [x] M1.4 技能源管理页只暴露列表视图和分栏视图
- [x] M1.5 技能源管理分栏视图按来源、Skill、批量挂载区域组织
- [x] M1.6 支持按来源把全部 Skill 批量挂载到某个 App/Profile
- [x] M1.7 源级批量挂载复用 `asset_mounts`，不引入第二套挂载状态
- [x] M1.8 前端目录边界收敛为 `app/components/config/hooks/i18n/layouts/mock/pages/router/schemas/services/store/styles/types/utils`
- [x] M1.9 保留 `services` 和 `pages` 命名作为当前项目约定
- [x] M1.10 App 快捷入口支持真实应用图标 token 和自定义 SVG path 资源
- [x] M1.11 导航菜单支持中英文本地化 label 覆盖
- [x] M1.12 后端契约支持按 kind 查询/扫描、取消挂载状态回写和 dialog 目录选择
- [x] M1.13 通过 `pnpm typecheck`、`pnpm test`、`cargo test`、`pnpm build`
- [ ] M1.14 补充真实桌面窗口手工验收两个页面的视图切换和控制台 error 检查
- [ ] M1.15 处理 Vite 单 chunk 超过 500 kB 的构建体积提示

## Phase 0：Spec 和项目边界

- [x] 0.1 明确产品名称：AssetIWeave
- [x] 0.2 明确产品定位：本地 AI 文件资产挂载管理桌面应用
- [x] 0.3 明确不依赖现有 skill-link-manager 脚本
- [x] 0.4 明确 cc-switch 只作为对照组和迁移来源
- [x] 0.5 确认存储方案：SQLite 主存储 + JSON 导出
- [x] 0.6 确认第一批 Profile 模板列表：Codex、Claude、Cursor、OpenCode、Gemini、Antigravity、OpenClaw、Custom
- [x] 0.7 确认当前产品阶段：已结束早期闭环搭建，进入具体功能开发
- [x] 0.8 确认挂载策略：默认直接 symlink 到源仓库真实资产，不做中间 symlink 池
- [x] 0.9 确认集中整理策略：作为 Export Assets 功能复制到用户指定目录

## Phase 1：Tauri 项目基础

- [x] 1.1 创建 Tauri 2 + React + TypeScript + Rust 项目
- [x] 1.2 配置 pnpm、Vite、TypeScript
- [x] 1.3 配置 Rust crate 结构
- [x] 1.4 配置格式化和基础检查
- [x] 1.5 建立基础测试框架
- [x] 1.6 创建应用基础布局：左侧导航、顶部状态栏、主内容区
- [x] 1.7 设置应用名称、Bundle ID、图标占位

## Phase 2：核心数据模型

- [x] 2.1 定义 `Source`
- [x] 2.2 定义 `Asset`
- [ ] 2.3 定义 `MetadataOverlay`
- [x] 2.4 定义 `TargetProfile`
- [x] 2.5 定义 `DeploymentPlan`
- [x] 2.6 定义 `DeploymentAction`
- [x] 2.7 定义 `DeploymentState`
- [x] 2.8 定义枚举：`AssetKind`、`AssetFormat`、`DeploymentStrategy`、`AppKind`
- [x] 2.9 为核心模型编写基础测试
- [x] 2.10 定义 `AssetMount`
- [x] 2.11 定义 `AppShortcut`

## Phase 3：本地存储

- [x] 3.1 初始化 App 数据目录
- [x] 3.2 集成 SQLite
- [x] 3.3 创建数据库 schema
- [x] 3.4 实现基础 schema migration
- [ ] 3.5 实现 JSON export/import 基础能力
- [ ] 3.6 实现备份文件生成策略
- [ ] 3.7 编写存储层测试
- [x] 3.8 实现 Navigation SQLite seed 和读取
- [x] 3.9 实现 App Shortcut SQLite seed 和读取
- [x] 3.10 实现 `asset_mounts` 表和 repository

## Phase 4：Source 管理

- [x] 4.1 实现 `list_sources`
- [x] 4.2 实现 `create_source`
- [x] 4.3 实现 `update_source`
- [x] 4.4 实现 `delete_source`
- [ ] 4.5 实现源路径校验
- [ ] 4.6 实现 include/exclude glob 校验
- [x] 4.7 前端实现 Sources 页面
- [x] 4.8 前端实现添加/编辑 Source 表单
- [x] 4.9 扫描和启动校验时自动移除缺失 Source 的资产记录
- [x] 4.10 前端实现技能源导入弹窗和目录选择入口
- [x] 4.11 技能源管理支持列表视图和分栏视图切换
- [x] 4.12 技能源分栏视图支持按来源浏览 Skill
- [x] 4.13 技能源分栏视图支持来源级批量挂载

## Phase 5：资产扫描和分类

- [x] 5.1 实现目录扫描器
- [x] 5.2 实现 include/exclude glob 匹配
- [x] 5.3 实现稳定 Asset ID 生成
- [x] 5.4 实现包含 `SKILL.md` 的目录识别
- [x] 5.5 实现基础 Markdown prompt/rule 路径识别
- [x] 5.6 实现 custom/unclassified fallback
- [x] 5.7 实现基础 description 提取
- [x] 5.8 实现 `scan_sources` Tauri 命令
- [ ] 5.9 编写扫描和分类测试 fixture
- [x] 5.10 增加 Source scanner/origin 元数据
- [x] 5.11 拆分 Skill 专用 scanner，Skill 扫描只识别 `SKILL.md` 目录
- [x] 5.12 支持按 Git 仓库推断 `repo_root` 和 `scan_root`
- [x] 5.13 实现 `list_skill_sources` 和 `scan_skill_sources`
- [x] 5.14 标记 App 目标目录来源，默认禁止直接跨 App 挂载
- [x] 5.15 实现 App 本地 Skill 收养复制到 `~/.assetiweave/library/skills`
- [x] 5.16 启动时只校验已登记资产状态，不探索新资产，并移除已删除资产
- [x] 5.17 目录资产 hash 覆盖整个目录内容而不是只看入口文件
- [ ] 5.18 补齐 frontmatter/title 精细解析

## Phase 6：Catalog 和元数据覆盖层

- [x] 6.1 实现 `list_assets`
- [ ] 6.2 实现 `update_asset_metadata`
- [ ] 6.3 实现 `bulk_update_assets`
- [ ] 6.4 实现 metadata overlay 与扫描结果合并
- [x] 6.5 前端实现 Catalog 列表
- [x] 6.6 前端实现搜索
- [ ] 6.7 前端实现资产详情抽屉
- [ ] 6.8 前端实现批量设置标签、分组、启用状态
- [x] 6.9 前端实现资产行路径/描述/来源默认展示
- [x] 6.10 前端实现 Mount Targets 展开卡片
- [x] 6.11 前端实现 App 快捷挂载图标
- [x] 6.12 前端统一禁止 App 专属/App 本地来源直接挂载入口
- [x] 6.13 Mount Target 卡片显示真实 App 挂载目录和软链接检测状态
- [x] 6.14 前端实现资产总览卡片视图
- [x] 6.15 资产总览视图切换限制为列表视图和卡片视图

## Phase 7：Profile 管理

- [x] 7.1 实现 `list_profiles`
- [ ] 7.2 实现 `create_profile`
- [ ] 7.3 实现 `update_profile`
- [ ] 7.4 实现 `delete_profile`
- [x] 7.5 实现 Profile 模板：Codex
- [x] 7.6 实现 Profile 模板：Claude
- [x] 7.7 实现 Profile 模板：Cursor
- [x] 7.8 实现 Profile 模板：OpenCode
- [x] 7.9 实现 Profile 模板：Gemini
- [x] 7.10 实现 Profile 模板：Antigravity
- [x] 7.11 实现 Profile 模板：OpenClaw
- [x] 7.12 实现 Profile 模板：Custom
- [ ] 7.13 前端实现 Profiles 页面
- [ ] 7.14 前端实现 Profile 规则编辑

## Phase 8：决策解释和计划生成

- [ ] 8.1 实现资产启用状态判断
- [x] 8.2 实现 Profile 支持 kind 判断
- [ ] 8.3 实现 include/exclude 规则匹配
- [ ] 8.4 实现显式 include/exclude 覆盖
- [ ] 8.5 实现 `explain_asset`
- [x] 8.6 实现目标目录状态读取
- [x] 8.7 实现基础 create/skip/conflict 动作生成
- [x] 8.8 实现基础 `create_plan`
- [ ] 8.9 前端实现 Plan 页面
- [x] 8.10 前端实现基础动作详情和原因展示
- [x] 8.11 改造计划生成：只基于已启用 `asset_mounts`
- [ ] 8.12 补齐 update/remove/stale 动作生成

## Phase 9：部署执行

- [x] 9.1 实现基础 symlink adapter
- [x] 9.2 实现 copy adapter
- [x] 9.3 实现目标路径安全校验
- [x] 9.4 实现非受管文件保护
- [x] 9.5 实现 DeploymentState 写入
- [ ] 9.6 实现 stale managed asset 清理
- [x] 9.7 实现基础 `execute_plan`
- [ ] 9.8 前端实现执行确认和结果展示
- [ ] 9.9 编写真实文件系统集成测试
- [x] 9.10 改造部署策略为默认 `symlink_to_source`

## Phase 10：默认模板和首次启动体验

- [ ] 10.1 首次启动创建默认数据目录
- [x] 10.2 提供常见 source 示例
- [x] 10.3 提供常见 Profile 模板
- [ ] 10.4 支持扫描 `~/.cc-switch/skills` 作为普通 source
- [ ] 10.5 支持扫描用户选择的任意目录
- [ ] 10.6 实现空状态和引导提示

## Phase 11：设置、导入导出和日志

- [ ] 11.1 实现 Settings 页面
- [ ] 11.2 实现配置导出
- [ ] 11.3 实现配置导入
- [ ] 11.4 实现最近操作日志
- [ ] 11.5 实现错误报告和复制诊断信息
- [ ] 11.6 实现数据库备份入口

## Phase 12：验证和打磨

- [ ] 12.1 验证 Sources -> Catalog -> Profiles -> Plan -> Execute 完整闭环
- [ ] 12.2 验证不修改源文件
- [ ] 12.3 验证不删除非受管目标文件
- [ ] 12.4 验证 disabled asset 不部署
- [ ] 12.5 验证 unclassified asset 默认不部署
- [ ] 12.6 验证每个部署动作都有解释原因
- [ ] 12.7 用真实本机 skill/prompt/rule 目录做手工验收
- [ ] 12.8 修复 UI 文案、空状态、错误状态
- [x] 12.9 验证资产总览列表/卡片视图切换
- [x] 12.10 验证技能源管理列表/分栏视图切换
- [x] 12.11 验证两个页面 Toolbar 视图选项符合各自业务语义

## Phase 13：前端产品化基础

- [x] 13.1 建立 Tailwind CSS 技术选型和 design token
- [x] 13.2 实现 Catalog 当前页面
- [x] 13.3 实现 SQLite 驱动的导航菜单
- [x] 13.4 实现 SQLite 驱动的 App 快捷挂载入口配置
- [x] 13.5 实现通知消息渲染出口
- [x] 13.6 实现中英文 i18n 基础
- [x] 13.7 完成前端组件化重构：app/pages/components/hooks/services/mock/utils
- [x] 13.8 优化 Mount Target 卡片视觉和选中态
- [x] 13.9 抽取统一 DataToolbar 组件族
- [x] 13.10 为资产总览实现卡片视图
- [x] 13.11 为技能源管理实现 Finder-like 分栏视图
- [x] 13.12 统一 Toolbar 组件但保留页面级视图模式声明
- [x] 13.13 完成前端工程分层重构：layouts/router/store/styles/types 顶层边界
- [x] 13.14 保留 services 和 pages 命名作为当前 React/Tauri 项目约定
- [x] 13.15 支持 App 快捷入口真实应用图标和自定义 SVG 图标资源

## Phase 14：后端挂载功能（当前核心开发方向）

- [x] 14.1 创建 `asset_mounts` schema
- [x] 14.2 实现 `asset_mounts` repository
- [x] 14.3 定义 `AssetMount` DTO 和序列化模型
- [x] 14.4 实现 `list_asset_mounts(asset_id?)`
- [x] 14.5 实现 `toggle_asset_mount(asset_id, profile_id)`
- [x] 14.6 实现 `set_asset_mount(asset_id, profile_id, enabled, strategy?)`
- [x] 14.7 前端快捷 App 图标点击接入真实 Tauri command
- [x] 14.8 前端 Mount Target 卡片点击接入真实 Tauri command
- [x] 14.9 页面初始化加载资产挂载状态
- [x] 14.10 `create_plan` 改为读取已启用挂载关系
- [x] 14.11 执行部署时直接 symlink 到源资产真实路径
- [x] 14.12 UI 展示已挂载、未挂载、冲突、断链状态
- [x] 14.13 为挂载关系和计划生成补充基础测试
- [x] 14.14 前端实现来源级批量设置 `asset_mounts`
- [x] 14.15 批量挂载时过滤 App 专属/App 本地来源的直接跨 App 挂载入口
- [x] 14.16 后端实现取消真实挂载并回写最新 `AssetMountStatus`
- [x] 14.17 后端支持按 AssetKind 查询和扫描资产

## Phase 15：导出和长期方向

- [ ] 15.1 实现 Export Assets：复制真实资产到用户指定目录
- [ ] 15.2 实现导出 manifest.json
- [ ] 15.3 实现导出筛选：全部、按类型、按 Source、按 Profile、按挂载状态
- [x] 15.4 实现 App Shortcut 设置 UI：启用/隐藏/排序/图标/颜色
- [ ] 15.5 Git 源 clone/pull 管理
- [ ] 15.6 MCP 配置管理和合并部署
- [ ] 15.7 App memory 管理
- [ ] 15.8 Slash command 管理
- [ ] 15.9 后台 watcher
- [ ] 15.10 登录启动项
- [ ] 15.11 只读导入 cc-switch 数据库
- [ ] 15.12 轻量 CLI
- [ ] 15.13 插件化分类器和部署 adapter
- [ ] 15.14 多机器配置同步

## Phase 16：Skill 场景分组管理

- [x] 16.1 在现有 `skills.groups` 子导航下接入分组业务页面
- [x] 16.2 定义 `AssetGroup`、`AssetGroupRules`、`AssetGroupDetail`、`AssetGroupResolvedMember`
- [x] 16.3 创建 `asset_groups` 和 `asset_group_members` SQLite schema
- [x] 16.4 实现 Skill 分组 repository 和规则实时解析
- [x] 16.5 实现分组 CRUD、手动成员维护和批量挂载 Tauri commands
- [x] 16.6 分组批量挂载/卸载只影响本组成员，不清空 Profile 其他挂载
- [x] 16.7 前端实现三栏分组工作台：分组、成员 Skill、规则与批量挂载
- [x] 16.8 前端 services/schema/mock/i18n 接入分组能力
- [x] 16.9 补充分组规则、路由、schema、批量挂载测试
