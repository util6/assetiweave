import * as React from "react";

import { cn } from "@/lib/utils";

export interface SkeletonProps extends Omit<React.HTMLAttributes<HTMLDivElement>, "aria-hidden"> {
  lines?: number;
}

export type PageSkeletonKind =
  | "catalog"
  | "conversations"
  | "groups"
  | "manual"
  | "memory-library"
  | "memory-overview"
  | "memory-dreams"
  | "memory-recall"
  | "memory-detail"
  | "mounts"
  | "prompts"
  | "sources"
  | "web-records";

export function Skeleton({ className, ...props }: React.HTMLAttributes<HTMLDivElement>) {
  return <div aria-hidden="true" className={cn("aurora-skeleton rounded-xl", className)} {...props} />;
}

export function SkeletonText({ className, lines = 3 }: SkeletonProps) {
  const lineCount = Math.max(1, Math.floor(lines));

  return (
    <div aria-hidden="true" className={cn("grid gap-2", className)}>
      {Array.from({ length: lineCount }, (_, index) => (
        <Skeleton className={cn("h-3", index === lineCount - 1 ? "w-2/3" : "w-full")} key={index} />
      ))}
    </div>
  );
}

export function PageSkeleton({ kind, label }: { kind: PageSkeletonKind; label: string }) {
  return (
    <div aria-busy="true" className="min-h-0 flex-1" role="status">
      <span className="sr-only">{label}</span>
      {kind === "catalog" ? <CatalogPageSkeleton /> : null}
      {kind === "sources" ? <SourcesPageSkeleton /> : null}
      {kind === "groups" ? <WorkbenchPageSkeleton columns={3} /> : null}
      {kind === "mounts" ? <WorkbenchPageSkeleton columns={3} /> : null}
      {kind === "conversations" ? <ConversationsPageSkeleton /> : null}
      {kind === "web-records" ? <WebRecordsPageSkeleton /> : null}
      {kind === "prompts" ? <PromptsPageSkeleton /> : null}
      {kind === "memory-library" ? <MemoryLibraryPageSkeleton /> : null}
      {kind === "memory-overview" ? <MemoryOverviewPageSkeleton /> : null}
      {kind === "memory-dreams" ? <MemoryDreamsPageSkeleton /> : null}
      {kind === "memory-recall" ? <MemoryRecallPageSkeleton /> : null}
      {kind === "memory-detail" ? <MemoryDetailSkeleton /> : null}
      {kind === "manual" ? <ManualPageSkeleton /> : null}
    </div>
  );
}

function PageFrame({ children, className }: { children: React.ReactNode; className?: string }) {
  return <div className={cn("flex min-h-0 flex-1 flex-col gap-[var(--app-section-gap)] overflow-hidden px-[var(--app-page-x)] py-[var(--app-page-y)]", className)}>{children}</div>;
}

function PageHeaderSkeleton({ metrics = 0 }: { metrics?: number }) {
  return (
    <div className="flex min-w-0 flex-nowrap items-start justify-between gap-4">
      <div className="grid min-w-0 flex-1 gap-2">
        <Skeleton className="h-2.5 w-24 rounded-full" />
        <Skeleton className="h-8 w-52 max-w-[70%] rounded-lg" />
        <Skeleton className="h-3 w-80 max-w-[85%] rounded-full" />
      </div>
      {metrics > 0 ? <MetricSkeletonRow count={metrics} /> : null}
    </div>
  );
}

function MetricSkeletonRow({ count }: { count: number }) {
  return (
    <div className="ml-auto flex max-w-[50%] flex-nowrap gap-2 overflow-hidden">
      {Array.from({ length: count }, (_, index) => (
        <Skeleton className="h-10 w-24 shrink-0 rounded-2xl" key={index} />
      ))}
    </div>
  );
}

function ToolbarSkeleton({ controls = 4, actions = 1 }: { controls?: number; actions?: number }) {
  return (
    <div className="grid min-w-0 grid-cols-[minmax(0,1fr)_auto] items-center gap-3 overflow-hidden rounded-2xl border border-theme-card-border/55 bg-theme-toolbar/45 p-2 shadow-[var(--theme-shadow-toolbar)] backdrop-blur-md">
      <div className="flex min-w-0 gap-2 overflow-hidden">
        <Skeleton className="h-10 w-64 shrink-0 rounded-2xl" />
        {Array.from({ length: Math.max(0, controls - 1) }, (_, index) => (
          <Skeleton className="h-10 w-28 shrink-0 rounded-2xl" key={index} />
        ))}
      </div>
      <div className="flex shrink-0 gap-2">
        {Array.from({ length: actions }, (_, index) => (
          <Skeleton className="h-10 w-28 rounded-2xl" key={index} />
        ))}
      </div>
    </div>
  );
}

