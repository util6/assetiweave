import * as React from "react";

import { AppSkeleton, type AppSkeletonProps } from "./AppSkeleton";
import type { DistributiveOmit } from "./skeletonTypes";

export type SkeletonBoundaryProps = DistributiveOmit<AppSkeletonProps, "children"> & {
  children: React.ReactNode;
  fallbackChildren?: React.ReactNode;
  loading: boolean;
};

export function SkeletonBoundary({
  children,
  fallbackChildren,
  loading,
  ...skeletonProps
}: SkeletonBoundaryProps): React.ReactElement {
  return loading ? (
    <AppSkeleton {...skeletonProps}>{fallbackChildren}</AppSkeleton>
  ) : (
    <>{children}</>
  );
}
