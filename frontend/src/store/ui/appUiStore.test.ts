import { afterEach, describe, expect, it } from "vitest";
import { useAppUiStore } from "./appUiStore";

afterEach(() => useAppUiStore.setState(useAppUiStore.getInitialState(), true));

describe("appUiStore", () => {
  it("打开设置不改其他共享UI状态", () => {
    useAppUiStore.getState().setLogViewerOpen(true);
    useAppUiStore.getState().openSettings("general.appearance");
    expect(useAppUiStore.getState().settingsPanel).toBe("general.appearance");
    expect(useAppUiStore.getState().logViewerOpen).toBe(true);
    useAppUiStore.getState().closeSettings();
    expect(useAppUiStore.getState().settingsPanel).toBeNull();
  });

  it("打开更新弹窗不改其他共享UI状态", () => {
    useAppUiStore.getState().openSettings("general.appearance");
    useAppUiStore.getState().openUpdateDialog("intro");
    expect(useAppUiStore.getState().updateDialogMode).toBe("intro");
    expect(useAppUiStore.getState().settingsPanel).toBe("general.appearance");
    useAppUiStore.getState().closeUpdateDialog();
    expect(useAppUiStore.getState().updateDialogMode).toBeNull();
  });

  it("切换日志查看器状态", () => {
    useAppUiStore.getState().setLogViewerOpen(true);
    expect(useAppUiStore.getState().logViewerOpen).toBe(true);
    useAppUiStore.getState().setLogViewerOpen(false);
    expect(useAppUiStore.getState().logViewerOpen).toBe(false);
  });

  it("AppRouter 不再自持 settingsOpen / settingsPanel / logViewerOpen useState", async () => {
    const { readFileSync } = await import("node:fs");
    const source = readFileSync(
      new URL("../../router/AppRouter.tsx", import.meta.url),
      "utf8",
    );
    expect(source).not.toMatch(/useState.*logViewerOpen/);
    expect(source).not.toMatch(/useState.*settingsOpen/);
    expect(source).not.toContain("useState<SettingsPanelId>");
  });
});
