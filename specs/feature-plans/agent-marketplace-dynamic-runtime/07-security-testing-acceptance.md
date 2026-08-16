# Security, Testing and Acceptance

| 字段 | 值 |
|---|---|
| 状态 | Proposed |
| 安全边界 | 远程目录/分发物不可信；核心进程策略和 app-owned 路径可信 |
| 测试策略 | fixture 优先、分层验证、无真实网络依赖、失败注入 |

## 1. 威胁模型

### 1.1 受保护资产

- 用户本地文件和 workspace。
- app-owned SQLite、settings、runtime root 和缓存。
- API key/token 等 secret。
- Agent execution 的 prompt/result。
- 主进程可用性和后台任务响应性。
- 当前可用的旧 Agent installation。

### 1.2 不可信输入

- 远程 curated catalog HTTP 响应。
- Catalog 引用的 Binary/npm/PyPI artifact。
- npm/uv/package manager 输出和失败行为。
- archive 内文件名、文件类型、数量、大小和压缩比。
- System CLI 的 stdout/stderr、版本字符串和退出状态。
- 已安装 Agent 的 ACP 消息、tool/permission 请求和 stderr。
- 前端/Engine/CLI 传入的 agent ID、distribution ID、task ID 和确认字段。
- 本地其他进程对 app-owned installation 的删除或修改。

### 1.3 非安全保证

MVP 的 SHA-256/package integrity 用于验证下载与精选元数据一致，不等同于 Vendor 代码安全审计或代码签名。递归目录 hash 也不会把不可信 Agent 变成可信，因此本设计不引入伪造的 trust 状态。

## 2. 安全控制

### 2.1 Catalog

- remote catalog URL 来自编译/受控配置，不接受 Market UI 任意 URL。
- 只允许 HTTPS，限制 redirect 次数，最终 host 仍必须满足 allowlist。
- 响应上限 5 MiB，超时和取消有界。
- JSON/schema/semantic validate 成功前不替换 cache。
- 禁止 catalog raw secret、shell string、hook、相对本地 source、Git source。
- 未知 schema major、protocol、distribution、capability 必须 fail closed。

### 2.2 Binary 下载和解压

- URL 不允许 `file:`、loopback、私网重定向或非 HTTPS；artifact host 来自精选发布 policy。
- 下载上限 512 MiB；已知 `size` 超限在下载前拒绝，流式超限立即取消。
- SHA-256 在解压前校验。
- 解压后总大小上限 1 GiB、文件数上限 20,000、单文件上限 512 MiB。
- 拒绝绝对路径、`..` traversal、NUL、Windows drive/UNC、symlink、hardlink、device、FIFO/socket。
- canonical target 必须在 staging root 内。
- 防止 overwrite staging 外文件；重复 archive entry 必须拒绝或按固定 fail-closed 规则处理，MVP 采用拒绝。
- 解压和 hash loop 必须支持 cancellation，并有压缩炸弹预算。

### 2.3 Npx/npm

- package 必须是规范 npm name；version 必须精确 semver，禁止 tag/range/URL/Git/path。
- 使用 argv 调用 npm，不通过 shell。
- 强制 `--ignore-scripts --no-audit --no-fund --omit=dev --save-exact`。
- `npm_config_userconfig`、cache 等只使用受控路径；secret/token 只通过受控 credential channel，日志不输出。
- 安装后核对 lock 中目标 package version/integrity 和 local bin boundary。
- 执行阶段只运行 local bin/entry，不运行 npm/npx。
- 若目标包必须 lifecycle script，当前 catalog item 不受支持，需新 ADR。

### 2.4 Uvx/uv

- package/name/version 必须精确，禁止 URL/Git/path/editable。
- 通过 argv 调用 `uv tool install`，不使用 shell。
- `UV_TOOL_DIR`/`UV_TOOL_BIN_DIR` 固定在 staging；不得写入用户全局 uv tool root。
- 记录 exact version、uv version 和 tool metadata；command 必须在 app-owned bin。
- 执行阶段不运行 `uvx` 或安装命令。

### 2.5 System

- command candidate 只来自精选索引。
- resolver 输出必须是普通 executable；不接受用户输入的 shell string/alias/function。
- version/protocol probe 有 timeout、output cap、process tree cleanup。
- 绑定不复制、不 chmod、不 hash；卸载不删除。
- PATH 变化后只有显式/启动轻量检查才更新 observation；持久 definition 不通过 shell 重新解析。

### 2.6 Environment 和 Secrets

