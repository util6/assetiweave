# T01：建立 Team 与固定成员编排

## Outcome

用户可以在独立 Team 板块创建、查看和编辑一个 Team，指定唯一 Leader、一个或多个有序 Teammate，以及每个成员的 Agent/模型。

## Blocked by

None。以 Issue #19 和当前 Agent Market catalog 为输入。

## Read

- Contracts：C-D01、C-D02、C-D04、C-L02、C-A01、C-S01。
- Seams：S01、S09、S10、S11、S12、S14、S15；Tests TS01、TS04、TS05。
- Gates：G0、G1、G2、G3、G4。

## Red test first

通过 AppService + 临时 SQLite 创建包含两个 Leader 或零 Teammate 的 Team 必须失败；合法 Team 重开数据库后保持成员顺序、Agent/模型和互不相同的 `execution_context_key`。

## Execution steps

1. 建立 tenant-scoped Team/TeamMember 事实、校验和 repository。完成标准：数据库重开后读取结果稳定，非法角色组合无法提交。
2. 暴露薄 AppService、Tauri、Engine 和 Go CLI create/list/get/update。完成标准：所有入口返回同一 DTO/错误，CLI 未直接访问 SQLite。
3. 接入 Team route、service、roster 编辑和空状态。完成标准：UI 只通过 service 操作，重复 Agent/模型成员可保存且顺序可见。

## Acceptance

- [ ] 一个 Team 恰有一个 Leader、至少一个 Teammate。
- [ ] 同 Agent/模型的两个成员拥有不同稳定 context key。
- [ ] 编辑后的 roster 顺序在应用/数据库重开后保持。
- [ ] Team CRUD 不改变任何 Conversation 表。
- [ ] Desktop、Engine 和 CLI 对合法/非法输入一致。

## Non-goals

TeamRun、聊天、Persistent Session、草稿、执行、mailbox、MCP 和后台任务。

## Ticket-specific stop

如果必须修改 Conversation schema、把 roster 放入 settings JSON，或为 Team 创建前端直连 invoke，按 Stop protocol 报告。
