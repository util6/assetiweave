import { renderToStaticMarkup } from "react-dom/server";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { I18nProvider } from "../../../i18n/I18nProvider";
import type { HeaderTabItem, RailMenuItem } from "../../../router/types";
import { SideRail } from "./SideRail";

const headerTabs: HeaderTabItem[] = [
  { id: "skills", label: "Skills", assetKind: "skill", enabled: true },
  { id: "mcp", label: "MCP", assetKind: "mcp", enabled: true },
];

const railItems: RailMenuItem[] = [
  { id: "logs", label: "Logs", icon: "file-text", scope: "global", enabled: true, position: "secondary" },
  { id: "settings", label: "Settings", icon: "settings", scope: "settings", enabled: true, position: "secondary" },
];

describe("SideRail", () => {
  beforeEach(() => {
    vi.stubGlobal("localStorage", {
      getItem: () => "zh",
      setItem: vi.fn(),
    });
    vi.stubGlobal("navigator", { language: "zh-CN" });
  });

  it("starts in collapsed mode with an explicit expand control", () => {
    const html = renderSideRail(false);

    expect(html).toContain('data-expanded="false"');
    expect(html).toContain('aria-expanded="false"');
    expect(html).toContain("展开侧边栏");
    expect(html).not.toContain("data-side-rail-label");
  });

  it("renders navigation labels when expanded", () => {
    const html = renderSideRail(true);

    expect(html).toContain('data-expanded="true"');
    expect(html).toContain('aria-expanded="true"');
    expect(html).toContain("收起侧边栏");
    expect(html).toContain("data-side-rail-label");
    expect(html).toContain(">技能<");
    expect(html).toContain(">日志<");
  });
});

function renderSideRail(expanded: boolean) {
  return renderToStaticMarkup(
    <I18nProvider>
      <SideRail
        activeHeaderTabId="skills"
        activeId="logs"
        expanded={expanded}
        headerTabs={headerTabs}
        items={railItems}
        onExpandedChange={vi.fn()}
        onHeaderTabSelect={vi.fn()}
        onItemSelect={vi.fn()}
      />
    </I18nProvider>,
  );
}
