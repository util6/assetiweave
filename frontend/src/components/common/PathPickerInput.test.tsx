/* @vitest-environment jsdom */

import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { PathPickerInput } from "./PathPickerInput";

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("PathPickerInput", () => {
  it("renders a filesystem path input with a right-side picker button", () => {
    const onPick = vi.fn();

    render(
      <PathPickerInput
        aria-label="Source directory"
        onChange={vi.fn()}
        onPick={onPick}
        pickLabel="Choose source directory"
        value="~/code-space/skills"
      />,
    );

    expect(screen.getByLabelText<HTMLInputElement>("Source directory").value).toBe("~/code-space/skills");
    fireEvent.click(screen.getByRole("button", { name: "Choose source directory" }));

    expect(onPick).toHaveBeenCalledTimes(1);
  });

  it("disables the input and picker while a path is being picked", () => {
    render(
      <PathPickerInput
        aria-label="Target directory"
        onChange={vi.fn()}
        onPick={vi.fn()}
        pickLabel="Choose target directory"
        picking
        value="~/.codex/skills"
      />,
    );

    expect(screen.getByLabelText<HTMLInputElement>("Target directory").disabled).toBe(true);
    expect(screen.getByRole<HTMLButtonElement>("button", { name: "Choose target directory" }).disabled).toBe(true);
  });
});
