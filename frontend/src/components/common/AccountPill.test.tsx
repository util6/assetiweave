// @vitest-environment jsdom

import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";
import { AccountPill } from "./AccountPill";

const storageMock = (() => {
  let store: Record<string, string> = {};
  return {
    getItem: (key: string) => store[key] ?? null,
    setItem: (key: string, value: string) => {
      store[key] = value.toString();
    },
    clear: () => {
      store = {};
    },
    removeItem: (key: string) => {
      delete store[key];
    },
  };
})();
Object.defineProperty(window, "localStorage", {
  value: storageMock,
  writable: true,
});

describe("AccountPill", () => {
  beforeEach(() => {
    storageMock.clear();
  });

  afterEach(() => {
    cleanup();
  });

  it("renders current account display name and provider icon", () => {
    const handleSelect = vi.fn();
    render(
      <AccountPill
        provider="codex"
        currentAccountId="acc_codex_work"
        onSelectAccount={handleSelect}
      />,
    );

    expect(screen.getByText("Codex 工作主号")).not.toBeNull();
  });

  it("opens dropdown and allows switching account", () => {
    const handleSelect = vi.fn();
    render(
      <AccountPill
        provider="codex"
        currentAccountId="acc_codex_work"
        onSelectAccount={handleSelect}
      />,
    );

    const pillButton = screen.getByRole("button", {
      name: /Codex 工作主号/i,
    });
    fireEvent.click(pillButton);

    // Dropdown is visible
    expect(screen.getByText("切换 CODEX 账号")).not.toBeNull();
    const otherAccount = screen.getByText("Codex 个人备用号");
    expect(otherAccount).not.toBeNull();

    fireEvent.click(otherAccount);
    expect(handleSelect).toHaveBeenCalledWith("acc_codex_personal");
  });
});
