import {
  AppSkeleton,
  Skeleton,
  SkeletonColumn,
  SkeletonList,
  SkeletonSurface,
  SkeletonText,
} from "../foundation/skeleton";

export function ConversationsPageSkeleton({ label }: { label: string }): React.ReactElement {
  return (
    <AppSkeleton label={label} layout="columns" layoutProps={{ columns: 3 }}>
      <SkeletonColumn>
        <ConversationListSkeleton />
      </SkeletonColumn>
      <SkeletonColumn>
        <ConversationQuestionListSkeleton />
      </SkeletonColumn>
      <SkeletonColumn grow={2}>
        <ConversationPreviewSkeleton />
      </SkeletonColumn>
    </AppSkeleton>
  );
}

export function ConversationListSkeleton(): React.ReactElement {
  return <ConversationListItemsSkeleton rows={4} />;
}

export function ConversationPreviewSkeleton(): React.ReactElement {
  return (
    <div className="grid min-h-full place-items-center p-4" aria-hidden="true">
      <SkeletonSurface className="grid w-full max-w-[35rem] content-start gap-4 p-5">
        <div className="flex items-center gap-3">
          <Skeleton className="size-11 rounded-2xl" />
          <div className="grid gap-2">
            <Skeleton className="h-3 w-20 rounded-full" />
            <Skeleton className="h-4 w-44 rounded-md" />
          </div>
        </div>
        <SkeletonText lines={4} />
        <Skeleton className="h-24 w-full rounded-2xl" />
      </SkeletonSurface>
    </div>
  );
}

export function ConversationTurnSkeleton(): React.ReactElement {
  return (
    <SkeletonSurface className="conversation-turn-skeleton grid gap-4 rounded-xl p-4">
      <div className="flex items-center justify-between gap-3">
        <Skeleton className="h-3 w-28 rounded-full" />
        <Skeleton className="h-3 w-20 rounded-full" />
      </div>
      <SkeletonText lines={3} />
      <Skeleton className="h-5 w-36 rounded-md" />
      <SkeletonSurface className="grid gap-3 rounded-xl p-3">
        <Skeleton className="h-4 w-2/3 rounded-md" />
        <SkeletonText lines={5} />
      </SkeletonSurface>
    </SkeletonSurface>
  );
}

export function ConversationLoadingState({ label }: { label: string }): React.ReactElement {
  return (
    <AppSkeleton label={label} layout="list" scope="content">
      <ConversationListItemsSkeleton rows={4} />
    </AppSkeleton>
  );
}

export function ConversationPreviewLoadingState({ label }: { label: string }): React.ReactElement {
  return (
    <AppSkeleton label={label} layout="cards" scope="content">
      <ConversationPreviewSkeleton />
    </AppSkeleton>
  );
}

function ConversationQuestionListSkeleton(): React.ReactElement {
  return <ConversationListItemsSkeleton rows={5} />;
}

function ConversationListItemsSkeleton({ rows }: { rows: number }): React.ReactElement {
  return (
    <SkeletonList>
      {Array.from({ length: rows }, (_, index) => (
        <div className="flex min-w-0 gap-3 rounded-xl border border-theme-card-border/45 bg-theme-control/25 p-3" key={index}>
          <Skeleton className="size-8 shrink-0 rounded-xl" />
          <div className="grid min-w-0 flex-1 gap-2">
            <Skeleton className="h-3.5 w-4/5 rounded-md" />
            <Skeleton className="h-3 w-full rounded-full" />
            <Skeleton className="h-3 w-2/3 rounded-full" />
          </div>
        </div>
      ))}
    </SkeletonList>
  );
}
