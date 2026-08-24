# 0007: 移除仅包含命令的 Execution 外壳

> 状态：已接受
> 决策日期：2026-08-11
> 决策证据：`8f0ee79`
> 记录日期：2026-08-11

## 背景

ADR 0006 引入了 `source_execution_id`，使 Core 能够确定性地对交错的 Command 和 Result Card 进行分组，而无需依赖相邻关系、文本或时间戳启发式规则。在会话载荷清洗优化后，成功的 Shell 输出和低价值的读取/搜索输出通常在持久化前已被过滤移除，而文件变更则作为独立的 Card 存在。因此，大多数仅包含命令的 Execution 节点只是增加了一个父级外壳，并暴露了一个不透明的源调用 ID，未能为用户提供更多有效信息。

源执行标识对于精确关联 Command/Result、传递结果状态以及保留了失败诊断或其他有意义 Result Card 的少数执行仍然具有重要价值。

## 决策

1. 在规范化和持久化的会话 Part 中保留可为空的 `source_execution_id`。
2. 当 Execution 包含有意义的 Result Card 时，保留基于 `(turn_id, source_execution_id)` 的精确分组。
3. Core 将没有 Result 子节点的 Command 投影为普通的 Card 节点，而不是 Execution 节点。
4. 前端在过滤掉隐藏和空的 Result 后，也将旧版或仅含状态的 Execution 节点展平，从而绝不渲染仅含命令的外壳。
5. 原始源调用 ID 仅作为内部关联数据，不在 Execution 头部展示。
6. 移除两个针对 Execution 投影的数据库索引。目前的查询路径是按 Question/Turn 加载 Part 并在内存中进行分组；没有任何 SQL 查询按 `source_execution_id` 进行过滤。
7. 保留仅有结果的 Execution 节点，因为缺失 Command 可能是源记录不完整所致，而 Result 依然具备参考价值。

## 后果

- 正常的成功命令直接渲染为带有状态和退出码的普通 Command Card。
- 包含诊断信息的执行依然将 Command 和 Result 组合渲染在一起。
- 文件变更保持为独立的 Card。
- 现有的 `source_execution_id` 数据和持久化的 Part 标识完全兼容；不需要重新解析会话或迁移 ID。
- 移除未使用的索引减少了数据库模式和写入开销，同时保留了关联字段。
- 包含仅含命令 Execution 节点的旧版 Engine 响应会被前端自动展平。

## 取代关系

本 ADR 仅取代 0006 中要求所有识别出的 Command 都必须出现在 Execution 展示外壳内的规定。0006 中关于保留源标识和精确 Command/Result 关联的决策依然有效。

## 参考资料

- `agent-docs/adr/0006-source-execution-grouping.md`
- 已淘汰的全局设计总册（以代码、测试与 ADR 为准）
