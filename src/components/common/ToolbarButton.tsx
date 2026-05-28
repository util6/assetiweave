export function ToolbarButton({ icon, label }: { icon: React.ReactNode; label: string }) {
  return (
    <button
      className="inline-flex h-9 items-center justify-center gap-2 whitespace-nowrap rounded-xl border border-border bg-surface-high/90 px-3 text-body-sm text-on-surface-variant shadow-[inset_0_1px_0_rgba(255,255,255,0.04)] transition-colors hover:bg-surface-highest hover:text-on-surface"
      type="button"
    >
      {icon}
      <span>{label}</span>
    </button>
  );
}
