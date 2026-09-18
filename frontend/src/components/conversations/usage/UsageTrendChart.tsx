import React, { useState } from "react";
import type { UsageDailyTrendBucket } from "../../../types/usage";
import { formatChineseTokens } from "../../../types/usage";
import { cn } from "../../../lib/utils";

interface UsageTrendChartProps {
  data: UsageDailyTrendBucket[];
  className?: string;
}

type ChartMetric = "tokens" | "requests";

// Cubic Bezier Spline Path Generator
function getSplinePaths(
  points: { x: number; y: number }[],
  height: number,
): { linePath: string; areaPath: string } {
  if (points.length === 0) return { linePath: "", areaPath: "" };
  if (points.length === 1) {
    return {
      linePath: `M ${points[0].x} ${points[0].y}`,
      areaPath: "",
    };
  }

  let linePath = `M ${points[0].x} ${points[0].y}`;

  for (let i = 0; i < points.length - 1; i++) {
    const p0 = i > 0 ? points[i - 1] : points[i];
    const p1 = points[i];
    const p2 = points[i + 1];
    const p3 = i !== points.length - 2 ? points[i + 2] : p2;

    const cp1x = p1.x + (p2.x - p0.x) / 6;
    const cp1y = p1.y + (p2.y - p0.y) / 6;

    const cp2x = p2.x - (p3.x - p1.x) / 6;
    const cp2y = p2.y - (p3.y - p1.y) / 6;

    linePath += ` C ${cp1x.toFixed(1)} ${cp1y.toFixed(1)}, ${cp2x.toFixed(1)} ${cp2y.toFixed(1)}, ${p2.x.toFixed(1)} ${p2.y.toFixed(1)}`;
  }

  const first = points[0];
  const last = points[points.length - 1];
  const areaPath = `${linePath} L ${last.x} ${height} L ${first.x} ${height} Z`;

  return { linePath, areaPath };
}

