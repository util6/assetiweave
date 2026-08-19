/* @vitest-environment jsdom */

import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { useConversationsController } from "./useConversationsController";

vi.mock("../../app/backgroundTasks/ConversationSyncProvider", () => ({
  useConversationSync: () => ({
    startSync: vi.fn(),
    taskFor: vi.fn(() => null),
  }),
}));

vi.mock("../../app/backgroundTasks/SearchIndexProvider", () => ({
  useSearchIndex: () => ({
    rebuild: vi.fn(),
    status: null,
    task: null,
  }),
}));

vi.mock("../../store/settings/AppSettingsProvider", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../../store/settings/AppSettingsProvider")>();
  return {
    ...actual,
    useAppSettings: () => ({ settings: { id: "settings" } }),
  };
});

afterEach(cleanup);

function Fixture({ recordKind }: { recordKind: "session" | "web" }) {
  const controller = useConversationsController({ recordKind });

  return (
    <>
      <output data-testid="view">{controller.sessionView}</output>
      <output data-testid="output-root">{controller.outputRoot}</output>
      <output data-testid="selected-question-count">{controller.selectedQuestionIds.size}</output>
      <output data-testid="selection">
        {[controller.selectedAppId, controller.selectedProjectKey, controller.selectedSessionId, controller.selectedQuestionId].join("/")}
      </output>
      <button onClick={() => controller.openSession("session-1")} type="button">
        Open session
      </button>
      <button
        onClick={() => controller.openConversationTarget({
          appId: "app-1",
          projectKey: "project-1",
          questionId: "question-1",
          searchTarget: null,
          sessionId: "session-1",
        })}
        type="button"
      >
        Open target
      </button>
      <button onClick={() => controller.toggleQuestionSelection("question-1", true)} type="button">
        Select question
      </button>
    </>
  );
}

describe("useConversationsController", () => {
  it("owns session navigation and selection state", () => {
    render(<Fixture recordKind="session" />);

    fireEvent.click(screen.getByRole("button", { name: "Open session" }));
    fireEvent.click(screen.getByRole("button", { name: "Select question" }));

    expect(screen.getByTestId("view").textContent).toBe("detail");
    expect(screen.getByTestId("selected-question-count").textContent).toBe("1");
  });

  it("centralizes app, project, session, and question target transitions", () => {
    render(<Fixture recordKind="session" />);

    fireEvent.click(screen.getByRole("button", { name: "Open target" }));

    expect(screen.getByTestId("view").textContent).toBe("detail");
    expect(screen.getByTestId("selection").textContent).toBe("app-1/project-1/session-1/question-1");
  });

  it("resets record-kind-specific view state when switching to web records", () => {
    const { rerender } = render(<Fixture recordKind="session" />);

    fireEvent.click(screen.getByRole("button", { name: "Open session" }));
    rerender(<Fixture recordKind="web" />);

    expect(screen.getByTestId("view").textContent).toBe("browser");
    expect(screen.getByTestId("output-root").textContent).toBe("~/Desktop/assetiweave-web-records");
    expect(screen.getByTestId("selected-question-count").textContent).toBe("0");
  });
});
