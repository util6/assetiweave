# E00：基线与合同冻结

## Outcome

确认主测试接缝（`AppService` + 临时 SQLite + 可控时钟 + Fake `AgentExecutor`）；发布 GitHub 子 [Issue #30](https://github.com/util6/assetiweave/issues/30) 并关联父 Issue #20；在 `00-execution-router.md`、`01-contract.md` 与 `03-ticket-map.md` 中同步显式修订 A & B 与契约；冻结预算参数策略；建立包含短会话、长会话、纠正、凭据混合等多场景的固定 fixture 集与 baseline 测量套件。

## Blocked by

None。父 [Issue #20](https://github.com/util6/assetiweave/issues/20) 与 [`08-bounded-evidence-execution-spec.md`](file:///Users/util6/code-space/assetiweave/agent-docs/feature-plans/memory-rewrite/08-bounded-evidence-execution-spec.md) 是输入。

## Read

- Contracts：C-D01、C-A01、C-A02、C-M07、C-M08、C-M09、C-S02、C-S04。
- Seams：`session_memory.rs`、`memory_redaction.rs`。
- Gates：G0、G1、G2、G7。

## Authority changed

固化两项显式修订（显式修订 A：取消逐 Session Markdown 依赖；显式修订 B：内置可编辑 Memory Recipe）；固化精简证据与受限补读执行合同；冻结首包与工具响应预算参数。

## Red test first

编写基线测试：在标准长会话与多节点 fixtures 下，测量现有旧版 `build_session_memory_prompt` 生成的字符数与未裁剪体积，断言当前无首包有界裁剪、缺乏短引用映射且无预算超限截断机制，为后续 E1–E8 提供可量化的对比依据。

## Execution steps

1. **发布与路由同步**：创建 GitHub 子 Issue #30，打上 `ready-for-agent`；在 Issue #20 追加拆分说明；同步 `00-execution-router.md`、`01-contract.md`、`03-ticket-map.md` 与 `08-bounded-evidence-execution-spec.md`。
2. **冻结预算与契约参数**：在 Rust 后端定义高密度归纳管线的预算常量（首包字符/节点预算、工具单次/累计响应上限、单项正文上限、补读次数上限）。
3. **构建固定 Fixture 集**：在测试模块构建涵盖正常对话、多问题长会话、用户中途否决纠正、Agent 声称通过但缺少验证、超长日志/代码、敏感凭据与 Git SHA 混杂的 fixture 数据。
4. **运行基线测量与验证**：记录当前全文 512 项证据的字符基线与潜在误伤点，确保基线测试可重复运行。

## Acceptance

- [x] GitHub 子 Issue #30 创建并打上 `ready-for-agent`，父 Issue #20 已评论同步。
- [x] 测试接缝确认为 `AppService` + 临时 SQLite + 可控时钟 + Fake `AgentExecutor`。
- [x] 显式修订 A & B 与 C-M07/C-M08/C-M09/C-S04 契约已写入并同步。
- [x] 预算策略参数在代码中明确定义并冻结（`BoundedMemoryBudgetPolicy`）。
- [x] 固定 fixture 集与 baseline 测试通过（`bounded_evidence_baseline_tests`）。

## Non-goals

E1 内部来源隔离过滤实现、E2 生产脱敏代码改写、E4 生产首包替换。
