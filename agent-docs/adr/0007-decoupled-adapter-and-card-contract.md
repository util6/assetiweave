# 0007: 会话记录从非插件化走向外部独立适配器包与声明式卡片契约

> **重要等级**：核心生态破局（P0 - 全系统第二大架构变动）  
> **状态**：已接受  
> **决策日期**：2026-07-26  
> **决策证据**：`93f699b`, `930fd84f`, `9efc-bdb`, `9f33-8ce`  
> **记录日期**：2026-08-25  

## 背景

早期会话解析逻辑直接硬编码在 Rust Core 内部，卡片类型也被死死限制在 5 种固定的全局枚举中。这带来了严重的扩展瓶颈：
1. 不同的 AI 工具（Codex、Claude Code、Cursor、Zcode、OpenCode）具有完全不同的专有日志格式与独特的语义记录（如 Codex 的 TaskList、Claude 的 Reasoning、Diff 代码对比、Skill 引用等）。
2. 每当需要支持新工具或新卡片类型，必须同时修改 Rust 解析器、数据库迁移、前端渲染器、国际化和主题，不仅协调成本极高，而且一旦宿主日志格式变更，用户必须等待桌面端发布新版本。

这是系统从**“单体封闭系统”**走向**“开放插件市场生态”**的里程碑转折点。

## 决策

1. **外部独立适配器包（Decoupled Adapter Package）**：将各宿主的解析代码彻底剥离出 Core，封装为标准化的独立外部包（`conversation-adapter-package.json` + `adapter.mjs`），在沙箱中由 Node 进程执行。
2. **声明式卡片契约（Declarative Card Contract）**：卡片的语义 `kind` 和可选的 `semantic_role` 由适配器自由声明并随包发布。
3. **Core 控制受控渲染器注册表（Renderer Registry）**：Core 仅提供受控的安全渲染器集合（`markdown`, `diff`, `command`, `subagent`, `task_list` 等），Adapter 只能通过 `allowed_renderers` 声明渲染偏好，严禁第三方适配器注入不受信任的 UI 脚本。

## 备选方案

### 继续在 Rust Core 中扩展全局枚举

- 缺点：每增加一个工具或卡片类型都会加剧跨层耦合与版本失配风险。
- 结论：否决。

### 允许适配器提供自定义前端 React 组件

- 优点：灵活性最高。
- 缺点：引入任意代码执行安全风险、样式崩坏、破坏主题系统与搜索索引稳定性。
- 结论：否决。

## 后果

- **生态解耦**：新增宿主支持或升级解析脚本（如支持 Codex 任务列表、修复 Diff 解析）只需独立更新外部适配器包，无需重新发布或重启 AssetIWeave 桌面端。
- **奠定双市场雏形**：为后续 Agent 插件市场与 Conversation Adapter 市场的全面统一奠定了第一块基石。
