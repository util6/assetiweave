# A-F13：成熟分栏组件接管 resize 与尺寸偏好

> **Status: PLANNED**。使用 `superpowers:executing-plans`，一轮只做本卡。

**Goal:** 删除自研分栏拖动/键盘 resize 算法，保留 Finder 式横向浏览与 SQLite 尺寸偏好。
**Depends:** A-F12、A-F06、A-C01。
**Contracts:** C-BASE、C-SETTINGS、C-UI。
**Gates:** G-FE、G-BEHAVIOR。

## 文件与接口

- Modify: `package.json`、`pnpm-lock.yaml`、`frontend/src/components/layout/ResizableColumns.tsx`、`ResizableColumns.test.tsx`（同目录）、`frontend/src/store/settings/useAppSettings.ts`。
- Create: `frontend/src/components/layout/columnLayouts.ts`、`columnLayouts.test.ts`。
- 消费者仍调用原 `ResizableColumnsProps`，包括 columns/minimumWidth/storageKey；不全仓换页面布局。
- Consumes: settings `columnLayouts: Record<string, number[]>`、`setColumnLayout(storageKey:string, weights:number[]):void` 与 `setColumnLayoutAsync(storageKey:string, weights:number[]):Promise<void>`（A-F06 定义）；库 Group/Panel/Separator/Layout。
- Produces: `toPanelLayout(weights: readonly number[]): Record<string, number>`，固定 IDs `column-0..n`，将合法权重正规化到总和100；`fromPanelLayout(layout: Record<string,number>, count:number): number[] | null`，只接受 count 个有限正数。

## 关键实现与测试

4.12.3 的实际 API 是 `Group orientation="horizontal"`、Panel/Separator；尺寸数字代表像素，百分比用字符串，layout map 使用百分数。Panel/Separator 是 Group 的直接 DOM 子节点，不新增破坏测量的 div 包裹。

```tsx
<Group orientation="horizontal" defaultLayout={toPanelLayout(weights)}>
  <Panel id="column-0" defaultSize="40%" minSize={minimumWidth}>
    {childArray[0]}
  </Panel>
  <Separator aria-label={ariaLabel} />
  <Panel id="column-1" defaultSize="60%" minSize={minimumWidth}>
    {childArray[1]}
  </Panel>
</Group>
```

实际实现按 columns/childArray 生成 N 列和 N-1 个 Separator，minSize 继承原 minWidthScale。外层 overflow 容器与可达底部滚动控件保留；Group canvas 至少能满足各 minWidth 的总和，不让新库把窄窗口挤出最小宽度。

```ts
import { expect, it } from "vitest";
import { fromPanelLayout, toPanelLayout } from "./columnLayouts";
it("权重转库 layout 后保持比例", () => {
  expect(toPanelLayout([1, 2, 1])).toEqual({"column-0":25,"column-1":50,"column-2":25});
  expect(fromPanelLayout({"column-0":25,"column-1":75}, 2)).toEqual([25,75]);
  expect(fromPanelLayout({"column-0":25}, 2)).toBeNull();
});
```

## 步骤

- [ ] 跑原 ResizableColumns 测试，保留最小宽度/窄屏/滚动可达的行为断言；新增 layout 转换和 native Separator 键盘测试。
- [ ] `pnpm add -E react-resizable-panels@4.12.3`；用锁定声明文件确认 Group/Panel 属性，组件真实切库，不包着旧 resize 算法。
- [ ] 安装测试所需的 ResizeObserver/元素尺寸 fixture，使用 Vitest 原有方式；不要 mock 掉整个库后声称键盘/尺寸验收完成。
- [ ] settings 数据成功读取后先用 SQLite `columnLayouts[storageKey]`；缺失才读旧 localStorage 的权重数组，合法且列数一致时经正常设置 mutation 一次导入；`await setColumnLayoutAsync(...)` 成功后再删除旧 key。没有 storageKey 的实例只维护局部布局。
- [ ] 用 `onLayoutChanged` 的提交回调持久化；过滤初始 mount/程序设置/尺寸重算，只有用户 resize 才保存（4.12.3 使用第二个参数 `meta.isUserInteraction`，只有 true 才提交）。重复相同权重不保存，每次拖动结束最多一次写。读取旧 prefs 不得在每次 render 自动回写。
- [ ] 设置异步到达时通过库 groupRef/setLayout 应用已存比例；这类程序变更不触发持久化回环。
- [ ] 删除原 ColumnDragState、pointer resize listener、resizeColumnWeights/resizeColumnDragWeights/getColumnBoundaries 等被库取代的机制。横向 scrollbar 的 ScrollMetrics/拖动逻辑仍属产品布局，保留独立代码与测试。
- [ ] 验证新增表单/设置测试均通过，不以快照更新掩盖控件不可达。

```sh
pnpm exec vitest run --config frontend/vite.config.ts frontend/src/components/layout/ResizableColumns.test.tsx frontend/src/components/layout/columnLayouts.test.ts frontend/src/store/settings
pnpm typecheck
pnpm lint
pnpm build
```

**完成：** 页面真用库 resize、键盘和 minSize；旧尺寸机制删除；SQLite 偏好重启后恢复；横向布局控制仍可达。
**API:** [官方仓库及发布 API](https://github.com/bvaughn/react-resizable-panels)。
