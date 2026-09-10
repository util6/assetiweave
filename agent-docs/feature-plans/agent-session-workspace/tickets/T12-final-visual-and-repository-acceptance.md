# T12：完成 AionUi 视觉、响应式、无障碍与全仓验收

## Outcome

共享聊天、Team 和 Memory observer 在固定 AionUi 基准下完成视觉与交互收口，所有 Contract/Gate、响应式、无障碍、性能和全仓验证具备可审计证据。

## Blocked by

T05、T06、T08、T10、T11。

## Scope

- Requirements：全部
- Contracts：全部
- Seams：全部
- Gates：G-12（包含 G-00–G-11）

## Preflight

1. 确认所有 blocker verified。
2. 读取所有 handoff、known limits 与 intentional differences。
3. 运行全量 baseline，任何失败先归因。
4. 固定 AionUi commit 与 AssetIWeave HEAD。

## Acceptance work

1. 逐项执行 `04-ui-interaction-spec.md` 的 20 个视觉场景。
2. 对照 AionUi 层次、密度、状态、默认展开、滚动、Composer 和 Team lanes。
3. 修复只属于本规格的视觉/交互差异；不扩大到 Explorer/SCM 等非目标。
4. 执行 320/768/1024/1440、dark/light、reduced-motion。
5. 执行完整 keyboard 与 screen-reader/ARIA 检查。
6. 执行 large fixture/render-count/lag recovery 性能检查。
7. 执行 logs/tracing/task event 正文泄漏审计。
8. 执行 architecture guard 与 Legacy `rg` 审计。
9. 运行全仓命令。
10. 进行 code review，修复所有 P0/P1 和范围内 P2。

## Acceptance criteria

- [ ] Team 与 AionUi 的核心 hierarchy/interaction 等价。
- [ ] interactive chat surface 真实由 Team 使用。
- [ ] 四种 Memory scope observer 可用且只读。
- [ ] 完整 request/thinking/tools/output/diff/terminal/error 可见。
- [ ] Task View 不内联 transcript，Agent terminal 与 Memory publish 可区分。
- [ ] replay/live/bounds/unavailable/tenant scope 正确。
- [ ] 20 个视觉场景有证据。
- [ ] 四个宽度、两主题、reduced-motion 通过。
- [ ] keyboard/ARIA/focus/contrast 通过。
- [ ] 无旧 renderer/sanitizer/第二 Authority/Aion dependency/direct invoke/raw colors。
- [ ] 所有自动化命令通过。
- [ ] review 无未解决 P0/P1。

## Verification

执行以下 required commands，并记录命令、退出码与失败归因：

```bash
pnpm typecheck
pnpm test
pnpm build
cargo fmt --all -- --check
cargo test --workspace
```

公开 Engine/CLI 变化时追加：

```bash
pnpm cli:contract
go vet -C cli ./...
go test -C cli -race ./...
pnpm cli:test:e2e
```

## Deliverables

- 测试命令与退出码；
- AionUi/AssetIWeave before-after screenshots；
- 20 场景矩阵；
- accessibility/performance/architecture 审计；
- intentional differences；
- final diff review；
- 中文 Conventional Commit hash；
- Parent Issue #31 的完成评论草稿。

## Non-goals

- 新增本规格外功能；
- 借最终验收顺手重构其他页面；
- 在失败命令未解决时宣称完成。

## Handoff

全部 G-12 通过后，更新 `10-progress.md` 为 verified，并向 #31 提交完成证据。Parent 是否关闭由 Issue Authority 决定。
