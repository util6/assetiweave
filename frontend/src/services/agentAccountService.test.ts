/* @vitest-environment jsdom */

import { describe, expect, it, beforeEach, afterEach, vi } from "vitest";
import {
  listAgentAccounts,
  createAgentAccount,
  updateAgentAccount,
  deleteAgentAccount,
} from "./agentAccountService";

function createMockLocalStorage(): Storage {
  const values = new Map<string, string>();
  return {
    get length() {
      return values.size;
    },
    clear: vi.fn(() => values.clear()),
    getItem: vi.fn((key: string) => values.get(key) ?? null),
    key: vi.fn((index: number) => Array.from(values.keys())[index] ?? null),
    removeItem: vi.fn((key: string) => values.delete(key)),
    setItem: vi.fn((key: string, value: string) => values.set(key, value)),
  };
}

describe("agentAccountService", () => {
  beforeEach(() => {
    vi.stubGlobal("localStorage", createMockLocalStorage());
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it("loads initial mock accounts by default", async () => {
    const accounts = await listAgentAccounts();
    expect(accounts.length).toBeGreaterThanOrEqual(3);
    expect(accounts.some((a) => a.provider === "codex")).toBe(true);
    expect(accounts.some((a) => a.provider === "antigravity")).toBe(true);
    expect(accounts.some((a) => a.provider === "claude")).toBe(true);
  });

  it("filters accounts by provider", async () => {
    const codexAccounts = await listAgentAccounts("codex");
    expect(codexAccounts.every((a) => a.provider === "codex")).toBe(true);
    expect(codexAccounts.length).toBeGreaterThanOrEqual(2);
  });

  it("creates, updates, and deletes an account", async () => {
    const created = await createAgentAccount({
      provider: "codex",
      displayName: "自动化测试专用号",
      email: "test@ai.org",
      authType: "oauth",
      state: "active",
    });

    expect(created.id).toContain("acc_codex_");
    expect(created.displayName).toBe("自动化测试专用号");

    const updated = await updateAgentAccount(created.id, {
      displayName: "自动化测试专用号 (已修改)",
      state: "rate_limited",
    });

    expect(updated?.displayName).toBe("自动化测试专用号 (已修改)");
    expect(updated?.state).toBe("rate_limited");

    const deleted = await deleteAgentAccount(created.id);
    expect(deleted).toBe(true);

    const accountsAfter = await listAgentAccounts();
    expect(accountsAfter.some((a) => a.id === created.id)).toBe(false);
  });
});
