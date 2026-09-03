# 任务二：执行与交接

## 角色与入口

- B00：执行型模型可完成事实核对，不修改生产架构。
- B01：研究/设计任务，可由能查官方资料和运行实验的模型执行；Flash/Luna 也必须按证据门推进，不凭架构偏好选择宿主。
- 决策审查：维护者明确采纳后记录在 Issue #23；普通执行模型不替代这一取舍。
- B02：将选定方案编译为精确代码施工卡；自检签名/依赖/测试和当前代码位置。
- 后续代码卡：Flash/Luna 使用 executing-plans 一次一张，按 baseline→characterization/red→接入→删除→验证→审查→交接流程。

## 启动 Prompt

```text
执行 agent-docs/feature-plans/plugin-architecture-refactor/00-execution-router.md。
本轮只执行 tickets/B00-baseline.md。
先核对任务一 A-G01 验收；若前置不满足，记录缺失证据，不改插件架构。
```

不要把任务一的修改遗漏后称为“重新规划”。出现漂移时列文件/符号/影响，修订本任务卡；不恢复旧实现匹配旧文档。

## 每轮交接评论

在 Issue #23 写：Ticket ID/状态、起止 revision、读取的前置证据、实际文件/依赖/接口、命令与退出码/测试数/平台、删除清单、审查结论、下一张唯一 ready 卡或具体阻断。

状态分开：`WAITING_FOR_TASK_1`、`DESIGNING`、`WAITING_FOR_DECISION`、`READY_FOR_IMPLEMENTATION`、`VERIFIED`。其中 READY_FOR_IMPLEMENTATION 只属于经过 B02 审查的具体代码卡，不把整个工作包地图直接标 ready-for-agent。

只在本轮明确包含提交时提交自己的文件，中文 Conventional Commit。保持既有用户修改；不另建 legacy/new/v2 树，不把本轮产生的长期知识随手归档到其他代码目录。
