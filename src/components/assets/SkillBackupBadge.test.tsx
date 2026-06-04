import { renderToStaticMarkup } from "react-dom/server";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { I18nProvider } from "../../i18n/I18nProvider";
import type { Asset } from "../../types";
import { SkillBackupBadge } from "./SkillBackupBadge";

describe("SkillBackupBadge", () => {
  beforeEach(() => {
    vi.stubGlobal("localStorage", {
      getItem: () => "zh",
      setItem: vi.fn(),
    });
    vi.stubGlobal("navigator", { language: "zh-CN" });
  });

  it("renders backed up and downloaded labels", () => {
    const backedUp = renderToStaticMarkup(
      <I18nProvider>
        <SkillBackupBadge asset={assetWithBackupState("backed_up")} />
      </I18nProvider>,
    );
    const downloaded = renderToStaticMarkup(
      <I18nProvider>
        <SkillBackupBadge asset={assetWithBackupState("downloaded")} />
      </I18nProvider>,
    );

    expect(backedUp).toContain("已备份");
    expect(downloaded).toContain("下载");
  });

  it("renders nothing when the asset has no backup status", () => {
    const html = renderToStaticMarkup(
      <I18nProvider>
        <SkillBackupBadge asset={{ ...baseAsset, backup_status: null }} />
      </I18nProvider>,
    );

    expect(html).toBe("");
  });
});

function assetWithBackupState(state: "backed_up" | "downloaded"): Asset {
  return {
    ...baseAsset,
    backup_status: {
      state,
      backup_path: "/tmp/backup/skill-a",
      hidden_asset_ids: [],
    },
  };
}

const baseAsset: Asset = {
  id: "skill-a",
  source_id: "source-a",
  name: "skill-a",
  kind: "skill",
  format: "directory",
  relative_path: "skill-a",
  absolute_path: "/tmp/source-a/skill-a",
  entry_file: null,
  description: null,
  content_hash: null,
  discovered_at: "2026-01-01T00:00:00Z",
  updated_at: "2026-01-01T00:00:00Z",
};
