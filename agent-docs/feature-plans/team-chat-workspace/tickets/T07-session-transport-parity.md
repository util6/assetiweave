# T07：暴露 member Session transport 与 Engine/CLI parity

## Outcome

Desktop、Engine 和 Go CLI 通过同一 AppService 能启动 member turn/replay、读取 stream snapshot/任务状态并取消；Tauri event 丢失时可用 snapshot/polling 恢复。

## Blocked by

T06。

## Read

- Contracts：TW-E02、TW-E06、TW-B01～TW-B05。
- Seams：S08～S12；Tests TS05、TS07、TS08。
- Gates：G0、G2、G3、G4。

## Red test first

从 Engine registry 调用 member turn start，fake Provider 尚未完成时取得 task/stream identity；随后 snapshot 查询包含增量 item，重复 Tauri event 与 polling snapshot 的 sequence 可被去重。旧 contract 无这些方法。

## Execution steps

1. 定义稳定公开操作：member turn start、member replay start、session stream snapshot、member task get/list/cancel。完成标准：DTO 不暴露 Provider Anchor、workspace、prompt 回显或 raw tool payload。
2. 添加薄 Tauri command 和 scoped event emission。完成标准：事件 envelope 含合并 identity/sequence，listener 丢失后 snapshot 可恢复。
3. 注册对应 Engine methods、risk/exposure 和 surface mapping。完成标准：所有 mutation/query 调用同一 AppService，无 transport 业务逻辑。
4. 生成 CLI contract 并扩展 Go Team 命令。完成标准：CLI 只调用 Engine，支持明确 team/member 输入和机器可读输出。
5. 增加 transport parity tests。完成标准：Desktop/Engine DTO、错误码和任务终态一致。

## Acceptance

- [ ] start/replay/snapshot/status/cancel 在 Tauri 与 Engine 上具有一致语义。
- [ ] Tauri event 是低延迟路径，snapshot/polling 能恢复漏事件且 sequence 可去重。
- [ ] Go CLI 通过 Engine 完成同一状态变化和读取，无 SQLite/Provider store 直连。
- [ ] generated contract 与 surface matrix 由命令更新且稳定。
- [ ] 公开 DTO、错误、event 和 task snapshot 不泄漏正文或 Resume Anchor。

## Non-goals

frontend store、member avatar、message renderer、Leader task card。

## Ticket-specific stop

如果需要在 Tauri/Engine 重复业务规则、手工编辑 generated contract，或 polling 只能读取持久正文，按 Stop protocol 报告。
