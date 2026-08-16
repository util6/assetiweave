import * as React from "react";

import { cn } from "@/lib/utils";
import { Skeleton, SkeletonText } from "../SkeletonPrimitive";
import { SkeletonList } from "../SkeletonSurface";
import type { ListSkeletonRecipeProps } from "../skeletonTypes";

export function ListSkeletonRecipe({
  children,
  density = "default",
  rows = 7,
}: ListSkeletonRecipeProps): React.ReactElement {
  const customChildren = React.Children.count(children) > 0;
  const rowCount = normalizeCount(rows);

  return (
    <SkeletonList density={density}>
      {customChildren
        ? children
        : Array.from({ length: rowCount }, (_, index) => <DefaultListRow index={index} key={index} />)}
    </SkeletonList>
  );
}

function DefaultListRow({ index }: { index: number }): React.ReactElement {
  return (
    <div className="flex min-w-0 gap-3 rounded-xl border border-theme-card-border/45 bg-theme-control/25 p-3">
      <Skeleton className="size-8 shrink-0 rounded-xl" />
      <div className="grid min-w-0 flex-1 content-start gap-2">
        <div className="flex min-w-0 items-center justify-between gap-3">
          <Skeleton className={cn("h-4 rounded-md", index % 3 === 0 ? "w-48" : "w-64 max-w-[60%]")} />
          <Skeleton className="h-6 w-16 shrink-0 rounded-full" />
        </div>
        <SkeletonText lines={2} />
        <div className="flex gap-2">
          <Skeleton className="h-5 w-20 rounded-full" />
          <Skeleton className="h-5 w-28 rounded-full" />
        </div>
      </div>
    </div>
  );
}

function normalizeCount(value: number): number {
  return Math.min(12, Math.max(1, Math.floor(value)));
}
