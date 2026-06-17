import { renderToStaticMarkup } from "react-dom/server";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { I18nProvider } from "../../i18n/I18nProvider";
import type { Asset, AssetMountStatus, Source } from "../../types";
import { SourceRow } from "./SourceRow";

describe("SourceRow", () => {
  beforeEach(() => {
    vi.stubGlobal("localStorage", {
      getItem: () => "zh",
      setItem: vi.fn(),
    });
    vi.stubGlobal("navigator", { language: "zh-CN" });
  });

  it("shows asset edit and delete buttons inside an expanded source", () => {
    const html = renderToStaticMarkup(
      <I18nProvider>
        <SourceRow
          appShortcuts={[]}
          assets={[asset]}
          busy={false}
          expanded
          expandedAssetIds={new Set()}
          mountStatusesByAssetId={new Map<string, AssetMountStatus[]>()}
          onAssetReveal={vi.fn()}
          onDelete={vi.fn()}
          onDeleteAsset={vi.fn()}
          onEdit={vi.fn()}
          onEditAsset={vi.fn()}
          onReveal={vi.fn()}
          onSetSourceMountProfile={vi.fn()}
          onToggleAsset={vi.fn()}
          onToggleExpanded={vi.fn()}
          onToggleMount={vi.fn()}
          profiles={[]}
          source={source}
        />
      </I18nProvider>,
    );

    expect(html).toContain('aria-label="编辑资产"');
    expect(html).toContain('aria-label="删除资产"');
  });
});

const asset: Asset = {
  id: "asset-id",
  source_id: "source-id",
  name: "review-workflow",
  kind: "skill",
  format: "directory",
  relative_path: "review-workflow",
  absolute_path: "/Users/util6/code-space/skills/review-workflow",
  entry_file: null,
  description: null,
  content_hash: null,
  discovered_at: "2026-01-01T00:00:00Z",
  updated_at: "2026-01-01T00:00:00Z",
};

const source: Source = {
  id: "source-id",
  name: "skills",
  kind: "local",
  root_path: "/Users/util6/code-space/skills",
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
};
