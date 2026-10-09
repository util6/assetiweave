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

vi.mock("../../services/catalog", () => ({
  probeSourcePath: vi.fn().mockImplementation(async (path: string) => {
    if (path.includes("skills-fork")) {
      return {
        candidates: [
          {
            is_git_repo: true,
            name: "claude-skills",
            path: `${path}/claude-skills`,
            relative_path: "claude-skills",
            remote_url: "https://github.com/foo/claude-skills.git",
            skill_count: 3,
          },
          {
            is_git_repo: true,
            name: "drawio-skill",
            path: `${path}/drawio-skill`,
            relative_path: "drawio-skill",
            remote_url: null,
            skill_count: 1,
          },
        ],
        default_source_origin: "local_folder",
        is_container: true,
        root_path: path,
      };
    }
    return {
      candidates: [],
      default_source_origin: "git_repo",
      is_container: false,
      root_path: path,
    };
  }),
}));

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
      expect(screen.getByText("请输入源目录路径。")).toBeTruthy();
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
          priority: 10,
          root_path: "/tmp/my-skills",
        }),
      );
      expect(onClose).toHaveBeenCalled();
    });
  });

  it("探测到多Git仓库容器目录时展示候选仓库并支持批量拆分导入", async () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    const onClose = vi.fn();
    render(
      <I18nProvider>
        <SourceImportDialog
          busy={false}
          onClose={onClose}
          onNotifyError={vi.fn()}
          onPickRootPath={vi.fn().mockResolvedValue("/home/user/skills-fork")}
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
    fireEvent.change(input, { target: { value: "/home/user/skills-fork" } });

    await waitFor(() => {
      expect(screen.getByText("检测到多 Git 仓库容器目录")).toBeTruthy();
      expect(screen.getAllByText("claude-skills").length).toBeGreaterThan(0);
      expect(screen.getAllByText("drawio-skill").length).toBeGreaterThan(0);
    });

    const form = dialog.querySelector("form")!;
    fireEvent.submit(form);

    await waitFor(() => {
      expect(onSubmit).toHaveBeenCalledWith([
        expect.objectContaining({
          name: "claude-skills",
          priority: 10,
          root_path: "/home/user/skills-fork/claude-skills",
          source_origin: "git_repo",
        }),
        expect.objectContaining({
          name: "drawio-skill",
          priority: 11,
          root_path: "/home/user/skills-fork/drawio-skill",
          source_origin: "git_repo",
        }),
      ]);
      expect(onClose).toHaveBeenCalled();
    });
  });

  it("探测到多Git仓库后可切换为单一本地目录导入", async () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    const onClose = vi.fn();
    render(
      <I18nProvider>
        <SourceImportDialog
          busy={false}
          onClose={onClose}
          onNotifyError={vi.fn()}
          onPickRootPath={vi.fn()}
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
    fireEvent.change(input, { target: { value: "/home/user/skills-fork" } });

    await waitFor(() => {
      expect(screen.getByText("作为单一本地目录整体导入")).toBeTruthy();
    });

    const singleModeBtn = screen.getByText("作为单一本地目录整体导入");
    fireEvent.click(singleModeBtn);

    const form = dialog.querySelector("form")!;
    fireEvent.submit(form);

    await waitFor(() => {
      expect(onSubmit).toHaveBeenCalledWith(
        expect.objectContaining({
          priority: 10,
          root_path: "/home/user/skills-fork",
          source_origin: "local_folder",
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
