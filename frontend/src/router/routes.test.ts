import { describe, expect, it } from "vitest";
import { fallbackNavigationModel } from "../mock/catalog";
import { normalizeNavigationModelRoutes } from "./routes";
import { createAppRouter } from "./createAppRouter";

describe("app route resolution", () => {
  it("keeps every implemented route on the router", () => {
    const router = createAppRouter();
    const routePaths = Object.keys(router.routesByPath);
    expect(routePaths).toContain("/skills/overview");
    expect(routePaths).toContain("/skills/sources");
    expect(routePaths).toContain("/skills/groups");
    expect(routePaths).toContain("/skills/mounts");
    expect(routePaths).toContain("/conversations/sessions");
    expect(routePaths).toContain("/conversations/web-records");
    expect(routePaths).toContain("/prompts/overview");
    expect(routePaths).toContain("/memory/recent");
    expect(routePaths).toContain("/memory/recall");
    expect(routePaths).toContain("/team/overview");
    expect(routePaths).toContain("/under-construction");
  });

  it("normalizes retired conversation sub-navigation entries to the sessions tab", () => {
    const normalized = normalizeNavigationModelRoutes({
      ...fallbackNavigationModel,
      activeHeaderTabId: "conversations",
      activeSubNavId: "adapters",
      subNavItems: {
        ...fallbackNavigationModel.subNavItems,
        conversations: [
          {
            id: "sessions",
            label: "Sessions",
            routeKey: "conversations.sessions",
            enabled: true,
          },
          {
            id: "web-records",
            label: "Web Records",
            routeKey: "conversations.web-records",
            enabled: true,
          },
          {
            id: "sources",
            label: "Sources",
            routeKey: "conversations.sources",
            enabled: true,
          },
          {
            id: "adapters",
            label: "Adapters",
            routeKey: "conversations.adapters",
            enabled: true,
          },
        ],
      },
    });

    expect(normalized.activeSubNavId).toBe("sessions");
    expect(
      normalized.subNavItems.conversations.map((item) => item.routeKey),
    ).toEqual(["conversations.sessions", "conversations.web-records"]);
  });
});
