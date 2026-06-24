import clsx from "clsx";

export interface PageMetric {
  label: string;
  value: number | string;
}

export function PageMetrics({
  className,
  metrics,
}: {
  className?: string;
  metrics: PageMetric[];
}) {
  if (metrics.length === 0) {
    return null;
  }

  return (
    <div className={clsx("ml-auto flex max-w-full flex-wrap justify-end gap-2 max-[920px]:ml-0 max-[920px]:justify-start", className)}>
      {metrics.map((metric) => (
        <div
          className="inline-flex h-10 min-w-[5.75rem] shrink-0 items-center justify-between gap-2 whitespace-nowrap rounded-xl border border-theme-control-border bg-theme-control/80 px-3 text-body-sm shadow-[inset_0_1px_0_rgb(var(--theme-inset-highlight)/0.42)]"
          data-page-metric=""
          key={metric.label}
        >
          <span className="whitespace-nowrap text-on-surface-variant">{metric.label}</span>
          <strong className="font-mono text-code-md text-primary">{metric.value}</strong>
        </div>
      ))}
    </div>
  );
}
