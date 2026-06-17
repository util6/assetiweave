import { renderToStaticMarkup } from "react-dom/server";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { I18nProvider } from "../../i18n/I18nProvider";
import type { Source } from "../../types";
import { SourceEditDialog } from "./SourceEditDialog";

describe("SourceEditDialog", () => {
  beforeEach(() => {
    vi.stubGlobal("localStorage", {
      getItem: () => "zh",
      setItem: vi.fn(),
    });
    vi.stubGlobal("navigator", { language: "zh-CN" });
  });

  it("offers a batch backup action for unbacked source assets", () => {
    const html = renderToStaticMarkup(
      <I18nProvider>
        <SourceEditDialog
          backupAssetCount={3}
          busy={false}
          onBackup={vi.fn()}
          onClose={vi.fn()}
          onNotifyError={vi.fn()}
          onPickRootPath={vi.fn()}
          onSubmit={vi.fn()}
          source={source}
        />
      </I18nProvider>,
    );

    expect(html).toContain("备份到目录 (3)");
  });
});

const source: Source = {
  id: "source-a",
  name: "skills",
  kind: "local",
  root_path: "/tmp/skills",
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
