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

  it("abbreviates home paths and omits native title on ReadonlyRow values", () => {
    const html = renderToStaticMarkup(
      <I18nProvider>
        <AssetEditDialog
          asset={{
            ...skillAsset,
            stored_path: "/Users/alice/skills/skill-a",
          }}
          busy={false}
          groups={[]}
          mountStatuses={[]}
          onBackup={vi.fn()}
          onClose={vi.fn()}
          onSetGroupMembership={vi.fn()}
          onSubmit={vi.fn()}
          onToggleMount={vi.fn()}
          profiles={[]}
          source={{
            id: "source-a",
            name: "source-a",
            kind: "local",
            root_path: "/Users/alice/skills",
            scanner_kind: "skill",
            source_origin: "local_folder",
            repo_root: null,
            scan_root: "",
            origin_app_kind: null,
            include_globs: ["**/SKILL.md"],
            exclude_globs: [],
            default_kind: "skill",
            enabled: true,
            priority: 0,
            last_scanned_at: null,
            last_scan_status: null,
          }}
        />
      </I18nProvider>,
    );

    expect(html).toContain("~/skills/skill-a");
    expect(html).not.toContain('title="~/skills/skill-a"');
    expect(html).not.toContain('title="source-a"');
    expect(html).not.toContain("/Users/alice/skills/skill-a");
  });
});

const skillAsset: Asset = {
  id: "skill-a",
  source_id: "source-a",
  name: "skill-a",
  kind: "skill",
  format: "directory",
  relative_path: "skill-a",
  stored_path: "/tmp/source-a/skill-a",
  entry_file: null,
  description: null,
  content_hash: null,
  discovered_at: "2026-01-01T00:00:00Z",
  updated_at: "2026-01-01T00:00:00Z",
};
