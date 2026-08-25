# 0011: 参照 Aion UI 接入 ACP 协议并实现按需插件化 Agent 市场

> **重要等级**：核心生态破局（P0 - 插件市场扩展）  
> **状态**：已接受  
> **参考项目/架构**：Aion UI（ACP 客户端交互范式）  
> **决策日期**：2026-08-16  
> **决策证据**：`32b03ab`, `01a00b02`, `01a00b57`  
> **记录日期**：2026-08-25  

## 背景

AssetIWeave 需要具备调用外部 AI 智能体执行任务的能力（如调用 Antigravity、Claude Code、OpenCode 执行翻译、提示词优化或辅助工作流）。参考 Aion UI 的实践，系统计划接入标准的 ACP（Agent Client Protocol）协议。

在规划 Agent 运行机制时面临重大选择：
1. 是在应用打包时把所有支持的 Agent 及其 Node/Python 依赖全部全量静态捆绑预装？
2. 还是构建一个轻量级、按需下载安装的插件式 Agent 市场？

## 决策

1. **按需插件化市场（On-Demand Marketplace）**：将 Agent 定义为可独立发布、按需下载安装的扩展包。用户仅在需要某个特定 Agent 时才触发下载与依赖初始化，避免应用安装包与磁盘空间无意义膨胀。
2. **声明式协议与探活（ACP & Availability Probe）**：通过 Manifest 声明 Agent 协议（`acp` 或 `native`）与调用命令，内置探活探针（Probe）以在执行前检查可执行文件与依赖环境是否就绪。
3. **隔离沙箱调用**：Agent 进程由底层统一的 HostProcess 模块在受控环境中拉起与监控，并捕获结构化退出码与错误详情。

## 备选方案

### 全量预装所有 Agent 依赖（Monolithic Bundling）

- 缺点：应用体积将膨胀至数 GB，且每个外部 Agent 更新版本都需要重新打包 AssetIWeave。
- 结论：否决。

## 后果

- 用户可以根据自身需要自由添加、升级或卸载 Agent，保持本地环境的极简。
- 为后续 Agent 市场与对话适配器市场的底座统一（Extension Kernel）奠定了清晰的领域原型。
