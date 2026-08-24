# AssetIWeave

AssetIWeave 是本地优先的 AI 文件资产挂载管理器。它把可发现的资产、目标应用和持久化的挂载意图组织为可审阅、可执行的本地工作流。

## 资产与部署

**资产（Asset）**：
由一个来源发现并编入目录的可管理文件资产。
_Avoid_: 文件、条目

**来源（Source）**：
提供资产的外部或应用自有目录；其内容默认只读，目录数据不承载 AssetIWeave 元数据。
_Avoid_: 文件夹、仓库

**目标 Profile（Profile）**：
某个应用或工作环境的可部署目标配置。
_Avoid_: 目标、配置

**挂载意图（Asset Mount）**：
资产与目标 Profile 之间持久化的期望关系，是是否应部署的唯一业务事实。
_Avoid_: 快捷挂载状态、链接状态

**部署计划（Deployment Plan）**：
根据挂载意图与目标实际状态生成的、可解释的文件系统变更集合。
_Avoid_: 挂载结果、执行记录

## 对话与记忆

**会话（Conversation Session）**：
从一个对话来源规范化得到的完整交互记录。
_Avoid_: 对话、线程

**卡片（Card）**：
从会话中提取、带有语义类型的最小可检索内容单元。
_Avoid_: 消息、块

**Memory**：
具有证据关系、作用范围与复核生命周期的持久结论；它不是普通资产文件。
_Avoid_: 记忆文件、聊天摘要

## 扩展运行时

**Agent**：
可被执行 Runtime 调用的受约束执行定义，描述协议、入口和能力，不等同于市场条目。
_Avoid_: 市场包、模型

**Agent 市场目录（Agent Market Catalog）**：
描述可安装 Agent 发行物的声明式目录，不代表当前已安装或可执行状态。
_Avoid_: Registry、已安装 Agent

**执行 Registry（Agent Registry）**：
供执行路径读取的不可变 Agent 定义快照。
_Avoid_: 市场目录、安装数据库
