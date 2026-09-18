import React from "react";
import type {
  UsageDateBreakdown,
  UsageModelBreakdown,
  UsageSourceBreakdown,
} from "../../../types/usage";
import { formatChineseTokens } from "../../../types/usage";
import { cn } from "../../../lib/utils";

interface UsageBreakdownTablesProps {
  models: UsageModelBreakdown[];
  sources: UsageSourceBreakdown[];
  dates: UsageDateBreakdown[];
  className?: string;
}

export const UsageBreakdownTables: React.FC<UsageBreakdownTablesProps> = ({
  models,
  sources,
  dates,
  className,
}) => {
  return (
    <div className={cn("flex flex-col gap-4", className)}>
      {/* Top Row: Side-by-side Tables for Models and Instances */}
      <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
        {/* Table 1: By Model (按模型) */}
        <div className="flex flex-col rounded-2xl border border-theme-card-border/80 bg-surface/75 p-4 shadow-[var(--theme-shadow-card)] backdrop-blur-xl">
          <div className="mb-3 px-1">
            <h3 className="text-body-md font-bold text-on-surface">按模型</h3>
          </div>
          <div className="overflow-x-auto">
            <table className="w-full text-left text-body-sm">
              <thead>
                <tr className="border-b border-theme-border/50 text-caption font-semibold text-on-surface-variant">
                  <th className="pb-2.5 pr-3">模型</th>
                  <th className="pb-2.5 pr-3 text-right">输入 Tokens</th>
                  <th className="pb-2.5 pr-3 text-right">缓存 Tokens</th>
                  <th className="pb-2.5 text-right">输出 Tokens</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-theme-border/30">
                {models.length === 0 ? (
                  <tr>
                    <td
                      colSpan={4}
                      className="py-6 text-center text-caption text-on-surface-variant"
                    >
                      暂无模型用量记录
                    </td>
                  </tr>
                ) : (
                  models.map((m) => (
                    <tr
                      key={`${m.provider}-${m.model}`}
                      className="hover:bg-theme-control/25 transition-colors"
                    >
                      <td className="py-2.5 pr-3 font-mono text-caption text-on-surface truncate max-w-[160px]">
                        {m.model}
                      </td>
                      <td className="py-2.5 pr-3 text-right font-mono text-caption text-on-surface">
                        {formatChineseTokens(m.inputTokens)}
                      </td>
                      <td className="py-2.5 pr-3 text-right font-mono text-caption text-on-surface">
                        {formatChineseTokens(m.cacheTokens)}
                      </td>
                      <td className="py-2.5 text-right font-mono text-caption text-on-surface">
                        {formatChineseTokens(m.outputTokens)}
                      </td>
                    </tr>
                  ))
                )}
              </tbody>
            </table>
          </div>
        </div>

        {/* Table 2: By Instance (按实例) */}
        <div className="flex flex-col rounded-2xl border border-theme-card-border/80 bg-surface/75 p-4 shadow-[var(--theme-shadow-card)] backdrop-blur-xl">
          <div className="mb-3 px-1">
            <h3 className="text-body-md font-bold text-on-surface">按实例</h3>
          </div>
          <div className="overflow-x-auto">
            <table className="w-full text-left text-body-sm">
              <thead>
                <tr className="border-b border-theme-border/50 text-caption font-semibold text-on-surface-variant">
                  <th className="pb-2.5 pr-3">实例</th>
                  <th className="pb-2.5 pr-3 text-right">输入 Tokens</th>
                  <th className="pb-2.5 pr-3 text-right">缓存 Tokens</th>
                  <th className="pb-2.5 pr-3 text-right">输出 Tokens</th>
                  <th className="pb-2.5 text-right font-bold">合计 Tokens</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-theme-border/30">
                {sources.length === 0 ? (
                  <tr>
                    <td
                      colSpan={5}
                      className="py-6 text-center text-caption text-on-surface-variant"
                    >
                      暂无实例用量记录
                    </td>
                  </tr>
                ) : (
                  sources.map((s) => (
                    <tr
                      key={s.sourceId}
                      className="hover:bg-theme-control/25 transition-colors"
                    >
                      <td className="py-2.5 pr-3 font-medium text-caption text-on-surface truncate max-w-[140px]">
                        {s.sourceName || s.appName}
                      </td>
                      <td className="py-2.5 pr-3 text-right font-mono text-caption text-on-surface">
                        {formatChineseTokens(s.inputTokens)}
                      </td>
                      <td className="py-2.5 pr-3 text-right font-mono text-caption text-on-surface">
                        {formatChineseTokens(s.cacheTokens)}
                      </td>
                      <td className="py-2.5 pr-3 text-right font-mono text-caption text-on-surface">
                        {formatChineseTokens(s.outputTokens)}
                      </td>
                      <td className="py-2.5 text-right font-mono text-caption font-bold text-primary">
                        {formatChineseTokens(s.totalTokens)}
                      </td>
                    </tr>
                  ))
                )}
              </tbody>
            </table>
          </div>
        </div>
      </div>

      {/* Bottom Row: Full-width Table for Dates (按日期) */}
      <div className="flex flex-col rounded-2xl border border-theme-card-border/80 bg-surface/75 p-4 shadow-[var(--theme-shadow-card)] backdrop-blur-xl">
        <div className="mb-3 px-1">
          <h3 className="text-body-md font-bold text-on-surface">按日期</h3>
        </div>
        <div className="overflow-x-auto">
          <table className="w-full text-left text-body-sm">
            <thead>
              <tr className="border-b border-theme-border/50 text-caption font-semibold text-on-surface-variant">
                <th className="pb-2.5 pr-4">日期</th>
                <th className="pb-2.5 pr-4 text-right">输入 Tokens</th>
                <th className="pb-2.5 pr-4 text-right">缓存 Tokens</th>
                <th className="pb-2.5 pr-4 text-right">输出 Tokens</th>
                <th className="pb-2.5 pr-4 text-right font-bold">
                  合计 Tokens
                </th>
                <th className="pb-2.5 text-right">请求</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-theme-border/30">
              {dates.length === 0 ? (
                <tr>
                  <td
                    colSpan={6}
                    className="py-6 text-center text-caption text-on-surface-variant"
                  >
                    暂无每日用量记录
                  </td>
                </tr>
              ) : (
                dates.map((d) => (
                  <tr
                    key={d.date}
                    className="hover:bg-theme-control/25 transition-colors"
                  >
                    <td className="py-2.5 pr-4 font-mono text-caption text-on-surface">
                      {d.date}
                    </td>
                    <td className="py-2.5 pr-4 text-right font-mono text-caption text-on-surface">
                      {formatChineseTokens(d.inputTokens)}
                    </td>
                    <td className="py-2.5 pr-4 text-right font-mono text-caption text-on-surface">
                      {formatChineseTokens(d.cacheTokens)}
                    </td>
                    <td className="py-2.5 pr-4 text-right font-mono text-caption text-on-surface">
                      {formatChineseTokens(d.outputTokens)}
                    </td>
                    <td className="py-2.5 pr-4 text-right font-mono text-caption font-bold text-on-surface">
                      {formatChineseTokens(d.totalTokens)}
                    </td>
                    <td className="py-2.5 text-right font-mono text-caption text-on-surface-variant">
                      {d.requestsCount.toLocaleString()}
                    </td>
                  </tr>
                ))
              )}
            </tbody>
          </table>
        </div>
      </div>
    </div>
  );
};
