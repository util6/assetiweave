export function Metric({ label, value }: { label: string; value: string | number }) {
  return (
    <div className="flex min-h-14 items-center justify-between rounded-xl border border-theme-card-border bg-theme-card/70 px-3.5 py-3 shadow-[inset_0_1px_0_rgb(var(--theme-inset-highlight)/0.42)]">
      <span className="text-label-caps uppercase text-outline">{label}</span>
      <strong className="text-h2 font-bold text-primary">{value}</strong>
    </div>
  );
}