function SurfaceSkeleton({ children, className }: { children: React.ReactNode; className?: string }) {
  return <div className={cn("overflow-hidden rounded-2xl border border-theme-card-border/65 bg-theme-card/55 shadow-[var(--theme-shadow-card)] backdrop-blur-xl", className)}>{children}</div>;
}

function ListSkeleton({ rows = 7, variant = "asset", className }: { rows?: number; variant?: "asset" | "source" | "session"; className?: string }) {
  return (
    <SurfaceSkeleton className={cn("aurora-list-surface !gap-2 !p-2", className)}>
      {Array.from({ length: rows }, (_, index) => (
        <div className={cn("aurora-list-row m-0 flex min-w-0 gap-3 px-4 py-3", variant === "source" ? "min-h-[6.5rem]" : variant === "session" ? "min-h-[5.5rem]" : "min-h-[5rem]")} key={index}>
          <Skeleton className={cn("mt-0.5 shrink-0 rounded-xl", variant === "source" ? "size-10" : "size-8")} />
          <div className="grid min-w-0 flex-1 content-start gap-2">
            <div className="flex min-w-0 items-center justify-between gap-3">
              <Skeleton className={cn("h-4 rounded-md", index % 3 === 0 ? "w-48" : "w-64 max-w-[60%]")} />
              <Skeleton className="h-6 w-16 shrink-0 rounded-full" />
            </div>
            <Skeleton className="h-3 w-[82%] max-w-full rounded-full" />
            <div className="flex gap-2">
              <Skeleton className="h-5 w-20 rounded-full" />
              <Skeleton className="h-5 w-28 rounded-full" />
              {variant !== "session" ? <Skeleton className="h-5 w-24 rounded-full" /> : null}
            </div>
          </div>
          {variant === "asset" ? <div className="flex shrink-0 gap-1"><Skeleton className="size-8 rounded-xl" /><Skeleton className="size-8 rounded-xl" /></div> : null}
        </div>
      ))}
    </SurfaceSkeleton>
  );
}

function WorkbenchSkeleton({ columns = 3, className, soft = false }: { columns?: number; className?: string; soft?: boolean }) {
  return (
    <SurfaceSkeleton className={cn("grid min-h-[34rem] grid-rows-[minmax(0,1fr)_auto]", soft && "aurora-workbench-surface", className)}>
      <div className={cn("grid min-h-0", columns === 2 ? "grid-cols-1 lg:grid-cols-2" : "grid-cols-1 lg:grid-cols-3")}>
        {Array.from({ length: columns }, (_, columnIndex) => (
          <section className={cn(soft ? "aurora-workbench-column flex min-h-0 flex-col" : "flex min-h-0 flex-col border-b border-theme-card-border/60 last:border-b-0 lg:border-b-0 lg:border-r lg:last:border-r-0")} key={columnIndex}>
            <div className={cn(soft ? "aurora-workbench-header flex h-14 shrink-0 items-center justify-between gap-3 px-4" : "flex h-14 shrink-0 items-center justify-between gap-3 border-b border-theme-card-border/60 bg-theme-card-header/55 px-4")}>
              <div className="flex min-w-0 items-center gap-2"><Skeleton className="size-5 rounded-md" /><Skeleton className="h-3 w-28 rounded-full" /></div>
              <Skeleton className="h-6 w-16 rounded-full" />
            </div>
            <div className="min-h-0 flex-1 overflow-hidden p-3">
              <div className="grid gap-2">
                {Array.from({ length: columnIndex === columns - 1 ? 4 : 6 }, (_, rowIndex) => (
                  <div className={cn(soft ? "aurora-workbench-item m-0 flex min-w-0 items-start gap-3 p-3" : "flex min-w-0 items-start gap-3 rounded-xl border border-theme-card-border/45 bg-theme-control/25 p-3")} key={rowIndex}>
                    <Skeleton className="size-8 shrink-0 rounded-xl" />
                    <div className="grid min-w-0 flex-1 gap-2"><Skeleton className="h-3 w-3/4 rounded-full" /><Skeleton className="h-2.5 w-full rounded-full" /><Skeleton className="h-2.5 w-2/3 rounded-full" /></div>
                  </div>
                ))}
              </div>
            </div>
          </section>
        ))}
      </div>
      <div className="sticky bottom-0 flex min-h-8 items-center gap-2 border-t border-theme-card-border/60 bg-theme-card-header/55 px-3 py-2"><Skeleton className="size-6 rounded-md" /><Skeleton className="h-2.5 flex-1 rounded-full" /><Skeleton className="size-6 rounded-md" /></div>
    </SurfaceSkeleton>
  );
}

