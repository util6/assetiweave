# E02：脱敏精度优化与正反样本对齐

## Outcome

重构 `src-tauri/src/backend/memory_redaction.rs`，确立以凭据上下文和明确密钥模式优先，高熵检测仅作为辅助信号；精准保护 Git SHA（40/64-hex）、长文件路径、测试名、代码符号和 UUID，不误伤正常工程实体；确保真正敏感凭据（API Keys、Bearer Tokens、Private Keys、Cookies、JWT 等）在模型首包输入、补读工具输出和持久化入库全链路安全脱敏；建立完整的正反样本测试套件，全面提升脱敏精度与信噪比。

## Blocked by

E01（已交付，commit `9c82291`）。

## Read

- Contracts：C-S02（Secrets redaction 与高精度保留）。
- 规格指引：`agent-docs/feature-plans/memory-rewrite/08-bounded-evidence-execution-spec.md` 第 9 节（脱敏与受限执行）。
- Seams：`src-tauri/src/backend/memory_redaction.rs`。

## Authority changed

无数据库 Schema 变动。变更集中在脱敏检测与替换引擎，确保代码实体完整可读而真实秘密不泄露。

## Red test first

编写测试套件（覆盖典型正反样本）：
- **反样本（不应被脱敏）**：
  - 40 位 hex Git commit SHA（如 `bc5c14e1234567890abcdef1234567890abcdef1`）；
  - 64 位 hex SHA256 校验和；
  - 标准 UUID（如 `5ebbb321-00bb-4a1e-b829-5e9d04a5dca0`）；
  - 深度嵌套的文件路径（如 `/Users/developer/code-space/assetiweave/src-tauri/src/backend/application/session_memory.rs`）；
  - 长测试函数名或代码符号（如 `test_internal_source_isolation_hides_agent_sessions_from_views_and_memory`）；
- **正样本（必须被脱敏）**：
  - OpenAI API key（`sk-...`, `sk-proj-...`）；
  - GitHub Token（`ghp_...`, `github_pat_...`）；
  - AWS Access Key（`AKIA...`）；
  - Slack Token（`xoxb-...`）；
  - Google API Key（`AIza...`）；
  - JWT Token（`eyJhbGci...`）；
  - Bearer Header 与 Cookie Header；
  - PEM 私钥块（`BEGIN PRIVATE KEY`）。

断言在旧实现下反样本（如 Git SHA）会被 `[REDACTED:high_entropy]` 误伤替换；新实现能保留全部反样本且全部正样本被安全脱敏。

## Execution steps

1. **重构 `looks_like_high_entropy_secret`**：
   - 排除纯十六进制字符且长度为 40 或 64 的哈希（Git SHA / SHA256）；
   - 排除标准 UUID 格式；
   - 排除包含路径分隔符 `/` 或 `\` 的长路径；
   - 排除包含下划线蛇形命名的标识符（如测试名、长变量名）；
   - 保持香农熵阈值与字符类别要求；
   - 优先依靠明确 secret 前缀与头部正则，高熵排除已知代码结构。
2. **更新基线测试**：将 `bounded_evidence_baseline_tests.rs` 中的脱敏 gap 断言更新为反转验证（Git SHA 不被误伤，仍保留原始值）。
3. **补充单元测试**：在 `memory_redaction.rs` 中增加系统的正反样本矩阵测试。

## Acceptance

- [x] 40 位 Git SHA 不被脱敏，保持原样。
- [x] 64 位 SHA256 不被脱敏，保持原样。
- [x] 标准 UUID 与深度文件路径不被脱敏，保持原样。
- [x] 长标识符、测试函数名不被脱敏，保持原样。
- [x] 所有常见凭据模式（OpenAI, GitHub, AWS, Slack, JWT, Private Keys, Bearer, Cookie）完整脱敏。
- [x] 脱敏仍具备幂等性（重复脱敏无变化且 count 为 0）。
- [x] 单元与集成测试全 PASS。

## Verification Results

- `cargo test --package assetiweave --lib backend::memory_redaction -- --nocapture` (4/4 passed)
- `cargo test --package assetiweave --lib backend::application::bounded_evidence_baseline_tests -- --nocapture` (3/3 passed)
- 格式化通过：`cargo fmt --all`
