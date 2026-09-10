# Luna / Flash 单卡执行手册

## 1. 工作单位

一次模型上下文只完成一张 `tickets/TNN-*.md`。执行卡定义 Outcome、blocker、Contract、Seam、Red Test、实现步骤和 Gate。卡外工作不顺手处理，发现后记录 handoff。

## 2. 固定循环

### 2.1 Locate

1. 读取 `00-execution-router.md` 的当前分支；
2. 读取父/子 Issue 最新正文与评论；
3. 运行 `git status --short`、`git branch --show-current`、`git log -1 --oneline`；
4. 用 `rg` 重新定位执行卡列出的符号；
5. 读取相关测试和调用点；
6. 确认 blocker 已完成。

完成标准：列出本卡实际生产入口、测试入口、调用者与不触碰文件。

### 2.2 Baseline

运行当前卡 targeted tests。已有失败先判断是否与本卡相关：

- 相关：记录并以最小 reproduction 收敛；
- 无关：记录为 baseline，不通过扩大范围修复；
- 来自其他未提交任务：不覆盖，协调后再执行。

完成标准：拥有可重复命令和明确 baseline 状态。

### 2.3 Red

先添加最少但足以证明用户行为缺失的测试。Red 必须因为目标能力缺失而失败，而不是因为 fixture、编译或 mock 配置错误。

完成标准：失败输出能直接对应当前卡 Acceptance Criterion。

### 2.4 Minimal

沿 tracer path 完成最小实现：Provider/runtime → AppService/transport → service/store → UI（当前卡需要的层）。不先写一整层再等待后续接线。

完成标准：Red 测试变 Green，且现有 targeted regression 仍 Green。

### 2.5 Converge

- 清除重复 adapter/helper；
- 收口命名与职责；
- 完成 loading/error/unavailable；
- 加入 bounds、schema 和 i18n；
- 只做服务于本卡 Outcome 的重构。

完成标准：没有临时代码、重复 Authority 或未完成标记。

### 2.6 Verify

运行执行卡 Gate 与命令，保存证据。UI 卡必须运行真实浏览器/Tauri 视图；纯 DOM 测试不能代替视觉验收。

完成标准：每条 Acceptance Criterion 都有测试或人工证据映射。

### 2.7 Review

检查：

- AppService/Service/Theme 边界；
- Session/Task/Memory Authority；
- logs/tracing 正文泄漏；
- Provider 缺失事实是否被伪造；
- AionUi 行为差异；
- 其他任务文件是否被改动；
- dead code、raw colors、direct invoke、手改生成文件。

完成标准：无 P0/P1，所有差异有规格依据。

### 2.8 Commit

只 stage 当前卡文件；确认 staged diff 不含其他任务。中文 Conventional Commit，一卡一个可回滚提交。

完成标准：commit hash + clean-for-this-card diff；共享工作树仍可能包含他人未提交文件，但未被本卡 stage。

### 2.9 Handoff

使用 `09-handoff-template.md`。只标记当前卡状态，列出下一 frontier，不自动开始。

## 3. AionUi 复刻协议

每个 UI 卡执行：

1. 打开 `03-codebase-seams.md` 指定的 AionUi 文件；
2. 记录其信息层次、状态、默认展开、滚动与响应式行为；
3. 找到 AssetIWeave 对应 Foundation/Common/service；
4. 先写行为测试；
5. 用 semantic token 重建；
6. 与固定 commit 的真实页面并排比较；
7. 记录有意差异。

“复刻”不等于 import 源组件。AionUi 的 UI library、IPC、cache、DB、router 不进入依赖图。

## 4. Drift Protocol

发现代码与文档漂移时：

1. 以代码/测试确定已实现事实；
2. 以最新 Issue 确定产品要求；
3. 分类：命名漂移、实现先行、规格冲突、其他任务未完成；
4. 命名漂移可在卡内按职责适配；
5. 实现先行则补 Red/Green 证据，不重复实现；
6. 规格冲突执行 Stop Protocol；
7. 其他任务未完成则保持 blocked，不在本卡补做其工作。

## 5. #33 共存协议

- #33 基线实现提交为 `dcb0fcbf`；执行 T09 时以当前 HEAD 重新定位 Task View/Memory Stage 接缝；
- 后续出现的 #33 未提交工作树内容视为他人所有；
- 不运行 reset/checkout/stash/clean 覆盖；
- 不手改其生成 contract；
- T09 的外部接缝前置条件已满足，只等待 T03；若当前代码发生新漂移，按 Stop Protocol 处理；
- 交叉修改基于最新 diff，采用 additive typed ref；
- 合并后同时运行 #31/#33 tests；
- Task View 继续不携带 Session items。

## 6. 代码质量约束

- React component 目标不超过约 200 行；超过即按单一职责拆分。
- Reducer/normalizer 为纯函数并有 table tests。
- 事件 identity、state monotonicity、bounds 在后端 Authority 实现，前端只做幂等合并。
- UI local state 与 server state 分离。
- 所有用户文案 i18n。
- 颜色/边框/阴影使用语义 token。
- Provider 内容经过 schema、bounds 与受控 renderer。
- Debug/tracing 对正文 redacted。

## 7. Stop Protocol

出现任一条件立即停止生产改动并提交问题说明：

- 需要新增 Agent transcript 数据库或修改 Memory Job 持久化模型；
- 需要建立第二个 Session registry/executor/TaskRuntime；
- 需要让页面直调 Tauri；
- 需要把完整 Session items 放入 Task View 或 task-updated event；
- 需要引入 AionUi runtime dependency；
- Provider 无事实却要求展示“完整”内容；
- 需要改变 Team role/review/ownership 或 Memory retry/publish；
- 需要覆盖其他任务未提交文件；
- 生成契约只能靠手改才能通过；
- 当前卡大于一个上下文窗口且无法保持 Green。

报告格式：触发条件、事实证据、影响 Contract IDs、两个可选解法、推荐解法、等待的决策。

## 8. Flash 防偏航检查

每完成一个关键步骤回答：

1. 当前改动是否直接服务本卡 Outcome？
2. 用户现在能看到一个新增的端到端行为吗？
3. 是否创建了第二套类型、store、renderer 或 Authority？
4. 是否把 AionUi 技术依赖误当成视觉需求？
5. 是否用推测数据填补 Provider 缺失？
6. 是否有 Red 与 Green 证据？
7. 是否触碰了 blocker/其他任务所有的文件？

任一答案异常，先收敛再继续。

## 9. 完成声明规则

“完成”只能在命令实际运行并看到退出码后声明。使用：

- `Implemented`：代码存在；
- `Verified`：指定 Gate 通过；
- `Blocked`：有明确 blocker/Stop 条件；
- `Not run`：命令未运行；
- `Pre-existing failure`：有 baseline 证据。

不得用“应该通过”“看起来完成”替代验证。
