# B02：把采纳的设计冻结为 Flash/Luna 代码施工卡

> **Status: BLOCKED_BY_DECISION**。本卡编写并审查代码执行计划，不修改生产实现。

**Goal:** 将 B01 已采纳接口与任务一实际基线转换成无猜测空间的小卡。
**Depends:** B01 的明确采纳记录。
**Read:** P-IMPLEMENT、`02-work-packages.md`、`04-verification-matrix.md`；使用 `superpowers:writing-plans` 与 `mattpocock-skills:writing-for-agents`。
**Modify/Create:** 本目录 `03-ticket-map.md` 和 `tickets/` 中新增的代码卡；真实决策按 ADR 治理处理。计划状态写 Issue #23。

## 步骤

- [ ] 从 B01 决定提取唯一选型、锁版本、Manifest/能力/version/error/cancel/lease schema；与 accepted code 中字段逐一对应。没有明确决策则返回 B01，不让执行卡写“自行选择”。
- [ ] W1–W6 分成各自可测试、可独立审查的小卡；每卡一般一个生产链路，不以“重构整个 Scanner”作为一个操作。共享 manifest/registry/安装器/Engine 文件默认串行，画出真实依赖。
- [ ] 每卡写齐：Goal/Contracts/Depends/Modify/Create/Test/Consumes/Produces、关键库 API 代码、真实失败回归或 green characterization、接管生产调用方、旧代码删除、命令与预期输出、schema/包兼容影响、停止条件。
- [ ] 数据/文件变化需要时单列迁移卡，定义样本 schema 与字段映射、事务/停写切换/回退，先跑副本。没有数据变化的卡明确不改 migration。
- [ ] 为所有新接口建立 producer/consumer 对照；引用的新文件必须标 Create；引用已删路径改为当前实际位置。用脚本核查路径与卡依赖无环，手工复核关键 API 对所选版本确实存在。
- [ ] 按验收矩阵逐格关联具体测试文件/用例；“独立插件无需主机重编译”“禁用/卸载活动调用”“旧数据与默认symlink保真”都必须有实际生产入口测试。
- [ ] 根 Agent 自检规格覆盖、占位词、签名一致性、依赖环、生产接入/删除证据；通过审查的首张代码卡在 Issue 标 READY_FOR_IMPLEMENTATION，其他保持依赖阻断。

## 完成条件

Flash/Luna 可以只读入口、当前卡和指定合同便实现一个完整可测结果，不需要重新选择库、运行时、插件协议或迁移策略。将首张 ready 卡的准确路径交给执行模型；不是把 W1–W6 表整块交付让模型自由发挥。
