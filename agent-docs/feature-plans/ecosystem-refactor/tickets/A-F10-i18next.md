# A-F10：react-i18next 接管翻译并接通SQLite语言偏好

> **Status: PLANNED**。执行时使用 `superpowers:executing-plans`，只执行本卡。

**Goal:** 删除自研翻译引擎，原语言偏好一次导入SQLite，实际UI使用i18next。
**Architecture:** bundled资源 + i18next实例 + native I18nextProvider；语言偏好保存复用settings mutation，初始化复用Rust原子命令。
**Tech Stack:** `../02-dependencies.md` 锁定的i18next/react-i18next。
**Spec:** [Issue #22](https://github.com/util6/assetiweave/issues/22)。
**Depends:** A-F09、A-C01、A-F06。
**Contracts:** C-BASE、C-FRONTEND、C-SETTINGS、C-UI。
**Read:** 入口、对应契约节、playbook。

## 文件与接口

- Modify: `package.json`、`pnpm-lock.yaml`、`frontend/src/app/AppProviders.tsx`、`frontend/src/main.tsx`、`frontend/src/i18n/messages.ts`、`domain.ts`、`navigation.ts`、所有旧I18nProvider import消费者（通过 `rg -l 'i18n/I18nProvider|./I18nProvider' frontend/src` 确定闭包）。
- Create: `frontend/src/i18n/createAppI18n.ts`、`localeBootstrap.ts`、`useI18n.ts`、`I18nEffects.tsx`、`types.ts`、`localeBootstrap.test.ts`、`createAppI18n.test.ts`。
- Create资源: `frontend/src/i18n/resources/{zh,en}/{common,catalog,conversations,memory,team,settings}.ts`。每locale六个文件，原文案仅一份；messages.ts降为资源汇总/类型导出。
- Test: 既有 `i18n/navigation.test.ts`、settings及受import变化影响的组件测试。
- Consumes: A-C01 `AppLocale = "zh" | "en"`、`AppSettings.locale:AppLocale|null`、`initializeAppLocaleIfUnset(locale:AppLocale):Promise<AppSettingsFile>`；A-F06 `appSettingsKey/useSaveAppSettings`。
- Produces（本卡创建）:

```ts
export function createAppI18n(locale: AppLocale): Promise<i18n>;
export function resolveInitialLocale(stored: string | null, navigatorLanguage?: string): AppLocale;
export function ensureAppLocale(file: AppSettingsFile, storage: Pick<Storage, "getItem" | "removeItem">, navigatorLanguage?: string): Promise<AppSettingsFile>;
export type Translator = (key: TranslationKey, params?: TranslationParams) => string;
export function useI18n(): { locale: AppLocale; setLocale(locale: AppLocale): void; t: Translator };
```

`useI18n`仅组合native `useTranslation`和settings mutation，保留现有调用形状，不实现插值/回退/词典查找。`setLocale`即时changeLanguage，保存失败由settings错误状态呈现且回到当前权威值；遵守A-F06旧响应不覆盖新draft规则。

## Red 与关键实现

先保存现有中文/英文导航及含参数文案测试green。下面测试定义全部fixture，放 `localeBootstrap.test.ts`：

```ts
import { expect, it, vi } from "vitest";
import { ensureAppLocale } from "./localeBootstrap";
import type { AppSettingsFile } from "../services/appSettings";
const initialize = vi.hoisted(() => vi.fn());
vi.mock("../services/appSettings", () => ({ initializeAppLocaleIfUnset: initialize }));

it("SQLite显式语言优先于旧localStorage", async () => {
  const file: AppSettingsFile = {
    config_dir: "/tmp/app", config_path: "/tmp/app/data.db",
    conversation_adapter_dir: "/tmp/app/adapters", settings: { locale: "en" },
  };
  const storage = { getItem: vi.fn(() => "zh"), removeItem: vi.fn() };
  initialize.mockClear();
  expect(await ensureAppLocale(file, storage, "zh-CN")).toBe(file);
  expect(initialize).not.toHaveBeenCalled();
});
```

增加null语言分支：storage=`zh`、navigator=`en-US`，initialize返回 `{...file,settings:{locale:"en"}}`（模拟其他窗口先赢CAS）；最终用返回的en并删除旧键，不能用自己提出的zh覆盖。初始化失败保留旧键，不写假成功标记。storage访问抛异常时继续使用navigator，不使应用崩溃。

初始化API：

```ts
const instance = createInstance();
await instance.use(initReactI18next).init({
  lng: locale, fallbackLng: "zh", resources: {
    zh: { translation: messages.zh }, en: { translation: messages.en },
  },
  keySeparator: false, nsSeparator: false, returnNull: false,
  interpolation: { escapeValue: false },
});
```

main.tsx 只创建一次 i18next 实例：以现有启动 settings cache 中合法 locale 或 resolveInitialLocale 的临时值调用 createAppI18n，完成初始化后挂 React 树；AppProviders 的 I18nextProvider 使用该实例。I18nEffects 等 Query settings 成功后应用 SQLite/CAS 返回语言，cache 初始值不自动写库。初始化失败有明确可见错误入口，不让空白应用被当成成功。测试每例新实例，不修改全局语言污染其他文件。

资源分桶按现有键前缀：conversation相关→conversations，memory/recall/recent→memory，team→team，settings/theme/typography→settings，asset/source/profile/group/mount/skill→catalog，其余→common；完整键不改名。转换前记录两语言键集合，转换后自动逐键比较值，避免同时漏掉两语言而测试仍通过。

## 步骤

- [ ] **Baseline**：保存原messages键值清单于测试运行内存/临时输出，不提交第二份词典；记录现有插值和回退行为。
- [ ] **Red**：添加bootstrap优先级/CAS返回/失败保留键、native i18next `t`插值测试；旧Provider生产引用guard先red。
- [ ] **Migrate**：安装库、移动资源、创建实例；I18nEffects读取settings query并初始化locale后用返回值更新cache；所有真实消费者换native hook组合。
- [ ] **Clean**：删除 `I18nProvider.tsx` 的Context、interpolate、独立locale状态与localStorage写入；旧键只出现在一次性迁移模块/测试。SQLite已明确locale时旧key可清除，但不再触发初始化命令。
- [ ] **Verify**：运行下列命令，证明无硬编码单语言回归。

```sh
pnpm exec vitest run --config frontend/vite.config.ts frontend/src/i18n frontend/src/store/settings frontend/src/components/settings/GlobalSettingsDialog.sync.test.tsx
pnpm typecheck
pnpm test
pnpm lint
```

## 验收与停止

所有当前文案可用；语言只持久化SQLite；native i18next实际被调用。A-C01原子命令未完成则停止，不用前端read-then-save伪造CAS。没有云翻译依赖、网络资源加载或新国际化框架。

**API 来源:** [useTranslation](https://react.i18next.com/latest/usetranslation-hook)、[i18next配置](https://www.i18next.com/overview/configuration-options)。
