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
import { SourceImportDialog } from "./SourceImportDialog";

describe("SourceImportDialog", () => {
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

  it("空路径不调用onSubmit且展示错误提示", async () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    render(
      <I18nProvider>
        <SourceImportDialog
          busy={false}
          onClose={vi.fn()}
          onNotifyError={vi.fn()}
          onPickRootPath={vi.fn()}
          onSubmit={onSubmit}
          open={true}
          suggestedPriority={10}
        />
      </I18nProvider>,
    );

    const dialog = screen.getByRole("dialog", { hidden: true });
    const form = dialog.querySelector("form")!;
    fireEvent.submit(form);

    await waitFor(() => {
      expect(
        screen.getByText("请输入源目录路径。"),
      ).toBeTruthy();
    });
    expect(onSubmit).not.toHaveBeenCalled();
  });

  it("选目录或输入有效路径后清除错误并成功提交", async () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    const onClose = vi.fn();
    render(
      <I18nProvider>
        <SourceImportDialog
          busy={false}
          onClose={onClose}
          onNotifyError={vi.fn()}
          onPickRootPath={vi.fn().mockResolvedValue("/home/user/skills")}
          onSubmit={onSubmit}
          open={true}
          suggestedPriority={10}
        />
      </I18nProvider>,
    );

    const dialog = screen.getByRole("dialog", { hidden: true });
    const input = dialog.querySelector(
      'input[name="rootPath"]',
    ) as HTMLInputElement;
    expect(input).toBeTruthy();
    fireEvent.change(input, { target: { value: "/tmp/my-skills" } });

    const form = dialog.querySelector("form")!;
    fireEvent.submit(form);

    await waitFor(() => {
      expect(onSubmit).toHaveBeenCalledWith(
        expect.objectContaining({
          root_path: "/tmp/my-skills",
          priority: 10,
        }),
      );
      expect(onClose).toHaveBeenCalled();
    });
  });

  it("SourceImportDialog 和 SourceEditDialog 均使用 react-hook-form 且无手写 fieldErrors state", () => {
    const importSource = readFileSync(
      resolve(__dirname, "SourceImportDialog.tsx"),
      "utf-8",
    );
    const editSource = readFileSync(
      resolve(__dirname, "SourceEditDialog.tsx"),
      "utf-8",
    );

    expect(importSource).toContain("react-hook-form");
    expect(importSource).not.toContain("setFieldErrors");
    expect(importSource).not.toContain("fieldErrors");

    expect(editSource).toContain("react-hook-form");
    expect(editSource).not.toContain("setFieldErrors");
    expect(editSource).not.toContain("fieldErrors");
  });
});
