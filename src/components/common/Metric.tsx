export function Metric({ label, value }: { label: string; value: string | number }) {
  return (
    <div className="flex min-h-14 items-center justify-between rounded-xl border border-border bg-surface-card/60 px-3.5 py-3">
      <span className="text-label-caps uppercase text-outline">{label}</span>
      <strong className="text-h2 font-bold text-primary">{value}</strong>
    </div>
  );
}
