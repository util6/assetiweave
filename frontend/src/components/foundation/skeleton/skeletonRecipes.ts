import type * as React from "react";

import { CardsSkeletonRecipe } from "./recipes/CardsSkeletonRecipe";
import { ColumnsSkeletonRecipe } from "./recipes/ColumnsSkeletonRecipe";
import { ListSkeletonRecipe } from "./recipes/ListSkeletonRecipe";
import type {
  CardsSkeletonRecipeProps,
  ColumnsSkeletonRecipeProps,
  ListSkeletonRecipeProps,
  SkeletonRecipeRegistry,
} from "./skeletonTypes";

export const skeletonRecipes: SkeletonRecipeRegistry = {
  list: {
    component: ListSkeletonRecipe,
    defaults: { density: "default", rows: 7 } satisfies Omit<ListSkeletonRecipeProps, "children">,
  },
  cards: {
    component: CardsSkeletonRecipe,
    defaults: { cards: 6, columns: 2, density: "default" } satisfies Omit<CardsSkeletonRecipeProps, "children">,
  },
  columns: {
    component: ColumnsSkeletonRecipe,
    defaults: { columns: 3, density: "default" } satisfies Omit<ColumnsSkeletonRecipeProps, "children">,
  },
};

export type AnySkeletonRecipeProps = {
  children?: React.ReactNode;
  [key: string]: unknown;
};
