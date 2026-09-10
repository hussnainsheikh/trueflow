import { invoke } from "@tauri-apps/api/core";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { VoiceModeManager } from "./VoiceModeManager";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const invokeMock = vi.mocked(invoke);
const project = {
  id: "project-general",
  name: "General",
  description: "Writing context",
  defaultVoiceId: "voice-my",
  defaultModeId: "mode-clean",
  createdAt: "2026-08-29 12:00:00",
  updatedAt: "2026-08-29 12:00:00",
  archivedAt: null,
  active: true,
};
const voices = [
  {
    id: "voice-my",
    projectId: null,
    name: "My Voice",
    description: "Natural writing style",
    instructions: "Preserve the author's natural style.",
    examples: [],
    avoidances: "Generic corporate prose.",
    isDefault: true,
    isBuiltin: true,
    createdAt: "2026-08-29 12:00:00",
    updatedAt: "2026-08-29 12:00:00",
  },
  {
    id: "voice-kael",
    projectId: project.id,
    name: "Kael",
    description: "Terse character dialogue",
    instructions: "Use restrained, terse sentences.",
    examples: ["Hold the gate."],
    avoidances: "Modern slang.",
    isDefault: false,
    isBuiltin: false,
    createdAt: "2026-08-29 12:00:00",
    updatedAt: "2026-08-29 12:00:00",
  },
];
const modes = [
  {
    id: "mode-clean",
    name: "Clean Dictation",
    description: "Light cleanup only.",
    systemInstructions: "Preserve wording.",
    requiresVoice: false,
    supportsSelection: false,
    isBuiltin: true,
    position: 0,
    createdAt: "2026-08-29 12:00:00",
    updatedAt: "2026-08-29 12:00:00",
  },
  {
    id: "mode-dialogue",
    name: "Dialogue",
    description: "Write character dialogue.",
    systemInstructions: "Apply the selected Voice.",
    requiresVoice: true,
    supportsSelection: false,
    isBuiltin: true,
    position: 2,
    createdAt: "2026-08-29 12:00:00",
    updatedAt: "2026-08-29 12:00:00",
  },
];

describe("VoiceModeManager", () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  it("loads Rust-owned Voices and Modes and persists selections", async () => {
    invokeMock.mockImplementation(async (command, args) => {
      if (command === "list_projects") return [project];
      if (command === "list_voices") return voices;
      if (command === "list_modes") return modes;
      if (command === "set_project_voice") {
        return {
          ...project,
          defaultVoiceId: (args as { voiceId: string }).voiceId,
        };
      }
      if (command === "set_project_mode") {
        return {
          ...project,
          defaultModeId: (args as { modeId: string }).modeId,
        };
      }
      throw new Error(`unexpected command: ${command}`);
    });

    render(<VoiceModeManager />);

    expect((await screen.findAllByText("Kael")).length).toBeGreaterThan(0);
    fireEvent.change(screen.getByLabelText("Selected Voice"), {
      target: { value: "voice-kael" },
    });
    fireEvent.change(screen.getByLabelText("Selected Mode"), {
      target: { value: "mode-dialogue" },
    });

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("set_project_voice", {
        projectId: project.id,
        voiceId: "voice-kael",
      });
      expect(invokeMock).toHaveBeenCalledWith("set_project_mode", {
        projectId: project.id,
        modeId: "mode-dialogue",
      });
    });
  });

  it("creates a project-scoped Voice through a typed command", async () => {
    invokeMock.mockImplementation(async (command, args) => {
      if (command === "list_projects") return [project];
      if (command === "list_voices") return voices;
      if (command === "list_modes") return modes;
      if (command === "create_voice") {
        const input = (args as { input: object }).input;
        return { ...voices[1], ...input, id: "voice-new" };
      }
      throw new Error(`unexpected command: ${command}`);
    });

    render(<VoiceModeManager />);
    await screen.findAllByText("Kael");
    fireEvent.change(screen.getByLabelText("Name"), {
      target: { value: "Elara" },
    });
    fireEvent.change(screen.getByLabelText("Writing instructions"), {
      target: { value: "Write lyrical but precise sentences." },
    });
    fireEvent.click(screen.getByRole("button", { name: "Create Voice" }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("create_voice", {
        input: expect.objectContaining({
          projectId: project.id,
          name: "Elara",
          instructions: "Write lyrical but precise sentences.",
        }),
      }),
    );
  });
});