function CardGridSkeleton({ columns = 2 }: { columns?: number }) {
  return (
    <div className={cn("grid min-h-0 gap-4 overflow-hidden", columns === 2 ? "xl:grid-cols-2" : "xl:grid-cols-3")}>
      {Array.from({ length: columns * 2 }, (_, index) => (
        <SurfaceSkeleton className="grid min-h-[10rem] content-start gap-3 p-4" key={index}>
          <div className="flex items-center gap-3"><Skeleton className="size-8 rounded-xl" /><Skeleton className="h-4 w-36 rounded-md" /></div>
          <SkeletonText lines={index % 2 === 0 ? 3 : 4} />
          <Skeleton className="h-8 w-2/3 rounded-xl" />
        </SurfaceSkeleton>
      ))}
    </div>
  );
}

function CatalogPageSkeleton() {
  return <PageFrame><PageHeaderSkeleton metrics={2} /><ToolbarSkeleton controls={5} actions={2} /><ListSkeleton rows={8} variant="asset" /></PageFrame>;
}

function SourcesPageSkeleton() {
  return <PageFrame><PageHeaderSkeleton metrics={3} /><ToolbarSkeleton controls={4} actions={2} /><ListSkeleton rows={6} variant="source" /></PageFrame>;
}

function WorkbenchPageSkeleton({ columns }: { columns: number }) {
  return <PageFrame><PageHeaderSkeleton metrics={2} /><ToolbarSkeleton controls={5} actions={2} /><WorkbenchSkeleton columns={columns} soft /></PageFrame>;
}

export function ListContentSkeleton({ label, rows = 7, variant = "asset" }: { label: string; rows?: number; variant?: "asset" | "source" | "session" }) {
  return (
    <div aria-busy="true" className="min-h-0" role="status">
      <span className="sr-only">{label}</span>
      <ListSkeleton rows={rows} variant={variant} />
    </div>
  );
}

export function WorkbenchContentSkeleton({ columns = 3, label }: { columns?: number; label: string }) {
  return (
    <div aria-busy="true" className="min-h-0" role="status">
      <span className="sr-only">{label}</span>
      <WorkbenchSkeleton columns={columns} soft />
    </div>
  );
}

function ConversationsPageSkeleton() {
  return <PageFrame className="py-6"><PageHeaderSkeleton metrics={4} /><ToolbarSkeleton controls={5} actions={3} /><WorkbenchSkeleton columns={3} soft /></PageFrame>;
}

function WebRecordsPageSkeleton() {
  return <PageFrame className="py-6"><PageHeaderSkeleton metrics={2} /><ToolbarSkeleton controls={3} actions={3} /><WorkbenchSkeleton columns={2} soft /></PageFrame>;
}

function PromptsPageSkeleton() {
  return (
    <PageFrame>
      <PageHeaderSkeleton />
      <ToolbarSkeleton controls={3} actions={1} />
      <div className="grid min-h-[29rem] flex-1 place-items-center overflow-hidden rounded-[2rem] border border-theme-card-border/60 bg-theme-card/35 p-6 shadow-[var(--theme-shadow-panel)] backdrop-blur-md">
        <div className="grid h-full w-full max-w-3xl content-start gap-4 rounded-[2rem] border border-theme-card-border/70 bg-theme-card/75 p-5 shadow-[0_28px_72px_rgb(var(--theme-panel-shadow)/0.34)]">
          <div className="flex items-center justify-between gap-3 border-b border-theme-card-border/60 pb-4"><div className="flex gap-2"><Skeleton className="h-6 w-20 rounded-full" /><Skeleton className="h-6 w-24 rounded-full" /></div><div className="flex gap-2"><Skeleton className="size-8 rounded-xl" /><Skeleton className="size-8 rounded-xl" /><Skeleton className="size-8 rounded-xl" /></div></div>
          <div className="grid min-h-[16rem] flex-1 content-start gap-3 py-2"><SkeletonText lines={8} className="w-full" /><Skeleton className="h-20 w-full rounded-xl" /></div>
          <div className="flex justify-between gap-3 border-t border-theme-card-border/60 pt-4"><Skeleton className="h-3 w-32 rounded-full" /><Skeleton className="h-3 w-24 rounded-full" /></div>
          <div className="grid grid-cols-2 gap-px overflow-hidden rounded-xl border border-theme-card-border/60"><Skeleton className="h-11 rounded-none" /><Skeleton className="h-11 rounded-none" /></div>
        </div>
      </div>
    </PageFrame>
  );
}

