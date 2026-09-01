# T01：建立 Recent source slice 与项目目录解析

## Outcome

通过 AppService 查询滚动 72 小时内最后活跃的 canonical Conversation Session，并按规范化项目目录返回同一数据集的项目/时间排序结果。不同宿主只表现为 source agent 元数据。

## Blocked by

None。父 Issue #20 与 Conversation 当前合同是输入。

## Read

- Contracts：C-D01、C-D03、C-D04、C-A01、C-A05、C-S03。
- Seams：S01、S02、S05；Tests TS01。
- Gates：G0、G1、G2、G7。

## Authority changed

新增项目目录解析和 Recent read model 的应用语义；Conversation 仍是来源事实，本卡不持久化 AI Recent Event。

## Red test first

使用可控时钟与多宿主 Conversation fixtures 调用 AppService：五天前创建但昨天活跃的 Session 被返回；昨天导入但十天前结束的不返回；同 Git 根子目录和 symlink 合并，不同 worktree 分离。

## Execution steps

1. 定位 canonical Session 中 last activity、cwd、登记项目和 source agent 的当前来源。完成标准：每个输入字段有唯一生产来源，缺失字段行为写入测试。
2. 实现纯项目目录 resolver。完成标准：登记根优先、其次 worktree、最后规范化 cwd；symlink/case fixtures 稳定，不按 remote 合并。
3. 建立 AppService Recent list read workflow 与 DTO。完成标准：72h 由注入时钟计算，项目/时间排序只改变投影顺序，不复制查询规则。
4. 补齐 tenant、无 cwd、非 Git 和空结果分支。完成标准：跨 tenant 不可见，无目录 Session 使用规范定义的可显示 fallback group。

## Acceptance

- [ ] 滚动 72 小时只使用 Session last activity。
- [ ] project resolution 顺序与 C-D03 一致。
- [ ] 同一 worktree 子目录聚合，不同 worktree 分离。
- [ ] Codex/AntiGravity 等 provider 不出现在业务分支。
- [ ] 项目/时间模式包含相同 Session 身份集合。
- [ ] AppService 测试重开数据库后仍得到相同归属。

## Non-goals

AI Session Memory、Recent Event 六分类、Outbox、Job、页面、Engine/CLI 和旧 Memory 删除。

## Ticket-specific stop

若 canonical Conversation 不提供可证明的 last activity 或 cwd 来源，按 Stop protocol 报告缺失合同；不从 provider 私有日志补读。
