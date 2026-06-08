import { renderToStaticMarkup } from "react-dom/server";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { I18nProvider } from "../../i18n/I18nProvider";
import { SkillAcquireDialog } from "./SkillAcquireDialog";

describe("SkillAcquireDialog", () => {
  beforeEach(() => {
    vi.stubGlobal("localStorage", {
      getItem: () => "zh",
      setItem: vi.fn(),
    });
    vi.stubGlobal("navigator", { language: "zh-CN" });
  });

  it("renders search and import controls", () => {
    const html = renderToStaticMarkup(
      <I18nProvider>
        <SkillAcquireDialog
          onAcquired={async () => undefined}
          onClose={vi.fn()}
          onNotifyError={vi.fn()}
          open
        />
      </I18nProvider>,
    );

    expect(html).toContain("搜索并导入 Skill");
    expect(html).toContain("GitHub URL");
    expect(html).toContain("预览计划");
    expect(html).toContain("远程 Skill 安全提示");
    expect(html).toContain("不会自动执行或信任远程代码");
    expect(html).toContain("导入");
  });
});
