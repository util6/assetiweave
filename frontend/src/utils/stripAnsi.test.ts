import { describe, expect, it } from "vitest";
import { stripAnsi } from "./stripAnsi";

describe("stripAnsi", () => {
  it("strips common ANSI color and style escape sequences", () => {
    const raw =
      "\u001b[31mRed text\u001b[0m and \u001b[32;1mBold green\u001b[0m";
    expect(stripAnsi(raw)).toBe("Red text and Bold green");
  });

  it("handles complex cursor and terminal control codes", () => {
    const raw = "\u001b[?25h\u001b[2K\rStarting process...\u001b[1A";
    expect(stripAnsi(raw)).toBe("\rStarting process...");
  });

  it("returns plain text unchanged and handles null/undefined/empty safely", () => {
    expect(stripAnsi("Simple text")).toBe("Simple text");
    expect(stripAnsi("")).toBe("");
    expect(stripAnsi(null)).toBe("");
    expect(stripAnsi(undefined)).toBe("");
  });
});
