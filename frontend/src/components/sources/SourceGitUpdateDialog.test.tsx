/* @vitest-environment jsdom */
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { I18nProvider } from "../../i18n/I18nProvider";
import type { GitSourcePullTaskSnapshot, Source } from "../../types";
import { SourceGitUpdateDialog } from "./SourceGitUpdateDialog";

const mockStartGitSourcePull = vi.fn();
const mockCancelGitSourcePull = vi.fn();
let mockGitSourcePullSnapshot: GitSourcePullTaskSnapshot | null = null;

vi.mock("../../app/backgroundTasks/CatalogTaskProvider", () => ({
  useCatalogTasks: () => ({
    cancelGitSourcePull: mockCancelGitSourcePull,
    gitSourcePull: mockGitSourcePullSnapshot,
    startGitSourcePull: mockStartGitSourcePull,
  }),
}));

const mockSources: Source[] = [
  {
    default_kind: "skill",
    enabled: true,
    exclude_globs: [],
    id: "source-1",
    include_globs: ["**/SKILL.md"],
    kind: "custom",
    name: "claude-skills",
    priority: 10,
    root_path: "/path/to/claude-skills",
    scan_root: "/path/to/claude-skills",
    scanner_kind: "skill",
    source_origin: "git_repo",
  },
  {
    default_kind: "skill",
    enabled: true,
    exclude_globs: [],
    id: "source-2",
    include_globs: ["**/SKILL.md"],
    kind: "git_checkout",
    name: "drawio-skill",
    priority: 10,
    root_path: "/path/to/drawio-skill",
    scan_root: "/path/to/drawio-skill",
    scanner_kind: "skill",
    source_origin: "local_folder",
  },
  {
    default_kind: "skill",
    enabled: true,
    exclude_globs: [],
    id: "source-3",
    include_globs: ["**/SKILL.md"],
    kind: "local",
    name: "local-folder",
    priority: 10,
    root_path: "/path/to/local-folder",
    scan_root: "/path/to/local-folder",
    scanner_kind: "skill",
    source_origin: "local_folder",
  },
];

