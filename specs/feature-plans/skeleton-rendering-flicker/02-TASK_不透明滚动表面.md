# Task 02：不透明滚动表面与合成隔离

## 1. Objective

在任何 Skeleton、调度或虚拟化逻辑之前，为复杂内容滚动区域建立完全不透明且合成隔离的底层，优先消除 WebKit compositor bleed-through。

依赖：Task 01 完成并记录基线。

## 2. Component contract

新增：

```text
frontend/src/components/common/rendering/RenderSafeScrollSurface.tsx
```

接口：

```ts
export interface RenderSafeScrollSurfaceProps
  extends React.HTMLAttributes<HTMLDivElement> {
  children: React.ReactNode;
}

export const RenderSafeScrollSurface = React.forwardRef<
  HTMLDivElement,
  RenderSafeScrollSurfaceProps
>(...);
```

行为：

- 输出一个真实 scroll element。
- 默认 `overflow: auto`。
- 接受并透传 ref，供 Task 03 和 Task 05 使用。
- 合并调用方 className。
- 设置 `data-render-safe-scroll-surface=""`。
- 不创建 scroll listener。
- 不包含 Skeleton 或业务 loading 状态。

## 3. Required CSS

统一 class：

```css
.render-safe-scroll-surface {
  position: relative;
  min-width: 0;
  min-height: 0;
  overflow: auto;
  isolation: isolate;
  contain: paint;
  overscroll-behavior: contain;
  background: rgb(var(--color-background));
  background-clip: border-box;
}

.render-safe-scroll-content {
  position: relative;
  z-index: 0;
  min-width: 0;
  background: rgb(var(--color-background));
}
```

规范：

- 根 scroll surface 和直接内容承载层都必须有无 alpha 的背景。
- 禁止在这两个 class 上使用 `backdrop-filter`。
- 禁止使用 `transform: translateZ(0)` 作为默认修复。
- 禁止长期设置 `will-change: transform`。
- 外层装饰性 `.conversation-surface` 可以继续保留玻璃效果，但滚动 viewport 内层必须不透明。
- Skeleton 和真实卡片可以继续使用语义层次背景；底部始终由 scroll surface 兜底。

## 4. Conversations insertion point

当前 `QuestionPreview` 内部滚动区域：

```tsx
<div className="min-h-0 flex-1 overflow-auto px-5 py-5">
```

改为：

```tsx
<RenderSafeScrollSurface
  className="min-h-0 flex-1"
  ref={previewScrollRef}
>
  <div className="render-safe-scroll-content px-5 py-5">
    {content}
  </div>
</RenderSafeScrollSurface>
```

约束：

- `previewScrollRef` 在 Task 02 可以只声明和透传，Task 03 开始消费。
- Padding 必须位于 inner content，不放在 scroll element；Task 05 的 virtual offset 以无 padding 的 scroll element 为坐标系。
- 不改变 Header 和工具栏 sticky 行为。
- 不把整个 `conversation-surface` 改成纯色；只处理实际纵向滚动层。
- 水平 `ResizableColumns` viewport 不在本任务改造范围，除非基线证明它也暴露背景。

## 5. Other candidate surfaces

Task 02 只强制接入 Conversations 预览。以下候选只记录，不批量修改：

- Conversation 问题列表。
- Memory Recall 内容区。
- LogViewer 长日志区。
- Manual 长文档区。
- 未来的长 Markdown 预览。

Task 07 根据实测决定推广。

## 6. Tests

组件测试必须验证：

- ref 指向根 scroll element。
- children 正常渲染。
- 自定义 className 正常合并。
- 根节点具有 `data-render-safe-scroll-surface`。
- Conversations 预览使用该组件。

CSS 无法仅靠 jsdom 证明不透明，必须增加源代码约束测试或静态断言：

- `.render-safe-scroll-surface` 包含 `rgb(var(--color-background))`。
- 对应规则不含 `/ 0.`、`backdrop-filter`。

静态断言应尽量限定在目标 CSS rule，不扫描整个样式文件。

## 7. Manual verification

按照 Task 01 相同步骤进行 10 轮滚动，分别记录：

- 背景穿透是否减少或消失。
- 玻璃视觉是否只在滚动内容区被收敛。
- sticky header 是否正常。
- macOS 弹性滚动边缘是否仍显示不透明背景。
- 浅色和深色主题是否均无错误底色。

该任务即使完全消除视觉穿透，也不能取消后续 Boundary 和 Virtualization；后续层解决空白和长帧，不只解决背景颜色。

## 8. Files likely touched

```text
frontend/src/components/common/rendering/RenderSafeScrollSurface.tsx
frontend/src/components/common/rendering/RenderSafeScrollSurface.test.tsx
frontend/src/pages/conversations/ConversationsPage.tsx
frontend/src/pages/conversations/ConversationsPage.test.tsx
frontend/src/styles/index.css
```

## 9. Acceptance criteria

- [ ] Conversations 真实纵向滚动元素使用 RenderSafeScrollSurface。
- [ ] 滚动元素和内容承载层均有不透明语义背景。
- [ ] 热路径没有 backdrop-filter。
- [ ] ref 可被后续任务复用。
- [ ] 既有布局和交互测试通过。
- [ ] Tauri WebKit 完成 10 轮人工滚动记录。

## 10. Verification

```bash
pnpm vitest run --config frontend/vite.config.ts \
  frontend/src/components/common/rendering/RenderSafeScrollSurface.test.tsx \
  frontend/src/pages/conversations/ConversationsPage.test.tsx
pnpm typecheck
pnpm build
```

## 11. Commit

```text
fix: 为复杂内容预览增加不透明滚动表面
```
