# 渲染视口与快速滚动缺口修复：执行路由

## 1. 任务目标

本专项修复两条相互独立、必须分阶段归因的渲染问题链：

1. Conversation Question 预览缺少有界视口，导致虚拟化把整篇长内容当作可视区域；在视口修正后，虚拟窗口又因异步提交而在大跨度滚动时短暂无行覆盖。
2. Skill 资产列表未虚拟化，同时在长列表表面与每行操作区叠加背景模糊，形成独立的 WebKit 绘制压力。

本专项是 ADR-0010 的补充修复，不重写统一 Skeleton、Render Scheduler、Conversation Card 或内容投影架构。

## 2. Luna 必读顺序

Luna 开始实现前必须按顺序读取：

1. `AGENTS.md`
2. `CONTEXT.md`
3. `agent-docs/adr/0010-five-layer-skeleton-rendering-scheduler.md`
4. `agent-docs/adr/0014-bounded-viewport-and-synchronous-virtual-range.md`
5. 本目录 `01-contract.md`
6. 本目录 `02-current-baseline.md`
7. 本目录 `03-work-packages.md`
8. 本目录 `04-verification-matrix.md`
9. 本目录 `05-luna-execution-playbook.md`
10. 实现完成后填写 `06-handoff-template.md` 与 `07-progress.md`

`docs/knowledge/渲染修复.md` 是调查上下文和原始证据，不是可跳步执行清单；发生冲突时，以代码、测试、ADR-0014 和本目录契约为准。

## 3. 执行顺序与硬门禁

```text
RVP-00 基线保护与复现
  -> RVP-01 Conversation 高度约束链
  -> RVP-02 VirtualizedCollection 同步范围提交
  -> Gate A：Conversation DOM 覆盖与浏览器回归
  -> RVP-03 Skill 列表绘制减负
  -> Gate B：原生 WebKit 归因采样
  -> RVP-04（条件触发）Skill 列表虚拟化
  -> RVP-05 全量回归、原生验收与文档收口
```

硬门禁：

- RVP-01 与 RVP-02 必须分开提交或至少分开采样，禁止一次性叠加后只报告“感觉改善”。
- Gate A 未通过时，不进入 Skill 列表改造。
- RVP-03 完成后必须重新采样；只有达到 `03-work-packages.md` 的触发条件，才执行 RVP-04。
- DOM 不覆盖、几何错误、DOM 已覆盖但像素空白是三类问题，禁止用同一补丁混合处理。
- 每个阶段只引入解决该阶段证据所需的最小变量；禁止顺手叠加 `translateZ(0)`、`will-change`、额外 containment、增大 Overscan 或更多预渲染行。
- Tauri macOS WebKit 是画面是否修复的最终裁决环境；jsdom 和浏览器预览只承担结构与机制验证。

## 4. 业务与架构边界

本专项允许修改：

- 前端应用壳层的可用高度传递，但只能增加明确的布局契约，不能改变路由业务语义。
- Conversation Workspace、QuestionPreview 与公共虚拟化集合的布局/提交策略。
- Catalog/AssetList 的滚动归属、绘制样式与达到门禁后的列表虚拟化。
- 渲染压力夹具、诊断探针、结构测试和行为测试。
- ADR-0010 的后续说明与本专项文档状态。

本专项禁止修改：

- Conversation DTO、SQLite、Engine 协议、AppService 或适配器投影语义。
- Conversation Turn、Part、Content Node、Card 的身份规则。
- Skill 资产的挂载、编辑、删除、筛选、排序和备份业务语义。
- 统一 Skeleton 的视觉体系和复杂内容渲染器。
- 第三方来源目录或任何源资产内容。

## 5. 完成定义

只有同时满足以下条件才可将专项标记为完成：

- Conversation 展开/收起问题列表时，预览滚动元素都有稳定、有限、非零的 `clientHeight`。
- 80 Turn 压力样本的挂载量由可视范围、Overscan 与 pinned/eager 项决定，而不是接近 80。
- 规定的大跨度往返滚动采样中，每个采样帧都有真实行或 Skeleton 行覆盖整个视口。
- Skill 列表不再在长列表整体与每行操作区叠加背景模糊。
- 130 与 1,000 条 Skill 资产的原生验收均有记录；若执行 RVP-04，1,000 条时挂载量必须与视口相关而非与总数线性增长。
- 展开、挂载、编辑、删除、筛选、排序、搜索定位、列宽调整、翻译和复制等既有行为无回归。
- `04-verification-matrix.md` 中所有必需门禁通过，`07-progress.md` 有证据而不是结论性描述。