describe("SourceGitUpdateDialog", () => {
  beforeEach(() => {
    vi.stubGlobal("localStorage", {
      getItem: () => "zh",
      setItem: vi.fn(),
    });
    vi.stubGlobal("navigator", { language: "zh-CN" });
    mockStartGitSourcePull.mockReset();
    mockCancelGitSourcePull.mockReset();
    mockGitSourcePullSnapshot = null;
  });

  afterEach(() => {
    cleanup();
  });

  it("当无 Git 仓库来源时展示空状态", () => {
    render(
      <I18nProvider>
        <SourceGitUpdateDialog
          onClose={vi.fn()}
          onNotifyError={vi.fn()}
          open
          sources={[mockSources[2]]} // only local folder
        />
      </I18nProvider>,
    );

    expect(
      screen.getAllByText("当前没有来源于 Git 仓库的技能源。").length,
    ).toBeGreaterThan(0);
  });

  it("仅展示 Git 来源并支持单选、多选与全选", async () => {
    render(
      <I18nProvider>
        <SourceGitUpdateDialog
          onClose={vi.fn()}
          onNotifyError={vi.fn()}
          open
          sources={mockSources}
        />
      </I18nProvider>,
    );

    // Should contain claude-skills and drawio-skill, but NOT local-folder
    expect(screen.getByText("claude-skills")).toBeTruthy();
    expect(screen.getByText("drawio-skill")).toBeTruthy();
    expect(screen.queryByText("local-folder")).toBeNull();

    // Default to select all (2 selected)
    expect(screen.getByText("更新所选 (2)")).toBeTruthy();

    // Click "取消全选"
    const toggleButton = screen.getByText("取消全选");
    fireEvent.click(toggleButton);

    expect(screen.getByText("更新所选 (0)")).toBeTruthy();
    expect(screen.getByText("全选")).toBeTruthy();

    // Click on claude-skills row checkbox
    const checkboxes = screen.getAllByRole("checkbox");
    fireEvent.click(checkboxes[0]);

    expect(screen.getByText("更新所选 (1)")).toBeTruthy();
  });

  it("点击一键全部更新触发 startGitSourcePull", async () => {
    mockStartGitSourcePull.mockResolvedValueOnce({
      id: "task-1",
      status: "running",
      progress: { completed: 0, phase: "pulling", total: 2 },
      started_at: "2026-01-01T00:00:00Z",
      results: [],
    });

    render(
      <I18nProvider>
        <SourceGitUpdateDialog
          onClose={vi.fn()}
          onNotifyError={vi.fn()}
          open
          sources={mockSources}
        />
      </I18nProvider>,
    );

    const pullAllButton = screen.getByText("一键全部更新");
    fireEvent.click(pullAllButton);

    await waitFor(() => {
      expect(mockStartGitSourcePull).toHaveBeenCalledWith([
        "source-1",
        "source-2",
      ]);
    });
  });

  it("显示运行中任务进度与结果状态徽章", () => {
    mockGitSourcePullSnapshot = {
      id: "task-test",
      status: "running",
      progress: {
        completed: 1,
        current_source_name: "drawio-skill",
        phase: "pulling",
        total: 2,
      },
      started_at: "2026-01-01T00:00:00Z",
      results: [
        {
          changed: true,
          message: "Fast-forward 3 files changed",
          source_id: "source-1",
          source_name: "claude-skills",
          success: true,
        },
      ],
    };

    render(
      <I18nProvider>
        <SourceGitUpdateDialog
          onClose={vi.fn()}
          onNotifyError={vi.fn()}
          open
          sources={mockSources}
        />
      </I18nProvider>,
    );

    // Shows progress
    expect(screen.getByText("正在更新 (1/2)...")).toBeTruthy();
    expect(screen.getByText("50%")).toBeTruthy();

    // First source is completed with changes
    expect(screen.getByText("更新成功")).toBeTruthy();
    expect(screen.getByText("Fast-forward 3 files changed")).toBeTruthy();

    // Second source is currently pulling
    expect(screen.getByText("更新中...")).toBeTruthy();
  });

  it("当多个技能源属于同一Git仓库时按仓库聚合并展示聚合徽章", async () => {
    const multiSourcesSameRepo: Source[] = [
      {
        default_kind: "skill",
        enabled: true,
        exclude_globs: [],
        id: "source-a",
        include_globs: ["**/SKILL.md"],
        kind: "custom",
        name: "Mattpocock skills (root)",
        priority: 10,
        repo_root: "~/fork-code/skills-fork/Mattpocock skills",
        root_path: "~/fork-code/skills-fork/Mattpocock skills",
        scan_root: "~/fork-code/skills-fork/Mattpocock skills",
        scanner_kind: "skill",
        source_origin: "git_repo",
      },
      {
        default_kind: "skill",
        enabled: true,
        exclude_globs: [],
        id: "source-b",
        include_globs: ["**/SKILL.md"],
        kind: "custom",
        name: "Mattpocock skills (subfolder)",
        priority: 10,
        repo_root: "~/fork-code/skills-fork/Mattpocock skills",
        root_path: "~/fork-code/skills-fork/Mattpocock skills/skills",
        scan_root: "~/fork-code/skills-fork/Mattpocock skills/skills",
        scanner_kind: "skill",
        source_origin: "git_repo",
      },
    ];

    render(
      <I18nProvider>
        <SourceGitUpdateDialog
          onClose={vi.fn()}
          onNotifyError={vi.fn()}
          open
          sources={multiSourcesSameRepo}
        />
      </I18nProvider>,
    );

    // Only 1 repository card should be rendered instead of 2
    expect(screen.getAllByRole("checkbox").length).toBe(1);
    expect(screen.getByText("包含 2 个技能源")).toBeTruthy();
    expect(screen.getByText("更新所选 (1)")).toBeTruthy();

    const updateBtn = screen.getByText("立即更新");
    fireEvent.click(updateBtn);

    await waitFor(() => {
      expect(mockStartGitSourcePull).toHaveBeenCalledWith([
        "source-a",
        "source-b",
      ]);
    });
  });
});
