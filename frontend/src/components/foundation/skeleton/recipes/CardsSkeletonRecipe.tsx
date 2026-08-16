import * as React from "react";

import { Skeleton, SkeletonText } from "../SkeletonPrimitive";
import { SkeletonCardGrid, SkeletonSurface } from "../SkeletonSurface";
import type { CardsSkeletonRecipeProps } from "../skeletonTypes";

export function CardsSkeletonRecipe({
  cards = 6,
  children,
  columns = 2,
  density = "default",
}: CardsSkeletonRecipeProps): React.ReactElement {
  const customChildren = React.Children.count(children) > 0;
  const cardCount = Math.min(12, Math.max(1, Math.floor(cards)));

  return (
    <SkeletonCardGrid columns={columns} density={density}>
      {customChildren
        ? children
        : Array.from({ length: cardCount }, (_, index) => (
            <SkeletonSurface className="grid min-h-[10rem] content-start gap-3 p-4" key={index}>
              <div className="flex items-center gap-3">
                <Skeleton className="size-8 rounded-xl" />
                <Skeleton className="h-4 w-36 rounded-md" />
              </div>
              <SkeletonText lines={index % 2 === 0 ? 3 : 4} />
              <Skeleton className="h-8 w-2/3 rounded-xl" />
            </SkeletonSurface>
          ))}
    </SkeletonCardGrid>
  );
}
