import {
  AppSkeleton,
  Skeleton,
  SkeletonColumn,
  SkeletonSurface,
  SkeletonText,
} from "../foundation/skeleton";

export function MemoryOverviewSkeleton({ label }: { label: string }): React.ReactElement {
  return (
    <AppSkeleton label={label} layout="cards" scope="content">
      {Array.from({ length: 4 }, (_, index) => (
        <SkeletonSurface className="grid min-h-[10rem] content-start gap-3 p-4" key={index}>
          <div className="flex items-center gap-3">
            <Skeleton className="size-8 rounded-xl" />
            <Skeleton className="h-4 w-36 rounded-md" />
          </div>
          <SkeletonText lines={index % 2 === 0 ? 3 : 4} />
          <Skeleton className="h-8 w-2/3 rounded-xl" />
        </SkeletonSurface>
      ))}
    </AppSkeleton>
  );
}

export function MemoryDreamSkeleton({ label }: { label: string }): React.ReactElement {
  return (
    <AppSkeleton label={label} layout="columns" layoutProps={{ columns: 2 }} scope="content">
      <SkeletonColumn>
        <div className="grid gap-3">
          <SkeletonSurface className="grid content-start gap-3 p-4">
            <Skeleton className="h-5 w-28 rounded-md" />
            <SkeletonText lines={4} />
            <div className="grid grid-cols-3 gap-2">
              <Skeleton className="h-14" />
              <Skeleton className="h-14" />
              <Skeleton className="h-14" />
            </div>
          </SkeletonSurface>
          <SkeletonSurface className="grid content-start gap-3 p-4">
            <Skeleton className="h-5 w-24 rounded-md" />
            <SkeletonText lines={3} />
          </SkeletonSurface>
          <SkeletonSurface className="grid content-start gap-2 p-4">
            <Skeleton className="h-5 w-28 rounded-md" />
            {Array.from({ length: 4 }, (_, index) => <Skeleton className="h-12" key={index} />)}
          </SkeletonSurface>
        </div>
      </SkeletonColumn>
      <SkeletonColumn grow={2}>
        <SkeletonSurface className="grid min-h-[30rem] content-start gap-4 p-5">
          <Skeleton className="h-5 w-24 rounded-md" />
          <Skeleton className="h-48 w-full rounded-2xl" />
          <SkeletonText lines={7} />
        </SkeletonSurface>
      </SkeletonColumn>
    </AppSkeleton>
  );
}

export function MemoryRecallSkeleton({ label }: { label: string }): React.ReactElement {
  return (
    <AppSkeleton label={label} layout="columns" layoutProps={{ columns: 2 }} scope="content">
      <SkeletonColumn>
        <div className="grid gap-3">
          <SkeletonSurface className="grid content-start gap-4 p-4">
            <Skeleton className="h-5 w-28 rounded-md" />
            <Skeleton className="h-10 w-full rounded-xl" />
            <Skeleton className="h-10 w-full rounded-xl" />
            <SkeletonText lines={2} />
          </SkeletonSurface>
          <SkeletonSurface className="grid content-start gap-3 p-4">
            <Skeleton className="h-5 w-24 rounded-md" />
            <div className="grid grid-cols-2 gap-2">
              {Array.from({ length: 4 }, (_, index) => <Skeleton className="h-16" key={index} />)}
            </div>
          </SkeletonSurface>
        </div>
      </SkeletonColumn>
      <SkeletonColumn grow={2}>
        <SkeletonSurface className="grid min-h-[30rem] content-start gap-4 p-5">
          <Skeleton className="h-5 w-28 rounded-md" />
          <Skeleton className="h-28 w-full rounded-2xl" />
          <SkeletonText lines={8} />
        </SkeletonSurface>
      </SkeletonColumn>
    </AppSkeleton>
  );
}

export function MemoryLibraryContentSkeleton({ label }: { label: string }): React.ReactElement {
  return <AppSkeleton label={label} layout="columns" layoutProps={{ columns: 2 }} scope="content" />;
}

export function MemoryDetailSkeleton(): React.ReactElement {
  return (
    <div aria-hidden="true" className="grid min-h-[18rem] content-start gap-5 p-5">
      <div className="grid gap-3 border-b border-theme-card-border/60 pb-4">
        <div className="flex gap-2">
          <Skeleton className="h-6 w-20 rounded-full" />
          <Skeleton className="h-6 w-20 rounded-full" />
          <Skeleton className="h-6 w-24 rounded-full" />
        </div>
        <Skeleton className="h-7 w-2/3 rounded-lg" />
        <Skeleton className="h-3 w-40 rounded-full" />
      </div>
      <SkeletonText lines={5} />
      <SkeletonSurface className="grid gap-3 p-3">
        <Skeleton className="h-5 w-28 rounded-md" />
        <SkeletonText lines={4} />
      </SkeletonSurface>
    </div>
  );
}
