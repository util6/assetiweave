# Task 07：全局验收、推广准入与清理

## 1. Objective

验证 Conversations 首批接入确实修复问题，建立跨页面推广规则，并清理重复实现。该任务不要求立即虚拟化整个前端。

依赖：Task 06 完成且所有自动化测试通过。

## 2. Verification matrix

### 2.1 Environments

| Environment | Required |
|---|---|
| Vitest + jsdom | 必须 |
| Vite browser preview | 必须 |
| Tauri macOS WebKit | 必须，最终判定 |
| Light theme | 必须 |
| Dark theme | 必须 |
| `prefers-reduced-motion` | 必须 |
| 100% UI scale | 必须 |
| 项目支持的非默认 typography/font scale | 必须至少一组 |

### 2.2 Input modes

- 触控板快速滚动。
- 触控板惯性滚动。
- 鼠标滚轮。
- 滚动条 thumb 拖动。
- PageUp/PageDown。
- Home/End。
- active search target 程序化定位。
- ResizableColumns 宽度拖动后继续滚动。

### 2.3 Content states

- 默认折叠 Result。
- 展开 Result/Diff。
- 正在翻译。
- 翻译成功。
- 翻译失败。
- 搜索命中屏幕外 Card。
- Content visibility filter 开关后。
- 小于 12 Turn 的非虚拟路径。
- 80 Turn 压力路径。

## 3. Quantitative acceptance

使用 Task 01 同一 Fixture 和窗口条件。

| Metric | Target |
|---|---|
| 10 轮高速往返滚动背景穿透 | 0 次 |
| fast phase 新 ready commits | 0 |
| 同时挂载 Turn | 不超过 visible range + 双侧 Overscan + pinned，且远小于 80 |
| pinned items | ≤ 4 |
| eager items | ≤ 2 |
| 停止滚动后第一个 visible Skeleton 开始恢复 | 下一 animation frame |
| 停止后 visible Skeleton ready/queued | 300ms 内 |
| Console error | 0 |
| ResizeObserver loop warning | 0 |
| React duplicate key warning | 0 |
| Translation duplicate start | 0 |

性能 trace 目标：

- 相比 Task 01 基线，高速滚动期间最长主线程任务不得变差。
- 如果基线存在超过 50ms 的滚动长任务，改造后应减少数量；若没有减少，必须记录原因后才可推广。
- 不设置脱离设备环境的绝对 FPS 承诺；以同设备、同 Fixture 前后对比为准。

## 4. Visual acceptance protocol

每个主题执行：

1. 开始屏幕录制，帧率不低于 60fps。
2. 从顶部快速滚动到底部再返回，重复 10 次。
3. 拖动 scrollbar thumb 快速跳跃三次。
4. 在滚动过程中观察新进入区域必须显示 Skeleton 或真实内容。
5. 确认不存在：
   - 窗口背景。
   - 白色/透明条带。
   - 整个 viewport 退回 Skeleton。
   - 已 ready Card 闪回 Skeleton。
6. 停止滚动，确认 Skeleton 从 viewport 近处开始恢复。
7. 展开 Diff、启动 translation 后重复。
8. 关闭 virtualization flag 重复一次，确认安全回滚路径可用。

人工验收结果应记录在 PR 描述，不创建运行日志进入仓库。

## 5. Accessibility acceptance

- Tab 顺序不会跳到已卸载元素。
- 当前 focus item 保持 pinned。
- active search target 可以定位屏幕外 Turn。
- virtual item 包含 `aria-posinset` 和 `aria-setsize`。
- 滚动 Skeleton 不逐项播报 loading。
- 完整 Export/Copy 数据不依赖挂载 DOM。
- reduced-motion 下 shimmer 始终关闭。

## 6. Global rollout admission

其他页面只有满足下列任一条件才接入 VirtualizedCollection：

1. 稳定集合长度可能达到 20 项以上，且单项包含复杂子树。
2. 压力测试发现滚动长任务超过 50ms。
3. Tauri WebKit 实际复现透明空洞或未绘制区域。
4. DOM 审计发现该滚动区域长期挂载超过 500 个元素节点。

