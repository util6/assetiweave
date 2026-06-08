import { describe, expect, it } from "vitest";
import { navigationModel } from "../router/menu";
import { hasManualDocument, manualDocuments } from "./registry";

describe("manual registry", () => {
  it("covers every configured sub navigation route", () => {
    const routeKeys = Object.values(navigationModel.subNavItems)
      .flat()
      .filter((item) => item.enabled)
      .map((item) => item.routeKey);

    expect(manualDocuments).toHaveLength(new Set(manualDocuments.map((document) => document.routeKey)).size);
    expect(routeKeys.filter((routeKey) => !hasManualDocument(routeKey))).toEqual([]);
  });

  it("uses route-specific manual overviews instead of shared placeholders", () => {
    const zhOverviews = manualDocuments.map((document) => document.content.zh.overview);
    const enOverviews = manualDocuments.map((document) => document.content.en.overview);

    expect(new Set(zhOverviews)).toHaveLength(manualDocuments.length);
    expect(new Set(enOverviews)).toHaveLength(manualDocuments.length);
  });
});
