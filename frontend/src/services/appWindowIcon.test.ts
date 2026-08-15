import { describe, expect, it } from "vitest";
import { appWindowIconAsset } from "./appWindowIcon";

describe("appWindowIcon", () => {
  it("keeps separate display and minimized assets", () => {
    expect(appWindowIconAsset("display")).toContain("app-icon-display.png");
    expect(appWindowIconAsset("minimized")).toContain("app-icon-minimized.png");
  });
});
