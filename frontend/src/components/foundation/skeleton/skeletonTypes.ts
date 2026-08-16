import type * as React from "react";

export type SkeletonDensity = "compact" | "default" | "comfortable";
export type SkeletonScope = "page" | "content";

export interface ListSkeletonRecipeProps {
  children?: React.ReactNode;
  density?: SkeletonDensity;
  rows?: number;
}

export interface CardsSkeletonRecipeProps {
  cards?: number;
  children?: React.ReactNode;
  columns?: 2 | 3;
  density?: SkeletonDensity;
}

export interface ColumnsSkeletonRecipeProps {
  children?: React.ReactNode;
  columns?: 2 | 3;
  density?: SkeletonDensity;
}

export interface SkeletonRecipePropsMap {
  list: ListSkeletonRecipeProps;
  cards: CardsSkeletonRecipeProps;
  columns: ColumnsSkeletonRecipeProps;
}

export type SkeletonLayoutName = keyof SkeletonRecipePropsMap;

export interface SkeletonRecipeDefinition<
  Props extends { children?: React.ReactNode },
> {
  component: React.ComponentType<Props>;
  defaults: Omit<Props, "children">;
}

export type SkeletonRecipeRegistry = {
  [Layout in SkeletonLayoutName]: SkeletonRecipeDefinition<
    SkeletonRecipePropsMap[Layout]
  >;
};

export type DistributiveOmit<T, Key extends PropertyKey> = T extends unknown
  ? Omit<T, Key>
  : never;
