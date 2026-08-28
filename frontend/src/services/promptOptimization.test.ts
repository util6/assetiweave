import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  buildPromptOptimizationPrompt,
  checkPromptOptimizationAvailability,
  optimizePromptContent,
} from "./promptOptimization";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

describe("promptOptimization", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
  });

  it("preserves the input language in the default optimization prompt", () => {
    const prompt = buildPromptOptimizationPrompt({
      text: "完善这个实现任务",
    });

    expect(prompt).toContain("Preserve the working language of the input");
    expect(prompt).toContain("完善这个实现任务");
    expect(prompt).not.toContain("Translate the content");
  });

  it("invokes the dedicated prompt optimization command", async () => {
    vi.stubGlobal("window", { __TAURI_INTERNALS__: {} });
    vi.mocked(invoke).mockResolvedValueOnce({ optimized_text: "明确验收标准" });

    await expect(optimizePromptContent({
      agentId: "opencode",
      model: "model/a",
      provider: "cli",
      text: "完善任务",
    })).resolves.toEqual({ optimized_text: "明确验收标准" });

    expect(invoke).toHaveBeenCalledWith("optimize_prompt", {
      params: {
        agent_id: "opencode",
        model: "model/a",
        prompt: expect.stringContaining("完善任务"),
        provider: "cli",
      },
    });
  });

  it("checks the dedicated prompt optimization assignment", async () => {
    vi.stubGlobal("window", { __TAURI_INTERNALS__: {} });
    vi.mocked(invoke).mockResolvedValueOnce({ available: true, error: null, version: "1.0.0" });

    await expect(checkPromptOptimizationAvailability()).resolves.toEqual({
      available: true,
      error: null,
      version: "1.0.0",
    });
    expect(invoke).toHaveBeenCalledWith("check_prompt_optimization_availability");
  });
});
