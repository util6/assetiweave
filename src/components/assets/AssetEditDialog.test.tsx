import { renderToStaticMarkup } from "react-dom/server";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { I18nProvider } from "../../i18n/I18nProvider";
import type { Asset } from "../../types";
import { AssetEditDialog } from "./AssetEditDialog";

describe("AssetEditDialog", () => {
  beforeEach(() => {
    vi.stubGlobal("localStorage", {
      getItem: () => "zh",
      setItem: vi.fn(),
    });
    vi.stubGlobal("navigator", { language: "zh-CN" });
  });

  it("offers a backup-to-directory action for a skill", () => {
    const html = renderToStaticMarkup(
      <I18nProvider>
        <AssetEditDialog
          asset={skillAsset}
          busy={false}
          groups={[]}
          mountStatuses={[]}
          onBackup={vi.fn()}
          onClose={vi.fn()}
          onSetGroupMembership={vi.fn()}
          onSubmit={vi.fn()}
          onToggleMount={vi.fn()}
          profiles={[]}
        />
      </I18nProvider>,
    );

    expect(html).toContain("备份到目录");
  });
});

const skillAsset: Asset = {
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
