/* @vitest-environment jsdom */

import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { appShortcutIconCatalog } from "../../config/appShortcutIcons";
import { BuiltinAppIconCatalog } from "./BuiltinAppIconCatalog";

afterEach(() => {
  cleanup();
});

describe("BuiltinAppIconCatalog", () => {
  it("renders every scanned built-in icon, including Hermes", () => {
    render(<BuiltinAppIconCatalog title="应用图标" />);

    expect(screen.getByRole("list", { name: "应用图标" })).toBeTruthy();
    expect(screen.getByText("hermes")).toBeTruthy();
    expect(screen.getAllByRole("listitem")).toHaveLength(appShortcutIconCatalog.length);
  });
});
