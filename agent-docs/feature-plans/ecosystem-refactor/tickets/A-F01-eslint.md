# A-F01：ESLint 接管前端静态规则

> **Status: PLANNED**。执行时使用 `superpowers:executing-plans`，一轮只做本卡；现在仅记录计划。

**Goal:** 用成熟 lint 规则检查真实前端导入边界，替代这部分文本匹配检查。
**Architecture:** ESLint flat config + TypeScript parser + React Hooks 规则；Rust/Go 边界继续由已有脚本检查。
**Tech Stack:** 精确版本和安装命令见 `../02-dependencies.md`。
**Spec:** [Issue #22](https://github.com/util6/assetiweave/issues/22)。
**Depends:** A00；须通过入口的工作树/基线检查。
**Contracts:** C-BASE、C-FRONTEND。
**Read:** `../00-execution-router.md`、`../01-contract.md` 对应小节、`../05-playbook.md`；不加载其他卡。

## 文件与接口

- Modify: `package.json`、`pnpm-lock.yaml`、`frontend/src/architectureBoundaries.test.ts`、`scripts/check-module-boundaries.sh`。
- Create: `eslint.config.mjs`、`frontend/src/eslintConfig.test.ts`。
- Test: 新 lint 配置测试及既有 `frontend/src/architectureBoundaries.test.ts`。
- Consumes: 仓库 services-only IPC 规则；现有 TypeScript/React 源码。
- Produces: `pnpm lint`（真实 `frontend/src` 全量检查）；`pnpm lint:architecture`（同一配置的边界检查，不创建第二配置）；ESLint default export 使用 flat-config 数组。

配置限制运行时导入 `@tauri-apps/api/core`、`@tauri-apps/api/event` 到 `frontend/src/services/**`；类型导入可显式允许。`@tauri-apps/plugin-*` 的运行时操作也应收敛 services。按调用方迁移已有违例，保持调用参数和返回类型。React Hooks 规则覆盖生产 `.ts/.tsx`；格式交给 A-F02。

## Red 与关键实现

先运行现有边界测试保存 green。新建下面的测试，首次因配置缺失或规则尚未接管变 red；不能以当前业务行为本来通过为由跳过 red。

```ts
import { ESLint } from "eslint";
import { fileURLToPath } from "node:url";
import { expect, it } from "vitest";

it("拒绝组件直接调用 Tauri，允许 services 调用", async () => {
  const cwd = fileURLToPath(new URL("../../", import.meta.url));
  const eslint = new ESLint({ cwd });
  const source = 'import { invoke } from "@tauri-apps/api/core"; export const load = () => invoke("list_assets");';
  const [component] = await eslint.lintText(source, {
    filePath: "frontend/src/components/ImportBoundaryProbe.ts",
  });
  const [service] = await eslint.lintText(source, {
    filePath: "frontend/src/services/importBoundaryProbe.ts",
  });
  expect(component.messages.some((m) => m.ruleId === "no-restricted-imports")).toBe(true);
  expect(service.messages.some((m) => m.ruleId === "no-restricted-imports")).toBe(false);
});
```

规则使用 `no-restricted-imports` 的 `patterns`，按 files/ignores 限定调用层；同文件动态 `import()` 用 `no-restricted-syntax` 的 AST selector 补齐，并增加对应 `lintText` 测试。生产 import-boundary 禁令不靠字符串包含测试。现有主题/领域分层断言若不是 import 规则，保留。

## 步骤

- [ ] **Baseline**：记录 `pnpm typecheck` 与下面既有边界测试结果；已有失败单独记入交接。
- [ ] **Red**：加入上述正反例和动态 import 例；证明组件被漏检而 services 不误报。
- [ ] **Migrate**：按依赖清单安装；创建 flat config/scripts，修复本轮规则检出的真实调用方；每搬一个操作补服务测试。
- [ ] **Clean**：删除脚本与 Vitest 中已被新规则覆盖的前端 import 字符串检查；保留 Rust/Go 和业务结构检查；没有全仓 `eslint-disable`。
- [ ] **Verify**：执行以下命令并记录退出码；只包含当前卡文件的中文提交由执行流程决定。

```sh
pnpm exec vitest run --config frontend/vite.config.ts frontend/src/eslintConfig.test.ts frontend/src/architectureBoundaries.test.ts
pnpm lint
pnpm typecheck
pnpm test:boundaries
```

## 验收与停止

真实生产文件进入 lint；错误导入报错、合法 service 导入通过；业务测试保持 green。若某现有规则无法用 ESLint 表达，保留该条而非退回所有文本规则。若锁定依赖不兼容 Node 22/TypeScript，按 playbook 停止并报告 peer dependency 证据，不擅自升级 React/TS。

**API 来源:** [ESLint flat config](https://eslint.org/docs/latest/use/configure/configuration-files)、[no-restricted-imports](https://eslint.org/docs/latest/rules/no-restricted-imports)、[typescript-eslint](https://typescript-eslint.io/getting-started/)。
