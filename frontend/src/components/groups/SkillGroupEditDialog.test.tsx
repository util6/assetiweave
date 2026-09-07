/* @vitest-environment jsdom */

import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { renderToStaticMarkup } from "react-dom/server";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { I18nProvider } from "../../i18n/I18nProvider";
import type { Asset, AssetGroupDetail } from "../../types";
import { SkillGroupEditDialog } from "./SkillGroupEditDialog";

describe("SkillGroupEditDialog", () => {
  beforeEach(() => {
    vi.stubGlobal("localStorage", {
      getItem: () => "zh",
      setItem: vi.fn(),
    });
    vi.stubGlobal("navigator", { language: "zh-CN" });
    vi.stubGlobal(
      "ResizeObserver",
      class {
        disconnect() {}
        observe() {}
        unobserve() {}
      },
    );
  });

  it("offers a batch backup action for unbacked group members", () => {
    const html = renderToStaticMarkup(
      <I18nProvider>
        <SkillGroupEditDialog
          assets={[asset("skill-a"), asset("skill-b")]}
          busy={false}
          detail={groupDetail}
          onBackup={vi.fn()}
          onClose={vi.fn()}
          onSubmit={vi.fn()}
        />
      </I18nProvider>,
    );

    expect(html).toContain("备份到目录 (2)");
  });

  it("backs up the currently selected draft members", () => {
    const onBackup = vi.fn();
    render(
      <I18nProvider>
        <SkillGroupEditDialog
          assets={[asset("skill-a"), asset("skill-b"), asset("skill-c")]}
          busy={false}
          detail={{
            ...groupDetail,
            manual_asset_ids: ["skill-a"],
            members: [{ asset_id: "skill-a", origin: "manual" }],
          }}
          onBackup={onBackup}
          onClose={vi.fn()}
          onSubmit={vi.fn()}
        />
      </I18nProvider>,
    );

    expect(
      screen.getByRole<HTMLButtonElement>("button", { name: "备份到目录 (1)" })
        .disabled,
    ).toBe(false);

    const skillBRow = screen.getByText("skill-b").closest("label");
    expect(skillBRow).not.toBeNull();
    fireEvent.click(skillBRow!.querySelector("input")!);
    fireEvent.click(screen.getByRole("button", { name: "备份到目录 (2)" }));

    expect(onBackup).toHaveBeenCalledWith(["skill-a", "skill-b"]);
  });

  it("提交时保留 manual_asset_ids，rule-only 成员不混入 onSubmit 第二参数", async () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    render(
      <I18nProvider>
        <SkillGroupEditDialog
          assets={[asset("skill-a"), asset("skill-rule-only")]}
          busy={false}
          detail={{
            ...groupDetail,
            manual_asset_ids: ["skill-a"],
            members: [
              { asset_id: "skill-a", origin: "manual" },
              { asset_id: "skill-rule-only", origin: "rule" },
            ],
          }}
          onBackup={vi.fn()}
          onClose={vi.fn()}
          onSubmit={onSubmit}
        />
      </I18nProvider>,
    );

    const dialog = screen.getByRole("dialog", { hidden: true });
    const form = dialog.querySelector("form")!;
    fireEvent.submit(form);

    await waitFor(() => {
      expect(onSubmit).toHaveBeenCalledWith(
        expect.objectContaining({
          id: "group-a",
          name: "Review",
        }),
        ["skill-a"],
      );
    });
    expect(onSubmit.mock.calls[0][1]).not.toContain("skill-rule-only");
  });
});

afterEach(() => {
  cleanup();
});

const groupDetail: AssetGroupDetail = {
  group: {
    id: "group-a",
    name: "Review",
    description: null,
    color: "#10b981",
    asset_kind: "skill",
    display_icon: null,
    icon_svg: null,
    enabled: true,
    sort_order: 0,
    rules: { source_ids: [], relative_path_globs: [], name_contains: null },
    created_at: "2026-01-01T00:00:00Z",
    updated_at: "2026-01-01T00:00:00Z",
  },
  manual_asset_ids: ["skill-a", "skill-b"],
  members: [
    { asset_id: "skill-a", origin: "manual" },
    { asset_id: "skill-b", origin: "manual" },
  ],
};

function asset(id: string): Asset {
  return {
    id,
    source_id: "source-a",
    name: id,
    kind: "skill",
    format: "directory",
    relative_path: id,
    absolute_path: `/tmp/${id}`,
    entry_file: null,
    description: null,
    content_hash: null,
    discovered_at: "2026-01-01T00:00:00Z",
    updated_at: "2026-01-01T00:00:00Z",
  };
}
