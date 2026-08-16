import * as React from "react";

import { cn } from "@/lib/utils";

export interface SkeletonProps extends Omit<React.HTMLAttributes<HTMLDivElement>, "aria-hidden"> {}

export function Skeleton({ className, ...props }: SkeletonProps): React.ReactElement {
  return <div {...props} aria-hidden="true" className={cn("aurora-skeleton rounded-xl", className)} />;
}

export interface SkeletonTextProps {
  className?: string;
  lines?: number;
}

export function SkeletonText({ className, lines = 3 }: SkeletonTextProps): React.ReactElement {
  const lineCount = Math.max(1, Math.floor(lines));

  return (
    <div aria-hidden="true" className={cn("grid gap-2", className)}>
      {Array.from({ length: lineCount }, (_, index) => (
        <Skeleton
          className={cn("h-3", index === lineCount - 1 ? "w-2/3" : "w-full")}
          key={index}
        />
      ))}
    </div>
  );
}
