# 0004: 参照飞书 CLI 架构建立统一 Stdio Engine 与 Go CLI 跨进程体系

> **重要等级**：基石级（P0）  
> **状态**：已接受  
> **参考项目**：[larksuite/cli](https://github.com/larksuite/cli)（飞书 / Lark Suite 官方命令行架构模式）  
> **决策日期**：2026-06-03  
> **决策证据**：`34d5e73`, `ce93e7e`, `019e829e`  
> **记录日期**：2026-08-25  

## 背景

AI 智能体与自动化脚本需要能够脱离 GUI，在终端中直接操作 AssetIWeave 的全部能力（挂载、检索、同步、状态查询）。项目决定开发独立的 Go 客户端（`aiwc`）。

这引发了核心架构分歧：Go CLI 是作为独立的业务实现（直接读写 SQLite 和操作软链接），还是与 Tauri 桌面端共享同一个业务内核？

## 决策

1. **唯一业务权威（AppService）**：所有持久化业务逻辑、校验规则、状态转换和事件通知全部收口在 Rust 层的 `AppService`。
2. **Stdio JSON 跨进程 Engine 协议**：参考 [larksuite/cli](https://github.com/larksuite/cli) 的解耦架构模式，Go CLI 作为轻量客户端，通过标准输入输出（Stdio）与后台拉起的 `assetiweave-engine` 进行 JSON RPC/Protocol 通信。
3. **生成的协议契约（Contract CodeGen）**：Engine 方法、入参 DTO、风险等级与权限要求由 Rust Contract 自动生成至 `cli/internal/schema/contract.json`，严禁手工修改。
4. **禁止 CLI 直连数据库**：Go CLI 绝对禁止直连 SQLite 或绕过 Engine 直接修改文件系统，保证桌面端与 CLI 的行为 100% 同构且无并发写冲突。

## 备选方案

### Go CLI 独立实现业务逻辑（Independent Go Core）

- 优点：Go 独立编译，无需拉起 Rust 进程。
- 缺点：必须用 Go 重写全部 SQLite ORM、路径展开规则、软链接计划与校验状态机。只要有一处实现不一致就会造成数据库损坏或状态脱节；双倍维护成本。
- 结论：坚决否决。

## 后果

- 任何新功能只需在 Rust 核心实现一次，桌面端与 CLI 即可同步自动获得该能力。
- 为后续 AI Agent 自动化调用（如通过 `assetiweave-memory` Skill 驱动 CLI）奠定了高度可靠的基础。
