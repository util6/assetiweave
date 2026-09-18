// @vitest-environment jsdom

import { fireEvent, render, screen, cleanup } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { UsageHeader } from "./UsageHeader";
import { UsageHeroCard } from "./UsageHeroCard";
import { UsageTrendChart } from "./UsageTrendChart";
import { UsageToolbar } from "./UsageToolbar";
import { UsageBreakdownTables } from "./UsageBreakdownTables";
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
    costsByCurrency: [{ currency: "USD", amount: 555.6927 }],
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
    expect(screen.getByText("$555.6927")).toBeDefined();
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
});
