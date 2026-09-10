import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useMemo, useState } from "react";
import type { FormEvent } from "react";

import {
  parseProject,
  parseProjects,
  parseVocabulary,
  parseVocabularyEntry,
  vocabularyKinds,
  type Project,
  type ProjectInput,
  type VocabularyEntry,
  type VocabularyInput,
  type VocabularyKind,
} from "../types/projects";

const emptyVocabulary: VocabularyInput = {
  term: "",
  pronunciationHint: null,
  kind: "other",
  description: null,
  aliases: [],
  forbiddenVariants: [],
  caseSensitive: false,
};

export function ProjectVocabularyManager() {
  const [projects, setProjects] = useState<Project[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [vocabulary, setVocabulary] = useState<VocabularyEntry[]>([]);
  const [projectName, setProjectName] = useState("");
  const [projectDescription, setProjectDescription] = useState("");
  const [entry, setEntry] = useState<VocabularyInput>(emptyVocabulary);
  const [editingEntryId, setEditingEntryId] = useState<string | null>(null);
  const [aliasesText, setAliasesText] = useState("");
  const [variantsText, setVariantsText] = useState("");
  const [error, setError] = useState<string | null>(null);

  const selected = useMemo(
    () => projects.find((project) => project.id === selectedId) ?? null,
    [projects, selectedId],
  );

  const loadProjects = useCallback(async () => {
    const loaded = parseProjects(
      await invoke<unknown>("list_projects", { includeArchived: true }),
    );
    setProjects(loaded);
    setSelectedId((current) =>
      loaded.some((project) => project.id === current)
        ? current
        : (loaded.find((project) => project.active)?.id ??
          loaded[0]?.id ??
          null),
    );
  }, []);

  const loadVocabulary = useCallback(async (projectId: string) => {
    setVocabulary(
      parseVocabulary(await invoke<unknown>("list_vocabulary", { projectId })),
    );
  }, []);

  useEffect(() => {
    void loadProjects().catch((reason: unknown) => setError(message(reason)));
  }, [loadProjects]);

  useEffect(() => {
    if (!selectedId) {
      setVocabulary([]);
      return;
    }
    void loadVocabulary(selectedId).catch((reason: unknown) =>
      setError(message(reason)),
    );
  }, [loadVocabulary, selectedId]);

  const mutateProjects = async (operation: () => Promise<unknown>) => {
    setError(null);
    try {
      await operation();
      await loadProjects();
    } catch (reason) {
      setError(message(reason));
    }
  };

  const createProject = async (event: FormEvent) => {
    event.preventDefault();
    const input: ProjectInput = {
      name: projectName,
      description: projectDescription,
    };
    await mutateProjects(async () => {
      const created = parseProject(
        await invoke<unknown>("create_project", { input }),
      );
      setProjectName("");
      setProjectDescription("");
      setSelectedId(created.id);
    });
  };

  const selectProject = async (projectId: string) => {
    await mutateProjects(async () => {
      parseProject(await invoke<unknown>("set_active_project", { projectId }));
      setSelectedId(projectId);
    });
  };

  const renameProject = async (project: Project) => {
    const name = window.prompt("Project name", project.name);
    if (name === null) return;
    await mutateProjects(async () => {
      parseProject(
        await invoke<unknown>("update_project", {
          projectId: project.id,
          input: { name, description: project.description },
        }),
      );
    });
  };

  const archiveProject = async (project: Project) => {
    if (
      !window.confirm(
        `${project.archivedAt ? "Unarchive" : "Archive"} ${project.name}?`,
      )
    ) {
      return;
    }
    await mutateProjects(async () => {
      parseProject(
        await invoke<unknown>(
          project.archivedAt ? "unarchive_project" : "archive_project",
          { projectId: project.id },
        ),
      );
    });
  };

  const deleteProject = async (project: Project) => {
    if (
      !window.confirm(`Permanently delete ${project.name} and its vocabulary?`)
    )
      return;
    await mutateProjects(() =>
      invoke("delete_project", { projectId: project.id }),
    );
  };

  const submitVocabulary = async (event: FormEvent) => {
    event.preventDefault();
    if (!selectedId) return;
    const input: VocabularyInput = {
      ...entry,
      aliases: lines(aliasesText),
      forbiddenVariants: lines(variantsText),
    };
    setError(null);
    try {
      if (editingEntryId) {
        parseVocabularyEntry(
          await invoke<unknown>("update_vocabulary_entry", {
            projectId: selectedId,
            entryId: editingEntryId,
            input,
          }),
        );
      } else {
        parseVocabularyEntry(
          await invoke<unknown>("create_vocabulary_entry", {
            projectId: selectedId,
            input,
          }),
        );
      }
      resetVocabularyForm();
      await loadVocabulary(selectedId);
    } catch (reason) {
      setError(message(reason));
    }
  };

  const editVocabulary = (value: VocabularyEntry) => {
    setEditingEntryId(value.id);
    setEntry({
      term: value.term,
      pronunciationHint: value.pronunciationHint,
      kind: value.kind,
      description: value.description,
      aliases: value.aliases,
      forbiddenVariants: value.forbiddenVariants,
      caseSensitive: value.caseSensitive,
    });
    setAliasesText(value.aliases.join("\n"));
    setVariantsText(value.forbiddenVariants.join("\n"));
  };

  const deleteVocabulary = async (value: VocabularyEntry) => {
    if (!selectedId || !window.confirm(`Delete vocabulary term ${value.term}?`))
      return;
    setError(null);
    try {
      await invoke("delete_vocabulary_entry", {
        projectId: selectedId,
        entryId: value.id,
      });
      await loadVocabulary(selectedId);
    } catch (reason) {
      setError(message(reason));
    }
  };

  const resetVocabularyForm = () => {
    setEditingEntryId(null);
    setEntry(emptyVocabulary);
    setAliasesText("");
    setVariantsText("");
  };

  return (
    <section className="management" aria-labelledby="projects-title">
      <header className="management-header">
        <div>
          <p className="eyebrow">Project context</p>
          <h2 id="projects-title">Projects & vocabulary</h2>
        </div>
        <label>
          Active project
          <select
            aria-label="Active project"
            value={projects.find((project) => project.active)?.id ?? ""}
            onChange={(event) => void selectProject(event.target.value)}
          >
            {projects
              .filter((project) => !project.archivedAt)
              .map((project) => (
                <option key={project.id} value={project.id}>
                  {project.name}
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

      <div className="management-grid">
        <div>
          <h3>Projects</h3>
          <ul className="record-list">
            {projects.map((project) => (
              <li
                key={project.id}
                className={project.id === selectedId ? "selected" : ""}
              >
                <button
                  type="button"
                  className="record-main"
                  onClick={() => setSelectedId(project.id)}
                >
                  <strong>{project.name}</strong>
                  <span>
                    {project.active
                      ? "Active"
                      : project.archivedAt
                        ? "Archived"
                        : "Project"}
                  </span>
                </button>
                <div className="record-actions">
                  {!project.active && !project.archivedAt && (
                    <button
                      type="button"
                      onClick={() => void selectProject(project.id)}
                    >
                      Select
                    </button>
                  )}
                  <button
                    type="button"
                    onClick={() => void renameProject(project)}
                  >
                    Rename
                  </button>
                  <button
                    type="button"
                    onClick={() => void archiveProject(project)}
                  >
                    {project.archivedAt ? "Unarchive" : "Archive"}
                  </button>
                  <button
                    type="button"
                    className="danger"
                    onClick={() => void deleteProject(project)}
                  >
                    Delete
                  </button>
                </div>
              </li>
            ))}
          </ul>
          <form
            className="compact-form"
            onSubmit={(event) => void createProject(event)}
          >
            <h3>Create project</h3>
            <label>
              Name
              <input
                required
                value={projectName}
                onChange={(event) => setProjectName(event.target.value)}
              />
            </label>
            <label>
              Description
              <textarea
                value={projectDescription}
                onChange={(event) => setProjectDescription(event.target.value)}
              />
            </label>
            <button type="submit">Create project</button>
          </form>
        </div>

        <div>
          <h3>{selected ? `${selected.name} vocabulary` : "Vocabulary"}</h3>
          {selected?.archivedAt && (
            <p className="muted">
              Unarchive this project to change its vocabulary.
            </p>
          )}
          <ul className="record-list vocabulary-list">
            {vocabulary.map((value) => (
              <li key={value.id}>
                <div className="record-main static">
                  <strong>{value.term}</strong>
                  <span>
                    {labelKind(value.kind)}
                    {value.pronunciationHint
                      ? ` · ${value.pronunciationHint}`
                      : ""}
                  </span>
                </div>
                <div className="record-actions">
                  <button
                    type="button"
                    disabled={Boolean(selected?.archivedAt)}
                    onClick={() => editVocabulary(value)}
                  >
                    Edit
                  </button>
                  <button
                    type="button"
                    className="danger"
                    disabled={Boolean(selected?.archivedAt)}
                    onClick={() => void deleteVocabulary(value)}
                  >
                    Delete
                  </button>
                </div>
              </li>
            ))}
          </ul>
          {selected && !selected.archivedAt && (
            <form
              className="compact-form"
              onSubmit={(event) => void submitVocabulary(event)}
            >
              <h3>{editingEntryId ? "Edit vocabulary" : "Add vocabulary"}</h3>
              <label>
                Term <small>Correct spelling</small>
                <input
                  required
                  value={entry.term}
                  onChange={(event) =>
                    setEntry({ ...entry, term: event.target.value })
                  }
                />
              </label>
              <label>
                Pronunciation <small>How you say it</small>
                <input
                  value={entry.pronunciationHint ?? ""}
                  onChange={(event) =>
                    setEntry({
                      ...entry,
                      pronunciationHint: event.target.value || null,
                    })
                  }
                />
              </label>
              <label>
                Type
                <select
                  value={entry.kind}
                  onChange={(event) =>
                    setEntry({
                      ...entry,
                      kind: event.target.value as VocabularyKind,
                    })
                  }
                >
                  {vocabularyKinds.map((kind) => (
                    <option key={kind} value={kind}>
                      {labelKind(kind)}
                    </option>
                  ))}
                </select>
              </label>
              <label>
                Description
                <textarea
                  value={entry.description ?? ""}
                  onChange={(event) =>
                    setEntry({
                      ...entry,
                      description: event.target.value || null,
                    })
                  }
                />
              </label>
              <label>
                Aliases <small>One per line</small>
                <textarea
                  value={aliasesText}
                  onChange={(event) => setAliasesText(event.target.value)}
                />
              </label>
              <label>
                Forbidden variants{" "}
                <small>Common transcription mistakes, one per line</small>
                <textarea
                  value={variantsText}
                  onChange={(event) => setVariantsText(event.target.value)}
                />
              </label>
              <label className="checkbox">
                <input
                  type="checkbox"
                  checked={entry.caseSensitive}
                  onChange={(event) =>
                    setEntry({ ...entry, caseSensitive: event.target.checked })
                  }
                />
                Case-sensitive matching
              </label>
              <div className="form-actions">
                <button type="submit">
                  {editingEntryId ? "Save vocabulary" : "Add vocabulary"}
                </button>
                {editingEntryId && (
                  <button type="button" onClick={resetVocabularyForm}>
                    Cancel
                  </button>
                )}
              </div>
            </form>
          )}
        </div>
      </div>
    </section>
  );
}

function lines(value: string): string[] {
  return value
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean);
}

function labelKind(kind: VocabularyKind): string {
  return kind.slice(0, 1).toUpperCase() + kind.slice(1);
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
  return "The project change could not be completed.";
}
