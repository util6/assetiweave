export function ToolbarButton({ icon, label }: { icon: React.ReactNode; label: string }) {
  return (
    <button
      className="inline-flex h-9 items-center justify-center gap-1.5 whitespace-nowrap rounded-xl border border-theme-control-border bg-theme-control/95 px-2.5 text-body-sm text-theme-control-fg shadow-[inset_0_1px_0_rgb(var(--theme-inset-highlight)/0.42)] transition-colors hover:bg-theme-control-hover hover:text-on-surface"
      type="button"
    >
      {icon}
      <span>{label}</span>
    </button>
  );
}
