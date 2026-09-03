# A-F16：前端成熟生态收口与 CI 门禁

> **Status: PLANNED**。使用 `superpowers:executing-plans`。

**Goal:** 完成前端旧机制删除、真实边界检查与持续验证，不把引库留在局部样板。
**Depends:** A-F15。
**Contracts:** C-BASE、C-FRONTEND、C-TASK、C-SETTINGS、C-UI。
**Gates:** G-FE、G-BEHAVIOR。

## 文件与接口

- Inspect/Modify: `frontend/src/app/AppProviders.tsx`、Router/Query/Settings/i18n 的本次新模块、`package.json`、`eslint.config.mjs`、`.github/workflows/ci.yml`、`frontend/src/architectureBoundaries.test.ts`。
- Create: `frontend/src/ecosystemAdoption.test.ts`，仅检查旧机制无生产导入；行为证据仍来自对应卡测试。
- Produces: 实际 `pnpm lint`、`pnpm format:check` 进入现有 frontend CI job；Windows job 保持原有前端构建回归。

## 步骤

- [ ] 运行前面每张前端卡的 regression；将旧 Provider/import 转移期的剩余项列清单并逐一删除。
- [ ] 检查 AppProviders：保留库 Provider 与必要业务 runtime owner；不以 Provider 数量为唯一指标删除任务投影或全局进度。QueryClient/I18next 实例不在每次 render 创建。
- [ ] 按代码引用检查旧 AppRouter 匹配、routeLoaders、i18n Context/interpolate、asyncCache、旧 settings数据 Context 均退出生产；库外薄 facade 只表达业务，不持另一套状态。

```ts
import { expect, it } from "vitest";
const production = import.meta.glob("./**/*.{ts,tsx}", {
  query: "?raw", import: "default", eager: true,
}) as Record<string, string>;
it("生产代码没有重新导入已删除共享缓存", () => {
  const offenders = Object.entries(production)
    .filter(([path]) => !/\.test\.[tj]sx?$/.test(path))
    .filter(([, source]) => /from\s+["'][^"']*\/asyncCache["']/.test(source))
    .map(([path]) => path);
  expect(offenders).toEqual([]);
});
```

该检查只约束“已删除机制”；正常 import 架构规则由 ESLint，不新造静态分析框架。`as` 只对 Vite raw glob 的构建输出做明确类型边界，不用于伪造业务测试对象。

- [ ] CI 在 install 后执行 lint/format:check；脚本用当前唯一 config 和适当文件范围，不启用一套只检查新文件的长期例外。Prettier 避开生成合同/编译产物/业务资源快照，以免生成器与formatter永远互相改写。
- [ ] 运行全前端验证和现有边界脚本；记录库替换前后机制删除/保留表，交给 A-C02 跨层统一，不开插件平台。

```sh
pnpm lint
pnpm format:check
pnpm typecheck
pnpm test
pnpm build
pnpm check:boundaries
pnpm test:boundaries
```

**完成：** 所有前端新库均有真实消费者、旧同职责实现删除、CI 检查实际运行、业务测试通过。平台专属桌面结果由 A-G01 统一记录，不把浏览器 preview 冒充桌面验收。
