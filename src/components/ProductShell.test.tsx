import { invoke } from "@tauri-apps/api/core";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import {
  AccountScreen,
  HistoryScreen,
  MicrophoneSettings,
  Onboarding,
  SettingsScreen,
} from "./ProductShell";
import type { DictationSnapshot } from "../types/dictation";
import type { ProductSettings } from "../types/product";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const invokeMock = vi.mocked(invoke);
const settings: ProductSettings = {
  onboardingState: "in_progress",
  onboardingStep: "welcome",
  historyEnabled: true,
  analyticsEnabled: false,
  showOverlay: true,
  dictationShortcut: "Option+Space",
  rewriteShortcut: "Option+Shift+Space",
  dictationPaused: false,
  microphoneDeviceId: null,
  microphoneDeviceName: null,
};
const snapshot = {
  state: "idle",
  microphonePermission: "granted",
  accessibilityPermission: "granted",
  projectId: "project-1",
} as DictationSnapshot;

describe("Milestone 7 product surfaces", () => {
  beforeEach(() => invokeMock.mockReset());

  it("persists the next onboarding step through Rust", async () => {
    invokeMock.mockResolvedValue({ ...settings, onboardingStep: "microphone" });
    const refresh = vi.fn().mockResolvedValue(undefined);
    render(
      <Onboarding
        settings={settings}
        snapshot={snapshot}
        refresh={refresh}
        onAccount={vi.fn()}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Get started" }));
    expect(invokeMock).toHaveBeenCalledWith("update_onboarding", {
      update: { state: "in_progress", step: "microphone" },
    });
  });

  it("opens the microphone privacy pane after permission denial", async () => {
    invokeMock.mockResolvedValue(undefined);
    render(
      <Onboarding
        settings={{ ...settings, onboardingStep: "microphone" }}
        snapshot={{ ...snapshot, microphonePermission: "denied" }}
        refresh={vi.fn().mockResolvedValue(undefined)}
        onAccount={vi.fn()}
      />,
    );
    fireEvent.click(
      screen.getByRole("button", { name: "Open Microphone settings" }),
    );
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "open_microphone_settings_command",
      ),
    );
  });

  it("loads bounded local history and exposes copy/delete actions", async () => {
    invokeMock.mockResolvedValue({
      items: [
        {
          id: "h1",
          kind: "dictation",
          projectName: "General",
          voiceName: "My Voice",
          modeName: "Clean Dictation",
          sourceApp: "TextEdit",
          finalText: "A saved result.",
          truncated: false,
          status: "completed",
          errorCode: null,
          createdAt: "2026-08-30 10:00:00",
        },
      ],
      total: 1,
      hasMore: false,
    });
    render(<HistoryScreen />);
    expect(await screen.findByText("A saved result.")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Copy result" }));
    expect(invokeMock).toHaveBeenCalledWith("copy_history_item", { id: "h1" });
  });

  it("saves explicit history, analytics, and overlay preferences", async () => {
    invokeMock.mockImplementation(async (command) =>
      command === "diagnostics"
        ? {
            applicationVersion: "0.1.0",
            os: "macos",
            architecture: "aarch64",
            environment: "test",
            schemaVersion: 4,
            transcriptionProvider: "deepgram",
            transformationProvider: "openai",
            authenticationConfigured: false,
            analyticsConfigured: false,
            crashReportingConfigured: false,
            microphonePermission: "granted",
            accessibilityPermission: "granted",
            lastErrorCategory: null,
          }
        : { ...settings, onboardingState: "ready", onboardingStep: "complete" },
    );
    render(
      <SettingsScreen
        settings={{
          ...settings,
          onboardingState: "ready",
          onboardingStep: "complete",
        }}
        onSaved={vi.fn()}
      />,
    );
    fireEvent.click(
      screen.getByLabelText("Share content-free product analytics"),
    );
    fireEvent.click(screen.getByRole("button", { name: "Save settings" }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("update_product_settings", {
        update: {
          historyEnabled: true,
          analyticsEnabled: true,
          showOverlay: true,
        },
      }),
    );
  });

  it("prevents duplicate settings writes while a save is pending", async () => {
    let finishRegistration: (() => void) | undefined;
    invokeMock.mockImplementation((command) => {
      if (command === "diagnostics")
        return Promise.resolve({
          applicationVersion: "0.1.0",
          os: "macos",
          architecture: "aarch64",
          environment: "test",
          schemaVersion: 4,
          transcriptionProvider: "deepgram",
          transformationProvider: "openai",
          authenticationConfigured: false,
          analyticsConfigured: false,
          crashReportingConfigured: false,
          microphonePermission: "granted",
          accessibilityPermission: "granted",
          lastErrorCategory: null,
        });
      if (command === "update_shortcuts")
        return new Promise((resolve) => {
          finishRegistration = () => resolve(undefined);
        });
      return Promise.resolve(settings);
    });
    render(<SettingsScreen settings={settings} onSaved={vi.fn()} />);
    const save = screen.getByRole("button", { name: "Save settings" });
    fireEvent.click(save);
    expect(
      await screen.findByRole("button", { name: "Saving settings…" }),
    ).toBeDisabled();
    fireEvent.click(save);
    expect(
      invokeMock.mock.calls.filter(
        ([command]) => command === "update_shortcuts",
      ),
    ).toHaveLength(1);
    finishRegistration?.();
    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: "Save settings" }),
      ).toBeEnabled(),
    );
  });

  it("renders an actionable configured signed-out account state", async () => {
    invokeMock.mockResolvedValue({
      state: "signedOut",
      configured: true,
      signedIn: false,
      email: null,
      provider: null,
      offline: false,
      cloudReady: false,
    });
    render(<AccountScreen />);
    expect(
      await screen.findByText(
        /sign in is required before cloud transcription/i,
      ),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Sign in or create account" }),
    ).toBeEnabled();
  });

  describe("microphone selection", () => {
    const devices = [
      { id: "coreaudio:built-in", name: "MacBook Microphone", isDefault: true },
      { id: "coreaudio:usb", name: "Studio USB Mic", isDefault: false },
    ];

    it("lists devices and persists a selection through Rust", async () => {
      invokeMock.mockImplementation((command, args) => {
        if (command === "list_microphones")
          return Promise.resolve({
            devices,
            selectedId: null,
            selectedName: null,
            selectedAvailable: true,
          });
        if (command === "select_microphone")
          return Promise.resolve({
            devices,
            selectedId: (args as { deviceId: string }).deviceId,
            selectedName: "Studio USB Mic",
            selectedAvailable: true,
          });
        // The suite-level beforeEach returns the mock, which Vitest then
        // invokes as a cleanup hook with no command.
        return Promise.resolve(undefined);
      });
      render(<MicrophoneSettings />);
      const picker = await screen.findByLabelText("Microphone");
      await waitFor(() => expect(picker).toBeEnabled());
      expect(
        screen.getByRole("option", {
          name: "System default (MacBook Microphone)",
        }),
      ).toBeInTheDocument();
      fireEvent.change(picker, { target: { value: "coreaudio:usb" } });
      await waitFor(() =>
        expect(invokeMock).toHaveBeenCalledWith("select_microphone", {
          deviceId: "coreaudio:usb",
        }),
      );
      await waitFor(() => expect(picker).toHaveValue("coreaudio:usb"));
    });

    it("returns to the system default with a null device", async () => {
      invokeMock.mockResolvedValue({
        devices,
        selectedId: "coreaudio:usb",
        selectedName: "Studio USB Mic",
        selectedAvailable: true,
      });
      render(<MicrophoneSettings />);
      const picker = await screen.findByLabelText("Microphone");
      await waitFor(() => expect(picker).toHaveValue("coreaudio:usb"));
      fireEvent.change(picker, { target: { value: "" } });
      await waitFor(() =>
        expect(invokeMock).toHaveBeenCalledWith("select_microphone", {
          deviceId: null,
        }),
      );
    });

    it("explains the fallback when the saved microphone is disconnected", async () => {
      invokeMock.mockResolvedValue({
        devices: devices.slice(0, 1),
        selectedId: "coreaudio:usb",
        selectedName: "Studio USB Mic",
        selectedAvailable: false,
      });
      render(<MicrophoneSettings />);
      expect(
        await screen.findByText(/Studio USB Mic is disconnected/),
      ).toBeInTheDocument();
      expect(
        screen.getByRole("option", { name: "Studio USB Mic (disconnected)" }),
      ).toBeDisabled();
    });

    it("surfaces enumeration errors and rejects malformed payloads", async () => {
      invokeMock.mockResolvedValue({ devices: "nope" });
      render(<MicrophoneSettings />);
      expect(await screen.findByRole("alert")).toHaveTextContent(
        "Invalid microphone payload",
      );
    });
  });
});
