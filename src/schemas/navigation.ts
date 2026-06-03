import * as z from "zod";
import type { AppShortcut } from "../types";
import type { NavigationModel } from "../router/types";
import { appKindSchema, assetKindSchema } from "./domain";

export const navigationIconSchema = z.enum([
  "archive",
  "boxes",
  "brain",
  "command",
  "file-code",
  "file-text",
  "gauge",
  "grid",
  "layers",
  "navigation",
  "rocket",
  "settings",
  "shield",
  "sparkles",
]);

export const menuScopeSchema = z.enum(["global", "asset-catalog", "profile", "settings"]);

const localizedNavigationLabelsSchema = z
  .strictObject({
    en: z.string().trim().min(1).optional(),
    zh: z.string().trim().min(1).optional(),
  })
  .optional();

export const railMenuItemSchema = z.strictObject({
  enabled: z.boolean(),
  icon: navigationIconSchema,
  id: z.string().trim().min(1),
  label: z.string().trim().min(1),
  labels: localizedNavigationLabelsSchema,
  position: z.enum(["primary", "secondary"]),
  scope: menuScopeSchema,
});

export const headerTabItemSchema = z.strictObject({
  assetKind: assetKindSchema.optional(),
  enabled: z.boolean(),
  id: z.string().trim().min(1),
  label: z.string().trim().min(1),
  labels: localizedNavigationLabelsSchema,
});

export const subNavItemSchema = z.strictObject({
  enabled: z.boolean(),
  id: z.string().trim().min(1),
  label: z.string().trim().min(1),
  labels: localizedNavigationLabelsSchema,
  routeKey: z.string().trim().min(1),
});

export const navigationModelSchema = z.strictObject({
  activeHeaderTabId: z.string().trim().min(1),
  activeRailId: z.string().trim().min(1),
  activeSubNavId: z.string().trim().min(1),
  headerTabs: z.array(headerTabItemSchema),
  railItems: z.array(railMenuItemSchema),
  subNavItems: z.record(z.string(), z.array(subNavItemSchema)),
}) satisfies z.ZodType<NavigationModel>;

export const appShortcutSchema = z.strictObject({
  accentColor: z.string().regex(/^#[0-9a-fA-F]{6}$/),
  appKind: appKindSchema,
  displayIcon: z.string().trim().min(1).max(48),
  enabled: z.boolean(),
  iconSvg: z
    .strictObject({
      paths: z
        .array(
          z.strictObject({
            clipRule: z.enum(["evenodd", "nonzero"]).optional(),
            d: z.string().trim().min(1),
            fillRule: z.enum(["evenodd", "nonzero"]).optional(),
          }),
        )
        .min(1),
      viewBox: z.string().trim().min(1).optional(),
    })
    .nullable()
    .optional(),
  profileId: z.string().trim().min(1),
  profileName: z.string().trim().min(1),
}) satisfies z.ZodType<AppShortcut>;

export const appShortcutListSchema = z.array(appShortcutSchema);
