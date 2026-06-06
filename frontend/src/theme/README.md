# Theme Architecture

Assetiweave uses a TypeScript theme registry, not ad hoc CSS color edits. A theme is a complete visual recipe that defines semantic tokens for surfaces, controls, navigation, buttons, status colors, and effects.

## Adding A Theme

1. Add one `defineTheme(...)` entry in `src/theme/themes.ts`.
2. Keep the existing token shape complete. Do not add partial themes.
3. Provide `id`, `labelKey`, `mode`, `swatches`, and all token groups.
4. Do not edit components to make a theme work. If a new visual pattern is needed, add a recipe in `src/theme/recipes.ts`.

## Component Rules

- Layout utilities are allowed in product components: `flex`, `grid`, `gap-*`, `px-*`, `min-h-*`, `overflow-*`.
- Color, border, shadow, hover, focus, and scrim styles must come from theme tokens or foundation components.
- Prefer `Panel`, `DialogFrame`, `SurfaceButton`, `Badge`, `FieldFrame`, and `EmptyState` from `src/components/foundation`.
- Do not put raw hex, fixed `rgba(...)`, or Tailwind color families such as `bg-purple-*`, `border-slate-*`, or `text-white` into `src/components`, `src/pages`, or `src/layouts`.
- Dynamic user accent colors may be imported from `src/theme/themes.ts`; do not hardcode them in components.
