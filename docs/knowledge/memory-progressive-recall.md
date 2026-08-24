# Memory 与渐进式 Recall（回忆）操作

Memory 是构建在规范化会话卡片（Conversation Card）之上的独立本地优先（local-first）领域。SQLite 是系统的唯一事实来源。Dream 笔记、Recall 提取、问答结果以及正式 Memory 均保留证据快照；不会修改任何源文件仓库。

## 运行时与隐私

- 自动 Dream（Auto-Dream）默认处于禁用状态。启用它或选择 AI 合成可能会启动已配置的 OpenCode 或 Gemini CLI，并可能通过该运行时的网络链路发送脱敏文本。
- 预览（Preview）、概览（Overview）、知识库（Library）、仅查看证据的 Recall 以及新鲜度校验均在本地完成，不会调用外部 AI。
- AI 执行工作在应用自有的空临时目录中运行，具备超时控制、限制 stdout/stderr 输出上限、进程树级级联取消、确定性机密信息脱敏以及证据 ID 有效性校验。
- 会话内容属于不可信数据。Memory 管道绝不会将其作为 Shell 指令执行。

## 容量与调用限制

- **Dream**：单次最多处理 8 个 Session、40 个 Question 和 60,000 个输入字符；生成的笔记大小不超过 6 KiB。
- **Recall 第一阶段（Phase 1）**：每个批次最多处理 8 个 Question 和 30,000 个字符，并发度为 2，支持单批次自动重试一次。
- **Recall 预览**：明确报告检索后端、总计/选中/跳过数量、不可用源的包含情况以及截断信息。全量整理（Full Organize）支持分页且必须指定明确的作用域。
- **模型输出**：仅生成审查候选（review candidates）。用户必须显式接受或编辑候选内容后，它才会转变为正式 Memory。

## 故障恢复与一致性行为

- Dream 游标推进与笔记/证据持久化处于同一个事务中。发生失败、取消、无效输出或磁盘错误时，游标不会向前推进。
- 第一阶段的提取数据保留 30 天，以便在不改动正式 Memory 的前提下检查和重试第二阶段。候选条目的创建、版本修订、证据链接以及 Recall 完成状态在一个事务中最终提交。
- 应用启动时会将残留的处于排队中/运行中的 Memory 任务标记为已中断（interrupted）。桌面端任务注册表暴露事件更新与轮询回退机制，并接入应用退出时的二次确认警告。
- 新鲜度校验仅在会话版本发生变化后对选定条目执行。校验能够准确区分证据变更、证据缺失以及源不可用三种状态，同时保留原始证据快照摘录。

## 发布验证

在仓库根目录下运行以下命令进行验证：

```bash
cargo fmt --all -- --check
cargo test --workspace -- --test-threads=1
cargo test --workspace memory_recall_100k_scope_first_page_p95_is_below_350ms -- --ignored --test-threads=1
pnpm typecheck
pnpm test
pnpm build
pnpm cli:contract
go vet -C cli ./...
go test -C cli -race ./...
pnpm cli:test:e2e
```

桌面端手动验证检查项：

1. 打开全部四个 Memory 子页面，确认概览页面不会触发 AI 调用。
2. 在禁用自动 Dream 的情况下预览 Dream，然后使用测试夹具或已配置的运行时执行一次手动 Dream。
3. 构建精确的 Session 和 Web Recall 数据包，并为六类 Card 家族分别打开证据详情。
4. 启动一次 AI Recall，切换离开当前页面，观察全局进度条，执行取消操作，并验证持久化的运行记录已被标记为取消。
5. 模拟 Card 变更、Card 移除以及 Session 丢失的情况；验证三种新鲜度标签和快照回退展现。
6. 在 Memory 任务正在运行时尝试关闭应用，确认会弹出阻止退出的确认警告。

性能测试夹具会在临时数据库中生成 100,000 条合成 Question/Card 数据行，对限定范围的第一页查询进行预热，并严格执行 p95 低于 350ms 的门禁标准。深度偏移分页情况由产品遥测单独报告，不计入首命中延迟指标。
