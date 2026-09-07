# 验证矩阵与验收协议

## 1. 自动化门禁

### 1.1 定向测试

```bash
pnpm vitest run --config frontend/vite.config.ts \
  frontend/src/components/common/rendering \
  frontend/src/components/assets \
  frontend/src/pages/catalog \
  frontend/src/pages/conversations/ConversationsPage.test.tsx \
  frontend/src/pages/conversations/ConversationsPage.sync.test.tsx \
  frontend/src/components/layout/ResizableColumns.test.tsx
```

### 1.2 全量前端门禁

```bash
pnpm typecheck
pnpm lint
pnpm test
pnpm build
pnpm artifacts:check
```

本专项不改变 Engine/DTO；若实现越界触及 Engine 合约，必须额外运行 `pnpm cli:contract` 并说明越界理由，否则视为规格偏离。

## 2. Conversation 结构与行为矩阵

| 场景 | 断言 | 自动化 | 原生 |
|---|---|---:|---:|
| 80 Turn，问题列表展开 | preview `clientHeight` 有限、稳定、非零 | 必须 | 必须 |
| 80 Turn，问题列表收起 | 与展开分支共享有界滚动模型 | 必须 | 必须 |
| 小于 12 Turn | 非虚拟路径功能完整 | 必须 | 抽样 |
| 向下大跨度跳转 | 每帧 row/Skeleton coverage 100% | 必须 | 必须 |
| 向上返回已浏览区域 | 每帧 row/Skeleton coverage 100% | 必须 | 必须 |
| scrollbar thumb 连续往返 | 无背景穿透 | 浏览器抽样 | 必须 |
| PageUp/PageDown/Home/End | 与指针滚动一致 | 必须 | 必须 |
| 搜索命中屏幕外 Card | 先定位 Turn，再定位 Card | 必须 | 必须 |
| 调整分栏宽度 | 重测后无明显高度跳动 | 必须 | 必须 |
| 展开 Result/Diff | 状态保持且行高更新 | 必须 | 必须 |
| 翻译中卸载/重挂 | 不取消、不重复启动 | 必须 | 抽样 |
| Copy/Split/Export | 基于完整数据，功能不变 | 必须 | 抽样 |

## 3. Skill 资产矩阵

| 场景 | 断言 | RVP-03 | RVP-04（若触发） |
|---|---|---:|---:|
| 130 条 list | 列表路径 blur element = 0 | 必须 | 必须 |
| 130 条 list | 唯一纵向 scroll owner | 必须 | 必须 |
| 130 条 list | 快速往返无画面缺口 | 必须 | 必须 |
| 1,000 条 list | 记录 mounted AssetRow 数 | 必须记录 | 必须有界 |
| 展开行 | 展开状态不丢失 | 必须 | 必须 |
| 快速挂载 | 正确 Asset/Profile 被调用一次 | 必须 | 必须 |
| 编辑/删除 | 正确 Asset 被调用一次 | 必须 | 必须 |
| 筛选/排序 | 稳定 Asset ID，不串行 | 必须 | 必须 |
| list/grid 切换 | 不出现双纵向滚动条 | 必须 | 必须 |
| sticky toolbar | 固定工具栏可保留有限 blur | 必须 | 必须 |

## 4. 定量指标

### 4.1 Conversation

| 指标 | 通过标准 |
|---|---:|
| 80 Turn 内部 viewport | 等于窗口剩余内容区；禁止接近 scrollHeight |
| 80 Turn mounted Turn | visible + 双侧 Overscan + pinned/eager，远小于 80 |
| 16 次跳转 × 3 帧 | 48/48 帧 coverage = 100% |
| 重复实验 | 再次 48/48 帧 coverage = 100% |
| fast phase 新 real-content commit | 0 |
| Console error/warning | 0 个相关错误；0 个 flushSync/ResizeObserver/duplicate-key 警告 |

### 4.2 Skill

| 指标 | RVP-03 标准 | RVP-04 标准 |
|---|---:|---:|
| AssetList 路径 blur element | 0 | 0 |
| 纵向 scroll owner | 1 | 1 |
| 130 条滚动长任务 | 记录；>50ms 触发 RVP-04 | 相比 RVP-03 不变差 |
| 1,000 条 mounted AssetRow | 记录 | 与 viewport 相关、远小于 1,000 |
| 10 轮快速往返画面缺口 | 0 | 0 |

## 5. 原生 Tauri/WebKit 固定协议

### 5.1 环境

- 窗口：1280×820；另测一个用户常用尺寸。
- 主题：light、dark。
- motion：默认、`prefers-reduced-motion`。
- 内容：80 Turn Conversation；130 与 1,000 Asset。
- 每组录屏至少 60fps；同设备、同窗口、同数据比较前后。

### 5.2 操作序列

Conversation 每个布局分支执行：

1. 从顶部逐步向下浏览至少 5 个 viewport。
2. 将 scrollbar thumb 快速拖到 75%。
3. 快速返回 15%。
4. 跳到 90%，再回到 30%。
5. 拖到底部，再回顶部。
6. 连续往返共 10 轮。
7. 展开一个长 Diff/Result 后重复。
8. 调整分栏宽度后重复。
9. 触发一个屏幕外搜索命中后重复。

Skill 列表执行相同的 10 轮往返，并分别在 130/1,000 条、list/grid 模式抽样。

### 5.3 空白发生时的同步取证

同一时间点记录：

- 录屏帧时间戳。
- scrollTop/clientHeight/scrollHeight。
- mounted keys。
- row intervals 与 coverage ratio。
- 空白中心点的 `elementsFromPoint()`。
- Performance trace 中对应 long task、style/layout/paint 时间。

禁止只提交截图后猜测根因。

## 6. 回归与可访问性

- Tab 焦点不跳到 body；焦点所在虚拟项按既有规则 pin。
- `aria-posinset`、`aria-setsize` 保留。
- Skeleton 不逐行播报 loading。
- reduced-motion 下 shimmer 行为保持现有契约。
- Copy all、Export、Search、筛选和排序依赖完整内存数据而非挂载 DOM。
- list/grid 切换、问题列表展开/收起后没有嵌套同向滚动条。

## 7. 失败判定

以下任一项发生即不通过：

- 用固定像素高度替代窗口剩余空间。
- 只修展开或只修收起布局。
- 通过增大 Overscan 掩盖 coverage 缺口。
- 同步提交导致 fast phase 渲染重型内容。
- Skill 绘制减负与虚拟化同批落地，无法分辨收益来源。
- DOM coverage 100% 但原生画面仍空白，却直接宣布完成。
- 只运行 CSS 字符串测试或 jsdom 就声称 WebKit 问题已修复。