- 远程 catalog 只能声明 allowlisted env reference ID 和少量 allowlisted非敏感常量。
- `PATH`、`HOME`、动态 loader、shell startup、proxy 等敏感覆盖由核心策略决定，catalog 不能任意设置。
- DB/catalog/cache/task snapshot 不保存 raw secret。
- 日志不输出完整 argv/env；如果 argv 未来包含 secret 引用解析值，只输出参数数量或安全 preview。
- package manager credential 文件权限必须受限，并在 task 后清理临时凭据。

### 2.7 核心执行策略不可覆盖

Agent package/market metadata 不得改变：

- Translation 的空 app-owned workspace。
- MCP 空注入或核心批准的 MCP policy。
- permission 自动拒绝策略。
- prompt/output/time/process 限制。
- cancellation、terminate/kill tree 和 stderr cap。
- 日志脱敏。
- model 显式选择失败不得静默回退。

### 2.8 路径删除

managed 删除必须同时满足：

1. DB ownership=`managed`。
2. `install_dir` 非空。
3. canonical install_dir 是 canonical runtime root 的后代。
4. 路径布局中的 agent/version/distribution 与 row 匹配。
5. 不是 runtime root 本身、用户 home、父目录或 symlink 跳转。

任一不满足返回 `unsafe_install_path`，不执行删除。

## 3. 资源和时间预算

| 资源 | 默认上限 | 行为 |
|---|---:|---|
| Catalog HTTP 响应 | 5 MiB | 超限拒绝，保留旧 catalog |
| Catalog refresh | 60 s | timeout，保留旧 catalog |
| 完整 lifecycle task | 10 min | cancel/terminate，旧安装不变 |
| Binary artifact | 512 MiB | 流式超限取消 |
| 单解压文件 | 512 MiB | 拒绝 archive |
| 解压总量/managed install | 1 GiB | 拒绝并清 staging |
| 解压文件数 | 20,000 | 拒绝并清 staging |
| System/version probe | 8 s | failed observation |
| Conformance | 30 s | kill tree，安装不激活 |
| Probe stdout | 1 MiB | output_limit |
| Probe stderr | 256 KiB | output_limit |
| 并行 lifecycle tasks | 2（不同 Agent） | 其余 queued |
| 同 Agent task | 1 | 返回已有 task/conflict |
| staging 保留 | 24 h | 启动 recovery 清理 |

具体常量应集中定义并可测试，禁止分散 magic numbers。

## 4. 测试分层

### 4.1 Pure Domain Tests

目标：无需 DB、网络、真实 npm/uv/Agent。

- catalog JSON/schema/semantic validation。
- ID/version/target/path validation。
- distribution selection deterministic order。
- execution readiness 真值表。
- error mapping 和 redaction。
- preview token identity。

### 4.2 Repository Tests

使用临时 `ASSETIWEAVE_DB_PATH`：

- migration DDL/约束/index。
- tenant isolation。
- upsert/enable/health/delete。
- ownership/install_dir CHECK。
- Registry candidate query。
- transaction rollback 和旧 row 恢复。

### 4.3 Installer Fixture Tests

所有 installer 通过 fake command runner/local fixture server 注入：

- Binary fixture archive/hash/target。
- fake npm 创建 package-lock/local bin。
- fake uv 创建 tool metadata/local command。
- fake System resolver/version process。

CI 单元测试不得访问真实 Registry/npm/PyPI/Vendor 下载。

### 4.4 Lifecycle Integration Tests

使用临时 DB、runtime root、fixture catalog、fake conformance：

- staging、activation、Registry generation。
- update failure preserves old。
- cancellation at every phase。
- DB/Registry swap failure compensation。
- cleanup warning/recovery。
- active execution conflict。

### 4.5 Protocol/Process Tests

复用现有 fake ACP 和 process tree tests：

- installed resolved definition 可 initialize/session/new/close。
- conformance 无 prompt、无 MCP、拒绝 permission。
- timeout/cancel/exit 均无残留 child/grandchild。
- OpenCode ACP fail 不产生 CLI execution fallback。

### 4.6 API/Frontend/CLI Tests

- AppService method/DTO。
- Engine registry contract/risk/confirmation。
- Tauri command start/get/cancel/event。
- frontend service invoke shape/schema。
- Provider event + polling merge。
- Market/Installed/preview/assignment UX。
- Go command calls Engine、preview/confirm、wait/json exit code。
- installed state 在 Engine/CLI/Desktop 语义一致。

## 5. 规范性测试矩阵

### 5.1 Catalog

