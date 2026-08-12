export function Metric({ label, value }: { label: string; value: string | number }) {
  return (
    <div className="aurora-metric flex min-h-14 items-center justify-between rounded-2xl border border-theme-card-border/70 bg-theme-card/65 px-3.5 py-3 shadow-[var(--theme-shadow-card),inset_0_1px_0_rgb(var(--theme-inset-highlight)/0.18)] backdrop-blur-md">
      <span className="text-label-caps uppercase text-outline">{label}</span>
      <strong className="text-h2 font-bold text-primary">{value}</strong>
    </div>
  );
}
