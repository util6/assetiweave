import { describe, expect, it } from "vitest";
import { Button } from "../ui/button";
import { SurfaceButton } from "./SurfaceButton";

describe("SurfaceButton", () => {
  it("is an alias of the canonical UI Button primitive", () => {
    expect(SurfaceButton).toBe(Button);
  });
});
