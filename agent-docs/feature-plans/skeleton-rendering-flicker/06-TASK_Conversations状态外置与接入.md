# Task 06：Conversations 状态外置与首批接入

## 1. Objective

将 Conversations Question Preview 接入 RenderSafeScrollSurface、RenderActivityProvider、VirtualizedCollection 和 Deferred Skeleton Boundary，同时保证翻译、展开、搜索定位、复制和焦点行为正确。

依赖：Tasks 01–05 全部完成并通过 Checkpoint。

## 2. Required refactoring boundaries

v1 只重构前端展示状态，不改变：

- Conversation DTO。
- `buildConversationDisplayNodes` 的语义结果。
- Card renderer registry。
- Markdown、Diff、Result、Tool 的具体渲染器。
- 翻译后端调用和后台任务协议。
- Split/Merge/Export 业务行为。

## 3. Turn presentation model

从 `QuestionPreview` 当前 map 内联逻辑提取纯函数：

```ts
export interface ConversationTurnPresentation {
  blocks: ConversationContentBlock[];
  displayNodes?: ConversationDisplayNode[];
  hasContent: boolean;
  promptBlockId: string;
  turn: ConversationTurn;
}

export function buildConversationTurnPresentations(
  question: ConversationQuestionDetail,
): ConversationTurnPresentation[];
```

规则：

- 输出顺序与 `question.turns` 一致。
- `turn.id` 是 VirtualizedCollection 稳定 key。
- `promptBlockId` 保持 `${turn.id}-question`，不得破坏已有 deep link。
- Structured cards 和 legacy parts 的构建结果必须与迁移前相同。
- 函数纯粹、无 React state、无翻译调用。

## 4. Persistent UI state controller

新增建议位置：

```text
frontend/src/components/conversations/useConversationContentController.ts
```

目标接口：

```ts
export interface ConversationContentController {
  cancelTranslation(blockId: string): Promise<void>;
  copyBlock(block: ConversationContentBlock): Promise<void>;
  expandedBlockIds: ReadonlySet<string>;
  getTranslatedText(block: ConversationContentBlock): string | undefined;
  getTranslationError(blockId: string): TranslationUiError | undefined;
  getTranslationPhase(blockId: string): AiExecutionPhase | undefined;
  isCopied(blockId: string): boolean;
  isTranslating(blockId: string): boolean;
  toggleExpanded(blockId: string): void;
  translateBlock(block: ConversationContentBlock): Promise<void>;
  translationAvailability: TranslationAvailabilityStatus;
}
```

Hook 输入必须覆盖当前 `ConversationContentCards` 已使用的翻译依赖：

```ts
export interface UseConversationContentControllerOptions {
  blocks: readonly ConversationContentBlock[];
  onCopyError?: (message: string) => void;
  onTranslationError?: (message: string) => void;
  recordKind: ConversationRecordKind;
  t: Translator;
  translationAvailabilityChecker?: () => Promise<OpencodeTranslationAvailability>;
  translationSaver?: (
    request: ConversationPartTranslationUpdateRequest,
  ) => Promise<void>;
  translationSettings: ResolvedConversationTranslationSettings;
  translationTaskController?: ConversationTranslationTaskController;
  translator?: (
    request: ConversationCardTranslationRequest,
  ) => Promise<OpencodeTranslationResult>;
}
```

`TranslationUiError`、`TranslationAvailabilityStatus` 如被 Controller 和 Card 同时使用，必须从 Conversation 模块公共类型文件导出，不得复制定义。

### 4.1 State ownership

Controller 位于 `QuestionPreview` 生命周期，持有：

```text
expandedBlockIds
translatedBlocks
translationErrors
translatingBlockIds
translationTaskByBlockId
translationAvailability
copiedBlockId + reset timer
handledTerminalTaskIds
mountedRef
```

`ConversationContentCards` 和 `ConversationContentCard` 改为受控消费：

- `expanded` 来自 Controller。
- 展开按钮调用 `toggleExpanded(block.id)`。
- 翻译、取消、复制通过 Controller。
- Card/Turn 卸载不清除 Controller 状态。

### 4.2 Correctness rules

- 正在运行的 translation task 不因 Turn unmount 取消。
- Turn 重新挂载后通过 task ID 恢复 phase。
- 已翻译文本重新挂载后仍显示。
- Result/Diff 展开状态重新挂载后仍保持。
- Question ID 改变时创建新的 Controller 生命周期，旧问题状态可以释放。
- `QuestionPreview` unmount 时清理 copy timer；后台 task 仍由全局 task provider 管理。
- translation availability 对整个 Question 只检查一次，不得每个 Turn 重复检查。

