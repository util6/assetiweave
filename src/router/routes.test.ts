import { describe, expect, it } from "vitest";
import { fallbackNavigationModel } from "../mock/catalog";
import { resolveAppRoute } from "./routes";

describe("app route resolution", () => {
  it("routes the existing skills groups tab to the skill groups page", () => {
    expect(
      resolveAppRoute(
        {
          ...fallbackNavigationModel,
          activeHeaderTabId: "skills",
          activeSubNavId: "groups",
        },
        "groups",
      ),
    ).toBe("skill-groups");
  });

  it("keeps the existing skill sources route", () => {
    expect(resolveAppRoute(fallbackNavigationModel, "sources")).toBe("sources");
  });

  it("routes the skills mounts tab to the app-centered mount page", () => {
    expect(resolveAppRoute(fallbackNavigationModel, "mounts")).toBe("skill-mounts");
  });

  it("keeps the default skills overview route on the catalog page", () => {
    expect(resolveAppRoute(fallbackNavigationModel, "overview")).toBe("catalog");
  });

  it("routes the conversations tab to the conversations page", () => {
    expect(
      resolveAppRoute(
        {
          ...fallbackNavigationModel,
          activeHeaderTabId: "conversations",
          activeSubNavId: "sessions",
        },
        "sessions",
      ),
    ).toBe("conversations");
  });

  it("keeps all conversation sub-navigation entries on the conversations page", () => {
    expect(
      resolveAppRoute(
        {
          ...fallbackNavigationModel,
          activeHeaderTabId: "conversations",
          activeSubNavId: "sources",
        },
        "sources",
      ),
    ).toBe("conversations");

    expect(
      resolveAppRoute(
        {
          ...fallbackNavigationModel,
          activeHeaderTabId: "conversations",
          activeSubNavId: "adapters",
        },
        "adapters",
      ),
    ).toBe("conversations");
  });

  it("routes enabled but unimplemented navigation entries to the under-construction page", () => {
    expect(
      resolveAppRoute(
        {
          ...fallbackNavigationModel,
          activeHeaderTabId: "mcp",
          activeSubNavId: "servers",
        },
        "servers",
      ),
    ).toBe("under-construction");

    expect(
      resolveAppRoute(
        {
          ...fallbackNavigationModel,
          activeHeaderTabId: "prompts",
          activeSubNavId: "overview",
        },
        "overview",
      ),
    ).toBe("under-construction");
  });
});
