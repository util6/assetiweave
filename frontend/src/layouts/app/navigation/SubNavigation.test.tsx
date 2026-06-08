import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi, beforeEach } from "vitest";
import { I18nProvider } from "../../../i18n/I18nProvider";
import { SubNavigation } from "./SubNavigation";

describe("SubNavigation", () => {
  beforeEach(() => {
    vi.stubGlobal("localStorage", {
      getItem: () => "zh",
      setItem: vi.fn(),
    });
    vi.stubGlobal("navigator", { language: "zh-CN" });
  });

  it("renders enabled sub navigation tabs without page-level actions", () => {
    const html = renderToStaticMarkup(
      <I18nProvider>
        <SubNavigation
          activeId="overview"
          items={[
            { id: "overview", label: "Catalog Overview", routeKey: "skills.overview", enabled: true },
            { id: "sources", label: "Skill Sources", routeKey: "skills.sources", enabled: true },
          ]}
          onSelect={vi.fn()}
        />
      </I18nProvider>,
    );

    expect(html).toContain("目录总览");
    expect(html).toContain("技能源管理");
    expect(html).not.toContain("打开当前页面使用手册");
  });
});
