# T12：切换 Engine、CLI 与内置 Skill 到新合同

## Outcome

Engine 与 Go CLI 可列出近期、解析上下文、读取项目 Memory、按 scope 重建、查询/取消/重试任务并执行 Recall；内置 Skill 使用相同只读/工作流合同，不再引用旧 Dream/candidate/Evidence API。

## Blocked by

T03、T05、T06、T10、T11。公共行为必须先稳定。

## Read

- Contracts：C-A01、C-A04、C-R03、C-S01、C-S02、C-X01–C-X03。
- Seams：S10–S12、S16、S17；Tests TS05。
- Gates：G0、G1、G2、G4、G5、G7。

## Authority changed

无新业务 Authority；新增/替换 adapter surface 与生成 contract。

## Red test first

Engine registry/surface 测试对新 canonical method 全部可达，CLI command test 断言请求/错误与 Rust DTO 一致；旧 `memory.dream.*`、旧 `memory.recall.preview/run` 不再出现在可调用表面。Skill 测试只调用新合同。

## Execution steps

1. 定义并注册新 Engine methods、DTO exposure、risk/confirmation。完成标准：所有方法调用 AppService，不复制规则。
2. 更新 Tauri/Engine surface mapping 并运行生成 contract。完成标准：生成物只由 `pnpm cli:contract` 产生，diff 经审查。
3. 重写 Go `memory` commands 与 help。完成标准：recent/context/project/rebuild/task/recall 可用，CLI 不读 SQLite。
4. 重写内置 Memory Skill/manifest/script。完成标准：工作流与新工具一致，权限仍由产品 allowlist 执行。
5. 添加 surface parity 与 CLI-to-Engine tests。完成标准：错误、tenant、取消/重试和结构化 Recall 结果一致。

## Acceptance

- [ ] 新公开方法覆盖 Issue #20 最小集合。
- [ ] Engine/Tauri/CLI 返回一致 DTO 与错误。
- [ ] CLI 只走 Engine。
- [ ] generated contract 由命令生成。
- [ ] Skill 不引用 Dream/candidate/旧 Evidence。
- [ ] 旧公开 method 从 registry/surface 退出。
- [ ] Recall tool 权限不依赖 Skill 提示。

## Non-goals

Desktop 页面最终切换、旧数据库归档/删除、通用 Agent CLI。

## Ticket-specific stop

如果只能靠手工编辑 `cli/internal/schema/contract.json` 或 Skill 需要绕过 AppService/工具 allowlist，停止并报告。
