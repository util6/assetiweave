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
      {/* Left Big Card (Col 5) - Warm orange-accented glow hero surface */}
      <div className="relative overflow-hidden rounded-2xl border border-theme-nav-active-border/45 bg-[linear-gradient(135deg,rgb(var(--theme-nav-active)/0.42),rgb(var(--theme-card-bg)/0.86)_42%,rgb(var(--theme-card-header)/0.92))] p-5 shadow-[var(--theme-shadow-card),0_12px_32px_rgb(var(--theme-glow)/0.12),inset_0_1px_0_rgb(var(--theme-inset-highlight)/0.24)] backdrop-blur-xl lg:col-span-5 flex flex-col justify-between transition-all duration-200">
        {/* Top subtle sheen line */}
        <div
          className="pointer-events-none absolute inset-x-0 top-0 h-px bg-gradient-to-r from-transparent via-[rgb(var(--theme-glow)/0.55)] to-transparent"
          aria-hidden="true"
        />

        <div className="flex items-start justify-between gap-4">
          <div className="flex items-center gap-3">
            <div className="flex size-11 items-center justify-center rounded-2xl bg-[linear-gradient(135deg,rgb(var(--theme-button-primary-bg)/0.92),rgb(var(--color-primary-strong)/0.75))] text-theme-button-primary-fg shadow-[0_8px_22px_rgb(var(--theme-glow)/0.32),inset_0_1px_0_rgb(var(--theme-inset-highlight)/0.32)]">
              <Zap className="size-5 fill-current" />
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
              <span className="font-bold text-primary font-mono text-body-md">
                {formatUsageCosts(hero.costsByCurrency)}
              </span>
            </div>
          </div>
        </div>
      </div>

      {/* Right 3 Sub-Cards (Col 7: 3 columns) */}
      <div className="grid grid-cols-1 gap-3.5 sm:grid-cols-3 lg:col-span-7">
        {/* Input Tokens */}
        <div className="relative flex flex-col justify-between overflow-hidden rounded-2xl border border-theme-card-border/75 bg-[linear-gradient(145deg,rgb(var(--theme-card-bg)/0.82),rgb(var(--theme-card-header)/0.74))] p-4 shadow-[var(--theme-shadow-card),inset_0_1px_0_rgb(var(--theme-inset-highlight)/0.14)] backdrop-blur-xl transition-all duration-200 hover:-translate-y-0.5 hover:border-theme-nav-active-border/40">
          <div className="flex items-center gap-1.5 text-caption font-medium text-on-surface-variant">
            <ArrowDown className="h-3.5 w-3.5 text-primary" />
            <span>输入 Tokens</span>
          </div>
          <div className="mt-2 text-2xl font-bold tracking-tight text-on-surface">
            {formatChineseTokens(hero.totalInputTokens)}
          </div>
        </div>

        {/* Cached Input Tokens */}
        <div className="relative flex flex-col justify-between overflow-hidden rounded-2xl border border-theme-card-border/75 bg-[linear-gradient(145deg,rgb(var(--theme-card-bg)/0.82),rgb(var(--theme-card-header)/0.74))] p-4 shadow-[var(--theme-shadow-card),inset_0_1px_0_rgb(var(--theme-inset-highlight)/0.14)] backdrop-blur-xl transition-all duration-200 hover:-translate-y-0.5 hover:border-status-create/40">
          <div className="flex items-center gap-1.5 text-caption font-medium text-status-create">
            <HardDrive className="h-3.5 w-3.5 text-status-create" />
            <span>缓存输入 Tokens</span>
          </div>
          <div className="mt-2 text-2xl font-bold tracking-tight text-status-create">
            {formatChineseTokens(cacheTokens)}
          </div>
        </div>

        {/* Output Tokens */}
        <div className="relative flex flex-col justify-between overflow-hidden rounded-2xl border border-theme-card-border/75 bg-[linear-gradient(145deg,rgb(var(--theme-card-bg)/0.82),rgb(var(--theme-card-header)/0.74))] p-4 shadow-[var(--theme-shadow-card),inset_0_1px_0_rgb(var(--theme-inset-highlight)/0.14)] backdrop-blur-xl transition-all duration-200 hover:-translate-y-0.5 hover:border-theme-nav-active-border/40">
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
