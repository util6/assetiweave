import React from "react";
import type { UsageHeroOverview } from "../../../types/usage";
import { formatChineseTokens, formatUsageCosts } from "../../../types/usage";
import { Zap, ArrowDown, ArrowUp, HardDrive } from "lucide-react";
import { cn } from "../../../lib/utils";

interface UsageHeroCardProps {
  hero: UsageHeroOverview;
  className?: string;
}

export const UsageHeroCard: React.FC<UsageHeroCardProps> = ({
  hero,
  className,
}) => {
  const cacheTokens = hero.cacheReadTokens + hero.cacheWriteTokens;

  return (
    <div className={cn("grid grid-cols-1 gap-3.5 lg:grid-cols-12", className)}>
      {/* Left Big Card (Col 5) */}
      <div className="relative overflow-hidden rounded-2xl border border-theme-card-border/80 bg-surface/75 p-5 shadow-[var(--theme-shadow-card)] backdrop-blur-xl lg:col-span-5 flex flex-col justify-between">
        <div className="flex items-start justify-between gap-4">
          <div className="flex items-center gap-3">
            <div className="flex h-10 w-10 items-center justify-center rounded-xl bg-primary/15 text-primary shadow-xs">
              <Zap className="h-5 w-5 fill-primary" />
            </div>
            <div className="flex flex-col">
              <span className="text-body-sm font-medium text-on-surface-variant">
                合计 Tokens
              </span>
              <span className="text-3xl font-extrabold tracking-tight text-on-surface mt-0.5">
                {formatChineseTokens(hero.totalTokens)}
              </span>
            </div>
          </div>

          <div className="flex flex-col items-end gap-1.5 text-right">
            <div className="flex items-center gap-1.5 text-caption text-on-surface-variant">
              <span>请求数</span>
              <span className="font-semibold text-on-surface font-mono">
                {hero.requestsCount.toLocaleString()}
              </span>
            </div>
            <div className="flex items-center gap-1.5 text-caption text-on-surface-variant">
              <span>预估费用</span>
              <span className="font-semibold text-primary font-mono">
                {formatUsageCosts(hero.costsByCurrency)}
              </span>
            </div>
          </div>
        </div>
      </div>

      {/* Right 3 Sub-Cards (Col 7: 3 columns) */}
      <div className="grid grid-cols-1 gap-3.5 sm:grid-cols-3 lg:col-span-7">
        {/* Input Tokens */}
        <div className="flex flex-col justify-between rounded-2xl border border-theme-card-border/80 bg-surface/75 p-4 shadow-[var(--theme-shadow-card)] backdrop-blur-xl">
          <div className="flex items-center gap-1.5 text-caption font-medium text-on-surface-variant">
            <ArrowDown className="h-3.5 w-3.5 text-primary" />
            <span>输入 Tokens</span>
          </div>
          <div className="mt-2 text-2xl font-bold tracking-tight text-on-surface">
            {formatChineseTokens(hero.totalInputTokens)}
          </div>
        </div>

        {/* Cached Input Tokens */}
        <div className="flex flex-col justify-between rounded-2xl border border-theme-card-border/80 bg-surface/75 p-4 shadow-[var(--theme-shadow-card)] backdrop-blur-xl">
          <div className="flex items-center gap-1.5 text-caption font-medium text-status-create">
            <HardDrive className="h-3.5 w-3.5 text-status-create" />
            <span>缓存输入 Tokens</span>
          </div>
          <div className="mt-2 text-2xl font-bold tracking-tight text-status-create">
            {formatChineseTokens(cacheTokens)}
          </div>
        </div>

        {/* Output Tokens */}
        <div className="flex flex-col justify-between rounded-2xl border border-theme-card-border/80 bg-surface/75 p-4 shadow-[var(--theme-shadow-card)] backdrop-blur-xl">
          <div className="flex items-center gap-1.5 text-caption font-medium text-on-surface-variant">
            <ArrowUp className="h-3.5 w-3.5 text-primary" />
            <span>输出 Tokens</span>
          </div>
          <div className="mt-2 text-2xl font-bold tracking-tight text-on-surface">
            {formatChineseTokens(hero.totalOutputTokens)}
          </div>
        </div>
      </div>
    </div>
  );
};