仅存在普通短列表时：

- 可以接入 RenderSafeScrollSurface。
- 可以使用 `content-visibility`。
- 不应为了架构统一强行启用 Virtualizer。

### 6.1 Candidate order

验证后按以下顺序评估，而不是直接实施：

1. LogViewer 长日志内容。
2. Memory Recall/Library 长结果。
3. Manual/Markdown 长文档。
4. Sources、Groups、Mounts 的长列表模式。
5. 其他通过指标触发的页面。

每个新接入点必须有独立回归测试和稳定 item key。

## 7. Feature flag rollout

发布阶段：

### Stage A：开发默认开启

- 开发和测试默认开启。
- 完成全部压力和回归测试。

### Stage B：产品默认开启、保留内部回滚

- 产品默认开启 deferred rendering 和 Turn virtualization。
- 不暴露用户设置 UI。
- 若发生严重回归，通过内部常量或小版本补丁关闭。

### Stage C：删除临时 flags

- 连续两个 release cycle 没有状态丢失、导航失败或 WebKit 回归后删除 flags。
- 删除 flag 时保留统一渲染路径，不恢复旧实现。

禁止让 feature flags 长期形成两套均需维护的架构。

## 8. Cleanup requirements

完成 Conversations 验证后审计：

```bash
rg -n "addEventListener\(.*scroll" frontend/src --glob '*.{ts,tsx}'
rg -n "requestAnimationFrame|setTimeout" frontend/src/components/common/rendering
rg -n "aurora-skeleton" frontend/src --glob '*.tsx'
rg -n "conversation-loading|conversation-preview-loading" frontend/src
rg -n "backdrop-filter" frontend/src/styles/index.css
```

审计要求：

- 每个 RenderSafeScrollSurface 只有 Controller 的一个 scroll listener。
- 每个 Provider 只有一个 Scheduler RAF 循环。
- 业务组件不直接输出 `.aurora-skeleton`。
- Conversations 没有第二套 Skeleton animation。
- 预览纵向 scroll element 没有 backdrop-filter。
- 非 Skeleton 的业务 timer、状态 pulse 不误删。

## 9. Full regression commands

```bash
pnpm typecheck
pnpm test
pnpm build
pnpm artifacts:check
```

定向测试：

```bash
pnpm vitest run --config frontend/vite.config.ts \
  frontend/src/components/common/rendering \
  frontend/src/components/conversations \
  frontend/src/pages/conversations/ConversationsPage.test.tsx \
  frontend/src/pages/conversations/ConversationsPage.sync.test.tsx \
  frontend/src/router/RouteTransition.test.tsx
```

依赖审计：

```bash
pnpm why @tanstack/react-virtual
pnpm licenses list --prod
```

## 10. Documentation completion

实现完成后：

- 更新本目录各 Task checkbox。
- 在 GitHub Issues（已取代文件版任务总册） 只根据实际通过的实现和 Git 历史更新状态。
- 如果常量、API 或 rollout 决策改变，先更新本 SPEC。
- PR 描述必须引用本目录总 SPEC 和已完成 Task。
- 不在产品 README 复制整套内部架构。

## 11. Final acceptance checklist

- [ ] 总 SPEC 的 14 项 Success Criteria 全部满足。
- [ ] 定量指标全部满足。
- [ ] Tauri WebKit 两种主题、六种输入路径通过。
- [ ] Translation、展开、搜索、焦点无状态回归。
- [ ] Layer 0 在 flags 关闭时仍有效。
- [ ] 没有页面按内容类型实现独立降级。
- [ ] 没有重复 scroll listener、Scheduler 或 Skeleton animation。
- [ ] 完整测试和构建通过。
- [ ] Bundle 增量和依赖许可证已记录。
- [ ] 推广候选只完成评估，不在同一 PR 顺手迁移。

## 12. Commit

```text
test: 完成会话滚动渲染稳定性验收
```
