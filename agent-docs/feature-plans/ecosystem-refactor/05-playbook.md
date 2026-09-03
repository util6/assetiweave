# Flash / Luna：单卡执行规程

## 每轮只完成一个可审查结果

1. **Locate**：按入口读取 Issue、当前卡和 Contract IDs；用 `rg` 确认 Modify/Test 路径和符号。卡内省略目录的文件以该段最近明确目录解析；有歧义先查现有 import，不猜路径。
2. **Baseline**：`git status --short`、`git diff --stat`，标出用户既有改动与本卡改动。隔离 worktree 必须包含确认的业务基线；不对脏目录 reset/clean/stash，不用仅 HEAD 副本丢失现有成果。
3. **Characterize / Red**：先把需要保持的现有行为测试跑绿；库接管/新增接口/新增回归测试应在旧实现上明确失败。区分两类证据，不能为制造 red 破坏正确实现。
4. **Migrate**：安装当前 owner 卡的依赖，先用锁定 API 做最小实现，再逐调用方迁移。业务转换留在领域 hook/service；不在库外重造同职责框架。
5. **Converge**：消费者切完当卡删除旧机制。确需跨卡薄兼容导出时在交接写明调用方和具体删除卡，维护期不得延伸到 A-G01 之后。
6. **Verify**：逐条跑当前卡命令及矩阵 gate。`rg` 仅证明引用变化，不替代行为测试；过滤后 0 tests 不是通过。
7. **Review**：检查业务保真、authority、真实生产接入、删除项、锁文件/API 兼容和范围。阻断项清零才报完成；任务结果和执行状态分开记录。
8. **Handoff**：用模板写 Issue 评论，关联精确 revision/patch。若本轮明确包含提交，只 add 当前卡文件，中文 Conventional Commit；本包不授权夹带提交原有用户改动。

## 何时暂停当前卡

依赖实际不兼容、必需接口缺失或签名冲突、前置卡未验收、无法区分重叠用户改动、出现没有测试定义的外部破坏性契约变化时，记录具体证据与需要修订的决策。模型只给出最小修订建议；不静默替换库、加新平台、改全仓 schema 或跳到插件架构。基线中不相关失败单独登记；若影响本卡验证，先分离原因后继续。

## 启动 Prompt

```text
执行 agent-docs/feature-plans/ecosystem-refactor/00-execution-router.md。
本轮只执行 tickets/A00-baseline.md，不开始其他卡。
保留当前工作区成果；按要求提交基线证据和下一张就绪卡。
```

后续调度消息直接指定 `03-ticket-map.md` 中唯一 ID；执行模型自行找到同 ID 文件，读取前置项验收评论。未指明 ID 时选择 map 顺序中首个 ready 卡；若多个共享核心文件，同样串行。

## 状态语义

- `PLANNED`：已写卡，尚未实施；不是已通过。
- `RUNNING`：只有一位 owner 执行该卡。
- `BLOCKED`：具体前置/决策阻断，有证据和恢复条件。
- `VERIFIED`：卡内步骤、行为证据、审查与交接齐全。
- `G-FINAL`：整个任务的独立结果；不能由任意单卡 VERIFIED 推断。

状态保存在 Issue 的最新任务台账与交接评论；仓库卡默认 PLANNED 是编制快照，不在两处维护相互竞争的实时进度表。
