import * as React from "react";

import { Skeleton, SkeletonText } from "../SkeletonPrimitive";
import { SkeletonColumn, SkeletonColumns } from "../SkeletonSurface";
import type { ColumnsSkeletonRecipeProps } from "../skeletonTypes";

export function ColumnsSkeletonRecipe({
  children,
  columns = 3,
  density = "default",
}: ColumnsSkeletonRecipeProps): React.ReactElement {
  const customChildren = React.Children.count(children) > 0;

  return (
    <SkeletonColumns columns={columns} density={density}>
      {customChildren
        ? children
        : Array.from({ length: columns }, (_, columnIndex) => (
            <SkeletonColumn key={columnIndex}>
              <div className="grid gap-2">
                {Array.from({ length: columnIndex === columns - 1 ? 3 : 4 }, (_, rowIndex) => (
                  <div className="flex min-w-0 items-start gap-3 rounded-xl border border-theme-card-border/45 bg-theme-control/25 p-3" key={rowIndex}>
                    <Skeleton className="size-8 shrink-0 rounded-xl" />
                    <div className="grid min-w-0 flex-1 gap-2">
                      <Skeleton className="h-3 w-3/4 rounded-full" />
                      <SkeletonText lines={2} />
                    </div>
                  </div>
                ))}
              </div>
            </SkeletonColumn>
          ))}
    </SkeletonColumns>
  );
}
