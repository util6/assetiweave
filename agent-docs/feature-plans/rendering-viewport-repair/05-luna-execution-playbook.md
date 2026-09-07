# Luna 精准执行手册

## 1. 开始前

```bash
git status --short
git diff -- frontend/src/layouts/app/AppLayout.tsx \
  frontend/src/router/AppRouter.tsx \
  frontend/src/pages/conversations/ConversationsPage.tsx \
  frontend/src/components/layout/ResizableColumns.tsx \
  frontend/src/components/common/rendering/VirtualizedCollection.tsx \
  frontend/src/pages/catalog/CatalogPage.tsx \
  frontend/src/components/assets/AssetList.tsx \
  frontend/src/components/assets/AssetRow.tsx \
  frontend/src/styles/index.css
```

规则：

- 工作区已有改动不是本专项的一部分；不得 `git checkout .`、`git reset --hard`、统一 stash 或全仓格式化。
- 只 stage 本专项文件与明确相关的测试。
- 每个阶段先写失败测试/诊断，再修改实现，再记录结果。
- commit 使用中文 Conventional Commits，遵循 `03-work-packages.md` 的建议粒度。

## 2. RVP-00 操作清单

- [ ] 读取执行路由、契约、基线、验证矩阵和 ADR。
- [ ] 记录当前 HEAD 与工作区状态到 `07-progress.md`。
- [ ] 运行现有渲染定向测试，记录测试数与结果。
- [ ] 扩展 Conversation stress harness 到真实组合接缝。
- [ ] 新增 130/1,000 Asset fixture。
- [ ] 实现 coverage helper：对 mounted row 区间排序、合并，并计算与 viewport 区间的交集比例。
- [ ] 在旧实现上得到失败证据；测试若直接通过，先检查是否又人工固定了 viewport。

禁止事项：不要先改 CSS；不要先把 `useFlushSync` 改为 true；不要把实验 600px 写入产品代码。

## 3. RVP-01 操作清单

- [ ] 从 AppLayout/route host 找到唯一 viewport 高度来源，优先建立可复用 CSS 变量或有界 flex contract。
- [ ] 使 ConversationShell 的标题区与主体区形成 `shrink-0 + min-h-0 flex-1`。
- [ ] 展开分支将 ResizableColumns 从最小高度容器变为 `h-full min-h-0`。
- [ ] 收起分支同步改造，禁止保留第二套高度语义。
- [ ] 检查 ResizableColumns 内部 grid、横向 viewport、Group、Panel 全链路高度。
- [ ] 将 QuestionPreview 根节点从 `min-h-full` 改为完整高度与可收缩语义。
- [ ] 确认 header `shrink-0`，RenderSafeScrollSurface 是唯一纵向 scroll owner。
- [ ] 执行展开/收起、小屏和列宽调整测试。
- [ ] 记录修复后 clientHeight、scrollHeight、mounted Turn。

停止条件：如果 viewport 仍随内容增长，不进入 RVP-02；继续查找第一个没有传递确定高度的祖先。

## 4. RVP-02 操作清单

- [ ] 在 VirtualizedCollection 显式启用 `useFlushSync: true`。
- [ ] 保持 Overscan、pinned/eager 限制、estimate 和 DeferredSkeletonBoundary 不变。
- [ ] 跑 16×3 frame coverage 测试。
- [ ] 重复一次，记录两次结果。
- [ ] 统计 fast phase 新 real-content commit，必须为 0。
- [ ] 检查控制台中的 flushSync 与 ResizeObserver 警告。
- [ ] 运行 Conversation 定向回归。

停止条件：coverage 仍小于 100% 时，打印第一个未覆盖 frame 的 scrollTop、viewport、virtual rows 和 phase；不要增加 Overscan。

## 5. Gate A 检查

- [ ] 问题列表展开通过。
- [ ] 问题列表收起通过。
- [ ] 快速向下通过。
- [ ] 快速向上返回已浏览区域通过。
- [ ] 搜索定位、展开、翻译、复制、分栏调整通过。
- [ ] 浏览器组合接缝无 DOM coverage 缺口。

全部勾选后才进入 RVP-03。

## 6. RVP-03 操作清单

- [ ] 为 Catalog 建立与页面 header/toolbar 分离的有界内容 scroll surface。
- [ ] list/grid 共用同一 scroll owner。
- [ ] 只对 AssetList 长表面禁用 backdrop-filter，不扩大到无关玻璃组件。
- [ ] 移除 AssetRow 操作区的 `backdrop-blur-md`。
- [ ] 使用语义 Theme Token 保持背景、边框和阴影。
- [ ] 此阶段保留 `assets.map(...)`。
- [ ] 跑 130 条业务回归与原生采样。
- [ ] 记录 blur element、scroll owner、long task 和画面结果。
- [ ] 按 Gate B 明确写出“执行 RVP-04”或“跳过 RVP-04”及证据。

## 7. RVP-04 条件操作清单

仅 Gate B 触发时执行：

- [ ] AssetList list 模式复用 VirtualizedCollection。
- [ ] Asset ID 作为稳定 key。
- [ ] expandedIds 保持外置。
- [ ] 根据 fixture 中位数设 collapsed estimate，并用 measureElement 处理展开行。
- [ ] 若资产行没有重型子树，关闭 deferred rendering，只使用虚拟范围。
- [ ] pin 当前 focus/必要操作项，不 pin 所有 expanded 项。
- [ ] 130/1,000 条验证 mounted count、coverage 和交互。
- [ ] grid 模式保持独立，未测量前不强行虚拟化。

## 8. 故障分流

### A. 视口没有行覆盖

检查顺序：

1. scrollElementRef 是否指向真实滚动元素。
2. clientHeight 是否非零且有界。
3. range 更新是否同步提交。
4. virtual row top/bottom 是否覆盖 viewport。
5. stable key/rangeExtractor 是否把预期范围排除。

### B. 行存在但错位

检查顺序：

1. measureElement 是否绑定在定位 wrapper。
2. ResizeObserver 是否收到真实高度。
3. 动态展开或分栏宽度变化后是否 measure。
4. estimate 与真实高度差是否造成锚点调整。
5. transform 与 scrollTop 是否处于同一坐标系。

### C. DOM 正确但画面空白

按单变量顺序采样：

1. backdrop-filter。
2. 半透明长表面。
3. 大面积阴影/渐变。
4. 既有 containment。
5. WebKit 合成层。

每次只改一项并重复固定协议；不得同时加入 GPU hack。

## 9. 完成前

- [ ] 执行全部自动化门禁。
- [ ] 完成 Tauri/WebKit light/dark 与 reduced-motion 验收。
- [ ] 更新 ADR-0010 的过度结论并引用 ADR-0014。
- [ ] 填写 `07-progress.md`，所有数字有命令或录屏依据。
- [ ] 填写 `06-handoff-template.md`。
- [ ] 检查 `git diff --check`。
- [ ] 检查提交仅包含本专项变更。

