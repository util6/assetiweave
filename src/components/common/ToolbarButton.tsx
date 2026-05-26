export function ToolbarButton({ icon, label }: { icon: React.ReactNode; label: string }) {
  return (
    <button
      className="inline-flex h-9 items-center justify-center gap-2 rounded-xl border border-border bg-surface-high px-3 text-body-sm text-on-surface-variant transition-colors hover:bg-surface-highest hover:text-on-surface"
      type="button"
    >
      {icon}
      <span>{label}</span>
    </button>
  );
}
