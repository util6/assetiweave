import { renderToStaticMarkup } from "react-dom/server";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { I18nProvider } from "../../i18n/I18nProvider";
import type { AppShortcut, SkillGroupExclusiveMountPreview } from "../../types";
import { SkillGroupExclusiveMountDialog } from "./SkillGroupExclusiveMountDialog";

describe("SkillGroupExclusiveMountDialog", () => {
  beforeEach(() => {
    vi.stubGlobal("localStorage", {
      getItem: () => "zh",
      setItem: vi.fn(),
    });
    vi.stubGlobal("navigator", { language: "zh-CN" });
  });

  it("renders all detail sections collapsed by default", () => {
    const html = renderToStaticMarkup(
      <I18nProvider>
        <SkillGroupExclusiveMountDialog
          busy={false}
          mode="exclusive"
          onClose={() => undefined}
          onConfirm={() => undefined}
          preview={preview}
          shortcut={shortcut}
        />
      </I18nProvider>,
    );

    const detailTags = html.match(/<details\b[^>]*>/g) ?? [];
    expect(detailTags).toHaveLength(4);
    expect(detailTags.every((tag) => !/\sopen(=|\s|>)/.test(tag))).toBe(true);
  });
});

const shortcut: AppShortcut = {
  profileId: "codex",
  profileName: "Codex",
  appKind: "codex",
  displayIcon: "app:codex",
  accentColor: "#10b981",
  enabled: true,
};

const preview: SkillGroupExclusiveMountPreview = {
  profile_id: "codex",
  group_ids: ["frontend"],
  selected_skill_ids: ["skill-a", "skill-b"],
  keep: [{ asset_id: "skill-a", name: "skill-a" }],
  mount: [{ asset_id: "skill-b", name: "skill-b" }],
  unmount: [{ asset_id: "skill-c", name: "skill-c" }],
  skipped: [{ asset_id: "skill-d", name: "skill-d", reason: "risk" }],
  keep_count: 1,
  mount_count: 1,
  unmount_count: 1,
  skipped_count: 1,
};