| ID | 场景 | 预期 |
|---|---|---|
| CAT-01 | bundled catalog valid | offline list succeeds |
| CAT-02 | cache valid | cache selected over bundled |
| CAT-03 | remote 304 | 保留 cache，更新 fetched metadata |
| CAT-04 | remote invalid JSON/schema | 保留旧 catalog，refresh failed |
| CAT-05 | duplicate Agent/distribution ID | reject catalog |
| CAT-06 | unknown schema major/protocol/type | reject catalog |
| CAT-07 | floating version/tag | reject item/catalog |
| CAT-08 | core incompatible | item visible but not installable |
| CAT-09 | new standard ACP item only in data | list/preview/install/Registry 全链路无需 Vendor code |

### 5.2 Distribution

| ID | 场景 | 预期 |
|---|---|---|
| DST-01 | compatible System + Binary | System recommended，二者均可显式选 |
| DST-02 | System version incompatible | Binary recommended；System 不可选 |
| DST-03 | Binary wrong target | 不可选 |
| DST-04 | Binary hash mismatch | artifact_integrity_failed；不解压/激活 |
| DST-05 | zip-slip/absolute/symlink/hardlink | archive_invalid |
| DST-06 | archive size/file count exceeded | archive_invalid；staging clean |
| DST-07 | Npx missing Node/npm | runtime_missing |
| DST-08 | Npx exact install | local bin + lock integrity；runtime argv 无 `npx -y` |
| DST-09 | Npx package URL/tag/range | schema/preflight reject |
| DST-10 | Uvx missing uv | runtime_missing |
| DST-11 | Uvx install | app-owned tool/bin；runtime argv 无 `uvx` |
| DST-12 | System uninstall | external executable unchanged |

### 5.3 Lifecycle/Registry

| ID | 场景 | 预期 |
|---|---|---|
| LIFE-01 | fresh install one Agent | DB one row；Registry one Agent |
| LIFE-02 | same Agent concurrent install | dedupe/conflict，不创建两个 task |
| LIFE-03 | different Agent install | bounded concurrency，可并发 |
| LIFE-04 | cancel download/install/probe | terminal cancelled；staging clean；旧 row不变 |
| LIFE-05 | conformance fail on first managed install | no active row/Registry definition |
| LIFE-06 | System ACP fail | installed diagnostic row；connected=false；不进 Registry |
| LIFE-07 | update success | generation +1；新 definition；旧目录清理 |
| LIFE-08 | update download/hash/probe fail | old row/definition/path unchanged |
| LIFE-09 | DB activation fail | Registry unchanged |
| LIFE-10 | Registry reload fail after DB write | DB compensated；old Registry unchanged |
| LIFE-11 | active execution update/uninstall | agent_in_use |
| LIFE-12 | running execution + unrelated Agent install | execution/UI不受阻塞 |
| LIFE-12A | 激活检查后并发发起新 execution | mutation gate 阻止其越过 DB/Registry 临界区 |
| LIFE-13 | disable/enable | Registry remove/add；文件保留 |
| LIFE-14 | managed uninstall | row/Registry/path removed，assignment按确认处理 |
| LIFE-15 | unsafe managed path | delete rejected |
| LIFE-16 | startup stale staging | bounded cleanup |
| LIFE-17 | startup missing entry | row broken；不进 Registry |
| LIFE-18 | Registry lookup during swap | only old or new complete snapshot，绝无 partial |

### 5.4 OpenCode/Migration

| ID | 场景 | 预期 |
|---|---|---|
| MIG-01 | assigned OpenCode + compatible CLI | idempotent system binding |
| MIG-02 | assigned OpenCode + no CLI | assignment preserved；agent_not_installed；no network |
| MIG-03 | CLI version success + ACP fail | installed=true/connected=false/execution_ready=false |
| MIG-04 | assigned old Npx Agent | no silent `npx -y`; install CTA |
| MIG-05 | unassigned hardcoded agents | Market only，不写 rows |
| MIG-06 | repeated startup | 不覆盖 managed choice/不重复迁移 |
| MIG-07 | Translation ACP failure | no `opencode run` process |

### 5.5 API/UI/CLI

| ID | 场景 | 预期 |
|---|---|---|
| API-01 | Engine/Tauri market list | same domain semantics |
| API-02 | Tauri lifecycle start | 快速返回 snapshot，不等待 I/O |
| API-03 | event dropped | polling restores terminal state |
| API-03A | one-shot Engine lifecycle run | 同一请求等待终态；无跨进程 task ID 轮询 |
| API-04 | compatibility catalog | callable；未安装条目不暴露临时 execution command |
| API-05 | contract generation | generated contract tests pass |
| UI-01 | open Agent settings | no probe-all process fanout |
| UI-02 | Market install | preview before start |
| UI-03 | active Agent task | only same Agent conflicting controls disabled |
| UI-04 | unavailable current assignment | preserved disabled value + CTA |
| UI-05 | System ACP failure | 显示“已安装，ACP 连接失败”，不显示 connected |
| CLI-01 | install without `--yes` | print preview，no side effect |
| CLI-02 | install `--yes --json` | 单次 Engine `agent.install.run` terminal JSON/正确 exit code |
| CLI-03 | uninstall System | Engine unbind；CLI 不删文件 |
| CLI-04 | Ctrl-C during install | Engine context cancellation 收敛 child/staging，旧安装不变 |