function MemoryLibraryPageSkeleton() {
  return <PageFrame><PageHeaderSkeleton /><ToolbarSkeleton controls={5} actions={2} /><MemoryLibraryContentSkeleton /></PageFrame>;
}

export function MemoryLibraryContentSkeleton() {
  return <WorkbenchSkeleton columns={2} />;
}

function MemoryOverviewPageSkeleton() {
  return <PageFrame><PageHeaderSkeleton /><MemoryOverviewSkeleton /></PageFrame>;
}

function MemoryDreamsPageSkeleton() {
  return <PageFrame><PageHeaderSkeleton /><MemoryDreamSkeleton /></PageFrame>;
}

function MemoryRecallPageSkeleton() {
  return <PageFrame><PageHeaderSkeleton /><MemoryRecallSkeleton /></PageFrame>;
}

export function MemoryOverviewSkeleton() {
  return <CardGridSkeleton columns={2} />;
}

export function MemoryDreamSkeleton() {
  return (
    <div className="grid min-h-0 flex-1 gap-4 overflow-hidden xl:grid-cols-[minmax(20rem,0.8fr)_minmax(32rem,1.7fr)]">
      <div className="flex min-h-0 flex-col gap-4 overflow-hidden">
        <SurfaceSkeleton className="grid content-start gap-3 p-4"><div className="flex items-center justify-between"><Skeleton className="h-5 w-28 rounded-md" /><Skeleton className="h-9 w-24 rounded-xl" /></div><SkeletonText lines={4} /><div className="grid grid-cols-3 gap-2"><Skeleton className="h-14" /><Skeleton className="h-14" /><Skeleton className="h-14" /></div></SurfaceSkeleton>
        <SurfaceSkeleton className="grid content-start gap-3 p-4"><Skeleton className="h-5 w-24 rounded-md" /><div className="flex gap-2"><Skeleton className="h-9 w-24 rounded-xl" /><Skeleton className="h-9 w-24 rounded-xl" /><Skeleton className="h-9 w-24 rounded-xl" /></div></SurfaceSkeleton>
        <SurfaceSkeleton className="min-h-0 flex-1 p-4"><Skeleton className="mb-3 h-5 w-28 rounded-md" /><div className="grid gap-2">{Array.from({ length: 4 }, (_, index) => <div className="grid gap-2 rounded-xl border border-theme-card-border/45 p-3" key={index}><Skeleton className="h-3 w-1/2 rounded-full" /><Skeleton className="h-2.5 w-full rounded-full" /><Skeleton className="h-2.5 w-3/4 rounded-full" /></div>)}</div></SurfaceSkeleton>
      </div>
      <SurfaceSkeleton className="grid min-h-[30rem] content-start gap-4 p-5"><div className="flex items-center justify-between border-b border-theme-card-border/60 pb-4"><Skeleton className="h-5 w-24 rounded-md" /><div className="flex gap-2"><Skeleton className="h-9 w-24 rounded-xl" /><Skeleton className="h-9 w-24 rounded-xl" /></div></div><Skeleton className="h-48 w-full rounded-2xl" /><div className="grid gap-3"><Skeleton className="h-4 w-24 rounded-md" />{Array.from({ length: 4 }, (_, index) => <div className="grid gap-2 rounded-xl border border-theme-card-border/45 p-3" key={index}><Skeleton className="h-2.5 w-1/3 rounded-full" /><Skeleton className="h-3 w-full rounded-full" /><Skeleton className="h-3 w-4/5 rounded-full" /></div>)}</div></SurfaceSkeleton>
    </div>
  );
}

