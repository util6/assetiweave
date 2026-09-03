import { describe, expect, it } from "vitest";
import { fallbackNavigationModel } from "../mock/catalog";
import { navigationPath } from "./navigationPath";

describe("navigationPath", () => {
  it("maps skills tabs to their route paths", () => {
    expect(navigationPath(fallbackNavigationModel, "overview")).toBe(
      "/skills/overview",
    );
    expect(navigationPath(fallbackNavigationModel, "sources")).toBe(
      "/skills/sources",
    );
    expect(navigationPath(fallbackNavigationModel, "groups")).toBe(
      "/skills/groups",
    );
    expect(navigationPath(fallbackNavigationModel, "mounts")).toBe(
      "/skills/mounts",
    );
  });

  it("maps conversations tabs to their route paths", () => {
    const model = {
      ...fallbackNavigationModel,
      activeHeaderTabId: "conversations",
    };
    expect(navigationPath(model, "sessions")).toBe("/conversations/sessions");
    expect(navigationPath(model, "web-records")).toBe(
      "/conversations/web-records",
    );
  });

  it("maps prompts overview tab to its route path", () => {
    const model = {
      ...fallbackNavigationModel,
      activeHeaderTabId: "prompts",
    };
    expect(navigationPath(model, "overview")).toBe("/prompts/overview");
  });

  it("maps memory tabs to their route paths", () => {
    const model = {
      ...fallbackNavigationModel,
      activeHeaderTabId: "memory",
    };
    expect(navigationPath(model, "recent")).toBe("/memory/recent");
    expect(navigationPath(model, "recall")).toBe("/memory/recall");
  });

  it("maps team tab to its route path", () => {
    const model = {
      ...fallbackNavigationModel,
      activeHeaderTabId: "team",
    };
    expect(navigationPath(model, "overview")).toBe("/team/overview");
  });

  it("routes retired conversation tabs and unimplemented tabs to /under-construction", () => {
    const model = {
      ...fallbackNavigationModel,
      activeHeaderTabId: "conversations",
    };
    expect(navigationPath(model, "sources")).toBe("/under-construction");
    expect(navigationPath(model, "adapters")).toBe("/under-construction");

    const mcpModel = {
      ...fallbackNavigationModel,
      activeHeaderTabId: "mcp",
    };
    expect(navigationPath(mcpModel, "servers")).toBe("/under-construction");
  });
});
