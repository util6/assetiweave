/* @vitest-environment jsdom */
import { describe, expect, it } from "vitest";
import { createAppRouter } from "./createAppRouter";

describe("createAppRouter", () => {
  it("工作区导航只改 memory history", async () => {
    const original = window.location.href;
    const router = createAppRouter("/skills/overview");
    await router.navigate({ to: "/skills/groups" });
    expect(router.state.location.pathname).toBe("/skills/groups");
    expect(window.location.href).toBe(original);
  });
});
