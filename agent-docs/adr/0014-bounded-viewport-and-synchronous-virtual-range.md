# 0014: 有界视口与同步虚拟范围是滚动渲染架构的前置不变量

> 状态：已接受
> 决策日期：2026-09-07
> 证据来源：`docs/knowledge/渲染修复.md`
> 关联决策：ADR-0010

## 背景

ADR-0010 建立了不透明滚动表面、虚拟化、Overscan、Skeleton、共享调度器和尺寸测量架构，但实际 Conversation 接入遗漏了两个更基础的前置条件：

1. 虚拟化 scroll element 必须拥有来自应用 viewport 的稳定、有限高度；`min-height` 不构成有界 viewport。
2. 滚动位置变化后，虚拟行壳必须在同一更新窗口及时覆盖新 viewport；Skeleton 位于行壳内部，行壳未提交时 Skeleton 也不存在。

对照实验表明，缺少高度约束时 80 Turn 全部挂载，内部 viewport 被内容撑到约 36,343px；补齐高度链后 viewport 约 469px、挂载 4 Turn。高度修正后，异步范围提交在 48 个采样帧中产生 15 个无行覆盖帧；同步范围提交两次实验均为 0。

Skill 资产列表没有接入 Conversation 虚拟窗口，因此其全部挂载与大面积背景模糊属于另一条性能问题链，必须独立归因。

## 决策

### 1. 有界视口

- 所有使用 VirtualizedCollection 的滚动区域必须从应用可用 viewport 获得明确高度。
- 高度链上的 flex/grid 子项必须允许收缩；标题/工具区固定，内容区取得剩余空间。
- `min-height` 只能表示下限，禁止作为 viewport 高度契约。
- scroll element 的 `clientHeight` 必须非零、稳定且显著小于长内容的 `scrollHeight`。

### 2. 同步虚拟范围

- 滚动 range 变化时，同步提交轻量虚拟行壳、定位 wrapper 与 Skeleton 边界。
- 复杂真实内容继续遵守 Render Scheduler，不因范围同步而在 fast phase 同步提交。
- Overscan 不能替代同步范围提交；增加 Skeleton 数量不能覆盖尚未存在的行壳。

### 3. 分层诊断

滚动空白必须分为：

1. viewport 没有 DOM 行覆盖；
2. DOM 存在但几何错误；
3. DOM 与几何正确但 WebKit 像素未绘制。

只有第三类才进入绘制/合成层优化。每次实验一次只改变一个变量。

### 4. 长列表准入

- 非虚拟长列表先建立明确 scroll owner 并消除覆盖长距离内容的重复背景模糊。
- 只有数据规模、长任务或原生绘制证据达到门禁时才接入 VirtualizedCollection。
- 长列表虚拟化必须使用稳定业务 ID，并把需要跨卸载保持的交互状态外置。

## 与 ADR-0010 的关系

本决策补充 ADR-0010，不否定其分层架构。ADR-0010 中“彻底根治”的后果描述必须理解为目标状态，而不是只要接入 Skeleton/虚拟化就自动成立。实现只有通过有界 viewport、逐帧 coverage 和 Tauri WebKit 验收后，才能声称具体接入点已修复。

## 后果

- 应用布局成为虚拟化正确性的一部分，测试必须覆盖真实祖先尺寸链。
- 虚拟范围同步会增加滚动事件中的轻量 React 提交，因此必须确保重型内容继续延迟，并用 trace 验证最长主线程任务不恶化。
- 原生绘制问题不能再由 jsdom 或 CSS 字符串测试宣告修复。
- Skill 等非虚拟长列表必须先独立测量，避免把 Conversation 的根因机械推广到所有页面。
