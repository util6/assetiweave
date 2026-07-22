import { describe, expect, it } from "vitest";
import type { Translator } from "./I18nProvider";
import { headerTabLabel, subNavLabel } from "./navigation";

const t: Translator = (key) => key;

describe("memory navigation labels", () => {
  it("uses the localized header label for the default Memory tab", () => {
    const item = { id: "memory", label: "Memory", enabled: true };

    expect(headerTabLabel(item, t, "zh")).toBe("nav.header.memory");
    expect(headerTabLabel(item, t, "en")).toBe("nav.header.memory");
  });

  it.each([
    ["overview", "Today / Continue", "memory.overview", "nav.sub.memory.overview"],
    ["dreams", "Dreams", "memory.dreams", "nav.sub.memory.dreams"],
    ["recall", "Recall", "memory.recall", "nav.sub.memory.recall"],
    ["library", "Library", "memory.library", "nav.sub.memory.library"],
  ])("uses a stable localized label for Memory %s", (id, label, routeKey, expected) => {
    expect(subNavLabel({ id, label, routeKey, enabled: true }, t, "zh")).toBe(expected);
  });

  it("preserves a user-defined Memory label", () => {
    expect(
      headerTabLabel(
        {
          id: "memory",
          label: "Knowledge Ledger",
          labels: { en: "Knowledge Ledger" },
          enabled: true,
        },
        t,
        "en",
      ),
    ).toBe("Knowledge Ledger");
  });
});