## 5. ConversationTurn component

提取：

```ts
export interface ConversationTurnProps {
  activeBlockId?: string | null;
  controller: ConversationContentController;
  index: number;
  model: ConversationTurnPresentation;
  onSplit?: (turnId: string) => Promise<void>;
  t: Translator;
}

export const ConversationTurn = React.memo(...);
```

要求：

- 根节点：

```tsx
<section
  className="conversation-turn"
  data-conversation-turn-id={model.turn.id}
>
```

- Prompt DOM ID、Copy、Split、highlight 行为保持不变。
- `ConversationContentCards` 接收同一 Controller。
- `React.memo` 比较稳定 model 引用和必要 props；不得写深度 JSON 比较。

## 6. Turn Skeleton

新增到统一 Conversations Feature Skeleton 文件：

```ts
export function ConversationTurnSkeleton(): React.ReactElement;
```

必须组合 Foundation：

```tsx
export function ConversationTurnSkeleton() {
  return (
    <SkeletonSurface className="grid gap-4 p-4">
      <Skeleton className="h-4 w-32" />
      <SkeletonText lines={3} />
      <SkeletonSurface className="grid gap-3 p-4">
        <Skeleton className="h-4 w-24" />
        <SkeletonText lines={4} />
      </SkeletonSurface>
    </SkeletonSurface>
  );
}
```

规则：

- 不拥有 status root。
- 不创建 Conversation 专属 shimmer。
- 大体表达 Prompt + Content Card 即可。
- 使用 `size="tall"`，不计算真实 Turn 高度。

## 7. QuestionPreview integration

目标结构：

```tsx
const previewScrollRef = useRef<HTMLDivElement>(null);
const virtualizedRef = useRef<VirtualizedCollectionHandle>(null);
const models = useMemo(
  () => buildConversationTurnPresentations(question),
  [question],
);
const controller = useConversationContentController({
  blocks: models.flatMap(collectBlocks),
  // existing dependencies
});

return (
  <div className="conversation-readable flex min-h-full flex-col">
    <QuestionPreviewHeader />
    <RenderSafeScrollSurface
      className="min-h-0 flex-1"
      ref={previewScrollRef}
    >
      <RenderActivityProvider scrollElementRef={previewScrollRef}>
        <div className="render-safe-scroll-content px-5 py-5">
          <VirtualizedCollection
            eagerKeys={activeTurnKeys}
            estimateSize={420}
            fallback={() => <ConversationTurnSkeleton />}
            getItemKey={(model) => model.turn.id}
            items={models}
            minItems={12}
            onItemReady={handleTurnReady}
            pinnedKeys={pinnedTurnKeys}
            ref={virtualizedRef}
            renderItem={(model, index) => (
              <ConversationTurn
                activeBlockId={activeBlockId}
                controller={controller}
                index={index}
                model={model}
                onSplit={onSplit
                  ? (turnId) => onSplit(question, turnId)
                  : undefined}
                t={t}
              />
            )}
            scrollElementRef={previewScrollRef}
            size="tall"
          />
        </div>
      </RenderActivityProvider>
    </RenderSafeScrollSurface>
  </div>
);
```

实际实现可以提取 Header，但不得改变布局和功能。

## 8. 搜索与深度链接导航 (Search & Deep-link Navigation)

当前 `document.getElementById(...).scrollIntoView()` 假设目标已挂载，虚拟化后不成立。

必须建立：

```ts
export function buildConversationBlockTurnIndex(
  models: readonly ConversationTurnPresentation[],
): ReadonlyMap<string, string>;
```

索引必须包含：

- promptBlockId。
- Card block ID。
- legacy anchor IDs。
- execution group 内 command/result block ID。

定位流程：

1. `activeBlockId` 查到所属 turnId。
2. 将 turnId 加入 eagerKeys 和 pinnedKeys。
3. 调用 `virtualizedRef.current?.scrollToKey(turnId, { align: "center", behavior: "auto" })`。
4. `onItemReady(turnId)` 到达后，在下一次 RAF 查找真实 block DOM。
5. 找到后调用现有 `scrollIntoView({ block: "center", behavior: "auto" })`。
6. 如果 block DOM 不存在，记录开发诊断，但不得无限 retry。
7. 最多等待一次 onReady + 一次 RAF；数据索引错误应由测试暴露。

## 9. Pinned turn calculation

Pinned keys 包含：

- active search target 所属 Turn。
- 当前焦点元素最近的 `[data-conversation-turn-id]`。
- 当前用户正在操作且本地交互尚未提交的 Turn。

