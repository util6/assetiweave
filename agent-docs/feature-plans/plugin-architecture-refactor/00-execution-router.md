# 任务二：插件架构重构 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: 使用 `superpowers:executing-plans`，一次执行一张卡。研究阶段先用 `mattpocock-skills:research`；设计冻结前不调度代码施工模型。

**Goal:** 在任务一的成熟生态基线上，扩大高变化业务能力的可替换性，支持已定义协议范围内的独立插件安装与演进。
**Architecture:** Rust AppService/Engine 保持业务与持久状态边界；应用插件提供对应领域能力。内置和独立插件遵守同一契约，包/生命周期治理共享；不把每个通用工具函数都变成插件。
**Tech Stack:** 继承任务一验收后的 React/Rust 库、已有 ExtensionKernel/Conversation/ACP 机制；额外宿主技术由 B01 的实测决策锁定，不预选 Cordis/Node/Wasm。
**Spec:** [Issue #23](https://github.com/util6/assetiweave/issues/23)，前置 [Issue #22](https://github.com/util6/assetiweave/issues/22)。本目录是第二次独立任务的执行包。

## Global Constraints

- **入场条件：任务一 A-G01 已验收。** 本目录不是任务一中的另一个阶段，不共享执行进度或交付定义。
- 保留 ADR0012 的共享治理、响应性和消除重复执行路径目标；历史 ADR 中的“已完成”措辞不是代码事实。
- 来源只读、SQLite/asset_mounts 权威、默认直接软链接、Conversation 内容身份、Go 经 Engine 调用共享业务保持。
- 内部模块可整体重写；外部协议破坏性变更显式版本化并联动消费者；用户数据保留/校验/回退单独保证。
- 在**已定义的能力协议**范围内增加插件无需重发主程序；新增此前不存在的宿主能力协议可能需要宿主升级，两者不混为一谈。
- 独立安装不等于运行中热卸载。额外运行时由应用管理，不要求普通用户搭建开发环境。
- 不重新实现任务一已由成熟库接管的 Router/Store/Query/i18n/logging/error/HTTP 基础设施。

## 状态与确定的执行队列

本任务当前 **WAITING_FOR_TASK_1**，未实施。任务一验收后：

1. [B00 接收基线](tickets/B00-baseline.md)。
2. [B01 技术验证与决策](tickets/B01-runtime-decision.md)：研究/实验任务，不是默认选择 JS/Wasm 的实现许可。
3. [B02 冻结代码施工卡](tickets/B02-freeze-implementation.md)：将经过审查的方案和实际源码编为可执行小卡，逐卡明确接口/测试/删除项。
4. 按 B02 生成并审查通过的施工卡执行 [工作包地图](02-work-packages.md)，最终通过 [验收矩阵](04-verification-matrix.md)。

**为什么先有决策门：** 用户确认的是业务目标和成熟生态原则，尚未选择执行新插件的宿主/运行时。本包明确研究输入、输出和验收，不让 Flash/Luna 把技术空白当成自由选择。工作包地图不是可直接编码的伪完整工单。

## 每轮读取

根 AGENTS → Issue #23 最新内容与 #22 验收交接 → 本页 → 当前卡 → [01-scope-and-gates.md](01-scope-and-gates.md) 对应 Gate → [05-playbook.md](05-playbook.md)。普通执行不加载整个任务一执行包，只读取其最终交接和必要契约。

任务二独立验收/关闭，不回写任务一为未完成，也不借此扩大其交付范围。
