export type Voice = {
  id: string;
  projectId: string | null;
  name: string;
  description: string;
  instructions: string;
  examples: string[];
  avoidances: string;
  isDefault: boolean;
  isBuiltin: boolean;
  createdAt: string;
  updatedAt: string;
};

export type VoiceInput = {
  projectId: string;
  name: string;
  description: string;
  instructions: string;
  examples: string[];
  avoidances: string;
};

export type Mode = {
  id: string;
  name: string;
  description: string;
  systemInstructions: string;
  requiresVoice: boolean;
  supportsSelection: boolean;
  isBuiltin: boolean;
  position: number;
  createdAt: string;
  updatedAt: string;
};

export function parseVoices(value: unknown): Voice[] {
  if (!Array.isArray(value) || !value.every(isVoice)) {
    throw new Error("Invalid Voices payload");
  }
  return value;
}

export function parseVoice(value: unknown): Voice {
  if (!isVoice(value)) throw new Error("Invalid Voice payload");
  return value;
}

export function parseModes(value: unknown): Mode[] {
  if (!Array.isArray(value) || !value.every(isMode)) {
    throw new Error("Invalid Modes payload");
  }
  return value;
}

function isVoice(value: unknown): value is Voice {
  return (
    isRecord(value) &&
    typeof value.id === "string" &&
    (typeof value.projectId === "string" || value.projectId === null) &&
    typeof value.name === "string" &&
    typeof value.description === "string" &&
    typeof value.instructions === "string" &&
    stringArray(value.examples) &&
    typeof value.avoidances === "string" &&
    typeof value.isDefault === "boolean" &&
    typeof value.isBuiltin === "boolean" &&
    typeof value.createdAt === "string" &&
    typeof value.updatedAt === "string"
  );
}

function isMode(value: unknown): value is Mode {
  return (
    isRecord(value) &&
    typeof value.id === "string" &&
    typeof value.name === "string" &&
    typeof value.description === "string" &&
    typeof value.systemInstructions === "string" &&
    typeof value.requiresVoice === "boolean" &&
    typeof value.supportsSelection === "boolean" &&
    typeof value.isBuiltin === "boolean" &&
    typeof value.position === "number" &&
    typeof value.createdAt === "string" &&
    typeof value.updatedAt === "string"
  );
}

function stringArray(value: unknown): value is string[] {
  return (
    Array.isArray(value) && value.every((item) => typeof item === "string")
  );
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}