由于翻译状态已外置，运行 translation task 本身不要求 pin Turn。

最多 4 个 pinned；遵循 VirtualizedCollection 限制。

## 10. Resize behavior

当以下事件发生时调用：

```ts
virtualizedRef.current?.measure();
```

事件：

- Question List collapsed/expanded。
- ResizableColumns 宽度改变并在 resize end 稳定后。
- 全局 typography/font scale 改变。
- 翻译文本插入或 Result 展开由 item ResizeObserver 自动测量，不额外全量 measure。

禁止在 splitter pointermove 每一个事件中同步 measure 全部内容；使用下一帧合并调用。

## 11. Feature flags

```tsx
enabled={renderingFlags.conversationTurnVirtualization}
```

```tsx
<DeferredSkeletonBoundary
  enabled={renderingFlags.deferredSkeletonRendering}
  ...
/>
```

Layer 0 `RenderSafeScrollSurface` 不受 flags 控制。

## 12. Tests

### Pure functions

- Turn presentation 保持顺序。
- structured/legacy 内容投影与迁移前一致。
- block-to-turn 索引覆盖 prompt、card 和 legacy anchors。

### Controller

- expanded 状态跨 Turn unmount/remount 保持。
- translated text 保持。
- task phase 恢复。
- availability 每 Question 只检查一次。
- unmount 清理 timer，不取消全局 task。
- Question 改变后状态隔离。

### Integration

- 80 Turn fixture 不再同时挂载全部 Turn。
- fast phase 新进入 Turn 显示 ConversationTurnSkeleton。
- idle 后可见 Turn ready。
- ready Turn 开始滚动后仍 ready。
- active search target 即使在未挂载 Turn 中也能定位。
- 展开 Card 滚出/滚回后仍展开。
- translation task 滚出/滚回后不重复启动。
- 关闭 virtualization flag 时全部 Turn 按正常流渲染。
- 关闭 deferred flag 时虚拟项立即渲染真实内容。

### Existing regression

必须继续通过：

- Copy Prompt/Card。
- Split/Merge。
- Export。
- Content visibility filter。
- Translation availability、start、cancel、success、failure、save failure。
- Active Card highlight。

## 13. Files likely touched

该任务必须拆成至少三个提交，避免单次超过 5 个文件。

### Commit A：状态外置

```text
frontend/src/components/conversations/useConversationContentController.ts
frontend/src/components/conversations/useConversationContentController.test.tsx
frontend/src/components/conversations/ConversationContentCards.tsx
frontend/src/components/conversations/ConversationContentCards.test.tsx
```

### Commit B：Turn model 和组件

```text
frontend/src/components/conversations/ConversationTurn.tsx
frontend/src/components/conversations/ConversationTurn.test.tsx
frontend/src/components/conversations/ConversationSkeleton.tsx
frontend/src/components/conversations/ConversationSkeleton.test.tsx
```

### Commit C：QuestionPreview 集成

```text
frontend/src/pages/conversations/ConversationsPage.tsx
frontend/src/pages/conversations/ConversationsPage.test.tsx
frontend/src/pages/conversations/ConversationsPage.sync.test.tsx
frontend/src/styles/index.css
```

## 14. Acceptance criteria

- [ ] v1 只按 Turn 虚拟化。
- [ ] 复杂 renderer 未增加 scroll/quality 分支。
- [ ] 80 Turn fixture 挂载数量受 viewport + overscan 限制。
- [ ] 未 ready Turn 始终显示统一 Skeleton。
- [ ] 展开和翻译状态跨卸载保持。
- [ ] active block 深链接可定位未挂载 Turn。
- [ ] resize 不造成每 pointermove 全量测量。
- [ ] Layer 0 始终启用，其他层可通过 flag 回滚。
- [ ] 原有 Conversation 回归测试全部通过。

## 15. Verification

```bash
pnpm vitest run --config frontend/vite.config.ts \
  frontend/src/components/conversations/useConversationContentController.test.tsx \
  frontend/src/components/conversations/ConversationTurn.test.tsx \
  frontend/src/components/conversations/ConversationSkeleton.test.tsx \
  frontend/src/components/conversations/ConversationContentCards.test.tsx \
  frontend/src/pages/conversations/ConversationsPage.test.tsx \
  frontend/src/pages/conversations/ConversationsPage.sync.test.tsx
pnpm typecheck
pnpm build
```

人工：执行 Task 01 的 10 轮滚动、展开状态、translation task 和 active search target 流程。

## 16. Commits

```text
refactor: 外置会话内容交互状态
refactor: 提取会话回合渲染单元
feat: 接入会话骨架渲染与回合虚拟化
```
