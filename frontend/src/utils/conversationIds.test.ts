import { describe, expect, it } from "vitest";
import { conversationIdFragment } from "./conversationIds";

describe("conversationIdFragment", () => {
  it("extracts the first eight lowercase characters from a conversation hash", () => {
    expect(
      conversationIdFragment(
        "conversation-session-ABCDEF0123456789abcdef0123456789abcdef0123456789abcdef0123456789",
      ),
    ).toBe("abcdef01");
  });

  it("extracts the owner hash from a content block id", () => {
    expect(
      conversationIdFragment(
        "conversation-part-1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef-answer",
      ),
    ).toBe("12345678");
  });

  it("extracts the same fragment from web record ids", () => {
    expect(
      conversationIdFragment(
        "web-record-session-fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210",
      ),
    ).toBe("fedcba98");
  });

  it("supports legacy hash lengths and falls back when no hash exists", () => {
    expect(conversationIdFragment("conversation-session-abcdef1234567890abcdef1234567890")).toBe("abcdef12");
    expect(conversationIdFragment("  Legacy-Session  ")).toBe("legacy-s");
    expect(conversationIdFragment("   ")).toBe("");
  });
});
