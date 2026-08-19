/* @vitest-environment jsdom */

import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ConfirmDialog } from "./ConfirmDialog";

vi.mock("../../i18n/I18nProvider", () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

afterEach(cleanup);

describe("ConfirmDialog", () => {
  it.each([
    [undefined, "z-50"],
    ["nested" as const, "z-[60]"],
  ])("uses the %s semantic layer", (layer, expectedClass) => {
    render(
      <ConfirmDialog
        busy={false}
        layer={layer}
        message="Are you sure?"
        onClose={vi.fn()}
        onConfirm={vi.fn()}
        open
        title="Confirm"
      />,
    );

    expect(screen.getByRole("dialog").parentElement?.className).toContain(expectedClass);
  });
});
