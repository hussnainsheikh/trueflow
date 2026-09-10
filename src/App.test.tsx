import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { MainWindow } from "./App";
import { OverlayView } from "./components/DictationOverlay";
import type { DictationSnapshot } from "./types/dictation";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));

const invokeMock = vi.mocked(invoke);
const listenMock = vi.mocked(listen);

const idleSnapshot: DictationSnapshot = {
  state: "idle",
  operationId: null,
  operationKind: null,
  shortcut: "Option + Space",
  rewriteShortcut: "Option + Shift + Space",
  rewriteStage: null,
  microphonePermission: "granted",
  accessibilityPermission: "granted",
  error: null,
  partialTranscript: null,
  finalTranscript: null,
  transcriptFinality: "none",
  degraded: false,
  insertion: null,
  rewrite: null,
  projectId: null,
  projectName: null,
  voiceId: null,
  voiceName: null,
  modeId: null,
  modeName: null,
  transformation: null,
};
const readyProductSettings = {
  onboardingState: "ready",
  onboardingStep: "complete",
  historyEnabled: true,
  analyticsEnabled: false,
  showOverlay: true,
  dictationShortcut: "Option+Space",
  rewriteShortcut: "Option+Shift+Space",
  dictationPaused: false,
};

describe("MainWindow", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    listenMock.mockReset();
    listenMock.mockResolvedValue(() => undefined);
  });

  it("shows the initialized foundation and Rust dictation status", async () => {
    invokeMock.mockImplementation(async (command) => {
      if (command === "foundation_status") {
        return {
          application: "Trueflow",
          environment: "test",
          schemaVersion: 2,
        };
      }
      if (command === "dictation_status") return idleSnapshot;
      if (command === "product_settings") return readyProductSettings;
      if (command === "list_projects") return [];
      if (command === "list_modes") return [];
      throw new Error(`unexpected command: ${command}`);
    });

    render(<MainWindow />);

    expect(await screen.findByText("Foundation ready")).toBeInTheDocument();
    expect(await screen.findByText("Option + Space")).toBeInTheDocument();
    expect(screen.getAllByText("Granted")).toHaveLength(2);
  });

  it("surfaces foundation initialization failure", async () => {
    invokeMock.mockImplementation(async (command) => {
      if (command === "foundation_status")
        throw new Error("database unavailable");
      if (command === "dictation_status") return idleSnapshot;
      if (command === "product_settings") return readyProductSettings;
      if (command === "list_projects") return [];
      if (command === "list_modes") return [];
      throw new Error(`unexpected command: ${command}`);
    });

    render(<MainWindow />);

    expect(
      await screen.findByText("The local foundation could not initialize."),
    ).toBeInTheDocument();
  });
});

describe("OverlayView", () => {
  it("renders a scalar audio level without receiving audio buffers", () => {
    render(
      <OverlayView
        snapshot={{ ...idleSnapshot, state: "listening", operationId: "op-1" }}
        audioLevel={0.42}
      />,
    );

    expect(screen.getByText("Listening")).toBeInTheDocument();
    expect(
      screen.getByRole("meter", { name: "Microphone level" }),
    ).toHaveAttribute("aria-valuenow", "42");
  });

  it("shows actionable Rust errors", () => {
    render(
      <OverlayView
        snapshot={{
          ...idleSnapshot,
          state: "recoverableError",
          operationId: "op-2",
          microphonePermission: "denied",
          error: {
            code: "MICROPHONE_PERMISSION_DENIED",
            message: "Microphone access is required.",
            retryable: true,
          },
        }}
        audioLevel={0}
      />,
    );

    expect(
      screen.getByText("Microphone access is required."),
    ).toBeInTheDocument();
    expect(screen.queryByRole("meter")).not.toBeInTheDocument();
  });

  it("renders revised partial or final transcript text without audio", () => {
    const { rerender } = render(
      <OverlayView
        snapshot={{
          ...idleSnapshot,
          state: "listening",
          operationId: "op-3",
          partialTranscript: "I think we should",
          transcriptFinality: "partial",
        }}
        audioLevel={0.1}
      />,
    );

    expect(screen.getByText("I think we should")).toBeInTheDocument();

    rerender(
      <OverlayView
        snapshot={{
          ...idleSnapshot,
          state: "completed",
          operationId: "op-3",
          finalTranscript: "I think we should wait.",
          transcriptFinality: "final",
        }}
        audioLevel={0}
      />,
    );
    expect(screen.getByText("I think we should wait.")).toBeInTheDocument();
  });

  it("distinguishes automatic insertion from clipboard-only recovery", () => {
    const copied = {
      ...idleSnapshot,
      state: "recoverableError" as const,
      operationId: "op-4",
      finalTranscript: "Recovered words",
      transcriptFinality: "final" as const,
      error: {
        code: "PASTE_DISPATCH_FAILED",
        message: "Couldn't insert automatically — text copied to clipboard.",
        retryable: true,
      },
      insertion: {
        kind: "copiedToClipboard" as const,
        strategy: "clipboardOnly" as const,
        targetApplication: "Writer",
        durationMs: 2,
        clipboardRestored: false,
        fallbackUsed: true,
        degradedTranscript: false,
        error: null,
      },
    };
    render(<OverlayView snapshot={copied} audioLevel={0} />);

    expect(
      screen.getByText(
        "Couldn't insert automatically — text copied to clipboard.",
      ),
    ).toBeInTheDocument();
    expect(screen.getByText("Recovered words")).toBeInTheDocument();
  });

  it("renders rewrite-specific lifecycle and completion states", () => {
    const { rerender } = render(
      <OverlayView
        snapshot={{
          ...idleSnapshot,
          state: "listening",
          operationId: "rewrite-1",
          operationKind: "selectionRewrite",
          rewriteStage: "listeningForInstruction",
        }}
        audioLevel={0.2}
      />,
    );
    expect(
      screen.getByText("Listening for rewrite instruction"),
    ).toBeInTheDocument();

    rerender(
      <OverlayView
        snapshot={{
          ...idleSnapshot,
          state: "completed",
          operationId: "rewrite-1",
          operationKind: "selectionRewrite",
          rewriteStage: null,
          rewrite: {
            kind: "rewrittenDirectly",
            targetApplication: "TextEdit",
            durationMs: 3,
            clipboardRestored: false,
            fallbackUsed: false,
            error: null,
          },
        }}
        audioLevel={0}
      />,
    );
    expect(screen.getByText("Selection rewritten")).toBeInTheDocument();
  });
});
