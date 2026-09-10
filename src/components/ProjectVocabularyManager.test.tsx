import { invoke } from "@tauri-apps/api/core";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { ProjectVocabularyManager } from "./ProjectVocabularyManager";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const invokeMock = vi.mocked(invoke);
const general = {
  id: "project-general",
  name: "General",
  description: "Default writing context",
  defaultVoiceId: null,
  defaultModeId: null,
  createdAt: "2026-08-29 12:00:00",
  updatedAt: "2026-08-29 12:00:00",
  archivedAt: null,
  active: true,
};
const entry = {
  id: "entry-1",
  projectId: general.id,
  term: "Aeryndor",
  pronunciationHint: "air-in-door",
  kind: "place",
  description: "A city",
  aliases: ["Erindor"],
  forbiddenVariants: ["Erin Door"],
  caseSensitive: false,
  createdAt: "2026-08-29 12:00:00",
  updatedAt: "2026-08-29 12:00:00",
};

describe("ProjectVocabularyManager", () => {
  beforeEach(() => invokeMock.mockReset());

  it("loads the active project and its isolated vocabulary", async () => {
    invokeMock.mockImplementation(async (command) => {
      if (command === "list_projects") return [general];
      if (command === "list_vocabulary") return [entry];
      return [];
    });

    render(<ProjectVocabularyManager />);

    expect(await screen.findByText("Aeryndor")).toBeInTheDocument();
    expect(screen.getByLabelText("Active project")).toHaveValue(general.id);
    expect(invokeMock).toHaveBeenCalledWith("list_vocabulary", {
      projectId: general.id,
    });
  });

  it("sends typed vocabulary input and refreshes the selected project", async () => {
    let entries: unknown[] = [];
    invokeMock.mockImplementation(async (command, args) => {
      if (command === "list_projects") return [general];
      if (command === "list_vocabulary") return entries;
      if (command === "create_vocabulary_entry") {
        const input = (args as { input: Record<string, unknown> }).input;
        entries = [{ ...entry, ...input }];
        return entries[0];
      }
      return [];
    });

    render(<ProjectVocabularyManager />);
    await screen.findByText("General vocabulary");
    fireEvent.change(screen.getByLabelText(/Term/), {
      target: { value: "Aeryndor" },
    });
    fireEvent.change(screen.getByLabelText(/Aliases/), {
      target: { value: "Erindor\nAirendor" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Add vocabulary" }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "create_vocabulary_entry",
        expect.objectContaining({
          projectId: general.id,
          input: expect.objectContaining({
            term: "Aeryndor",
            aliases: ["Erindor", "Airendor"],
          }),
        }),
      ),
    );
  });
});
