import * as React from "react";

import { SkeletonShell } from "./SkeletonShell";
import { skeletonRecipes, type AnySkeletonRecipeProps } from "./skeletonRecipes";
import type {
  SkeletonLayoutName,
  SkeletonRecipeDefinition,
  SkeletonRecipePropsMap,
  SkeletonScope,
} from "./skeletonTypes";

export interface AppSkeletonBaseProps {
  children?: React.ReactNode;
  className?: string;
  label: string;
  scope?: SkeletonScope;
}

export type AppSkeletonProps = {
  [Layout in SkeletonLayoutName]: AppSkeletonBaseProps & {
    layout: Layout;
    layoutProps?: Omit<SkeletonRecipePropsMap[Layout], "children">;
  };
}[SkeletonLayoutName];

export function AppSkeleton(props: AppSkeletonProps): React.ReactElement {
  const { children, className, label, layout, layoutProps, scope = "page" } = props;

  if (!label.trim()) {
    throw new Error("AppSkeleton requires a non-empty label");
  }

  const definition = skeletonRecipes[layout] as SkeletonRecipeDefinition<AnySkeletonRecipeProps>;
  const Recipe = definition.component;
  const recipeProps = {
    ...definition.defaults,
    ...layoutProps,
    children,
  } as AnySkeletonRecipeProps;

  return (
    <SkeletonShell className={className} label={label} scope={scope}>
      <Recipe {...recipeProps} />
    </SkeletonShell>
  );
}
