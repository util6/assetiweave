import type * as React from "react";

import { cn } from "@/lib/utils";
import type { SkeletonDensity } from "./skeletonTypes";
import { Skeleton } from "./SkeletonPrimitive";

const densityClasses: Record<SkeletonDensity, string> = {
  compact: "gap-2 p-2",
  default: "gap-3 p-3",
  comfortable: "gap-4 p-4",
};

export interface SkeletonSurfaceProps {
  children: React.ReactNode;
  className?: string;
}

export function SkeletonSurface({ children, className }: SkeletonSurfaceProps): React.ReactElement {
  return (
    <div
      aria-hidden="true"
      className={cn(
        "overflow-hidden rounded-2xl border border-theme-card-border/65 bg-theme-card shadow-[var(--theme-shadow-card)]",
        className,
      )}
    >
      {children}
    </div>
  );
}

export interface SkeletonListProps {
  children: React.ReactNode;
  className?: string;
  density?: SkeletonDensity;
}

export function SkeletonList({ children, className, density = "default" }: SkeletonListProps): React.ReactElement {
  return (
    <SkeletonSurface className={cn("grid", densityClasses[density], className)}>
      {children}
    </SkeletonSurface>
  );
}

export interface SkeletonCardGridProps {
  children: React.ReactNode;
  className?: string;
  columns?: 2 | 3;
  density?: SkeletonDensity;
}

export function SkeletonCardGrid({
  children,
  className,
  columns = 2,
  density = "default",
}: SkeletonCardGridProps): React.ReactElement {
  return (
    <div
      aria-hidden="true"
      className={cn(
        "grid min-h-0 overflow-hidden",
        densityClasses[density],
        columns === 2 ? "xl:grid-cols-2" : "xl:grid-cols-3",
        className,
      )}
    >
      {children}
    </div>
  );
}

export interface SkeletonColumnsProps {
  children: React.ReactNode;
  className?: string;
  columns?: 2 | 3;
  density?: SkeletonDensity;
}

export function SkeletonColumns({
  children,
  className,
  columns = 3,
  density = "default",
}: SkeletonColumnsProps): React.ReactElement {
  return (
    <SkeletonSurface className={cn("grid min-h-[24rem] grid-rows-[minmax(0,1fr)_auto]", className)}>
      <div
        className={cn(
          "grid min-h-0 lg:flex lg:flex-row",
          densityClasses[density],
          columns === 2 ? "grid-cols-1" : "grid-cols-1",
        )}
      >
        {children}
      </div>
      <div className="sticky bottom-0 flex min-h-8 items-center gap-2 border-t border-theme-card-border/60 bg-theme-card-header px-3 py-2">
        <Skeleton className="size-6 rounded-md" />
        <Skeleton className="h-2.5 flex-1 rounded-full" />
        <Skeleton className="size-6 rounded-md" />
      </div>
    </SkeletonSurface>
  );
}

export interface SkeletonColumnProps {
  children: React.ReactNode;
  className?: string;
  grow?: 1 | 2;
  header?: React.ReactNode;
}

export function SkeletonColumn({
  children,
  className,
  grow = 1,
  header,
}: SkeletonColumnProps): React.ReactElement {
  return (
    <section
      aria-hidden="true"
      className={cn(
        "flex min-h-0 min-w-0 flex-col border-b border-theme-card-border/60 last:border-b-0 lg:flex-[var(--skeleton-column-grow)_1_0%] lg:border-b-0 lg:border-r lg:last:border-r-0",
        className,
      )}
      style={{ "--skeleton-column-grow": grow } as React.CSSProperties}
    >
      <div className="flex h-14 shrink-0 items-center justify-between gap-3 border-b border-theme-card-border/60 bg-theme-card-header px-4">
        {header ?? (
          <>
            <div className="flex min-w-0 items-center gap-2">
              <Skeleton className="size-5 rounded-md" />
              <Skeleton className="h-3 w-28 rounded-full" />
            </div>
            <Skeleton className="h-6 w-16 rounded-full" />
          </>
        )}
      </div>
      <div className="min-h-0 flex-1 overflow-hidden p-3">{children}</div>
    </section>
  );
}
