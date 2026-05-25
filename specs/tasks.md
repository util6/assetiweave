# 任务清单：AssetIWeave

## Phase 0：Spec 和项目边界

- [x] 0.1 明确产品名称：AssetIWeave
- [x] 0.2 明确产品定位：本地 AI 文件资产管理桌面应用
- [x] 0.3 明确不依赖现有 skill-link-manager 脚本
- [x] 0.4 明确 cc-switch 只作为对照组和迁移来源
- [x] 0.5 确认 MVP 存储方案：SQLite 主存储 + JSON 导出
- [x] 0.6 确认第一批 Profile 模板列表：Codex、Claude、Cursor、OpenCode、Gemini、Antigravity、OpenClaw、Custom

## Phase 1：Tauri 项目基础

- [ ] 1.1 创建 Tauri 2 + React + TypeScript + Rust 项目
- [ ] 1.2 配置 pnpm、Vite、TypeScript
- [ ] 1.3 配置 Rust crate 结构
- [ ] 1.4 配置格式化和 lint
- [ ] 1.5 建立基础测试框架
- [ ] 1.6 创建应用基础布局：左侧导航、顶部状态栏、主内容区
- [ ] 1.7 设置应用名称、Bundle ID、图标占位

## Phase 2：核心数据模型

- [ ] 2.1 定义 `Source`
- [ ] 2.2 定义 `Asset`
- [ ] 2.3 定义 `MetadataOverlay`
- [ ] 2.4 定义 `TargetProfile`
- [ ] 2.5 定义 `DeploymentPlan`
- [ ] 2.6 定义 `DeploymentAction`
- [ ] 2.7 定义 `DeploymentState`
- [ ] 2.8 定义枚举：`AssetKind`、`AssetFormat`、`DeploymentStrategy`、`AppKind`
- [ ] 2.9 为核心模型编写序列化和校验测试

## Phase 3：本地存储

- [ ] 3.1 初始化 App 数据目录
- [ ] 3.2 集成 SQLite
- [ ] 3.3 创建数据库 schema
- [ ] 3.4 实现 schema migration
- [ ] 3.5 实现 JSON export/import 基础能力
- [ ] 3.6 实现备份文件生成策略
- [ ] 3.7 编写存储层测试

## Phase 4：Source 管理

- [ ] 4.1 实现 `list_sources`
- [ ] 4.2 实现 `create_source`
- [ ] 4.3 实现 `update_source`
- [ ] 4.4 实现 `delete_source`
- [ ] 4.5 实现源路径校验
- [ ] 4.6 实现 include/exclude glob 校验
- [ ] 4.7 前端实现 Sources 页面
- [ ] 4.8 前端实现添加/编辑 Source 表单

## Phase 5：资产扫描和分类

- [ ] 5.1 实现目录扫描器
- [ ] 5.2 实现 include/exclude glob 匹配
- [ ] 5.3 实现稳定 Asset ID 生成
- [ ] 5.4 实现包含 `SKILL.md` 的目录识别
- [ ] 5.5 实现 Markdown prompt/rule 识别
- [ ] 5.6 实现 custom/unclassified fallback
- [ ] 5.7 实现 frontmatter/标题/描述提取
- [ ] 5.8 实现 `scan_sources` Tauri 命令
- [ ] 5.9 编写扫描和分类测试 fixture

## Phase 6：Catalog 和元数据覆盖层

- [ ] 6.1 实现 `list_assets`
- [ ] 6.2 实现 `update_asset_metadata`
- [ ] 6.3 实现 `bulk_update_assets`
- [ ] 6.4 实现 metadata overlay 与扫描结果合并
- [ ] 6.5 前端实现 Catalog 表格
- [ ] 6.6 前端实现搜索和筛选
- [ ] 6.7 前端实现资产详情抽屉
- [ ] 6.8 前端实现批量设置标签、分组、启用状态

## Phase 7：Profile 管理

- [ ] 7.1 实现 `list_profiles`
- [ ] 7.2 实现 `create_profile`
- [ ] 7.3 实现 `update_profile`
- [ ] 7.4 实现 `delete_profile`
- [ ] 7.5 实现 Profile 模板：Codex
- [ ] 7.6 实现 Profile 模板：Claude
- [ ] 7.7 实现 Profile 模板：Cursor
- [ ] 7.8 实现 Profile 模板：OpenCode
- [ ] 7.9 实现 Profile 模板：Gemini
- [ ] 7.10 实现 Profile 模板：Antigravity
- [ ] 7.11 实现 Profile 模板：OpenClaw
- [ ] 7.12 实现 Profile 模板：Custom
- [ ] 7.13 前端实现 Profiles 页面
- [ ] 7.14 前端实现 Profile 规则编辑

## Phase 8：决策解释和计划生成

- [ ] 8.1 实现资产启用状态判断
- [ ] 8.2 实现 Profile 支持 kind 判断
- [ ] 8.3 实现 include/exclude 规则匹配
- [ ] 8.4 实现显式 include/exclude 覆盖
- [ ] 8.5 实现 `explain_asset`
- [ ] 8.6 实现目标目录状态读取
- [ ] 8.7 实现 create/update/remove/skip/conflict 动作生成
- [ ] 8.8 实现 `create_plan`
- [ ] 8.9 前端实现 Plan 页面
- [ ] 8.10 前端实现动作详情和原因展示

## Phase 9：部署执行

- [ ] 9.1 实现 symlink adapter
- [ ] 9.2 实现 copy adapter
- [ ] 9.3 实现目标路径安全校验
- [ ] 9.4 实现非受管文件保护
- [ ] 9.5 实现 DeploymentState 写入
- [ ] 9.6 实现 stale managed asset 清理
- [ ] 9.7 实现 `execute_plan`
- [ ] 9.8 前端实现执行确认和结果展示
- [ ] 9.9 编写真实文件系统集成测试

## Phase 10：默认模板和首次启动体验

- [ ] 10.1 首次启动创建默认数据目录
- [ ] 10.2 提供常见 source 示例
- [ ] 10.3 提供常见 Profile 模板
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

## Phase 13：非 MVP 后续方向

- [ ] 13.1 Git 源 clone/pull 管理
- [ ] 13.2 MCP 配置管理和合并部署
- [ ] 13.3 App memory 管理
- [ ] 13.4 Slash command 管理
- [ ] 13.5 后台 watcher
- [ ] 13.6 登录启动项
- [ ] 13.7 只读导入 cc-switch 数据库
- [ ] 13.8 轻量 CLI
- [ ] 13.9 插件化分类器和部署 adapter
- [ ] 13.10 多机器配置同步
