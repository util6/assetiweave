# 当前基线、证据与代码接缝

## 1. 代码基线

### 1.1 Conversation 高度链

当前关键结构：

```text
AppLayout main（只有 min-height）
  -> AppRouter route host（min-h-0 flex-1）
    -> ConversationShell（flex-1，无明确有界高度）
      -> SessionQuestionWorkspace
        -> ResizableColumns 或 collapsed section（min-h-[680px]）
          -> previewPanel section（h-full min-h-0）
            -> QuestionPreview（min-h-full）
              -> header
              -> RenderSafeScrollSurface（min-h-0 flex-1）
```

失效点：

- `frontend/src/layouts/app/AppLayout.tsx` 的主内容只有最小高度，允许内容继续撑高。
- `frontend/src/pages/conversations/ConversationsPage.tsx` 的 ConversationShell 没有把应用内容区变成有界高度上下文。
- 同文件的 SessionQuestionWorkspace 展开/收起分支都依赖 `min-h-[680px]`；最小高度不是最大值或可用高度。
- QuestionPreview 使用 `min-h-full`，不会强制自身等于父容器高度。
- 下游的 `min-h-0 flex-1` 只有在祖先已经有确定高度时才有效。

### 1.2 VirtualizedCollection 提交时序

`frontend/src/components/common/rendering/VirtualizedCollection.tsx` 当前显式配置：

```ts
useFlushSync: false
```

TanStack Virtual 在滚动范围变化后可以先更新滚动位置，再等待 React 常规提交新范围。旧虚拟行已经离开视口，而新范围行壳和位于行壳内部的 Skeleton 尚未出现时，会产生完整背景帧。

### 1.3 Skill 资产列表

当前关键结构：

- `frontend/src/pages/catalog/CatalogPage.tsx` 直接在页面文档流中渲染 AssetList。
- `frontend/src/components/assets/AssetList.tsx` 在 list 模式直接执行 `assets.map(...)`。
- `frontend/src/components/assets/AssetRow.tsx` 的每行操作区使用 `backdrop-blur-md`。
- `frontend/src/styles/index.css` 的 `.asset-list-surface` 与 `.aurora-list-surface` 都包含背景模糊，后者还包含 saturate。
- AssetToolbar 虽配置 sticky，但页面没有为资产内容声明唯一、有界的内部纵向滚动元素。

## 2. 已确认实验

### 2.1 Conversation 视口实验

条件：真实组件、仓库 80 Turn 压力数据、1280×820 浏览器窗口。

| 配置 | 内部滚动视口高度 | 挂载 Turn |
|---|---:|---:|
| 当前 Conversation Workspace | 36,343px | 80 |
| 完整高度约束链 | 469px | 4 |

辅助实验：在 600px 明确父高度内，仅将 QuestionPreview 从最小高度语义改为完整高度语义，内部视口从 36,343px 降到 503px，挂载量从 80 降到 4。

结论：有界视口是虚拟化正确工作的必要条件。

### 2.2 虚拟范围时序实验

前置：先修正高度链。执行 16 次大跨度跳转，每次采样 3 帧。

| 配置 | 48 帧中无任何虚拟行覆盖视口 |
|---|---:|
| `useFlushSync: false` | 15 |
| `useFlushSync: true` | 0 |
| `useFlushSync: true` 重复 | 0 |

结论：同步范围提交是“每帧至少有行壳或 Skeleton”的必要条件；Skeleton 数量本身不能弥补尚未提交的虚拟行。

### 2.3 Skill 列表绘制风险

| 指标 | 130 条样本 |
|---|---:|
| 列表总高度 | 约 16,597px |
| 带背景模糊元素 | 131 |
| 同时挂载 AssetRow | 130 |

结论：这是明确的绘制负担与扩展性风险，但还没有证据把原生 WebKit 同款空白唯一归因于背景模糊。

## 3. 现有测试为何漏报

- `VirtualizedCollection.test.tsx` 人工把滚动元素 `clientHeight` 固定为 240px，因此绕过了真实祖先高度失控。
- 现有测试主要验证挂载量小于总量，没有验证向前/向后大跨度滚动后的逐帧视口覆盖并集。
- `RenderSafeScrollSurface.test.tsx` 读取 CSS 字符串验证不透明、isolation 和 containment，没有执行布局或 WebKit 绘制。
- `RenderingStressHarness` 只把 80 Turn fixture 交给调用方，并未提供真实 Conversation Workspace 布局、滚动脚本或覆盖探针。
- Catalog/AssetList 没有针对长列表挂载数量、背景模糊数量或快速滚动的专项测试。

## 4. 测试接缝决策

优先使用一个最高组合接缝：

```text
真实 SessionQuestionWorkspace
  + 真实 ResizableColumns
  + 真实 QuestionPreview
  + 真实 RenderSafeScrollSurface
  + 真实 VirtualizedCollection
  + 80 Turn 确定性 fixture
```

该接缝负责验证高度链、滚动归属、展开/收起与挂载边界。公共 VirtualizedCollection 保留一个更低层的精确时序测试，用于验证大跨度滚动同帧覆盖。

Skill 侧使用一个 Catalog 内容接缝，组合真实 AssetList/AssetRow 与 130/1,000 条确定性 fixture，验证滚动归属、业务交互和条件虚拟化。CSS 字符串测试只做补充。

## 5. 诊断分类

每次空白采样必须记录以下值：

```text
timestamp
scrollTop
clientHeight
scrollHeight
mounted item keys
每个虚拟行的 [top, bottom]
viewport coverage ratio
document.elementsFromPoint() 抽样结果
屏幕录制时间点
```

按以下规则归因：

| 观测 | 分类 | 后续动作 |
|---|---|---|
| coverage < 100%，视口区间没有虚拟行 | 范围提交缺口 | 检查同步提交、scroll element、range 计算 |
| coverage = 100%，但行框错位、重叠或高度异常 | 测量/锚点缺陷 | 检查 measureElement、ResizeObserver、动态高度与列宽重测 |
| coverage = 100%，几何正确，但录屏画面空白 | WebKit 绘制/合成缺陷 | 隔离 blur、透明层、阴影和合成层；一次只改一个变量 |

## 6. 与既有架构的关系

- ADR-0010 的 Layer 0–5 继续有效，但新增两个前置不变量：有界视口和同步行壳提交。
- 现有渐进命令展示控制“何时投影/展示更多内容”，不等于限定真实 viewport。
- 已经读取或已经在内存中的数据不保证对应 DOM 仍挂载；虚拟化会卸载远离 viewport 的 Turn。
- 既有 DeferredSkeletonBoundary 和 RenderScheduler 继续负责复杂内容，不应被移除或改成同步渲染全部子树。

