import { renderToStaticMarkup } from "react-dom/server";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { I18nProvider } from "../i18n/I18nProvider";
import { ManualPage } from "./ManualPage";

describe("ManualPage", () => {
  beforeEach(() => {
    vi.stubGlobal("localStorage", {
      getItem: () => "zh",
      setItem: vi.fn(),
    });
    vi.stubGlobal("navigator", { language: "zh-CN" });
  });

  it("renders a searchable route-specific guide with sections and checklist items", () => {
    const html = renderToStaticMarkup(
      <I18nProvider>
        <ManualPage routeKey="skills.groups" onBack={vi.fn()} />
      </I18nProvider>,
    );

    expect(html).toContain("分组管理使用手册");
    expect(html).toContain("搜索本页手册");
    expect(html).toContain("skills.groups");
    expect(html).toContain("创建和维护分组");
    expect(html).toContain("批量挂载");
    expect(html).toContain("点击新建分组");
  });
});
