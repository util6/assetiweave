import { describe, expect, it } from "vitest";
import type { Asset } from "../types";
import { abbreviateHomePath, displayAssetPath } from "./path";

describe("path display utilities", () => {
  it("keeps already abbreviated home paths", () => {
    expect(abbreviateHomePath("~/code-space/assetiweave")).toBe("~/code-space/assetiweave");
    expect(abbreviateHomePath("~")).toBe("~");
  });

  it("abbreviates macOS user home paths", () => {
    expect(abbreviateHomePath("/Users/util6/code-space/assetiweave")).toBe("~/code-space/assetiweave");
    expect(abbreviateHomePath("/Users/util6")).toBe("~");
  });

  it("abbreviates Linux user home paths", () => {
    expect(abbreviateHomePath("/home/util6/code-space/assetiweave")).toBe("~/code-space/assetiweave");
    expect(abbreviateHomePath("/home/util6")).toBe("~");
  });

  it("abbreviates Windows user home paths with tilde", () => {
    expect(abbreviateHomePath("C:\\Users\\util6\\code-space\\assetiweave")).toBe("~/code-space/assetiweave");
    expect(abbreviateHomePath("%USERPROFILE%/code-space/assetiweave")).toBe("~/code-space/assetiweave");
  });

  it("leaves non-user paths unchanged", () => {
    expect(abbreviateHomePath("/Volumes/Assets/skills")).toBe("/Volumes/Assets/skills");
  });

  it("prefers the backend display path over the runtime absolute path", () => {
    expect(displayAssetPath({
      absolute_path: "D:\\shared\\skills\\review",
      display_path: "~/portable-label/review",
      relative_path: "review",
    } as Asset)).toBe("~/portable-label/review");
  });
});
