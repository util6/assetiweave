# 前端设计系统与视觉规范 (AssetIWeave UI)

> **核心依据**：[ADR-0009: 参照 Auroraqua-UI 进行前端整体视觉重构与组件分层体系](file:///Users/util6/code-space/assetiweave/agent-docs/adr/0009-auroraqua-ui-frontend-architecture.md)  
> **适用范围**：`frontend/src/` 下所有页面、组件、弹窗、表单与样式开发。  
> **治理效力**：项目级强制规范，所有 AI Agent 与开发者严禁违背。

---

## 1. 核心定位与黄金参考系

AssetIWeave 是面向开发者的本地优先（Local-first）桌面工具。前端界面追求工业级精致感、温润象牙/暗色玻璃拟态（Glassmorphism）、微光辉（Glow）、精细内阴影（Inset Highlight）以及类似 Mac 原生的圆润质感（Squircle & Pill 语言）。

**当编写或重构任何新 UI、组件或弹窗时，必须且只能以下列实现为黄金参考系（Golden Reference）：**

| 参考场景 | 参考源文件 | 核心特征 |
| --- | --- | --- |
| **对话记录浏览** | [ConversationsPage.tsx](file:///Users/util6/code-space/assetiweave/frontend/src/pages/conversations/ConversationsPage.tsx) | 毛玻璃悬浮工具栏、全胶囊搜索与下拉、高对比度图标与微动效、状态呼吸流 |
| **Skill 目录总览** | [CatalogPage.tsx](file:///Users/util6/code-space/assetiweave/frontend/src/pages/catalog/CatalogPage.tsx) | 统一页面顶栏 [PageHeader](file:///Users/util6/code-space/assetiweave/frontend/src/components/foundation/PageHeader.tsx)、指标卡片、标准卡片流与列表切换 |
| **通用工具栏体系** | [DataToolbar.tsx](file:///Users/util6/code-space/assetiweave/frontend/src/components/common/DataToolbar.tsx) | 严格统一的控件高度（`h-10`）、大圆角容器（`rounded-2xl`）、内凹光影、图标+文字组合 |
| **标准弹窗与对话框** | [DialogFrame.tsx](file:///Users/util6/code-space/assetiweave/frontend/src/components/foundation/DialogFrame.tsx) 与 [ConfirmDialog.tsx](file:///Users/util6/code-space/assetiweave/frontend/src/components/common/ConfirmDialog.tsx) | 居中毛玻璃遮罩、统一分段卡片背景、标准底栏按钮组排布与语义化状态色 |

---

## 2. 组件三层架构契约

严禁组件层级越界与自由发挥，任何 UI 元素必须严格属于以下三层之一：

```text
frontend/src/
├── components/foundation/     ← 第一层：原子级设计系统基石（通用容器、全局骨架、基础控件，含 TruncatedText）
├── components/ui/             ← 第一层补充：标准控件原语（Button, Input, Select, Switch, Tooltip）
├── components/common/         ← 第二层：跨业务通用的复合组件（DataToolbar, ConfirmDialog, 虚拟列表）
└── components/{domain}/       ← 第三层：业务领域特性组件（conversations, catalog, mounts, tasks 等）
```

### 层级收口红线

1. **业务组件禁止发明基础控件**：业务组件（`components/{domain}`）严禁手写原生 `<button>`、`<input>` 或自创方角边框来充当操作按钮或输入项。
2. **必须优先复用 Foundation / UI Primitives**：
   - 按钮：统一使用 `import { Button } from "@/components/ui/button"`（或 `components/foundation/SurfaceButton`）。
   - 弹窗：统一使用 `import { DialogFrame } from "@/components/foundation/DialogFrame"`。
   - 标签：统一使用 `import { Badge } from "@/components/foundation/Badge"`。
   - 悬停提示原语：统一使用 `import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from "@/components/ui/tooltip"`（`components/ui/tooltip.tsx`），基于 Radix Tooltip 封装毛玻璃浮层样式（`rounded-xl`、`backdrop-blur-xl`）及统一延迟/跳过延迟策略（`DEFAULT_TOOLTIP_DELAY_MS` / `DEFAULT_TOOLTIP_SKIP_DELAY_MS`）。
   - 截断文本原语：统一使用 `import { TruncatedText } from "@/components/foundation/TruncatedText"`（`components/foundation/TruncatedText.tsx`），封装单行（`lines={1}`，默认）及 2–3 行摘要截断（`lines={2 | 3}`），通过真实几何溢出检测与惰性 Tooltip 挂载在仅发生视觉溢出时按需展示完整内容。
   - 输入与表单容器：使用 `controlRecipe` 或 `FieldFrame`。
3. **工具栏必须使用 DataToolbar**：任何列表或页面的搜索、排序、筛选、批量操作工具条，统一采用 `DataToolbar` 及子组件（`ToolbarSearch`、`ToolbarSingleSelectDropdown`、`ToolbarActionButton`），严禁手写散乱的 Flex 工具条。

---

## 3. 几何与圆角规范 (Radius Hierarchy)

> [!CAUTION]
> **绝对禁止方角硬边！**  
> 严禁在按钮、输入框、标签、卡片上使用 `rounded-sm` (2px) 或 `rounded-md` (6px)！这类生硬方角会彻底摧毁整体界面的超椭圆与流体质感，产生粗糙的廉价后台感。

### 强制圆角标尺

| UI 元素类别 | 强制圆角规格 | 对应 Tailwind / Token | 说明与范例 |
| --- | --- | --- | --- |
| **外层容器与大弹窗** | 16px ~ 24px | `rounded-2xl` / `rounded-3xl` | [DialogFrame](file:///Users/util6/code-space/assetiweave/frontend/src/components/foundation/DialogFrame.tsx)、主工作区面板 |
| **卡片与子面板** | 12px ~ 16px | `rounded-xl` / `rounded-2xl` | `panelRecipe`、资产卡片、任务详情面板 |
| **工具栏与搜索容器** | 16px | `rounded-2xl` | `toolbarSurfaceRecipe`、搜索输入外框 |
| **标准交互按钮** | 12px ~ 16px 或 胶囊 | `rounded-xl` / `rounded-2xl` | `surfaceButtonRecipe`，高度通常为 `h-10`（或紧凑 `h-9`） |
| **标签 (Badge) 与 Tag** | 全胶囊 (Pill) | `rounded-full` | [Badge.tsx](file:///Users/util6/code-space/assetiweave/frontend/src/components/foundation/Badge.tsx)、状态指示器 |
| **分段选择器 (Segmented Tabs)** | 全胶囊 (Pill) | `rounded-full` | `ui-pill-tab`、任务过滤 Tab 切换 |

---

## 4. 色彩、光影与质感规范 (Tokens & Lighting)

### 绝对禁止硬编码
- **严禁使用原生十六进制或固定 RGB 色值**（如 `#ffffff`、`#3b82f6`、`rgba(0,0,0,0.5)`）。
- **严禁使用 Tailwind 默认调色盘名称**（如 `bg-gray-100`、`border-zinc-300`、`text-slate-600`、`text-black`）。

### 必须使用语义化 Theme Tokens
所有视觉呈现必须绑定语义化 Token，确保深色/浅色/自定义主题自适应：

| 语义通道 | 规范 Token 类名 | 典型应用 |
| --- | --- | --- |
| **主背景 / 卡片背景** | `bg-theme-card`, `bg-theme-card-header`, `bg-surface-elevated` | 面板主体、卡片背景、弹窗背景 |
| **控件表面** | `bg-theme-control`, `bg-theme-control/60`, `bg-theme-control-hover` | 按钮底色、输入框底色、下拉项 Hover |
| **边框颜色** | `border-theme-card-border`, `border-theme-control-border` | 卡片外框、控件分割线、输入框外框 |
| **主要文字** | `text-on-surface` | 标题、核心内容、正文字段 |
| **辅助 / 次要文字** | `text-on-surface-variant` | 描述、元数据、附属信息 |
| **占位 / 弱化提示** | `text-outline` | 输入框 placeholder、禁用文字 |
| **主要行动按钮** | `theme-primary-gradient border border-primary/30 text-theme-button-primary-fg` | “同步”、“保存”、“创建”等核心主操作 |
| **危险行动按钮** | `theme-danger-gradient text-theme-button-primary-fg` | “删除”、“解挂”、“强制终止” |
| **次要 / 取消按钮** | `border border-theme-control-border/80 bg-theme-control/70 text-theme-control-fg` | `variant="outline"`，搭配毛玻璃效果 |
| **光影质感** | `shadow-[var(--theme-shadow-control-inset)]` / `shadow-[var(--theme-glow)]` | 控件内凹质感与主操作外发光 |

---

## 5. 弹窗与操作区底栏规范 (Dialog & Modal Footers)

弹窗底栏必须保持统一的仪式感与操作预期：

1. **统一由 `DialogFrame` 驱动**：弹窗底栏必须传入 `DialogFrame` 的 `footer` 属性，享受框架自带的边框、背景与间距收口。
2. **按钮规格对齐**：
   - 按钮默认采用标准高度（`h-10` 或 `size="default"`），严禁为了省空间使用 `size="sm"` 把操作按钮降级为干瘪的 8px 小方块。
   - 主操作放右侧（如 `t("common.save")` 或 `t("common.close")`），使用 `variant="default"`。
   - 次操作放其左侧（如 `t("common.cancel")`），使用 `variant="outline"`。
   - 图标与文字间距固定为 `mr-1.5` 或 `gap-2`，图标统一尺寸为 `size-4`（16px）。

---

---

## 6. 分段选择器与标签切换规范 (Segmented Tabs & Switchers)

> [!IMPORTANT]
> **严禁对 Tab 分段切换使用按钮级强调渐变！**  
> `theme-primary-gradient` 是保存、同步、创建等 Action 按钮专用的强对比渐变。严禁将其用于 Tab 激活态，否则会形成刺眼的深绿/棕橙色方块，彻底破坏视觉层次。

### 统一使用 `PillTabs`

所有列表过滤、视图切换、状态分段一律使用统一组件 [PillTabs](file:///Users/util6/code-space/assetiweave/frontend/src/components/common/PillTabs.tsx)（`components/common/PillTabs`）：
1. **流体滑动指示器**：内置 `ui-pill-indicator`，利用物理曲线（`cubic-bezier(0.22, 1, 0.36, 1)`）在选项之间平滑滑动，拥有温润象牙微光背景与极细内阴影。
2. **文字高对比度**：激活态文字使用 `text-theme-nav-active-fg`（自然深暖色文字），杜绝反色冲突。
3. **全宽与紧凑支持**：支持 `fullWidth`（等宽拉伸填充侧栏或弹窗）与 `size="sm"`（紧凑型 `h-7`）/ `size="md"`（标准 `h-8`）。

---

## 7. 截断文本与溢出提示规范 (Truncated Text & Overflow Tooltip)

> [!CAUTION]
> **强制唯一入口与静态双门禁！**  
> 所有受宽度约束的单行文本及 1–3 行摘要截断，**必须**使用 `<TruncatedText>`（`components/foundation/TruncatedText.tsx`）。严禁在业务组件或通用组件中裸写 `truncate`、`text-ellipsis`、`line-clamp-1`、`line-clamp-2`、`line-clamp-3`、编写 CSS `text-overflow: ellipsis` / `-webkit-line-clamp`，或在截断文本上使用浏览器原生 `title=` 属性。该约束由 ESLint 架构检查（`pnpm lint` / `pnpm lint:architecture`）与架构边界测试（`architectureBoundaries.test.ts`）双层强制守卫。

### 强制规范

1. **统一组件入口**：
   - 单行截断（默认 `lines={1}`）与 2–3 行摘要截断（`lines={2}` / `lines={3}`）统一使用 `<TruncatedText>`。
   - 仅当文本发生真实溢出时才挂载并展示毛玻璃 `Tooltip`；嵌套在 `<button>` 或行交互控件内时默认保持 `focusable={false}`，避免点击后焦点回落误弹。
2. **路径缩写口径一致**：
   - 所有目录与文件便携路径显示（含可见文本与悬停 Tooltip 完整值），统一通过 `abbreviateHomePath`（`frontend/src/utils/path.ts`）转换为 `~` 缩写形态，严禁界面显示 `~` 缩写而悬停提示却回退为未缩写系统绝对路径。
3. **交互路径按钮与行动作文案收口**：
   - 承载路径展示的可点击按钮或表格行（如“在访达中显示”或整行点击说明），**严禁**将动作说明写在 `title` 上遮挡截断路径提示；动作说明统一迁入 `aria-label`，截断路径完整值交由内部 `<TruncatedText>` 独立提示。
4. **全局原生 `title=` 接管与静态门禁**：
   - 运行时由挂载于 [AppProviders.tsx](file:///Users/util6/code-space/assetiweave/frontend/src/app/AppProviders.tsx) 的 `GlobalNativeTitleBridge`（[NativeTitleBridge.tsx](file:///Users/util6/code-space/assetiweave/frontend/src/components/ui/NativeTitleBridge.tsx)）全局扫描并拦截 DOM 上的原生 HTML `title` 属性（搬运至 `data-aw-tooltip` 并在必要时补齐无障碍 `aria-label`），统一交由 Scheme C 样式 `Tooltip` Portal 渲染，避免操作系统/WebKit 弹出原生深灰色提示框（`[data-native-title-exempt]` 与 `<svg><title>` 保持豁免）。
   - ESLint（`eslint.config.mjs`）禁止在新代码或干净文件中于原生 HTML 标签、`<Button>`、`<Badge>`、`<TruncatedText>` 上新增 `title=` 属性；存量文件受 `scripts/native-title-baseline.json` 与 `architectureBoundaries.test.ts` 棘轮门禁约束，只允许减少不允许新增。
5. **明确豁免项**：
   - **6–8 行长正文预览卡片**：如 `PromptOverviewPage.tsx` 中的长正文折叠预览区域（`line-clamp-6` / `line-clamp-8` 等 6 行及以上多行截断）不强制改用 `<TruncatedText>`。
   - **纯图标操作按钮**：不含截断文本的纯图标按钮（如侧边导航栏折叠按钮或图标工具动作气泡）允许保留其非文本截断用途的提示机制。
   - **原生可编辑表单控件**：`<input>`、`<textarea>` 内部的文本滚动或原生输入截断行为不在本规范限制范围内。

---

## 8. 七大禁止反模式 (Seven Strict Anti-Patterns)

1. **反模式 1：方角小按钮**  
   ❌ 错误：`<button className="h-8 rounded-md border border-gray-300 px-2 text-sm">操作</button>`  
   ✅ 正确：`<Button variant="outline" size="default">操作</Button>`
2. **反模式 2：方角直角状态标签**  
   ❌ 错误：`<span className="rounded-md border border-primary/30 bg-primary/10 px-2 py-0.5 text-caption">进行中</span>`  
   ✅ 正确：`<Badge tone="primary">进行中</Badge>` 或 `<span className="inline-flex items-center rounded-full border border-primary/30 bg-primary/10 px-2.5 py-0.5 text-caption font-medium text-primary">进行中</span>`
3. **反模式 3：生硬小输入框**  
   ❌ 错误：`<input className="h-8 rounded-md border pl-8 text-sm" />`  
   ✅ 正确：使用 `ToolbarSearch`（带有 `rounded-2xl`、玻璃质感底色与统一聚焦光晕），或 `controlRecipe({ variant: "input" })`。
4. **反模式 4：自定义突兀渐变色**  
   ❌ 错误：私自编写 `bg-gradient-to-r from-blue-500 to-indigo-600` 或非统一色彩渐变。  
   ✅ 正确：直接使用全局定义经过调优的 `.theme-primary-gradient` 或 `.theme-danger-gradient`。
5. **反模式 5：Tab 切换误用 Action 按钮渐变**  
   ❌ 错误：在分段切换或 Tab 栏中给 active tab 设置 `theme-primary-gradient` 或随意使用纯色方块。  
   ✅ 正确：统一使用 `<PillTabs>` 组件，享受自带平滑滑动的 `ui-pill-indicator` 与温润象牙色微光。
6. **反模式 6：丢失交互动效**  
   可点击控件必须具备平滑微动效：Hover 阶段轻微位移（`hover:-translate-y-px`）与发光扩散，点击阶段复位（`active:translate-y-0`），过渡时长 `duration-200`。
7. **反模式 7：裸写截断类、CSS 省略号或用原生 `title=` 替代溢出提示**  
   ❌ 错误：`<span className="truncate" title={rawPath}>{abbreviateHomePath(rawPath)}</span>`、`<button title="在访达中显示"><span className="line-clamp-2">{path}</span></button>`，或在 `.css` 中手写 `text-overflow: ellipsis` / `-webkit-line-clamp`。  
   ✅ 正确：`<button aria-label="在访达中显示" type="button"><TruncatedText>{abbreviateHomePath(rawPath)}</TruncatedText></button>`，摘要多行截断使用 `<TruncatedText lines={2}>...</TruncatedText>`。
