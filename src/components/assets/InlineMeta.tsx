import clsx from "clsx";

export function InlineMeta({ label, value, mono = false }: { label: string; value: string; mono?: boolean }) {
  return (
    <div className={clsx("flex min-w-0 items-baseline gap-2", mono ? "max-w-60 shrink-0" : "max-w-[560px] flex-1")}>
      <span className="shrink-0 text-label-caps uppercase text-outline/90">{label}</span>
      <span
        className={clsx(
          "block min-w-0 overflow-hidden text-ellipsis whitespace-nowrap text-body-sm font-semibold text-on-surface/90",
          mono && "font-mono text-primary",
        )}
        title={value}
      >
        {value}
      </span>
    </div>
  );
}
