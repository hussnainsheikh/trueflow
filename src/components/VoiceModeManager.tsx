import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useMemo, useState } from "react";
import type { FormEvent } from "react";

import { parseProject, parseProjects, type Project } from "../types/projects";
import {
  parseModes,
  parseVoice,
  parseVoices,
  type Mode,
  type Voice,
  type VoiceInput,
} from "../types/writing";

const emptyVoice = (projectId: string): VoiceInput => ({
  projectId,
  name: "",
  description: "",
  instructions: "",
  examples: [],
  avoidances: "",
});

export function VoiceModeManager() {
  const [projects, setProjects] = useState<Project[]>([]);
  const [projectId, setProjectId] = useState<string | null>(null);
  const [voices, setVoices] = useState<Voice[]>([]);
  const [modes, setModes] = useState<Mode[]>([]);
  const [input, setInput] = useState<VoiceInput | null>(null);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [examplesText, setExamplesText] = useState("");
  const [error, setError] = useState<string | null>(null);

  const project = useMemo(
    () => projects.find((item) => item.id === projectId) ?? null,
    [projectId, projects],
  );

  const loadProjects = useCallback(async () => {
    const loaded = parseProjects(
      await invoke<unknown>("list_projects", { includeArchived: false }),
    );
    setProjects(loaded);
    setProjectId((current) =>
      loaded.some((item) => item.id === current)
        ? current
        : (loaded.find((item) => item.active)?.id ?? loaded[0]?.id ?? null),
    );
  }, []);

  const loadWriting = useCallback(async (selectedProjectId: string) => {
    const [loadedVoices, loadedModes] = await Promise.all([
      invoke<unknown>("list_voices", { projectId: selectedProjectId }),
      invoke<unknown>("list_modes"),
    ]);
    setVoices(parseVoices(loadedVoices));
    setModes(parseModes(loadedModes));
  }, []);

  useEffect(() => {
    void loadProjects().catch((reason: unknown) => setError(message(reason)));
  }, [loadProjects]);

  useEffect(() => {
    if (!projectId) return;
    setInput((current) =>
      current?.projectId === projectId ? current : emptyVoice(projectId),
    );
    setEditingId(null);
    setExamplesText("");
    void loadWriting(projectId).catch((reason: unknown) =>
      setError(message(reason)),
    );
  }, [loadWriting, projectId]);

  const selectVoice = async (voiceId: string) => {
    if (!projectId) return;
    await mutate(async () => {
      const updated = parseProject(
        await invoke<unknown>("set_project_voice", { projectId, voiceId }),
      );
      replaceProject(updated);
    });
  };

  const selectMode = async (modeId: string) => {
    if (!projectId) return;
    await mutate(async () => {
      const updated = parseProject(
        await invoke<unknown>("set_project_mode", { projectId, modeId }),
      );
      replaceProject(updated);
    });
  };

  const submitVoice = async (event: FormEvent) => {
    event.preventDefault();
    if (!projectId || !input) return;
    const submitted = { ...input, examples: lines(examplesText) };
    await mutate(async () => {
      if (editingId) {
        parseVoice(
          await invoke<unknown>("update_voice", {
            voiceId: editingId,
            input: submitted,
          }),
        );
      } else {
        parseVoice(await invoke<unknown>("create_voice", { input: submitted }));
      }
      resetForm(projectId);
      await loadWriting(projectId);
    });
  };

  const editVoice = (voice: Voice) => {
    if (!voice.projectId) return;
    setEditingId(voice.id);
    setInput({
      projectId: voice.projectId,
      name: voice.name,
      description: voice.description,
      instructions: voice.instructions,
      examples: voice.examples,
      avoidances: voice.avoidances,
    });
    setExamplesText(voice.examples.join("\n"));
  };

  const deleteVoice = async (voice: Voice) => {
    if (!projectId || !window.confirm(`Delete Voice ${voice.name}?`)) return;
    await mutate(async () => {
      await invoke("delete_voice", { projectId, voiceId: voice.id });
      await Promise.all([loadProjects(), loadWriting(projectId)]);
    });
  };

  const mutate = async (operation: () => Promise<void>) => {
    setError(null);
    try {
      await operation();
    } catch (reason) {
      setError(message(reason));
    }
  };

  const replaceProject = (updated: Project) => {
    setProjects((current) =>
      current.map((item) => (item.id === updated.id ? updated : item)),
    );
  };

  const resetForm = (selectedProjectId: string) => {
    setEditingId(null);
    setInput(emptyVoice(selectedProjectId));
    setExamplesText("");
  };

  return (
    <section className="management" aria-labelledby="writing-title">
      <header className="management-header">
        <div>
          <p className="eyebrow">Transformation context</p>
          <h2 id="writing-title">Voices & Modes</h2>
        </div>
        <label>
          Project
          <select
            aria-label="Voice project"
            value={projectId ?? ""}
            onChange={(event) => setProjectId(event.target.value)}
          >
            {projects.map((item) => (
              <option key={item.id} value={item.id}>
                {item.name}
              </option>
            ))}
          </select>
        </label>
      </header>

      {error && (
        <p className="management-error" role="alert">
          {error}
        </p>
      )}

      {project && (
        <div className="management-grid">
          <div>
            <h3>Voice</h3>
            <label>
              Selected Voice
              <select
                aria-label="Selected Voice"
                value={project.defaultVoiceId ?? ""}
                onChange={(event) => void selectVoice(event.target.value)}
              >
                {voices.map((voice) => (
                  <option key={voice.id} value={voice.id}>
                    {voice.name}
                  </option>
                ))}
              </select>
            </label>
            <ul className="record-list writing-list">
              {voices.map((voice) => (
                <li key={voice.id}>
                  <div className="record-main static">
                    <strong>{voice.name}</strong>
                    <span>{voice.description || voice.instructions}</span>
                  </div>
                  {!voice.isBuiltin && (
                    <div className="record-actions">
                      <button type="button" onClick={() => editVoice(voice)}>
                        Edit
                      </button>
                      <button
                        type="button"
                        className="danger"
                        onClick={() => void deleteVoice(voice)}
                      >
                        Delete
                      </button>
                    </div>
                  )}
                </li>
              ))}
            </ul>

            {input && (
              <form
                className="compact-form"
                onSubmit={(event) => void submitVoice(event)}
              >
                <h3>{editingId ? "Edit Voice" : "Create Voice"}</h3>
                <label>
                  Name
                  <input
                    required
                    value={input.name}
                    onChange={(event) =>
                      setInput({ ...input, name: event.target.value })
                    }
                  />
                </label>
                <label>
                  Description
                  <textarea
                    value={input.description}
                    onChange={(event) =>
                      setInput({ ...input, description: event.target.value })
                    }
                  />
                </label>
                <label>
                  Writing instructions
                  <textarea
                    required
                    value={input.instructions}
                    onChange={(event) =>
                      setInput({ ...input, instructions: event.target.value })
                    }
                  />
                </label>
                <label>
                  Examples <small>One representative example per line</small>
                  <textarea
                    value={examplesText}
                    onChange={(event) => setExamplesText(event.target.value)}
                  />
                </label>
                <label>
                  Avoidances
                  <textarea
                    value={input.avoidances}
                    onChange={(event) =>
                      setInput({ ...input, avoidances: event.target.value })
                    }
                  />
                </label>
                <div className="form-actions">
                  <button type="submit">
                    {editingId ? "Save Voice" : "Create Voice"}
                  </button>
                  {editingId && (
                    <button type="button" onClick={() => resetForm(project.id)}>
                      Cancel
                    </button>
                  )}
                </div>
              </form>
            )}
          </div>

          <div>
            <h3>Mode</h3>
            <label>
              Selected Mode
              <select
                aria-label="Selected Mode"
                value={project.defaultModeId ?? ""}
                onChange={(event) => void selectMode(event.target.value)}
              >
                {modes.map((mode) => (
                  <option
                    key={mode.id}
                    value={mode.id}
                    disabled={mode.supportsSelection}
                  >
                    {mode.name}
                  </option>
                ))}
              </select>
            </label>
            <ul className="record-list writing-list">
              {modes.map((mode) => (
                <li key={mode.id}>
                  <div className="record-main static">
                    <strong>{mode.name}</strong>
                    <span>{mode.description}</span>
                    {mode.supportsSelection && (
                      <span>
                        Available with selection rewrite in Milestone 6
                      </span>
                    )}
                  </div>
                </li>
              ))}
            </ul>
          </div>
        </div>
      )}
    </section>
  );
}

function lines(value: string): string[] {
  return value
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean);
}

function message(reason: unknown): string {
  if (
    typeof reason === "object" &&
    reason !== null &&
    "message" in reason &&
    typeof reason.message === "string"
  )
    return reason.message;
  if (reason instanceof Error) return reason.message;
  return "The writing configuration could not be updated.";
}
