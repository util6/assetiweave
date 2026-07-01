import { describe, expect, it } from "vitest";
import { fallbackNavigationModel } from "../mock/catalog";
import { normalizeNavigationModelRoutes, resolveAppRoute } from "./routes";

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

  it("routes web records to the independent web record page", () => {
    expect(
      resolveAppRoute(
        {
          ...fallbackNavigationModel,
          activeHeaderTabId: "conversations",
          activeSubNavId: "web-records",
        },
        "web-records",
      ),
    ).toBe("web-records");
  });

  it("routes the prompt overview tab to the prompt notes page", () => {
    expect(
      resolveAppRoute(
        {
          ...fallbackNavigationModel,
          activeHeaderTabId: "prompts",
          activeSubNavId: "overview",
        },
        "overview",
      ),
    ).toBe("prompts-overview");
  });

  it("does not route retired conversation source and adapter tabs to the conversations page", () => {
    expect(
      resolveAppRoute(
        {
          ...fallbackNavigationModel,
          activeHeaderTabId: "conversations",
          activeSubNavId: "sources",
        },
        "sources",
      ),
    ).toBe("under-construction");

    expect(
      resolveAppRoute(
        {
          ...fallbackNavigationModel,
          activeHeaderTabId: "conversations",
          activeSubNavId: "adapters",
        },
        "adapters",
      ),
    ).toBe("under-construction");
  });

  it("normalizes retired conversation sub-navigation entries to the sessions tab", () => {
    const normalized = normalizeNavigationModelRoutes({
      ...fallbackNavigationModel,
      activeHeaderTabId: "conversations",
      activeSubNavId: "adapters",
      subNavItems: {
        ...fallbackNavigationModel.subNavItems,
        conversations: [
          { id: "sessions", label: "Sessions", routeKey: "conversations.sessions", enabled: true },
          { id: "web-records", label: "Web Records", routeKey: "conversations.web-records", enabled: true },
          { id: "sources", label: "Sources", routeKey: "conversations.sources", enabled: true },
          { id: "adapters", label: "Adapters", routeKey: "conversations.adapters", enabled: true },
        ],
      },
    });

    expect(normalized.activeSubNavId).toBe("sessions");
    expect(normalized.subNavItems.conversations.map((item) => item.routeKey)).toEqual([
      "conversations.sessions",
      "conversations.web-records",
    ]);
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
          activeSubNavId: "templates",
        },
        "templates",
      ),
    ).toBe("under-construction");
  });
});
