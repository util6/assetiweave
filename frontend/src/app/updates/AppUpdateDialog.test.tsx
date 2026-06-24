import { renderToStaticMarkup } from "react-dom/server";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { I18nProvider } from "../../i18n/I18nProvider";
import { AppUpdateDialog } from "./AppUpdateDialog";

vi.mock("./AppUpdateProvider", () => ({
  useAppUpdater: () => ({
    checkForUpdates: vi.fn(),
    closeDialog: vi.fn(),
    dialogMode: "intro",
    dialogOpen: true,
    downloadAndInstall: vi.fn(),
    openReleases: vi.fn(),
    restartApp: vi.fn(),
    state: {
      currentVersion: "0.2.0",
      info: null,
      progress: 0,
      source: null,
      status: "idle",
      supported: true,
    },
  }),
}));

describe("AppUpdateDialog", () => {
  beforeEach(() => {
    vi.stubGlobal("localStorage", {
      getItem: () => "zh",
      setItem: vi.fn(),
    });
    vi.stubGlobal("navigator", { language: "zh-CN" });
  });

  it("shows current version highlights with an acknowledge action in intro mode", () => {
    const html = renderToStaticMarkup(
      <I18nProvider>
        <AppUpdateDialog />
      </I18nProvider>,
    );

    expect(html).toContain("当前版本功能介绍");
    expect(html).toContain("v0.2.0");
    expect(html).toContain("我知道了");
    expect(html).not.toContain("下载并安装");
    expect(html).not.toContain("再次检查");
  });
});
