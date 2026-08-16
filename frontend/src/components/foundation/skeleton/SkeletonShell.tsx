import type * as React from "react";

import { cn } from "@/lib/utils";
import type { SkeletonScope } from "./skeletonTypes";
import { SkeletonChrome } from "./SkeletonChrome";

export interface SkeletonShellProps {
  children: React.ReactNode;
  className?: string;
  label: string;
  scope?: SkeletonScope;
}

export function SkeletonShell({
  children,
  className,
  label,
  scope = "page",
}: SkeletonShellProps): React.ReactElement {
  return (
    <div
      aria-busy="true"
      className={cn(
        "app-skeleton-root min-h-0 min-w-0 flex-1",
        scope === "page" && "flex flex-col gap-[var(--app-section-gap)] overflow-hidden px-[var(--app-page-x)] py-[var(--app-page-y)]",
        scope === "content" && "overflow-hidden",
        className,
      )}
      role="status"
    >
      <span className="sr-only">{label}</span>
      {scope === "page" ? <SkeletonChrome /> : null}
      {children}
    </div>
  );
}
