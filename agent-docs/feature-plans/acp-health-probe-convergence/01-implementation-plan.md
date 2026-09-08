# ACP 单会话综合探针收口实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 将 ACP 连接健康检查、模型发现、缓存、生命周期身份、取消与前端状态同步收口为可验证的单会话综合探针。

**Architecture:** ACP probe 只创建一次进程并依次执行 initialize/session-new，返回协议连接结果与模型目录结果；Agent Market 以安装身份和 definition fingerprint 绑定短时模型缓存，并以按身份的 in-flight flight 合流并发请求。SQLite 安装健康字段是权威状态，模型缓存仅服务读取性能，生命周期变更主动清理缓存与 flight。

**Tech Stack:** Rust/Tokio/Tauri/SQLite、React 19/TypeScript、Vitest、fake ACP Node fixture。

**Spec:** `/Users/util6/.codex/attachments/7ef86a54-2e15-475c-8018-0698cfd3497b/pasted-text-1.txt`

## Global Constraints

- 保留 `AppService` 作为 Tauri 与 Engine 的业务编排边界。
- 不向第三方 Agent 目录写入应用状态；SQLite 仍是安装健康权威。
- 不引入新依赖；遵守现有 Rust/TypeScript 格式与错误契约。
- 保留工作树已有 ACP 改动，不覆盖无关用户修改。

---

### Task 1: 固化 ACP 单会话 probe、分阶段超时与模型目录错误分类

**Files:**
- Modify: `src-tauri/src/backend/ai_execution/types.rs`
- Modify: `src-tauri/src/backend/ai_execution/backends/acp.rs`
- Modify: `src-tauri/test-fixtures/fake-acp-agent.mjs`
- Test: ACP backend unit tests in `src-tauri/src/backend/ai_execution/backends/acp.rs`

**Acceptance criteria:**
- [ ] 一个 probe 只出现一次 initialize 与 session/new；session/new 成功即 protocol ready，模型目录异常不降级 protocol。
- [ ] probe 使用 spawn 10s、initialize 30s、session/new 30s、总 probe 65s，并让 cleanup 拥有独立上限。
- [ ] 模型目录公开区分 `model_list_empty`、`model_catalog_invalid`、`model_discovery_timeout`、`model_discovery_failed`；混合有效/损坏项按严格策略失败。
- [ ] 取消在 spawn、initialize、session/new 阶段均可打断，并进入已有 cleanup 进程回收路径。

**Verification:**
- [ ] `cargo test -p assetiweave --lib backend::ai_execution::backends::acp`
- [ ] 针对 fixture 的连接、空目录、损坏目录和取消测试通过。

### Task 2: 实现安装身份绑定缓存、singleflight、强制刷新与生命周期失效

**Files:**
- Modify: `src-tauri/src/backend/agent_market/runtime.rs`
- Modify: `src-tauri/src/backend/agent_market/lifecycle/install.rs`
- Modify: `src-tauri/src/backend/agent_market/lifecycle/uninstall.rs`
- Modify: `src-tauri/src/backend/agent_market/lifecycle/mod.rs`
- Modify: `src-tauri/src/backend/agent_market/repository.rs`
- Test: runtime/lifecycle tests in the modified Rust modules

**Acceptance criteria:**
- [ ] 自动模型读取可命中身份校验后的短时缓存；强制健康刷新永远绕过旧缓存但加入相同身份的 in-flight probe。
- [ ] 成功与失败并发请求均共享同一次 ACP 生命周期；失败可短时负缓存但不会无限复用。
- [ ] 缓存身份包含 agent_id、installation_id、definition fingerprint、enabled/可执行文件状态；卸载、停用、启用、安装、更新、重装和 registry definition 变化后不复用旧结果。
- [ ] 可取消的 probe 在没有有效调用方时终止并回收进程；安装变更与 probe 不持有互相阻塞的全局锁。

**Verification:**
- [ ] 并发成功/失败、强制刷新、disable/uninstall/reinstall、definition 变化和 executable 删除回归测试通过。
- [ ] `cargo test -p assetiweave --lib backend::agent_market`

### Task 3: 公开权威健康快照并修正 AppService/前端消费路径

**Files:**
- Modify: `src-tauri/src/backend/agents/types.rs`
- Modify: `src-tauri/src/backend/application/agent.rs`
- Modify: `src-tauri/src/adapters/tauri/commands.rs`
- Modify: `frontend/src/services/agentRuntime.ts`
- Modify: `frontend/src/components/settings/AgentSettingsPanel.tsx`
- Test: `src-tauri/src/backend/application/agent.rs` and `frontend/src/components/settings/AgentSettingsPanel.test.tsx`

**Acceptance criteria:**
- [ ] 模型请求不直接写入连接状态；健康快照/安装 revision 由后端返回或由权威市场快照消费。
- [ ] 卸载后 ACP 模型请求返回 installation-not-found；模型错误不会伪装成协议连接失败。
- [ ] 关闭模型弹窗会发出取消请求，旧结果不会写回 UI，后端 probe 进程会回收。
- [ ] 现有 native 与静态 Agent 模型流程保持兼容。

**Verification:**
- [ ] React 模型弹窗回归测试覆盖模型失败不改变主卡片状态、关闭后忽略结果和权威快照同步。
- [ ] Rust application tests and `pnpm typecheck` pass.

### Checkpoint: Complete

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo test --workspace`
- [ ] `pnpm typecheck && pnpm test && pnpm build`
- [ ] `git diff --check`
- [ ] Review correctness, architecture, security and performance; report any remaining gaps.
