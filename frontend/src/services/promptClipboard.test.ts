/* @vitest-environment jsdom */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { copyPromptImagesToClipboard, copyPromptTextToClipboard } from "./promptClipboard";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(async () => undefined),
}));

beforeEach(() => {
  Object.defineProperty(navigator, "clipboard", {
    configurable: true,
    value: {
      writeText: vi.fn(async () => undefined),
    },
  });
});

afterEach(() => {
  vi.clearAllMocks();
  delete (window as typeof window & { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
});

describe("promptClipboard", () => {
  it("uses the native Tauri clipboard command for image-only copies", async () => {
    Object.defineProperty(window, "__TAURI_INTERNALS__", {
      configurable: true,
      value: {},
    });
    const attachment = {
      dataUrl: "data:image/png;base64,ZGlhZ3JhbQ==",
      mimeType: "image/png",
      name: "diagram.png",
    };

    await copyPromptImagesToClipboard([attachment]);

    expect(invoke).toHaveBeenCalledWith("copy_prompt_card_to_clipboard", {
      params: {
        attachments: [attachment],
        text: "",
      },
    });
    expect(navigator.clipboard.writeText).not.toHaveBeenCalled();
  });

  it("falls back to image placeholders when image clipboard writes are unavailable outside Tauri", async () => {
    await copyPromptImagesToClipboard([{
      dataUrl: "data:image/png;base64,ZGlhZ3JhbQ==",
      mimeType: "image/png",
      name: "diagram.png",
    }]);

    expect(invoke).not.toHaveBeenCalled();
    expect(navigator.clipboard.writeText).toHaveBeenCalledWith("[image: diagram.png]");
  });

  it("copies text through the web clipboard path inside Tauri", async () => {
    Object.defineProperty(window, "__TAURI_INTERNALS__", {
      configurable: true,
      value: {},
    });

    await copyPromptTextToClipboard("Text only prompt.\n");

    expect(invoke).not.toHaveBeenCalled();
    expect(navigator.clipboard.writeText).toHaveBeenCalledWith("Text only prompt.");
  });
});