## 6. 故障注入点

实现必须为测试暴露可注入 seam，而不是靠真实故障：

- catalog fetcher。
- clock/ETag cache writer。
- command runner（npm/uv/System）。
- artifact downloader。
- archive extractor 或受控 fixture。
- conformance checker。
- installation repository transaction。
- Registry publisher/swap。
- per-agent lifecycle mutation gate。
- filesystem rename/delete。

生产默认实现通过 trait/struct dependency 注入；禁止新增全局 mutable test hook。

## 7. 响应性验收

测试或可重复人工证据必须证明：

1. Tauri `start_agent_installation` 在 worker 开始前返回 snapshot，不等待下载。
2. 安装期间 `agent.market.list`、设置导航和其他 Agent 操作仍响应。
3. 不持有 `AppState.lock` 运行 download/npm/uv/archive/conformance。
4. 同 Agent task 被 dedupe，不重复下载。
5. batch/列表刷新只在必要终态做一次，不因每个 phase 全量 reload UI。
6. App close 检测活跃 lifecycle task 并提示。

## 8. 日志审计

允许字段：

- task ID、agent ID、action、phase、distribution type、ownership。
- catalog/version、耗时、字节数、文件数。
- stable error code、exit code（如适用）、Registry generation。

禁止字段：

- prompt/result、raw ACP/tool payload。
- raw secret/token、完整 env value。
- package registry credential、认证 URL query。
- 未脱敏完整 stderr/stdout。
- 可能包含 secret 的完整 argv。

测试至少扫描 error view/log fixture，证明敏感 marker 不出现。

## 9. 质量门

### Checkpoint A：Domain/Store

```bash
cargo fmt --all -- --check
cargo test --manifest-path src-tauri/Cargo.toml backend::agent_market
cargo test --manifest-path src-tauri/Cargo.toml backend::store
```

### Checkpoint B：Runtime/Lifecycle

```bash
cargo test --manifest-path src-tauri/Cargo.toml backend::agents
cargo test --manifest-path src-tauri/Cargo.toml backend::ai_execution
cargo test --manifest-path src-tauri/Cargo.toml agent_market
```

### Checkpoint C：API/Contract/CLI

```bash
pnpm cli:contract
cargo test --workspace
go vet -C cli ./...
go test -C cli -race ./...
pnpm cli:test:e2e
```

### Checkpoint D：Frontend

```bash
pnpm typecheck
pnpm test
pnpm build
```

### Final

```bash
cargo fmt --all -- --check
cargo test --workspace
go vet -C cli ./...
go test -C cli -race ./...
pnpm typecheck && pnpm test && pnpm build
pnpm cli:test:e2e
```

如果本机 Rust 版本暂不满足仓库 `rust-version`，必须记录环境阻塞和 CI/兼容环境证据；不得把未运行测试标为 PASS。

## 10. 人工 Smoke Matrix

至少覆盖当前支持的 macOS 架构；发布前按精选 target 扩展 Linux/Windows：

1. 离线打开 Market。
2. OpenCode System bind 成功。
3. OpenCode System ACP 失败状态准确。
4. OpenCode managed Binary 安装/执行/卸载。
5. 一个 Npx Agent 固定安装并断网执行。
6. Hermes Uvx 固定安装并断网执行。
7. 更新失败保留旧版本。
8. 运行时尝试卸载被阻止。
9. 应用安装中退出提示、取消和重启 cleanup。
10. CLI 与 Desktop 同一 DB 状态一致。

## 11. Release Acceptance

发布前必须由评审者逐条签署：

- [ ] 产品完成定义 13 项全部满足。
- [ ] 四种 distribution fixtures 全部 PASS。
- [ ] 无 runtime `npx -y`/临时 `uvx`。
- [ ] OpenCode ACP fail 不再 connected，也无 CLI execution fallback。
- [ ] dynamic Registry install/update/uninstall 无需重启。
- [ ] active execution lifecycle conflict 测试 PASS。
- [ ] update/DB/Registry 故障注入保持旧版本。
- [ ] archive/path deletion 安全测试 PASS。
- [ ] no probe-all 前端回归 PASS。
- [ ] Tauri/Engine/CLI contract 和 e2e PASS。
- [ ] 日志/错误脱敏测试 PASS。
- [ ] 旧 assignment 迁移无静默网络、无 silent Agent fallback。
