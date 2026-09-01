# T04：按项目目录合并 Project Memory

## Outcome

同一规范化项目目录下来自不同 Agent 的成功 Session Memory 被串行合并为版本化 Project Memory 与原子发布的 last-success 项目 `MEMORY.md`；不同项目可并行。

## Blocked by

T02。T03 可并行开发，但集成 Checkpoint 需要两者都完成。

## Read

- Contracts：C-D02、C-D03、C-A01–C-A05、C-P04、C-P05、C-P07、C-P09、C-M02、C-M04、C-S02、C-S03。
- Seams：S01、S04–S06、S16；Tests TS01、TS02。
- Gates：G0、G1、G2、G6、G7。

## Authority changed

新增 Project Memory 版本、项目 consolidation Job/input fingerprint 和项目文档 last-success 投影。

## Red test first

两个不同 source agent、同项目目录的 Session Memory 入队时只启动一个项目 consolidation；Fake Agent barrier 证明同项目不重叠、另一项目可并行。第二次生成失败后 read API 与文件仍返回第一次成功版本。

## Execution steps

1. 用追加 migration 建立 Project Memory/version/source membership 与 scope fingerprint。完成标准：tenant+normalized project scope 唯一，输入集合有稳定排序。
2. 实现 Session success → Project dirty watermark → consolidation Job。完成标准：多个 Session 更新合并水位，不创建无界任务。
3. 实现 per-project 串行锁与跨项目有界并发。完成标准：AppService/Fake Agent 测试能观察到并发边界。
4. 校验结构化 Project Memory 并提交 last-success。完成标准：只消费成功且未失效 Session Memory，失败版本不覆盖 last-success。
5. 将项目 `MEMORY.md` 写入临时 app-owned workspace 后原子替换。完成标准：来源项目目录零写入，文件可由 SQLite 重建。

## Acceptance

- [ ] 多宿主 Session 按项目目录而非 provider 聚合。
- [ ] 同项目 consolidation 串行，不同项目可并行。
- [ ] 输入 fingerprint/watermark 幂等且可推进。
- [ ] 失败或不完整版本不替换 last-success。
- [ ] Project Memory 与文档可追溯到 Session/source locator。
- [ ] 文档只位于应用自有 workspace，使用原子发布。

## Non-goals

Global Memory、Context Resolver、完整失效级联、UI、Recall。

## Ticket-specific stop

如果项目锁需要全局串行所有项目，或 Project Memory 只能从原始 provider 日志生成，停止并报告。
