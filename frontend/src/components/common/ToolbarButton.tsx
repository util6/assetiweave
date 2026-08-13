import { toolbarSurfaceRecipe } from "../../theme/recipes";

export function ToolbarButton({ icon, label }: { icon: React.ReactNode; label: string }) {
  return (
    <button
      className={toolbarSurfaceRecipe({ className: "h-9 gap-1.5 px-2.5 text-body-sm" })}
      type="button"
    >
      {icon}
      <span>{label}</span>
    </button>
  );
}
