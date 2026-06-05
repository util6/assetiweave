import { renderToStaticMarkup } from "react-dom/server";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { I18nProvider } from "../../i18n/I18nProvider";
import { SkillBackupDirectorySetting } from "./SkillBackupDirectorySetting";

describe("SkillBackupDirectorySetting", () => {
  beforeEach(() => {
    vi.stubGlobal("localStorage", {
      getItem: () => "zh",
      setItem: vi.fn(),
    });
    vi.stubGlobal("navigator", { language: "zh-CN" });
  });

  it("shows the configured backup directory and a change action", () => {
    const html = renderToStaticMarkup(
      <I18nProvider>
        <SkillBackupDirectorySetting
          onOpen={vi.fn()}
          rootPath="/Volumes/Assets/skills"
        />
      </I18nProvider>,
    );

    expect(html).toContain("Skill 备份目录");
    expect(html).toContain("/Volumes/Assets/skills");
    expect(html).toContain("更改目录");
  });
});
