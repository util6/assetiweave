# A-C02：依赖接管后的跨层契约统一

> **Status: PLANNED**。在全部库替换卡通过后执行；不把“统一契约”扩成新的通用框架。

**Goal:** 对已经接入的库统一 ownership、调用边界、错误/任务语义和公共命令契约，清除过渡分叉。
**Depends:** A-F16、A-R03、A-R04、A-R05、A-R06、A-R07、A-R10、A-R11、A-R12、A-C01。
**Contracts:** C-BASE、C-FRONTEND、C-TASK、C-SETTINGS、C-ERROR、C-CONFIG。
**Gates:** G-FE、G-RUST、G-CONTRACT、G-BEHAVIOR。

## 文件与接口

- Inspect/Modify when drift exists: `frontend/src/services/` 内本任务改过的服务、`frontend/src/app/query/` 的 keys/options/事件桥、`frontend/src/app/AppProviders.tsx`、`src-tauri/src/backend/runtime/error.rs`、`src-tauri/src/adapters/engine/{registry.rs,protocol.rs,surface_mapping.rs}`、`src-tauri/src/adapters/tauri/commands.rs`。
- Test: 对应 service tests、`frontend/src/architectureBoundaries.test.ts`、Rust runtime/Engine tests、`cli/tests/cli_e2e/cli_e2e_test.go`。
- Generate: `cli/internal/schema/contract.json`、surface matrix。
- 不引入新的 codegen/IDL 或改变所有 public DTO 的命名风格；既有 Rust schemars/registry 是 Engine 参数契约源，Go 使用生成合同。前端通过 service 的明确 DTO/错误转换承接，不把重复定义一致性误称为“全部类型自动生成”。

## 逐项统一并证明

- [ ] **状态契约**：列出 Router、Query、Zustand、Settings、TaskRuntime 的唯一 owner；检查没有 Query 和 Context/Zustand 双写同一后端结果。过渡导出只保留业务 API，不持有第二份缓存/轮询。
- [ ] **服务契约**：检查 Tauri catch 分支；浏览器 preview 的 mock 选择必须在调用前依据运行环境决定，真实 Tauri 调用失败原样映射 WireError，不能 fallback 成成功数据。为每个改动服务补相同失败断言。

先在 `frontend/src/services/appSettings.test.ts` 落以下完整用例；其他真实服务复用其既有 desktop 环境 fixture，不能只添加 source grep：

```ts
import { invoke } from "@tauri-apps/api/core";
import { beforeEach, expect, it, vi } from "vitest";
import { getAppSettings } from "./appSettings";
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
beforeEach(() => vi.clearAllMocks());
it("Tauri 失败保留公开错误而不回退成 mock 成功", async () => {
  const wireError = {
    code: "conflict", message: "The task is already running.",
    retryable: true, details: { taskId: "fixture-task" },
  };
  vi.mocked(invoke).mockRejectedValue(wireError);
  await expect(getAppSettings()).rejects.toMatchObject(wireError);
});
```

- [ ] **任务契约**：从触发 → snapshot → event/poll → terminal → invalidate 走通一个扫描与一个 Memory/Team 任务。相同 ID 的重复终态不二次刷新；OneShotEngine 不返回无人执行的后台任务。
- [ ] **错误契约**：Rust AppError typed source 不出现在公开 details；已存在 code/retryable 分类不变。相同服务操作经 Tauri/Engine 呈现相同业务结果；Engine envelope 与 Tauri invoke 外壳可不同，不强行统一传输壳。
- [ ] **设置契约**：global settings key 唯一，locale 初始化与普通保存分工一致；config 不读取 UI settings。确认 A-C01 首次导入在旧库上完成且数据保留。
- [ ] **版本与生成**：对生成合同作语义 diff；只有新增 locale 方法且原字段不变时保持 protocol=1、contract=3 的既有兼容线。真正破坏性的请求/响应变更显式升级 contract 并联动 `cli/internal/protocol/` 的版本断言与 release 元数据；不让执行模型擅自忽略 handshake。没有破坏变化不为了“统一”刷版本。
- [ ] **实际验证**：先跑相关 regression，再执行 G-CONTRACT 全链。每个行为证据引用所属卡的测试，不再复制一套契约规则文档。

```sh
pnpm typecheck
pnpm lint
pnpm test
cargo test -p assetiweave runtime
cargo test -p assetiweave adapters::engine
pnpm cli:contract
pnpm gen:surface-matrix
pnpm check:surface-matrix
go vet -C cli ./...
go test -C cli -race ./...
```

## 完成条件

Issue 交接给出“契约项 → owner → 生产入口 → 测试”的闭环表。任何保留的双缓存、双路由、双轮询或 mock 吞错误均为阻断项。需要大范围重新设计时拆出明确修复卡、更新 map 并先审查，不在本卡直接启动插件平台。