export const UsageTrendChart: React.FC<UsageTrendChartProps> = ({
  data,
  className,
}) => {
  const [metric, setMetric] = useState<ChartMetric>("tokens");
  const [hoverIndex, setHoverIndex] = useState<number | null>(null);

  if (!data || data.length === 0) {
    return (
      <div
        className={cn(
          "flex h-64 flex-col items-center justify-center rounded-2xl border border-dashed border-theme-border/70 bg-theme-control/20 p-6 text-center text-on-surface-variant",
          className,
        )}
      >
        <span className="text-body-md font-medium">暂无每日用量趋势数据</span>
        <span className="text-caption text-on-surface-variant/70">
          扫描会话或选择更宽时间范围后将展示每日 Token 消耗趋势
        </span>
      </div>
    );
  }

  // Chart layout dimensions
  const width = 860;
  const height = 240;
  const paddingLeft = 40;
  const paddingRight = 40;
  const paddingTop = 36;
  const paddingBottom = 48;

  const chartWidth = width - paddingLeft - paddingRight;
  const chartHeight = height - paddingTop - paddingBottom;

  const totalValue = data.reduce(
    (acc, d) => acc + (metric === "tokens" ? d.totalTokens : d.requestsCount),
    0,
  );

  const maxValue = Math.max(
    ...data.map((d) => (metric === "tokens" ? d.totalTokens : d.requestsCount)),
    10,
  );

  // Compute point coordinates
  const points = data.map((d, index) => {
    const val = metric === "tokens" ? d.totalTokens : d.requestsCount;
    const x =
      data.length > 1
        ? paddingLeft + (index / (data.length - 1)) * chartWidth
        : paddingLeft + chartWidth / 2;
    const y = paddingTop + chartHeight - (val / maxValue) * chartHeight;
    return { ...d, val, x, y, index };
  });

  const { linePath, areaPath } = getSplinePaths(
    points,
    paddingTop + chartHeight,
  );

  // Identify peak points to show callout badges
  const peakBadges: typeof points = [];
  if (points.length >= 3) {
    for (let i = 0; i < points.length; i++) {
      const prev = i > 0 ? points[i - 1].val : 0;
      const curr = points[i].val;
      const next = i < points.length - 1 ? points[i + 1].val : 0;
      if (curr > prev && curr > next && curr >= maxValue * 0.35) {
        peakBadges.push(points[i]);
      }
    }
  }
  // If no peaks found or only 1, pick max
  if (peakBadges.length === 0 && points.length > 0) {
    const maxP = points.reduce((best, p) => (p.val > best.val ? p : best));
    if (maxP.val > 0) peakBadges.push(maxP);
  }

  // Pick display sample ticks along the x-axis (e.g. 5-7 labels max)
  const step = Math.max(1, Math.floor(points.length / 7));
  const tickPoints = points.filter(
    (p, i) => i % step === 0 || i === points.length - 1,
  );

  const activePoint = hoverIndex !== null ? points[hoverIndex] : null;

  return (
    <div
      className={cn(
        "relative flex flex-col rounded-2xl border border-theme-card-border/80 bg-surface/75 p-5 shadow-[var(--theme-shadow-card)] backdrop-blur-xl",
        className,
      )}
    >
      {/* Chart Top Header */}
      <div className="mb-4 flex items-center justify-between">
        <div className="flex items-baseline gap-3">
          <span className="text-body-md font-bold text-on-surface">
            会话用量
          </span>
          <span className="text-xl font-extrabold text-primary tracking-tight">
            {metric === "tokens"
              ? formatChineseTokens(totalValue)
              : `${totalValue.toLocaleString()} 次`}
          </span>
        </div>

        {/* Metric toggle pills */}
        <div className="flex items-center rounded-xl border border-theme-border/60 bg-theme-control/40 p-1">
          <button
            type="button"
            onClick={() => setMetric("tokens")}
            className={cn(
              "rounded-lg px-2.5 py-1 text-caption font-medium transition-all cursor-pointer",
              metric === "tokens"
                ? "bg-surface text-primary shadow-xs font-semibold"
                : "text-on-surface-variant hover:text-on-surface",
            )}
          >
            合计 Tokens
          </button>
          <button
            type="button"
            onClick={() => setMetric("requests")}
            className={cn(
              "rounded-lg px-2.5 py-1 text-caption font-medium transition-all cursor-pointer",
              metric === "requests"
                ? "bg-surface text-primary shadow-xs font-semibold"
                : "text-on-surface-variant hover:text-on-surface",
            )}
          >
            请求数
          </button>
        </div>
      </div>

      {/* SVG Smooth Curve Area */}
      <div className="relative w-full overflow-hidden">
        <svg
          className="h-auto w-full select-none overflow-visible"
          viewBox={`0 0 ${width} ${height}`}
          onMouseLeave={() => setHoverIndex(null)}
        >
          <defs>
            <linearGradient id="chartGradient" x1="0" y1="0" x2="0" y2="1">
              <stop
                offset="0%"
                stopColor="rgb(var(--primary))"
                stopOpacity="0.28"
              />
              <stop
                offset="100%"
                stopColor="rgb(var(--primary))"
                stopOpacity="0.0"
              />
            </linearGradient>
          </defs>

          {/* Background horizontal guideline */}
          <line
            x1={paddingLeft}
            y1={paddingTop + chartHeight}
            x2={width - paddingRight}
            y2={paddingTop + chartHeight}
            stroke="currentColor"
            className="text-theme-border/50"
            strokeWidth={1}
          />

          {/* Area fill */}
          {areaPath && <path d={areaPath} fill="url(#chartGradient)" />}

          {/* Line stroke */}
          {linePath && (
            <path
              d={linePath}
              fill="none"
              stroke="rgb(var(--primary))"
              strokeWidth={2.5}
              strokeLinecap="round"
              strokeLinejoin="round"
            />
          )}

          {/* Peak Callout Badges */}
          {peakBadges.map((peak) => (
            <g key={`peak-${peak.index}`}>
              {/* Vertical connector line */}
              <line
                x1={peak.x}
                y1={peak.y}
                x2={peak.x}
                y2={peak.y - 12}
                stroke="currentColor"
                className="text-primary/40"
                strokeWidth={1}
                strokeDasharray="2 2"
              />
              {/* Badge rectangle */}
              <rect
                x={peak.x - 30}
                y={peak.y - 25}
                width={60}
                height={18}
                rx={9}
                className="fill-surface stroke-primary/40"
                strokeWidth={1}
              />
              <text
                x={peak.x}
                y={peak.y - 13}
                textAnchor="middle"
                className="fill-primary text-[10px] font-bold font-mono"
              >
                {metric === "tokens"
                  ? formatChineseTokens(peak.val)
                  : `${peak.val} 次`}
              </text>
            </g>
          ))}

          {/* Points & Interactive Guidelines */}
          {points.map((p, index) => {
            const isHovered = hoverIndex === index;
            return (
              <g key={p.date}>
                {/* Data point dot */}
                <circle
                  cx={p.x}
                  cy={p.y}
                  r={isHovered ? 5.5 : 3}
                  className={cn(
                    "transition-all duration-150",
                    isHovered
                      ? "fill-primary stroke-surface stroke-[2.5]"
                      : "fill-surface stroke-primary stroke-[1.5]",
                  )}
                />

                {/* Hover guideline */}
                {isHovered && (
                  <line
                    x1={p.x}
                    y1={paddingTop}
                    x2={p.x}
                    y2={paddingTop + chartHeight}
                    stroke="rgb(var(--primary))"
                    strokeWidth={1.5}
                    strokeDasharray="3 3"
                    className="opacity-75"
                  />
                )}

                {/* Invisible hover trigger */}
                <rect
                  x={p.x - chartWidth / points.length / 2}
                  y={paddingTop}
                  width={chartWidth / points.length}
                  height={chartHeight + paddingBottom}
                  fill="transparent"
                  className="cursor-pointer"
                  onMouseEnter={() => setHoverIndex(index)}
                />
              </g>
            );
          })}

          {/* X-axis tick labels: Date on top, Tokens count below */}
          {tickPoints.map((tp) => {
            const dateStr = tp.date.length >= 10 ? tp.date.slice(5) : tp.date;
            const tokenStr = formatChineseTokens(tp.totalTokens);
            return (
              <g key={`tick-${tp.date}`}>
                <text
                  x={tp.x}
                  y={height - 24}
                  textAnchor="middle"
                  className="fill-on-surface-variant/80 text-[10px] font-mono"
                >
                  {dateStr}
                </text>
                <text
                  x={tp.x}
                  y={height - 10}
                  textAnchor="middle"
                  className="fill-on-surface-variant/60 text-[9px] font-mono"
                >
                  {tokenStr}
                </text>
              </g>
            );
          })}
        </svg>

        {/* Hover Tooltip Overlay */}
        {activePoint && (
          <div
            className="pointer-events-none absolute z-20 flex flex-col gap-1.5 rounded-xl border border-theme-border/80 bg-surface/95 p-3 text-caption shadow-xl backdrop-blur-md transition-all"
            style={{
              left: `${Math.min(Math.max(activePoint.x - 70, 10), width - 180)}px`,
              top: `${Math.max(activePoint.y - 120, 10)}px`,
            }}
          >
            <div className="flex items-center justify-between gap-4 border-b border-theme-border/50 pb-1">
              <span className="font-bold text-on-surface">
                {activePoint.date}
              </span>
              <span className="text-[11px] text-on-surface-variant">
                {activePoint.requestsCount} 次请求
              </span>
            </div>
            <div className="flex flex-col gap-0.5 text-[11px]">
              <div className="flex justify-between gap-4">
                <span className="text-on-surface-variant">合计 Tokens:</span>
                <span className="font-bold text-on-surface">
                  {formatChineseTokens(activePoint.totalTokens)}
                </span>
              </div>
              <div className="flex justify-between gap-4">
                <span className="text-on-surface-variant">输入 Tokens:</span>
                <span>{formatChineseTokens(activePoint.inputTokens)}</span>
              </div>
              <div className="flex justify-between gap-4">
                <span className="text-on-surface-variant">缓存 Tokens:</span>
                <span className="text-status-create font-medium">
                  {formatChineseTokens(activePoint.cacheTokens)}
                </span>
              </div>
              <div className="flex justify-between gap-4">
                <span className="text-on-surface-variant">输出 Tokens:</span>
                <span>{formatChineseTokens(activePoint.outputTokens)}</span>
              </div>
            </div>
          </div>
        )}
      </div>
    </div>
  );
};
