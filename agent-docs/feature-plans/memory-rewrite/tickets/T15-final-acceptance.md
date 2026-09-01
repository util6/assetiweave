# T15：完成 Memory 全链路验收

## Outcome

用确定性 fixtures 和桌面 smoke 证明 Conversation commit → durable Job → Session/Project/Global Memory → Context/Recent/Recall → exact navigation 的完整路径，以及 restart、权限、迁移和 UI 响应性；形成关闭父 Issue #20 的证据包。

## Blocked by

T14。所有实现与切换 Ticket 已完成。

## Read

- Contracts：01-contract.md 全部。
- Seams：S01–S17、TS01–TS07。
- Gates：G0–G8；行为 V01–V24。

## Authority changed

None。T15 只补验收测试、修复验收发现的同范围缺陷并生成证据；不新增产品能力。

## Red test first

先建立一个覆盖全链路的 acceptance test，确认它至少因一个尚未由单元测试串联的公开行为而 Red；若现有测试已完整覆盖，则先收集矩阵证据，不制造虚假 Red。

## Execution steps

1. 建立 V01–V24 证据清单并定位现有测试。完成标准：每项有测试名/命令；缺口明确，不用低层 mock 次数代替。
2. 补主链 AppService/Engine acceptance。完成标准：临时 DB、可控时钟、Fake Agent/Fake ACP 完整跑通并在重开数据库后验证。
3. 补 failure matrix。完成标准：重复、lease、retry、cancel、restart、删除、redaction、tenant、非法 locator、Agent exit 都有公开结果。
4. 运行 G0–G8。完成标准：每条命令、退出码、测试数和已知环境限制记录。
5. 执行桌面人工验收。完成标准：按 G8 后的六步记录截图/结果，验证响应性与视觉信息边界。
6. 对 `BASE..HEAD` 运行独立 Checkpoint Review。完成标准：P0/P1 清零，P2 有明确接受/后续 Issue。
7. 形成父 Issue #20 关闭评论。完成标准：链接 15 个子工单、提交、Gate、行为矩阵、迁移/归档和剩余非目标。

## Acceptance

- [ ] V01–V24 全部有可复现证据。
- [ ] G0–G8 通过或有精确、非产品缺陷的环境阻塞记录。
- [ ] 主链在数据库/应用重启后保持正确。
- [ ] Tenant、只读工具、redaction 和日志最小化通过。
- [ ] 两个页面和 exact navigation 经桌面验证。
- [ ] 旧公开表面与新路径依赖审计通过。
- [ ] 独立 review 无 P0/P1。
- [ ] 父 Issue 关闭证据完整。

## Non-goals

新增未在 Issue #20 的功能、性能大重构、通用多 Agent、自动注入外部原生 Session、旧数据迁入新模型。

## Ticket-specific stop

验收发现架构级缺陷时创建/回开负责它的最早 Ticket 并停止 T15；不在最终卡中堆叠跨层补丁。