export function MemoryRecallSkeleton() {
  return (
    <div className="grid min-h-0 flex-1 gap-4 overflow-hidden xl:grid-cols-[minmax(21rem,0.75fr)_minmax(32rem,1.65fr)]">
      <div className="flex min-h-0 flex-col gap-4 overflow-hidden"><SurfaceSkeleton className="grid content-start gap-4 p-4"><Skeleton className="h-5 w-28 rounded-md" /><div className="grid grid-cols-2 gap-2"><Skeleton className="h-10" /><Skeleton className="h-10" /></div><Skeleton className="h-10 w-full rounded-xl" /><Skeleton className="h-4 w-3/4 rounded-full" /><div className="flex gap-2"><Skeleton className="h-9 w-24 rounded-xl" /><Skeleton className="h-9 w-28 rounded-xl" /></div></SurfaceSkeleton><SurfaceSkeleton className="grid content-start gap-3 p-4"><Skeleton className="h-5 w-24 rounded-md" /><div className="grid grid-cols-2 gap-2"><Skeleton className="h-16" /><Skeleton className="h-16" /><Skeleton className="h-16" /><Skeleton className="h-16" /></div></SurfaceSkeleton></div>
      <SurfaceSkeleton className="grid min-h-[30rem] content-start gap-4 p-5"><div className="flex items-center justify-between border-b border-theme-card-border/60 pb-4"><Skeleton className="h-5 w-28 rounded-md" /><Skeleton className="h-9 w-24 rounded-xl" /></div><Skeleton className="h-28 w-full rounded-2xl" /><div className="grid gap-3">{Array.from({ length: 4 }, (_, index) => <div className="grid gap-2 rounded-xl border border-theme-card-border/45 p-3" key={index}><Skeleton className="h-3 w-2/5 rounded-full" /><Skeleton className="h-3 w-full rounded-full" /><Skeleton className="h-3 w-4/5 rounded-full" /></div>)}</div></SurfaceSkeleton>
    </div>
  );
}

export function MemoryDetailSkeleton() {
  return (
    <div aria-busy="true" className="grid min-h-[18rem] content-start gap-5 p-5" role="status">
      <div className="flex min-w-0 flex-wrap items-start justify-between gap-3 border-b border-theme-card-border/60 pb-4"><div className="grid min-w-0 flex-1 gap-3"><div className="flex gap-2"><Skeleton className="h-6 w-20 rounded-full" /><Skeleton className="h-6 w-20 rounded-full" /><Skeleton className="h-6 w-24 rounded-full" /></div><Skeleton className="h-7 w-2/3 rounded-lg" /><Skeleton className="h-3 w-40 rounded-full" /></div><div className="flex gap-2"><Skeleton className="h-9 w-20 rounded-xl" /><Skeleton className="h-9 w-20 rounded-xl" /></div></div>
      <SkeletonText lines={5} />
      <div className="grid gap-3"><Skeleton className="h-5 w-20 rounded-md" /><SurfaceSkeleton className="grid grid-cols-2 gap-3 p-3"><Skeleton className="h-8" /><Skeleton className="h-8" /><Skeleton className="h-8" /><Skeleton className="h-8" /></SurfaceSkeleton></div>
      <SurfaceSkeleton className="grid gap-3 p-3"><Skeleton className="h-5 w-28 rounded-md" />{Array.from({ length: 3 }, (_, index) => <div className="grid gap-2 rounded-xl border border-theme-card-border/45 p-3" key={index}><Skeleton className="h-2.5 w-1/3 rounded-full" /><Skeleton className="h-3 w-full rounded-full" /><Skeleton className="h-3 w-4/5 rounded-full" /></div>)}</SurfaceSkeleton>
    </div>
  );
}

function ManualPageSkeleton() {
  return (
    <PageFrame className="mx-auto w-full max-w-5xl">
      <PageHeaderSkeleton />
      <SurfaceSkeleton className="grid gap-5 p-6"><Skeleton className="h-8 w-2/3 rounded-lg" /><SkeletonText lines={4} /><Skeleton className="h-44 w-full rounded-2xl" /><SkeletonText lines={8} /><Skeleton className="h-28 w-4/5 rounded-2xl" /></SurfaceSkeleton>
    </PageFrame>
  );
}
