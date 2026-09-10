# T11：收缩旧 Team Session 呈现合同

## Outcome

Team 与 Memory 全部使用共享 Agent Session contract/store/renderers；旧 Team-only message renderer、tool sanitizer、重复 merge/scroll 逻辑和过渡 API 被安全删除。

## Blocked by

T08、T10。

## Scope

- Requirements：A-009、R-NFR-001/002/004
- Contracts：全部共享合同
- Seams：`03-codebase-seams.md` Legacy 收缩清单
- Gates：G-01–G-11

## Preflight

1. 确认 T08 与 T10 均已 verified，并读取全部 T01–T10 handoff。
2. 从 `03-codebase-seams.md` 的 Legacy 清单逐项重新执行 symbol/call-site 搜索。
3. 记录全仓 TypeScript/Rust tests baseline，并为每个待删入口确认新 Authority 的 Green 证据。
4. 先提交 guard tests，再删除重复实现；本卡不引入新的用户可见能力。

## Red/guard tests

收缩前先建立 guard：

1. Team 页面只通过 shared Agent Session module 渲染 items。
2. Memory observer 只通过相同 module。
3. repo search 不存在 tool detail 清空逻辑。
4. repo search 不存在旧 Team-only item renderer/auto-scroll 调用点。
5. 删除旧 DTO/API 后 TypeScript/Rust 编译与测试保持 Green。
6. Team/Memory canonical fixture 输出不变。

## Implementation steps

1. 列出所有旧 symbols 与调用点，分类为 migrate/delete/keep-domain。
2. 对仍使用旧 Team stream address 的调用点迁移到 public ref/service。
3. 删除 `sanitizeSessionItem` 的 tool-clear 行为与重复 merger。
4. 删除 TeamWorkspaceShell 中已迁移 renderer、step、scroll、composer 实现。
5. 合并重复 DTO/schema，保留 Team domain-only fields。
6. 删除兼容 commands/services 仅在无调用点且 contract regeneration 通过后进行。
7. 执行 dead code、direct invoke、raw color、second authority 审计。
8. 运行完整 #19/#20/#21/#31/#33 targeted regressions。

## Acceptance criteria

- [ ] 一个 Session item union、一个 merge state machine、一个 renderer family。
- [ ] Team-specific 文件只剩 Team domain composition。
- [ ] Task Center 不拥有 transcript store/renderer。
- [ ] 无 tool detail sanitizer/丢弃旧行为。
- [ ] 无 unused compatibility API。
- [ ] 生成契约与 TS/Rust schema 一致。
- [ ] 所有相关 Gate 通过。

## Verification

- `rg` legacy symbol/call-site audit；
- TypeScript/Rust compile；
- shared/team/memory/task tests；
- schema/contract generation；
- architecture review；
- G-01–G-11。

## Non-goals

- 与 Session 呈现无关的 Team/Memory refactor；
- 视觉 polish（T12）；
- 改变 domain API 语义。

## Handoff

T05、T06、T08、T10 与本卡完成后 T12 进入 frontier。附最终 Legacy 删除清单。
