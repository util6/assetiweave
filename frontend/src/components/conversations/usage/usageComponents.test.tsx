// @vitest-environment jsdom

import { fireEvent, render, screen, cleanup } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { UsageHeader } from "./UsageHeader";
import { UsageHeroCard } from "./UsageHeroCard";
import { UsageTrendChart } from "./UsageTrendChart";
import { UsageToolbar } from "./UsageToolbar";
import { DateRangePicker } from "./DateRangePicker";
import {
  UsageBreakdownTables,
  formatInstanceDisplayName,
  findMatchedAppShortcut,
} from "./UsageBreakdownTables";
import { formatChineseTokens } from "../../../types/usage";

describe("Usage Components", () => {
  afterEach(() => {
    cleanup();
  });
  const mockHero = {
    totalTokens: 3330000000,
    totalInputTokens: 3322000000,
    totalOutputTokens: 7900000,
    cacheReadTokens: 3218000000,
    cacheWriteTokens: 0,
    reasoningTokens: 120000,
    requestsCount: 22938,
    costsByCurrency: [{ currency: "CNY", amount: 555.69 }],
    costCoveredTokens: 3330000000,
    costCoverageRatio: 1.0,
    costCoveredRequests: 22938,
  };

  it("formats Chinese tokens properly", () => {
    expect(formatChineseTokens(3330000000)).toBe("33.30 亿");
    expect(formatChineseTokens(3322000000)).toBe("33.22 亿");
    expect(formatChineseTokens(7900000)).toBe("790.0 万");
    expect(formatChineseTokens(278000000)).toBe("2.78 亿");
    expect(formatChineseTokens(3751000)).toBe("375.1 万");
    expect(formatChineseTokens(52)).toBe("52");
    expect(formatChineseTokens(0)).toBe("0");
  });

  it("renders UsageHeader with title and back button", () => {
    const onBack = vi.fn();
    render(<UsageHeader onBack={onBack} />);
    expect(screen.getByText("会话用量")).toBeDefined();
    const backBtn = screen.getByRole("button", { name: "返回" });
    expect(backBtn).toBeDefined();
    fireEvent.click(backBtn);
    expect(onBack).toHaveBeenCalled();
  });

  it("renders UsageHeroCard with formatted token numbers and costs", () => {
    render(<UsageHeroCard hero={mockHero} />);
    expect(screen.getByText("33.30 亿")).toBeDefined();
    expect(screen.getByText("22,938")).toBeDefined();
    expect(screen.getByText("¥555.69")).toBeDefined();
    expect(screen.getByText("33.22 亿")).toBeDefined();
    expect(screen.getByText("32.18 亿")).toBeDefined();
    expect(screen.getByText("790.0 万")).toBeDefined();
  });

  it("renders UsageTrendChart with spline data and metric toggle", () => {
    const { rerender } = render(<UsageTrendChart data={[]} />);
    expect(screen.getByText("暂无每日用量趋势数据")).toBeDefined();

    rerender(
      <UsageTrendChart
        data={[
          {
            date: "2026-09-14",
            totalTokens: 3788000,
            requestsCount: 52,
            inputTokens: 3751000,
            outputTokens: 38000,
            cacheTokens: 2832000,
            costsByCurrency: [{ currency: "USD", amount: 0.65 }],
          },
        ]}
      />,
    );
    expect(screen.getByText("会话用量")).toBeDefined();
    expect(screen.getAllByText("378.8 万").length).toBeGreaterThan(0);

    // Switch metric to requests count
    const requestsToggle = screen.getByRole("button", { name: "请求数" });
    fireEvent.click(requestsToggle);
    expect(screen.getAllByText("52 次").length).toBeGreaterThan(0);
  });

  it("renders UsageToolbar with dropdowns, status, and notice", () => {
    const onTimeRangeChange = vi.fn();
    const onInstanceChange = vi.fn();
    const onRefresh = vi.fn();
    const onScan = vi.fn();

    render(
      <UsageToolbar
        timeRange="last_30_days"
        onTimeRangeChange={onTimeRangeChange}
        selectedInstance="all"
        onInstanceChange={onInstanceChange}
        sources={[
          {
            adapterId: "codex",
            sourceId: "src-1",
            sourceName: "默认实例",
            appName: "Codex",
            requestsCount: 100,
            totalTokens: 1000,
            inputTokens: 800,
            outputTokens: 200,
            cacheTokens: 500,
            costsByCurrency: [],
            status: "active",
            diagnostics: [],
          },
        ]}
        onRefresh={onRefresh}
        onScan={onScan}
        scanStatus={{
          scannedSourcesCount: 464,
          totalEventsCount: 22938,
          lastScannedAt: null,
          activeScanTaskId: null,
          sourceDiagnostics: [],
        }}
        totalRequests={22938}
        refreshing={false}
      />,
    );

    expect(screen.getByText("相对范围")).toBeDefined();
    expect(screen.getByText("实例")).toBeDefined();
    expect(screen.getByText(/已扫描 464 个本地来源/)).toBeDefined();
    expect(screen.getByText(/本地多源用量统计/)).toBeDefined();

    const scanBtn = screen.getByRole("button", { name: /重新扫描/ });
    fireEvent.click(scanBtn);
    expect(onScan).toHaveBeenCalled();
  });

  it("renders UsageBreakdownTables with side-by-side and full-width tables", () => {
    render(
      <UsageBreakdownTables
        models={[
          {
            model: "gpt-5.0-turbo",
            provider: "openai",
            requestsCount: 18450,
            totalTokens: 2741000000,
            inputTokens: 2741000000,
            outputTokens: 5667000,
            reasoningTokens: 80000,
            cacheTokens: 2665000000,
            cost: 450.2,
            currency: "USD",
            costBasis: "official",
          },
        ]}
        sources={[
          {
            adapterId: "codex",
            sourceId: "src-1",
            sourceName: "默认实例",
            appName: "Codex",
            requestsCount: 21500,
            totalTokens: 3101000000,
            inputTokens: 3101000000,
            outputTokens: 7780000,
            cacheTokens: 3100000000,
            costsByCurrency: [],
            status: "active",
            diagnostics: [],
          },
        ]}
        dates={[
          {
            date: "2026-09-14",
            requestsCount: 52,
            totalTokens: 3788000,
            inputTokens: 3751000,
            outputTokens: 38000,
            cacheTokens: 2832000,
            costsByCurrency: [],
          },
        ]}
      />,
    );

    expect(screen.getByText("按模型")).toBeDefined();
    expect(screen.getByText("按实例")).toBeDefined();
    expect(screen.getByText("按日期")).toBeDefined();
    expect(screen.getByText("gpt-5.0-turbo")).toBeDefined();
    expect(screen.getByText("默认实例")).toBeDefined();
    expect(screen.getByText("2026-09-14")).toBeDefined();
  });

  it("formats instance display name by removing local sessions suffix", () => {
    expect(formatInstanceDisplayName("Codex local sessions")).toBe("Codex");
    expect(formatInstanceDisplayName("Codex (local sessions)")).toBe("Codex");
    expect(formatInstanceDisplayName("Antigravity local session")).toBe("Antigravity");
    expect(formatInstanceDisplayName("Claude")).toBe("Claude");
  });

  it("resolves app shortcuts correctly from global settings catalog", () => {
    const mockShortcuts = [
      {
        profileId: "codex-profile",
        profileName: "Codex",
        appKind: "codex",
        accentColor: "#10b981",
        displayIcon: "app:codex",
      },
    ];

    const matched = findMatchedAppShortcut(
      {
        adapterId: "codex",
        sourceId: "codex-profile",
        sourceName: "Codex local sessions",
        appName: "Codex",
        requestsCount: 1,
        totalTokens: 1,
        inputTokens: 1,
        outputTokens: 1,
        cacheTokens: 1,
        costsByCurrency: [],
        status: "active",
        diagnostics: [],
      },
      mockShortcuts as any,
    );

    expect(matched?.accentColor).toBe("#10b981");
    expect(matched?.appKind).toBe("codex");
  });

  it("renders instance costs and paginates dates table when exceeding page size", () => {
    const mockDates = Array.from({ length: 15 }, (_, i) => ({
      date: `2026-09-${String(i + 1).padStart(2, "0")}`,
      requestsCount: 10 + i,
      totalTokens: 1000000 * (i + 1),
      inputTokens: 900000 * (i + 1),
      outputTokens: 100000 * (i + 1),
      cacheTokens: 500000 * (i + 1),
      costsByCurrency: [{ currency: "CNY", amount: 1.5 * (i + 1) }],
    }));

    render(
      <UsageBreakdownTables
        models={[]}
        sources={[
          {
            adapterId: "codex",
            sourceId: "src-1",
            sourceName: "Codex local sessions",
            appName: "Codex",
            requestsCount: 100,
            totalTokens: 2000000000,
            inputTokens: 1800000000,
            outputTokens: 200000000,
            cacheTokens: 1000000000,
            costsByCurrency: [{ currency: "CNY", amount: 217.46 }],
            status: "active",
            diagnostics: [],
          },
        ]}
        dates={mockDates}
      />,
    );

    // Strips "local sessions" and shows "Codex"
    expect(screen.getByText("Codex")).toBeDefined();
    // Shows estimated cost for the instance
    expect(screen.getByText("¥217.46")).toBeDefined();

    // Pagination: page 1 shows 2026-09-01 but not 2026-09-15
    expect(screen.getByText("2026-09-01")).toBeDefined();
    expect(screen.queryByText("2026-09-15")).toBeNull();
    expect(screen.getByText(/第 1 \/ 2 页/)).toBeDefined();

    // Click next page
    const nextBtn = screen.getByRole("button", { name: /下一页/ });
    fireEvent.click(nextBtn);

    // Now page 2 shows 2026-09-15 but not 2026-09-01
    expect(screen.getByText("2026-09-15")).toBeDefined();
    expect(screen.queryByText("2026-09-01")).toBeNull();
  });

  it("renders streamlined UsageToolbar with refresh and scan triggers", () => {
    const onRefresh = vi.fn();
    const onScan = vi.fn();
    render(
      <UsageToolbar
        timeRange="last_30_days"
        onTimeRangeChange={vi.fn()}
        selectedInstance="all"
        onInstanceChange={vi.fn()}
        sources={[]}
        onRefresh={onRefresh}
        onScan={onScan}
        scanStatus={{
          scannedSourcesCount: 1,
          totalEventsCount: 10,
          lastScannedAt: null,
          activeScanTaskId: null,
          sourceDiagnostics: [],
        }}
        totalRequests={10}
        refreshing={false}
      />,
    );

    const refreshBtn = screen.getByRole("button", { name: "刷新" });
    expect(refreshBtn).toBeDefined();
    fireEvent.click(refreshBtn);
    expect(onRefresh).toHaveBeenCalledTimes(1);

    const scanBtn = screen.getByRole("button", { name: /重新扫描/ });
    expect(scanBtn).toBeDefined();
    fireEvent.click(scanBtn);
    expect(onScan).toHaveBeenCalledTimes(1);
  });

  it("renders DateRangePicker, handles open, preset click, and clear", () => {
    const onChange = vi.fn();
    const { rerender } = render(
      <DateRangePicker
        startDate={null}
        endDate={null}
        onChange={onChange}
      />,
    );

    // Initial placeholder
    const trigger = screen.getByRole("button", { name: /选择日期范围/ });
    expect(trigger).toBeDefined();

    // Open calendar popover
    fireEvent.click(trigger);
    expect(screen.getByText("一")).toBeDefined();
    expect(screen.getByText("日")).toBeDefined();
    expect(screen.getByRole("button", { name: "今天" })).toBeDefined();
    expect(screen.getByRole("button", { name: "近7天" })).toBeDefined();
    expect(screen.getByRole("button", { name: "近30天" })).toBeDefined();

    // Click "今天" preset
    fireEvent.click(screen.getByRole("button", { name: "今天" }));
    expect(onChange).toHaveBeenCalledTimes(1);
    const [startArg, endArg] = onChange.mock.calls[0];
    expect(startArg).toMatch(/^\d{4}-\d{2}-\d{2}$/);
    expect(endArg).toMatch(/^\d{4}-\d{2}-\d{2}$/);
    expect(startArg).toBe(endArg);

    // Rerender with active range and test clear
    rerender(
      <DateRangePicker
        startDate="2026-03-01"
        endDate="2026-03-10"
        onChange={onChange}
      />,
    );
    expect(screen.getByText("2026-03-01 ~ 2026-03-10")).toBeDefined();
    const clearBtn = screen.getByTitle("清空范围");
    expect(clearBtn).toBeDefined();
    fireEvent.click(clearBtn);
    expect(onChange).toHaveBeenCalledWith(null, null);
  });

  it("renders UsageToolbar with DateRangePicker and handles bi-directional changes", () => {
    const onTimeRangeChange = vi.fn();
    const onDateRangeChange = vi.fn();

    render(
      <UsageToolbar
        timeRange="last_30_days"
        onTimeRangeChange={onTimeRangeChange}
        startDate={null}
        endDate={null}
        onDateRangeChange={onDateRangeChange}
        selectedInstance="all"
        onInstanceChange={vi.fn()}
        sources={[]}
        onRefresh={vi.fn()}
        onScan={vi.fn()}
        scanStatus={{
          scannedSourcesCount: 0,
          totalEventsCount: 0,
          lastScannedAt: null,
          activeScanTaskId: null,
          sourceDiagnostics: [],
        }}
        totalRequests={0}
        refreshing={false}
      />,
    );

    expect(screen.getByText("日期区间")).toBeDefined();
    const datePickerBtn = screen.getByRole("button", { name: /选择日期范围/ });
    expect(datePickerBtn).toBeDefined();

    // Open date picker and select preset
    fireEvent.click(datePickerBtn);
    fireEvent.click(screen.getByRole("button", { name: "近7天" }));

    expect(onDateRangeChange).toHaveBeenCalledTimes(1);
    expect(onTimeRangeChange).toHaveBeenCalledWith("custom");
  });

  it("renders By Project table and handles project selection click", () => {
    const onSelectProject = vi.fn();
    const mockProjects = [
      {
        projectPath: "~/code-space/assetiweave",
        displayName: "assetiweave",
        isUnassigned: false,
        requestsCount: 150,
        totalTokens: 2500000,
        inputTokens: 2000000,
        outputTokens: 500000,
        cacheTokens: 1000000,
        costsByCurrency: [{ currency: "CNY", amount: 12.34 }],
      },
      {
        projectPath: "__unassigned__",
        displayName: "未分配项目",
        isUnassigned: true,
        requestsCount: 20,
        totalTokens: 300000,
        inputTokens: 250000,
        outputTokens: 50000,
        cacheTokens: 100000,
        costsByCurrency: [{ currency: "CNY", amount: 1.5 }],
      },
    ];

    const { rerender } = render(
      <UsageBreakdownTables
        models={[]}
        sources={[]}
        projects={mockProjects}
        dates={[]}
        selectedProject="all"
        onSelectProject={onSelectProject}
      />,
    );

    expect(screen.getByText("按项目")).toBeDefined();
    expect(screen.getByText("共 2 个项目")).toBeDefined();
    expect(screen.getByText("assetiweave")).toBeDefined();
    expect(screen.getByText("未分配项目")).toBeDefined();
    expect(screen.getByText("250.0 万")).toBeDefined();
    expect(screen.getByText("¥12.34")).toBeDefined();

    // Click on project row
    const projectRow = screen.getByText("assetiweave").closest("tr");
    expect(projectRow).toBeDefined();
    fireEvent.click(projectRow!);
    expect(onSelectProject).toHaveBeenCalledWith("~/code-space/assetiweave");

    // Rerender with selected project
    rerender(
      <UsageBreakdownTables
        models={[]}
        sources={[]}
        projects={mockProjects}
        dates={[]}
        selectedProject="~/code-space/assetiweave"
        onSelectProject={onSelectProject}
      />,
    );

    expect(screen.getByText("已筛选")).toBeDefined();
    const clearBtn = screen.getByRole("button", { name: "清除项目过滤" });
    expect(clearBtn).toBeDefined();
    fireEvent.click(clearBtn);
    expect(onSelectProject).toHaveBeenCalledWith("all");
  });

  it("renders Project selector in UsageToolbar", () => {
    const onProjectChange = vi.fn();
    render(
      <UsageToolbar
        timeRange="last_30_days"
        onTimeRangeChange={vi.fn()}
        selectedInstance="all"
        onInstanceChange={vi.fn()}
        selectedProject="all"
        onProjectChange={onProjectChange}
        projects={[
          {
            projectPath: "~/code-space/assetiweave",
            displayName: "assetiweave",
            isUnassigned: false,
            requestsCount: 150,
            totalTokens: 2500000,
            inputTokens: 2000000,
            outputTokens: 500000,
            cacheTokens: 1000000,
            costsByCurrency: [{ currency: "CNY", amount: 12.34 }],
          },
        ]}
        sources={[]}
        onRefresh={vi.fn()}
        onScan={vi.fn()}
        scanStatus={{
          scannedSourcesCount: 1,
          totalEventsCount: 10,
          lastScannedAt: null,
          activeScanTaskId: null,
          sourceDiagnostics: [],
        }}
        totalRequests={10}
        refreshing={false}
      />,
    );

    expect(screen.getByText("项目")).toBeDefined();
  });
});
