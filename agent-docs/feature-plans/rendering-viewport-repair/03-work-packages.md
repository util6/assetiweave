# 工作包与依赖图

## RVP-00：保护基线与固化复现

### 目标

在修改实现前，把现有证据转成可重复的仓库内诊断能力，并保护工作区已有未提交修改。

### 实施

1. 记录 `git status --short` 与渲染相关文件的独立 diff；禁止清理、覆盖或格式化无关改动。
2. 扩展现有 80 Turn fixture/harness，使其能挂载真实 SessionQuestionWorkspace，而不只是调用任意 render callback。
3. 增加固定 130 与 1,000 条 Asset fixture；稳定 ID，不使用时间或随机数。
4. 增加仅在 test/DEV 可用的诊断探针，输出 `clientHeight`、`scrollHeight`、挂载 key、行区间和 coverage ratio。
5. 固化 16 次大跨度跳转脚本：在顶部、75%、15%、90%、30%、100% 等远距离位置间往返，完整序列写入测试 helper，禁止人工随意替换。
6. 先运行现有定向测试并记录基线；测试通过不等于问题消失。

### 产出

- 确定性 Conversation 与 Asset 压力夹具。
- 组合布局诊断 helper。
- 修改前基线记录，写入 `07-progress.md`。

### 完成门禁

- 在未修复代码上，诊断能复现 Conversation 视口接近内容总高度或挂载接近 80。
- 在有界实验容器中，诊断能复现异步范围提交的 coverage 缺口。
- 诊断代码不会进入生产导航或生产数据流。

## RVP-01：Conversation 有界高度与唯一滚动归属

### 目标

建立从应用内容 viewport 到 QuestionPreview 内部 scroll element 的完整高度约束链。

### 实施决策

1. 在应用布局层声明可复用的“路由内容可用高度”契约，值来自动态 viewport 与既有 titlebar/sub-navigation 布局变量；不要在 Conversation 中写死 600px、680px 或窗口截图尺寸。
2. ConversationShell 必须占满该可用高度，并包含 `min-height: 0`；页面标题与工具区为 `shrink-0`，会话主体为 `min-h-0 flex-1`。
3. SessionQuestionWorkspace 的展开分支：ResizableColumns 使用 `h-full min-h-0`，移除以 `min-h-[680px]` 充当高度契约的做法。
4. 收起分支：单预览 section 与展开分支共享同一个 `h-full min-h-0 overflow-hidden` 契约。
5. ResizableColumns 的外层 grid、第一行 viewport、横向 scroll viewport、panel group 和每个 panel 必须连续传递完整高度；底部横向滚动控件占自己的固定行，不得压出纵向页面滚动。
6. previewPanel 保持 `h-full min-h-0 min-w-0`。
7. QuestionPreview 根节点改为 `h-full min-h-0 flex flex-col`；标题区显式 `shrink-0`；RenderSafeScrollSurface 保持 `min-h-0 flex-1` 并成为唯一纵向 scroll element。
8. 小屏单列响应式布局必须明确选择：仍在有界主体中纵向排列，问题列表使用受限高度，预览取得剩余高度；禁止恢复由长内容撑开页面。
9. 页面本身与内部预览不得同时出现可滚动的同方向容器；通过诊断确认滚动事件只落在预览 surface。

### 测试

- 真实组合接缝覆盖问题列表展开/收起。
- 模拟 1280×820 和至少一个小屏宽度。
- 断言长内容增长时 viewport height 稳定、非零且小于 workspace height。
- 80 Turn 时 mounted count 远小于 80，并随 viewport 改变。
- 调整 ResizableColumns 宽度后调用测量，viewport height 不变。

### 完成门禁

- 调查条件下内部 viewport 约等于窗口剩余空间，而不是内容总高度。
- 展开与收起分支都通过；只修一个分支视为未完成。

### 建议提交

`fix(frontend): 修复会话预览高度约束链`

## RVP-02：同步提交虚拟行壳

### 目标

滚动位置改变时，同步提交新范围的定位 wrapper 和 Skeleton 边界，消除“旧行已离开、新行未提交”的帧级缺口。

### 实施决策

1. 将 VirtualizedCollection 的 TanStack Virtual 配置从显式异步改为显式 `useFlushSync: true`，保留为可审计决定。
2. 不改变 Overscan 3/5/8 策略，不增加 initial count，不将所有 items pin/eager。
3. 不绕过 DeferredSkeletonBoundary；fast phase 仍不提交新的复杂真实子树。
4. 检查同步回调内没有读取/写入业务状态、启动翻译、命令投影或其他重型副作用。
5. 若 React 19 在某个非滚动生命周期报告 flushSync 警告，先证明触发路径，再把同步限定到滚动 range 更新；禁止直接恢复全局异步配置。

### 测试

- 在公共组件测试中执行与基线一致的 16 次大跨度跳转。
- 每次跳转在连续 3 个 animation frame 采样，计算所有 mounted virtual rows 对 viewport 的覆盖并集。
- 48 个采样帧 coverage 必须全部为 100%。
- 同时断言 fast phase 的新真实内容 ready commit 为 0，证明同步的是行壳而非重型内容。
- 验证向下与向上两种方向，不能只验证加载新内容的向下路径。

### 完成门禁

- 两次独立重复实验都为 0 个无覆盖帧。
- 无 flushSync 警告、ResizeObserver loop、重复 key 或 Console error。

