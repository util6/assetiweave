/* @vitest-environment jsdom */
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { I18nProvider } from "../../i18n/I18nProvider";
import type { Asset } from "../../types";
import { SkillGroupCreateDialog } from "./SkillGroupCreateDialog";

describe("SkillGroupCreateDialog", () => {
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

  afterEach(cleanup);

  it("空名称阻止提交并显示错误提示", async () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    render(
      <I18nProvider>
        <SkillGroupCreateDialog
          assets={[]}
          busy={false}
          nextSortOrder={1}
          onClose={vi.fn()}
          onSubmit={onSubmit}
          open={true}
        />
      </I18nProvider>,
    );

    const dialog = screen.getByRole("dialog", { hidden: true });
    const form = dialog.querySelector("form")!;
    fireEvent.submit(form);

    await waitFor(() => {
      expect(
        screen.getByText("请输入分组名称。"),
      ).toBeTruthy();
    });
    expect(onSubmit).not.toHaveBeenCalled();
  });

  it("输入合法名称和选择资产后成功提交", async () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    const onClose = vi.fn();
    const assets: Asset[] = [mockAsset("skill-1", "Test Skill")];

    render(
      <I18nProvider>
        <SkillGroupCreateDialog
          assets={assets}
          busy={false}
          nextSortOrder={5}
          onClose={onClose}
          onSubmit={onSubmit}
          open={true}
        />
      </I18nProvider>,
    );

    const dialog = screen.getByRole("dialog", { hidden: true });
    const nameInput = dialog.querySelector('input[name="name"]') as HTMLInputElement;
    expect(nameInput).toBeTruthy();
    fireEvent.change(nameInput, { target: { value: "Engineering Skills" } });

    // 选择资产
    const assetCheckbox = screen.getByLabelText(/Test Skill/i);
    fireEvent.click(assetCheckbox);

    const form = dialog.querySelector("form")!;
    fireEvent.submit(form);

    await waitFor(() => {
      expect(onSubmit).toHaveBeenCalledWith(
        expect.objectContaining({
          name: "Engineering Skills",
          sort_order: 5,
          enabled: true,
        }),
        ["skill-1"],
      );
    });
  });

  it("SkillGroupCreateDialog 和 SkillGroupEditDialog 均使用 react-hook-form 且无手写 formError state", () => {
    const createSource = readFileSync(
      resolve(__dirname, "SkillGroupCreateDialog.tsx"),
      "utf-8",
    );
    const editSource = readFileSync(
      resolve(__dirname, "SkillGroupEditDialog.tsx"),
      "utf-8",
    );

    expect(createSource).toContain("react-hook-form");
    expect(createSource).not.toContain("setFormError");
    expect(createSource).not.toContain("formError");

    expect(editSource).toContain("react-hook-form");
    expect(editSource).not.toContain("setFormError");
    expect(editSource).not.toContain("formError");
  });
});

function mockAsset(id: string, name?: string): Asset {
  return {
    id,
    source_id: "source-a",
    name: name ?? id,
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
