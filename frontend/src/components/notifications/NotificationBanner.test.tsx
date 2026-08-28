import { renderToStaticMarkup } from "react-dom/server";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { I18nProvider } from "../../i18n/I18nProvider";
import { NotificationBanner } from "./NotificationBanner";

describe("NotificationBanner", () => {
  beforeEach(() => {
    vi.stubGlobal("localStorage", {
      getItem: () => "zh",
      setItem: vi.fn(),
    });
    vi.stubGlobal("navigator", { language: "zh-CN" });
  });

  it("renders as an out-of-flow top-right overlay", () => {
    const html = renderToStaticMarkup(
      <I18nProvider>
        <NotificationBanner
          notification={{ id: "notice-1", tone: "success", message: "已完成" }}
          onDismiss={vi.fn()}
        />
      </I18nProvider>,
    );

    expect(html).toContain("absolute");
    expect(html).toContain("inset-x-0");
    expect(html).toContain("top-[calc(var(--app-toolbar-top)-var(--app-window-titlebar-height))]");
    expect(html).toContain("pointer-events-none");
    expect(html).toContain("pointer-events-auto");
    expect(html).not.toContain("sticky");
    expect(html).not.toContain("app-notification-offset");
  });

  it("keeps long error details readable instead of truncating them to one line", () => {
    const html = renderToStaticMarkup(
      <I18nProvider>
        <NotificationBanner
          notification={{
            id: "notice-long-error",
            tone: "error",
            message: "The selected AI model is currently unavailable. Choose another model in Agent settings.",
          }}
          onDismiss={vi.fn()}
        />
      </I18nProvider>,
    );

    expect(html).toContain("whitespace-pre-wrap");
    expect(html).toContain("break-words");
    expect(html).not.toContain("whitespace-nowrap");
    expect(html).not.toContain("text-ellipsis");
  });
});