### 建议提交

`fix(frontend): 同步提交虚拟滚动行壳`

## Gate A：Conversation 机制验收

必须同时通过：

- 有界 viewport。
- 80 Turn 有界挂载。
- 48/48 帧完整覆盖，重复一次仍完整。
- 问题列表展开/收起、搜索定位、Result/Diff 展开、翻译、复制、分栏调整无回归。
- 浏览器预览中无 DOM 覆盖缺口。

Gate A 失败时停留在 Conversation 问题链，禁止用 Skill 样式改动掩盖。

## RVP-03：Skill 长列表绘制减负与滚动归属

### 目标

先消除已知的重复背景模糊成本，并让资产内容拥有明确、有界的 scroll element。

### 实施决策

1. Catalog 页面使用与应用布局一致的可用高度契约；PageHeader 与 AssetToolbar 位于滚动区外，内容区 `min-h-0 flex-1`。
2. DeploymentPlanPanel 与 AssetList/AssetGrid 的放置必须避免第二条页面纵向滚动；如果 plan 需要随内容滚动，应与列表位于同一个 scroll surface。
3. list/grid 模式共用同一个 scroll owner，切换时不新增嵌套滚动条。
4. 为 `.asset-list-surface.aurora-list-surface` 提供无背景模糊的明确覆盖规则，或将 AssetList 改用无 blur 的语义表面；不要全局删除其他有限尺寸组件的玻璃效果。
5. 从 AssetRow 操作区移除 `backdrop-blur-md`，用 `bg-theme-control`、边框、阴影和现有语义 token 保留层次。
6. 禁止硬编码原始色值；禁止使用 `translateZ(0)`、`will-change` 或新 containment 作为同阶段补丁。
7. 本阶段保持 `assets.map(...)`，以便独立测量“仅绘制减负”的效果。

### 测试

- CSS 防回归测试确认 AssetList 长表面和 AssetRow 操作区都没有 blur/backdrop-filter。
- 130 条 fixture 的业务行为测试覆盖展开、挂载、编辑、删除和列表/网格切换。
- 诊断记录 scroll owner 数量、scrollHeight、blur element count、长任务与原生录屏。

### 完成门禁

- 130 条样本中 AssetList 路径的背景模糊元素为 0；固定 toolbar 的玻璃效果不计入列表路径。
- 仅有一个纵向 scroll owner。
- 原生采样结果写入 `07-progress.md`，并明确是否触发 RVP-04。

### 建议提交

`perf(frontend): 降低技能资产列表绘制负担`

## Gate B：是否执行 Skill 列表虚拟化

满足任一条件即执行 RVP-04：

1. RVP-03 后，130 条原生 WebKit 快速往返仍出现 DOM 已覆盖但画面缺口。
2. 130 条同设备 trace 仍存在与滚动相关的 >50ms 主线程长任务。
3. 1,000 条样本的 AssetRow 挂载量仍接近 1,000，且滚动/交互响应明显退化。
4. 维护者明确要求 1,000 条规模也满足挂载量与 viewport 相关的目标。

未触发时，在 `07-progress.md` 写明证据并跳过 RVP-04；禁止为了“架构统一”强行虚拟化。

## RVP-04：条件触发的 Skill 列表虚拟化

### 目标

在绘制减负仍不足或 1,000 条规模不达标时，对 list 模式建立有界挂载范围。

### 实施决策

1. 复用 VirtualizedCollection，不自行实现第二套 virtualizer。
2. 以 Asset ID 为稳定 key；排序/筛选后身份不变。
3. expandedIds 保持由 Catalog controller 持有；AssetRow 卸载不得丢失展开状态。
4. 第一版只虚拟化 list 模式；grid 模式必须独立测量，不顺手复用不匹配的行估算。
5. 初始阈值以测量决定，建议从 40 项开始；最终值必须写入 `07-progress.md` 并附 130/1,000 条数据。
6. 使用动态测量处理展开行；collapsed estimate 取 fixture 中位数，不持久化精确行高。
7. 资产行不需要复杂内容延迟调度时，将 deferred rendering 关闭，避免引入不必要 Skeleton 生命周期；仅保留虚拟范围控制。
8. 当前拥有焦点或正在执行不可中断本地 UI 操作的 Asset 应 pin；普通展开项不因展开而永久 pin。
9. 全量数据操作（筛选、排序、批量操作）基于 assets 数据，不查询当前挂载 DOM。

### 测试

- 少于阈值时正常流渲染。
- 130/1,000 条时挂载量受 viewport + Overscan + pinned 限制。
- 向上/向下大跨度滚动 coverage 100%。
- 展开后测量更新不产生明显锚点跳动。
- focus、挂载、编辑、删除、筛选、排序后定位正确。

### 建议提交

`perf(frontend): 虚拟化长技能资产列表`

## RVP-05：全量验收与收口

1. 执行 `04-verification-matrix.md` 全部自动化门禁。
2. 按固定协议执行 Tauri macOS WebKit 验收，并关联录屏时间点与诊断输出。
3. 更新 ADR-0010 的后果描述：从“彻底根治”改为可验证结果，引用 ADR-0014。
4. 更新 `07-progress.md`，填写最终指标、提交、剩余风险和回滚点。
5. 用 `06-handoff-template.md` 输出 Luna 交接记录。

### 建议提交

`test(frontend): 完成滚动渲染缺口验收`

