# Task 01：基线复现与性能指标

## 1. Objective

在修改渲染路径前建立可重复的复杂内容压力样本、问题复现步骤和验收指标。该任务只增加测试/开发夹具和记录，不实施优化。

前置文档：

- `00-SPEC_骨架驱动的滚动渲染稳定性架构.md`
- `../SPEC_ 前端统一 Skeleton 架构.md`

## 2. Deliverables

1. 一个确定性的 Conversation 压力数据工厂。
2. 一个仅在测试或开发环境可用的渲染压力 Harness。
3. 基线测量清单。
4. 自动化结构指标测试。
5. Tauri WebKit 人工复现记录模板。

## 3. Stress fixture contract

新增建议位置：

```text
frontend/src/components/common/rendering/__fixtures__/conversationRenderingStressFixture.ts
frontend/src/components/common/rendering/__fixtures__/RenderingStressHarness.tsx
```

约束：

- Fixture 不进入正常产品数据流。
- Harness 只能在 `import.meta.env.DEV` 或测试构建中加载。
- 禁止增加生产导航项、持久化设置或后端数据。
- Fixture 使用稳定 ID，禁止 `Date.now()` 和随机数。

固定数据规模：

| Content | Count |
|---|---:|
| Turns | 80 |
| Markdown paragraphs per answer | 8 |
| Code blocks | 24 |
| Tool/command executions | 24 |
| Result blocks | 32 |
| Diff blocks | 12 |
| Tables | 8 |
| Mermaid blocks | 4 |
| KaTeX blocks | 8 |

Fixture 必须至少包含：

- 短 Turn。
- 常规 Turn。
- 超长 Turn。
- 连续多个代码和工具结果。
- 展开/收起状态可交互的 Result。
- 可启动 mock translation task 的 Card。

## 4. Manual reproduction protocol

在改造前后使用相同步骤：

1. 启动 `pnpm tauri:dev`。
2. 打开 Rendering Stress Harness 或等价 Conversation 压力记录。
3. 确认窗口尺寸为 1440×900，缩放 100%。
4. 从顶部使用触控板快速滚动到底部。
5. 拖动滚动条 thumb 从底部返回顶部。
6. 连续重复 10 轮。
7. 在浅色主题执行一次，在深色主题执行一次。
8. 展开至少两个 Result 后重复滚动。
9. 启动一个 translation task 后滚出并重新进入该 Turn。
10. 记录是否出现透明空洞、背景穿透、白闪、状态丢失和控制台异常。

## 5. Baseline metrics

必须记录：

| Metric | Collection method |
|---|---|
| 最大同时挂载 Turn 数 | DOM 查询 `[data-conversation-turn-id]` |
| 最大 Conversation Card DOM 数 | DOM 查询 `[data-conversation-card-id]` |
| 高速滚动最长主线程任务 | Web Inspector Performance trace |
| 10 轮滚动中的背景穿透次数 | 人工逐轮计数，必要时录屏逐帧检查 |
| 滚动后可交互恢复时间 | 从最后 scroll event 到按钮可点击 |
| Console errors/warnings | Web Inspector Console |
| Translation task continuity | 滚出/滚回后检查 task 和结果 |

本任务不设改造前必须达到的阈值；它记录事实。改造后使用总 SPEC 的 Success Criteria。

## 6. Automated diagnostics

允许增加开发期辅助函数：

```ts
export interface RenderingDiagnosticsSnapshot {
  mountedCardCount: number;
  mountedTurnCount: number;
  renderStateCounts: Record<"queued" | "ready" | "skeleton", number>;
  scrollPhase: "fast" | "idle" | "moving";
}
```

规则：

- 诊断必须从 DOM data attribute 或渲染 Controller 只读读取。
- 禁止把诊断写入 SQLite、localStorage 或正常日志文件。
- Production build 不显示诊断 UI。

## 7. Tests first

在实现优化前新增失败或基线测试：

- Stress fixture 恰好生成 80 个稳定 Turn ID。
- Fixture 包含规定的复杂内容种类。
- 现有未虚拟化页面在基线下挂载全部 80 个 Turn；该测试在 Task 06 改为验证 bounded mount。
- Harness 不在 production branch 中加载。

禁止用截图 snapshot 代替结构断言。

## 8. Files likely touched

```text
frontend/src/components/common/rendering/__fixtures__/conversationRenderingStressFixture.ts
frontend/src/components/common/rendering/__fixtures__/conversationRenderingStressFixture.test.ts
frontend/src/components/common/rendering/__fixtures__/RenderingStressHarness.tsx
frontend/src/pages/conversations/ConversationsPage.test.tsx
```

如果 Harness 接入现有开发入口需要修改超过一个额外文件，应拆成独立小提交。

## 9. Acceptance criteria

- [ ] 压力数据完全确定且不访问后端。
- [ ] Harness 仅在开发/测试环境存在。
- [ ] 10 轮滚动复现流程可由另一位开发者重复执行。
- [ ] 基线 DOM 数和视觉问题已有记录。
- [ ] Fixture 测试通过。
- [ ] 未修改产品渲染逻辑。

## 10. Verification

```bash
pnpm vitest run --config frontend/vite.config.ts \
  frontend/src/components/common/rendering/__fixtures__/conversationRenderingStressFixture.test.ts
pnpm typecheck
```

手工：

```bash
pnpm tauri:dev
```

## 11. Commit

```text
test: 增加复杂会话渲染压力夹具
```
