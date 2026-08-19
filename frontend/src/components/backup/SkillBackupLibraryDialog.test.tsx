/* @vitest-environment jsdom */

import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { SkillBackupLibraryDialog } from "./SkillBackupLibraryDialog";

vi.mock("../../i18n/I18nProvider", () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock("../../services/catalog", () => ({
  getSkillBackupSettings: vi.fn().mockResolvedValue({
    default_root_path: "~/.assetiweave/library/skills",
    display_default_root_path: "~/.assetiweave/library/skills",
    display_root_path: "~/.assetiweave/library/skills",
    exists: true,
    expanded_root_path: "~/.assetiweave/library/skills",
    is_default_root: true,
    root_path: "~/.assetiweave/library/skills",
  }),
  selectTargetDirectory: vi.fn(),
  updateSkillBackupSettings: vi.fn(),
}));

afterEach(cleanup);

describe("SkillBackupLibraryDialog", () => {
  it.each([
    [undefined, "z-50"],
    ["nested" as const, "z-[60]"],
  ])("uses the %s semantic layer", (layer, expectedClass) => {
    render(
      <SkillBackupLibraryDialog
        layer={layer}
        onClose={vi.fn()}
        onNotifyError={vi.fn()}
        open
      />,
    );

    expect(screen.getByRole("dialog").parentElement?.className).toContain(expectedClass);
  });
});
