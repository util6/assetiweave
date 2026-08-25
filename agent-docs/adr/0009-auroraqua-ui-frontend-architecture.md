# 0009: 参照 Auroraqua-UI 进行前端整体视觉重构与组件分层体系

> **重要等级**：核心级（P1）  
> **状态**：已接受  
> **参考项目**：[micromimo/Auroraqua-UI](https://github.com/micromimo/Auroraqua-UI)  
> **决策日期**：2026-08-12  
> **决策证据**：`3999cbe`, `52e8ffb`, `f4dc-ca8`  
> **记录日期**：2026-08-25  

## 背景

在项目演进至中期时，早期快速搭建的前端界面显现出明显的碎片化与简陋感：组件没有统一的分层约束，样式充斥着 Ad-hoc 的随意类名，弹窗容易发生嵌套遮罩错位与截断，深色模式缺乏层次感与视觉质感。

作为一款需要开发者高频常驻使用的生产力桌面工具，系统需要一套工业级、具有沉浸质感且结构清晰的前端架构。

## 决策

1. **设计系统升级**：参考开源设计体系 [micromimo/Auroraqua-UI](https://github.com/micromimo/Auroraqua-UI)，全面落地现代化暗色主题设计系统，引入玻璃拟物（Glassmorphism）、语义化 Theme Tokens、微妙微动效与 Mac Dock 风格圆角图标体系。
2. **严谨的组件三层架构**：
   - `components/foundation`：原子级设计系统基石（如 `DialogFrame`、`Button`、`Portal` 等基础容器）。
   - `components/common`：跨业务通用的复合组件（如 `DataToolbar`、`PathPickerInput`、虚拟化表面）。
   - `components/{domain}`：按业务领域内聚的特性模块（如 `assets`, `groups`, `sources`, `conversations`）。
3. **前端服务边界收口**：严禁页面组件直接 `invoke(...)` 调用 Tauri 后端，所有数据交互必须经过 `frontend/src/services/` 统一边界，保证前端拥有独立的 Mock 测试能力。

## 备选方案

### 引入重型第三方 UI 框架（如 Ant Design 或 Material UI）

- 缺点：包体积巨大，难以与 Tauri 桌面端轻量设计及自定义暗色毛玻璃美学深度融合，样式覆盖成本极高。
- 结论：否决。

## 后果

- 前端视觉品质与人机交互体验达到顶级桌面原生工具标准。
- 组件分层清晰，极大降低了后续功能扩展中的回归 Bug 概率。
