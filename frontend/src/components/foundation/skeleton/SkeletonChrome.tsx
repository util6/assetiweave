import { Skeleton } from "./SkeletonPrimitive";

export function SkeletonChrome(): React.ReactElement {
  return (
    <div className="grid min-w-0 shrink-0 gap-[var(--app-section-gap)]">
      <div className="flex min-w-0 flex-nowrap items-start justify-between gap-4">
        <div className="grid min-w-0 flex-1 gap-2">
          <Skeleton className="h-2.5 w-24 rounded-full" />
          <Skeleton className="h-8 w-52 max-w-[70%] rounded-lg" />
          <Skeleton className="h-3 w-80 max-w-[85%] rounded-full" />
        </div>
        <div className="ml-auto flex max-w-[50%] flex-nowrap gap-2 overflow-hidden">
          <Skeleton className="h-10 w-24 shrink-0 rounded-2xl" />
          <Skeleton className="h-10 w-24 shrink-0 rounded-2xl" />
        </div>
      </div>
      <div className="grid min-w-0 grid-cols-[minmax(0,1fr)_auto] items-center gap-3 overflow-hidden rounded-2xl border border-theme-card-border/65 bg-theme-toolbar p-2 shadow-[var(--theme-shadow-toolbar)]">
        <div className="flex min-w-0 gap-2 overflow-hidden">
          <Skeleton className="h-10 w-64 shrink-0 rounded-2xl" />
          <Skeleton className="h-10 w-28 shrink-0 rounded-2xl" />
          <Skeleton className="h-10 w-28 shrink-0 rounded-2xl" />
        </div>
        <Skeleton className="h-10 w-28 rounded-2xl" />
      </div>
    </div>
  );
}
