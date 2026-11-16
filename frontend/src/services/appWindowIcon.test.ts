import { describe, expect, it } from "vitest";
import { appWindowIconAsset } from "./appWindowIcon";

describe("appWindowIcon", () => {
  it("uses the main display icon asset", () => {
    expect(appWindowIconAsset("display")).toContain("app-icon-display.png");
    expect(appWindowIconAsset()).toContain("app-icon-display.png");
  });
});
