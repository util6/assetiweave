# A-F09：Zustand 接管共享UI状态

> **Status: PLANNED**。执行时使用 `superpowers:executing-plans`，只执行本卡。

**Goal:** 删除跨组件弹窗/面板状态的层层传递，后台数据仍由Query持有。
**Architecture:** 小型明确的UI store + selector订阅；局部表单与领域projection不迁入store。
**Tech Stack:** `../02-dependencies.md` 锁定的Zustand。
**Spec:** [Issue #22](https://github.com/util6/assetiweave/issues/22)。
**Depends:** A-F08。
**Contracts:** C-BASE、C-FRONTEND、C-UI、C-SETTINGS。
**Read:** 入口、契约相关节、playbook。

## 文件与接口

- Modify: `package.json`、`pnpm-lock.yaml`、`frontend/src/router/AppRouter.tsx`、`layouts/app/AppLayout.tsx`、`components/settings/GlobalSettingsDialog.tsx`、`app/updates/AppUpdateProvider.tsx`、`AppUpdateDialog.tsx`。
- Create: `frontend/src/store/ui/appUiStore.ts`、`appUiStore.test.ts`。
- Test: `router/AppRouter.test.tsx`、`app/updates/AppUpdateDialog.test.tsx`、`hooks/settings/useSettingsPanelController.test.tsx`。
- Consumes: 既有 `SettingsPanelId`、`AppUpdateDialogMode`；A-F06设置仍走Query mutation。
- Produces（本卡创建）:

```ts
export interface AppUiState {
  settingsPanel: SettingsPanelId | null;
  logViewerOpen: boolean;
  updateDialogMode: "intro" | "update" | null;
  openSettings(panel: SettingsPanelId): void;
  closeSettings(): void;
  setLogViewerOpen(open: boolean): void;
  openUpdateDialog(mode: "intro" | "update"): void;
  closeUpdateDialog(): void;
}
// 用 create<AppUiState>() 创建，保持库返回类型。
export const useAppUiStore: UseBoundStore<StoreApi<AppUiState>>;
```

store初值三个字段分别 `null/false/null`，actions只修改对应字段。持久设置、任务snapshot、asset数组、Tauri Update句柄、Team会话merge不进入store。设置面板内部collapsed groups仍是局部UI，没跨面板需求就保留原hook。

## Red 与关键实现

测试 `appUiStore.test.ts` 无React fixture：

```ts
import { afterEach, expect, it } from "vitest";
import { useAppUiStore } from "./appUiStore";
afterEach(() => useAppUiStore.setState(useAppUiStore.getInitialState(), true));

it("打开设置不改其他共享UI状态", () => {
  useAppUiStore.getState().setLogViewerOpen(true);
  useAppUiStore.getState().openSettings("general.appearance");
  expect(useAppUiStore.getState().settingsPanel).toBe("general.appearance");
  expect(useAppUiStore.getState().logViewerOpen).toBe(true);
  useAppUiStore.getState().closeSettings();
  expect(useAppUiStore.getState().settingsPanel).toBeNull();
});
```

实际组件用 `useAppUiStore((state) => state.settingsPanel)` 或单action selector；组合对象selector使用官方 `useShallow`，不每次制造无稳定性的新对象。更新器native handle继续在service/原资源owner，不持久化进Zustand。

## 步骤

- [ ] **Baseline**：保存设置开启到指定panel、更新dialog关闭/重开、日志viewer测试green。
- [ ] **Red**：添加store测试；加入AppRouter不再自持 `settingsOpen/logViewerOpen` useState 的接管guard。
- [ ] **Migrate**：安装库、创建store并迁移这三个真实UI消费者；删除只为转发open/close的中间props。
- [ ] **Clean**：删除旧共享UI state/effects；保留更新下载状态机、设置存储副作用与局部草稿。无需Provider包装store。
- [ ] **Verify**：下列命令通过；主题/locale等持久值未进入store。

```sh
pnpm exec vitest run --config frontend/vite.config.ts frontend/src/store/ui/appUiStore.test.ts frontend/src/router/AppRouter.test.tsx frontend/src/app/updates/AppUpdateDialog.test.tsx frontend/src/hooks/settings/useSettingsPanelController.test.tsx
pnpm typecheck
pnpm lint
```

## 验收与停止

共享UI状态唯一、selector使用明确、测试间reset；无新Zustand后端数据副本。若AppUpdate资源释放依赖Context生命周期，先保留资源owner，只迁dialog字段；不能为了减少Provider破坏native句柄关闭。

**API 来源:** [Zustand create](https://zustand.docs.pmnd.rs/apis/create)、[useShallow](https://zustand.docs.pmnd.rs/guides/prevent-rerenders-with-use-shallow)。
