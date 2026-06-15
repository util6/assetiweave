/* @vitest-environment jsdom */

import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { createRef, useState } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { DialogFrame } from "./DialogFrame";

afterEach(cleanup);

describe("DialogFrame", () => {
  it("closes on Escape when the dialog is interactive", () => {
    const onClose = vi.fn();

    render(
      <DialogFrame onClose={onClose} title="Edit source">
        <button type="button">Save</button>
      </DialogFrame>,
    );

    fireEvent.keyDown(document, { key: "Escape" });

    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("links the dialog to a screen-reader description when visible help text is omitted", () => {
    render(
      <DialogFrame onClose={() => undefined} title="Edit source">
        <button type="button">Save</button>
      </DialogFrame>,
    );

    const dialog = screen.getByRole("dialog");
    const descriptionId = dialog.getAttribute("aria-describedby");

    expect(descriptionId).toBeTruthy();
    expect(document.getElementById(descriptionId as string)?.textContent).toBe("Edit source");
  });

  it("does not close from Escape or the backdrop while busy", () => {
    const onClose = vi.fn();

    render(
      <DialogFrame busy onClose={onClose} title="Import source">
        <button type="button">Importing</button>
      </DialogFrame>,
    );

    fireEvent.keyDown(document, { key: "Escape" });
    const dialog = screen.getByRole("dialog");
    fireEvent.mouseDown(dialog.parentElement as HTMLElement);

    expect(onClose).not.toHaveBeenCalled();
  });

  it("focuses the requested control and keeps Tab focus inside the dialog", async () => {
    const initialFocusRef = createRef<HTMLInputElement>();

    render(
      <DialogFrame initialFocusRef={initialFocusRef} onClose={() => undefined} title="Create group">
        <input ref={initialFocusRef} aria-label="Group name" />
        <button type="button">Create</button>
      </DialogFrame>,
    );

    await waitFor(() => expect(document.activeElement).toBe(initialFocusRef.current));

    const closeButton = screen.getByRole("button", { name: "Close" });
    const createButton = screen.getByRole("button", { name: "Create" });
    createButton.focus();
    fireEvent.keyDown(createButton, { key: "Tab" });

    expect(document.activeElement).toBe(closeButton);
  });

  it("restores focus to the control that opened the dialog", async () => {
    function Fixture() {
      const [open, setOpen] = useState(false);
      return (
        <>
          <button onClick={() => setOpen(true)} type="button">
            Open dialog
          </button>
          {open && (
            <DialogFrame onClose={() => setOpen(false)} title="Confirm action">
              <button type="button">Confirm</button>
            </DialogFrame>
          )}
        </>
      );
    }

    render(<Fixture />);
    const trigger = screen.getByRole("button", { name: "Open dialog" });
    trigger.focus();
    fireEvent.click(trigger);
    fireEvent.keyDown(document, { key: "Escape" });

    await waitFor(() => expect(document.activeElement).toBe(trigger));
  });
});
