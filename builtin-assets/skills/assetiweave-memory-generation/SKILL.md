---
name: assetiweave-memory-generation
description: AssetIWeave 后台记忆提取与结构化沉淀生成模板
---

# AssetIWeave Memory Generation

本 Skill 作为后台生成策略输入，指导 Agent 从对话历史中提取高信号、可验证的记忆事实并结构化沉淀。

## 1. 角色与目标
形成高信号、可追溯的记忆事实。只提取经过验证或由用户明确陈述的有效信息，不总结冗余闲聊，不臆测无证据的结论。

## 2. 提取优先级
- 用户明确决定与指示 (decision)
- 架构、技术与业务约束 (constraint)
- 验证结论与测试结果 (verification)
- 明确失败原因与技术调查 (research)
- 持续存在的未解决阻塞 (blocker)
- 明确的待办事项与后续动作 (follow_up)

## 3. 忽略规则
- 忽略瞬时运行日志、网络抖动、临时环境噪音与心跳
- 忽略密码、Token、API Key、私钥等任何敏感秘密信息
- 忽略未经对话证据支持的推测和未确认假设

## 4. 状态判断
严格区分提案、执行中与已完成状态：
- active: 当前有效且需要持续关注的事项
- blocked: 存在未解除外部依赖或阻塞的事项
- waiting: 等待外部反馈、审查或依赖就绪的事项
- completed: 声明已完成但尚未经过运行验证的事项
- verified: 已经过运行验证、测试确认或生产校验的事项
- abandoned: 用户或团队明确废弃、放弃的事项
- superseded: 已被新决定或新约束取代的历史事项

## 5. 项目组织
严格按照 Work Order 提供的 project key 组织输出。未归属项目的条目使用 `unassigned` 分组。

## 6. 延续规则
跨窗口延续的事项必须引用 Work Order 提供的 prior Item ID（`continuesItemId`）。

## 7. 晋升提名规则
仅对具有长期复用价值的重大项目决定、关键架构约束、跨会话阻塞、重要调研结论或全局工作规则进行晋升提名（`promotionNomination`）。普通流水进展与临时进度严禁提名。

## 8. 工具读取步骤
优先消费 Work Order 首包中已下发的有界证据；仅在存在关键缺失信息时，按需调用有界读取工具补读证据片段。

## 9. 输出要求
严格只返回符合 `MemoryGenerationResultV2` JSON Schema 的纯 JSON 对象，严禁包含任何 Markdown 格式包裹、代码块反引号或解释文字。
