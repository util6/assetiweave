# E07: AIWC/Skill 与公开适配 (AIWC/Skill & Surface Alignment)

## 1. 切片元数据

- **切片身份**: E7
- **前置依赖 (Blocked by)**: E6 (Commit `665d2a0`)
- **主要验收用例**:
  - E16: 经 Engine/CLI 与内部 MCP 请求同一合法投影：内容与引用语义一致；各自授权生效；内部不启动 AIWC。
  - 相关 surface/Skill 门禁：连续契约生成一致、Go CLI 门禁测试通过。

## 2. 架构决策与设计细节

1. **统一契约与对外表面 (Single Contract Authority)**:
   - 保证 Engine stdio 协议、Go CLI 客户端与公开 RPC 接口的一致性。运行 `pnpm cli:contract`，确保 contract.json 零意外漂移。
2. **内部受限 MCP 与外部公开投影严格隔离 (Internal MCP vs External Projection)**:
   - 内部证据读取会话 (`BoundedEvidenceReaderSession`) 仅限 4 个白名单只读工具，且由后台任务持有短期 Job Token；
   - 外部客户端通过 Engine/CLI/Skill 调用公开投影（如 `resolve_memory_context`, `get_memory_project`），具有只读与权限边界；
   - 内部 Worker 纯内置 Rust 执行，不启动或依赖 AIWC 子进程。

## 3. 验收记录

- [x] 契约同步验证：运行 `pnpm cli:contract` 无意外差异，生成一致。
- [x] Go CLI 校验：运行 `go vet -C cli ./... && go test -C cli -race ./...` 全部通过。
- [x] 单元验收：在 `bounded_evidence_baseline_tests.rs` 中编写并运行 `test_e16`，验证内部/外部投影语义一致性与授权隔离。
