export function ToolbarButton({ icon, label }: { icon: React.ReactNode; label: string }) {
  return (
    <button
      className="inline-flex h-9 items-center justify-center gap-1.5 whitespace-nowrap rounded-2xl border border-theme-control-border/70 bg-theme-control/68 px-2.5 text-body-sm text-theme-control-fg shadow-[var(--theme-shadow-control-inset)] backdrop-blur-md transition-all hover:-translate-y-0.5 hover:bg-theme-control-hover hover:text-on-surface active:translate-y-0"
      type="button"
    >
      {icon}
      <span>{label}</span>
    </button>
  );
}
