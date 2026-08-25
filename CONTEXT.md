# AssetIWeave

AssetIWeave 是本地优先的 AI 文件资产挂载管理器。它把可发现的资产、目标应用和持久化的挂载意图组织为可审阅、可执行的本地工作流。

## 资产与场景装配

**资产（Asset）**：
由来源发现并纳入管理的能力单元（如 Skill、Rule、Prompt），是可供 Agent 按需装配的基本砖块。
_Avoid_: 文件、条目、插件、工具包

**来源（Source）**：
提供资产的外部或应用自有目录；其内容默认只读，目录数据不承载 AssetIWeave 元数据。
_Avoid_: 文件夹、仓库

**目标 Profile（Profile）**：
宿主 Agent（如 Codex、Claude Code、Cursor、Pi Agent）的运行环境投射点，定义其支持的资产类型与挂载路径。
_Avoid_: 目标、配置文件、环境

**资产分组（Asset Group）**：
面向特定工作场景（如“前端开发”、“系统排障”）聚合一组资产的逻辑单元，作为批量装配与场景切换的基本颗粒度。
_Avoid_: 文件夹、分类标签、插件包

**挂载意图（Asset Mount）**：
将特定资产与目标 Profile 绑定的持久化期望状态，是 Agent 当前激活能力的唯一业务事实。
_Avoid_: 快捷挂载状态、软链接记录

**互斥挂载（Exclusive Mount）**：
将指定分组部署至目标 Profile 并自动剥离所有未入组的同类资产，用于实现 Agent 上下文的纯净场景切换。
_Avoid_: 独占模式、覆盖挂载、单选挂载

## 技能（Skill）与跨 Agent 共享

**Skill（技能资产）**：
以目录为形态、以 `SKILL.md` 为入口定义 AI 工作流与操作范式的多文件能力资产。
_Avoid_: 插件、单脚本、指令包

**Skill 本地库（Skill Library）**：
独立于任何单一宿主 Agent 的中立本地资产中心，用于沉淀可跨 Agent 共享、调度的 Skill 资产。
_Avoid_: 备份目录、下载区、私有配置

**内置 Skill（Built-in Skill）**：
由特定宿主应用原生提供或 AssetIWeave 系统预置的固有 Skill；具有明确的源头从属性，防止向原宿主产生重复或自环挂载。
_Avoid_: 第三方扩展、共享副本

**远程 Skill（Remote Skill）**：
从外部代码库检索发现的扩展定义；作为预留的远端发现地基，需显式导入本地库后方可参与装配。
_Avoid_: 在线市场包、云端插件、自动安装项

## 对话记录（Conversation）

**会话（Conversation Session）**：
第三方 Agent（如 Codex、Claude Code、Cursor 等）一次完整会话的资产边界。
_Avoid_: 对话、线程、日志文件

**交互轮次（Conversation Turn）**：
来源系统中的一次原始用户交互轮次，是跨宿主同步与增量识别的基础结构单元。
_Avoid_: 逻辑问题、单条消息、步骤

**内容事实（Conversation Part）**：
Turn 内不可再分的标准化内容事实块，是会话内容的最小持久化单元（承载角色、类型、命令、文本与卡片元数据）。
_Avoid_: 消息块、文本段、独立卡片表

**语义问题（Conversation Question）**：
AssetIWeave 在 Turn 之上构造的二次语义分组；将因中断、“继续”跟进或微调追问产生的多个物理 Turn 聚合为一个完整的用户问题单元，是全文搜索与 Memory 的核心物化消费单元。
_Avoid_: 原始问题、单次输入、物理轮次

**卡片（Conversation Card / Card）**：
由 Part 根据适配器契约在读取时实时投影出的展示与语义 View Model，不是独立持久化数据库实体（Card ID 与 Part ID 等同）。
_Avoid_: 数据库卡片实体、独立卡片表

**对话来源（Conversation Source）**：
指向外部宿主存储会话记录的只读本地目录或数据库连接。
_Avoid_: 对话目录、日志仓库

**对话适配器（Conversation Adapter）**：
负责将不同宿主的专有记录格式解析并规范化为会话与轮次标准的扩展协议包。
_Avoid_: 读取插件、解析脚本

## 扩展生态与运行时（Extension & Runtime）

**扩展内核（Extension Kernel）**：
统管所有扩展包（Agent 与 Adapter）的身份标识、版本兼容性、沙箱进程调用与生命周期探活的底层运行时基座。
_Avoid_: 插件系统、调度器

**Agent 市场（Agent Market）**：
发现、安装与管理可执行 AI 智能体（Agent Package）的声明式目录与分发通道。
_Avoid_: 模型市场、插件商店

**对话适配器市场（Adapter Market）**：
发现、安装与更新各宿主对话解析插件（Adapter Package）的声明式目录与分发通道。
_Avoid_: 解析器市场、适配器仓库
