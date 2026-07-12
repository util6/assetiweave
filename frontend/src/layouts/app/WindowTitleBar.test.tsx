import { renderToStaticMarkup } from "react-dom/server";
import { afterEach, describe, expect, it, vi } from "vitest";
import { detectWindowChromeMode, WindowTitleBar } from "./WindowTitleBar";

describe("WindowTitleBar", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("renders drag and interactive window controls for Windows frameless chrome", () => {
    const html = renderToStaticMarkup(<WindowTitleBar mode="windows-frameless" />);

    expect(html).toContain('data-window-chrome="windows-frameless"');
    expect(html).toContain('data-tauri-drag-region="true"');
    expect(html).toContain('aria-label="Window controls"');
    expect(html).toContain('aria-label="Minimize window"');
    expect(html).toContain('aria-label="Toggle maximize window"');
    expect(html).toContain('aria-label="Close window"');
    expect(html.indexOf('data-tauri-drag-region="true"')).toBeLessThan(html.indexOf('aria-label="Window controls"'));
  });

  it("does not add app chrome for platforms using the native title bar", () => {
    const html = renderToStaticMarkup(<WindowTitleBar mode="native" />);

    expect(html).toBe("");
  });

  it("uses native chrome on macOS", () => {
    vi.stubGlobal("navigator", {
      platform: "MacIntel",
      userAgent: "Mozilla/5.0 (Macintosh; Intel Mac OS X 14_0)",
    });

    expect(detectWindowChromeMode()).toBe("native");
  });

  it("uses frameless custom chrome on Windows", () => {
    vi.stubGlobal("navigator", {
      platform: "Win32",
      userAgent: "Mozilla/5.0 (Windows NT 10.0; Win64; x64)",
    });

    expect(detectWindowChromeMode()).toBe("windows-frameless");
  });
});
