import { renderToStaticMarkup } from "react-dom/server";
import { Layers3 } from "lucide-react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { I18nProvider } from "../../i18n/I18nProvider";
import { ManualHelpButton } from "../../manuals/ManualHelpButton";
import { PageHeader } from "./PageHeader";

describe("PageHeader", () => {
  beforeEach(() => {
    vi.stubGlobal("localStorage", {
      getItem: () => "zh",
      setItem: vi.fn(),
    });
    vi.stubGlobal("navigator", { language: "zh-CN" });
  });

  it("renders the manual help action in the title row", () => {
    const html = renderToStaticMarkup(
      <I18nProvider>
        <PageHeader
          eyebrow="场景分组"
          icon={<Layers3 size={21} />}
          title="分组管理"
          titleAction={<ManualHelpButton onOpen={vi.fn()} />}
        />
      </I18nProvider>,
    );

    expect(html).toContain("场景分组");
    expect(html).toContain("分组管理");
    expect(html).toContain('aria-label="打开当前页面使用手册"');
    expect(html.indexOf("分组管理")).toBeLessThan(html.indexOf("打开当前页面使用手册"));
  });

  it("keeps the header in a single horizontal row when width is constrained", () => {
    const html = renderToStaticMarkup(
      <I18nProvider>
        <PageHeader
          actions={<div>来源 4</div>}
          eyebrow="资产目录"
          icon={<Layers3 size={21} />}
          title="目录总览"
          titleAction={<ManualHelpButton onOpen={vi.fn()} />}
        />
      </I18nProvider>,
    );

    expect(html).toContain("flex-nowrap");
    expect(html).toContain("overflow-hidden");
    expect(html).toContain("truncate");
    expect(html).toContain("whitespace-nowrap");
    expect(html).not.toContain("max-[920px]:flex-col");
    expect(html).not.toContain("w-full max-w-3xl shrink-0");
  });
});
