# A-F02：Prettier 接管格式并建立检查入口

> **Status: PLANNED**。执行时使用 `superpowers:executing-plans`，一轮只做本卡。

**Goal:** 前端格式由 Prettier 单一负责，格式化提交不混业务改动。
**Architecture:** 根目录统一配置；先覆盖 frontend 与新工程配置，Rust/Go 仍用 rustfmt/gofmt。
**Tech Stack:** `../02-dependencies.md` 锁定的 Prettier；不加入格式 lint 插件或另一格式器。
**Spec:** [Issue #22](https://github.com/util6/assetiweave/issues/22)。
**Depends:** A-F01。
**Contracts:** C-BASE、C-FRONTEND。
**Read:** `../00-execution-router.md`、契约对应小节、`../05-playbook.md`。

## 文件与接口

- Modify: `package.json`、`pnpm-lock.yaml`、`frontend/src/**/*.{ts,tsx,css}`、`frontend/vite.config.ts`、`frontend/tsconfig.json`、`eslint.config.mjs`（格式变化）。
- Create: `.prettierrc.json`、`.prettierignore`、`frontend/src/prettierConfig.test.ts`。
- Consumes: 双引号、分号、两空格、现有编译语义。
- Produces: `pnpm format`（对明确管理的前端/配置路径写入）；`pnpm format:check`（相同路径只检查）。
- 删除清单：只删除 ESLint 中与 Prettier 重复的样式规则；保留语义和架构规则。

## Red 与关键实现

Baseline 使用 `pnpm typecheck`。新配置/测试尚未创建时记录 red；不要为了制造 red 改坏生产代码。

```ts
import { format, resolveConfig } from "prettier";
import { fileURLToPath } from "node:url";
import { expect, it } from "vitest";

it("配置符合本仓库 TypeScript 格式", async () => {
  const file = fileURLToPath(new URL("./formatProbe.ts", import.meta.url));
  const options = await resolveConfig(file);
  expect(options).toMatchObject({ tabWidth: 2, singleQuote: false, semi: true });
  expect(await format("const label='asset'", { ...options, parser: "typescript" }))
    .toBe('const label = "asset";\n');
});
```

`.prettierrc.json` 内容：

```json
{ "tabWidth": 2, "singleQuote": false, "semi": true, "trailingComma": "all" }
```

`.prettierignore` 明确排除 `dist`、`node_modules`、`target`、生成的 Engine contract、内置第三方/vendor 资源、用户未要求格式化的文档目录。脚本使用明确 glob（例如 `"frontend/**/*.{ts,tsx,css,json}" "eslint.config.mjs" "package.json" ".prettierrc.json"`）；不执行仓库级 `prettier . --write`。

## 步骤

- [ ] **Baseline**：保存 typecheck 与 A-F01 lint 结果，确认没有并行编辑 `package.json`/lockfile。
- [ ] **Red**：加入配置测试并执行；记录缺少配置/格式规则的失败。
- [ ] **Migrate**：安装锁定版本、写配置/scripts，先 `format:check` 列出管理范围，再只格式化该范围。
- [ ] **Clean**：审阅 diff；所有 AST/业务内容变化退回单独后续卡，当前卡仅格式和工具接管。
- [ ] **Verify**：运行以下命令，确认第二次 format 不改变文件。

```sh
pnpm exec vitest run --config frontend/vite.config.ts frontend/src/prettierConfig.test.ts frontend/src/eslintConfig.test.ts
pnpm format:check
pnpm lint
pnpm typecheck
pnpm test
```

## 验收与停止

同一配置可写可查、范围内零格式差异，测试行为不变。若运行时新修改与他人工作交错，停止自动格式化该文件并报告；不覆盖用户工作。此卡不增加 Husky、CI 平台或新 E2E。

**API 来源:** [Prettier configuration](https://prettier.io/docs/configuration)、[API](https://prettier.io/docs/api)。
