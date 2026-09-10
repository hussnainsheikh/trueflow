export const vocabularyKinds = [
  "person",
  "place",
  "organization",
  "object",
  "technical",
  "other",
] as const;

export type VocabularyKind = (typeof vocabularyKinds)[number];

export type Project = {
  id: string;
  name: string;
  description: string;
  defaultVoiceId: string | null;
  defaultModeId: string | null;
  createdAt: string;
  updatedAt: string;
  archivedAt: string | null;
  active: boolean;
};

export type VocabularyEntry = {
  id: string;
  projectId: string;
  term: string;
  pronunciationHint: string | null;
  kind: VocabularyKind;
  description: string | null;
  aliases: string[];
  forbiddenVariants: string[];
  caseSensitive: boolean;
  createdAt: string;
  updatedAt: string;
};

export type ProjectInput = { name: string; description: string };

export type VocabularyInput = {
  term: string;
  pronunciationHint: string | null;
  kind: VocabularyKind;
  description: string | null;
  aliases: string[];
  forbiddenVariants: string[];
  caseSensitive: boolean;
};

export function parseProjects(value: unknown): Project[] {
  if (!Array.isArray(value) || !value.every(isProject)) {
    throw new Error("Invalid projects payload");
  }
  return value;
}

export function parseProject(value: unknown): Project {
  if (!isProject(value)) throw new Error("Invalid project payload");
  return value;
}

export function parseVocabulary(value: unknown): VocabularyEntry[] {
  if (!Array.isArray(value) || !value.every(isVocabularyEntry)) {
    throw new Error("Invalid vocabulary payload");
  }
  return value;
}

export function parseVocabularyEntry(value: unknown): VocabularyEntry {
  if (!isVocabularyEntry(value)) {
    throw new Error("Invalid vocabulary entry payload");
  }
  return value;
}

function isProject(value: unknown): value is Project {
  return (
    isRecord(value) &&
    typeof value.id === "string" &&
    typeof value.name === "string" &&
    typeof value.description === "string" &&
    nullableString(value.defaultVoiceId) &&
    nullableString(value.defaultModeId) &&
    typeof value.createdAt === "string" &&
    typeof value.updatedAt === "string" &&
    nullableString(value.archivedAt) &&
    typeof value.active === "boolean"
  );
}

function isVocabularyEntry(value: unknown): value is VocabularyEntry {
  return (
    isRecord(value) &&
    typeof value.id === "string" &&
    typeof value.projectId === "string" &&
    typeof value.term === "string" &&
    nullableString(value.pronunciationHint) &&
    vocabularyKinds.some((kind) => kind === value.kind) &&
    nullableString(value.description) &&
    stringArray(value.aliases) &&
    stringArray(value.forbiddenVariants) &&
    typeof value.caseSensitive === "boolean" &&
    typeof value.createdAt === "string" &&
    typeof value.updatedAt === "string"
  );
}

function nullableString(value: unknown): boolean {
  return typeof value === "string" || value === null;
}

function stringArray(value: unknown): value is string[] {
  return (
    Array.isArray(value) && value.every((item) => typeof item === "string")
  );
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}
