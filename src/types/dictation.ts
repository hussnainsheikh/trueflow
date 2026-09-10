export const dictationStates = [
  "idle",
  "starting",
  "listening",
  "finalizing",
  "transforming",
  "inserting",
  "completed",
  "recoverableError",
  "fatalError",
] as const;

export type DictationState = (typeof dictationStates)[number];

export const operationKinds = ["dictation", "selectionRewrite"] as const;
export type OperationKind = (typeof operationKinds)[number];

export const rewriteStages = [
  "capturingSelection",
  "listeningForInstruction",
  "finalizingInstruction",
  "transformingSelection",
  "revalidatingTarget",
  "replacingSelection",
] as const;
export type RewriteStage = (typeof rewriteStages)[number];

export const microphonePermissionStates = [
  "notDetermined",
  "granted",
  "denied",
  "restricted",
  "unavailable",
] as const;

export type MicrophonePermissionState =
  (typeof microphonePermissionStates)[number];

export const accessibilityPermissionStates = [
  "unknown",
  "granted",
  "denied",
  "unavailable",
] as const;

export type AccessibilityPermissionState =
  (typeof accessibilityPermissionStates)[number];

export type DictationError = {
  code: string;
  message: string;
  retryable: boolean;
};

export const transcriptFinalities = [
  "none",
  "partial",
  "final",
  "incomplete",
] as const;

export type TranscriptFinality = (typeof transcriptFinalities)[number];

export const insertionOutcomeKinds = [
  "insertedDirectly",
  "insertedByTyping",
  "insertedViaPaste",
  "copiedToClipboard",
  "failed",
] as const;

export type InsertionOutcomeKind = (typeof insertionOutcomeKinds)[number];

export const insertionStrategies = [
  "directAccessibility",
  "syntheticTyping",
  "paste",
  "clipboardOnly",
  "none",
] as const;

export type InsertionOutcome = {
  kind: InsertionOutcomeKind;
  strategy: (typeof insertionStrategies)[number];
  targetApplication: string | null;
  durationMs: number;
  clipboardRestored: boolean;
  fallbackUsed: boolean;
  degradedTranscript: boolean;
  error: DictationError | null;
};

export type TransformationStatus = {
  provider: string;
  fallbackUsed: boolean;
};

export const rewriteOutcomeKinds = [
  "rewrittenDirectly",
  "rewrittenViaPaste",
  "rewriteResultCopiedToClipboard",
  "rewriteCancelled",
  "rewriteFailed",
] as const;

export type RewriteOutcome = {
  kind: (typeof rewriteOutcomeKinds)[number];
  targetApplication: string | null;
  durationMs: number;
  clipboardRestored: boolean;
  fallbackUsed: boolean;
  error: DictationError | null;
};

export type DictationSnapshot = {
  state: DictationState;
  operationId: string | null;
  operationKind: OperationKind | null;
  shortcut: string;
  rewriteShortcut: string;
  rewriteStage: RewriteStage | null;
  microphonePermission: MicrophonePermissionState;
  accessibilityPermission: AccessibilityPermissionState;
  error: DictationError | null;
  partialTranscript: string | null;
  finalTranscript: string | null;
  transcriptFinality: TranscriptFinality;
  degraded: boolean;
  insertion: InsertionOutcome | null;
  rewrite: RewriteOutcome | null;
  projectId: string | null;
  projectName: string | null;
  voiceId: string | null;
  voiceName: string | null;
  modeId: string | null;
  modeName: string | null;
  transformation: TransformationStatus | null;
};

export type AudioLevelEvent = {
  operationId: string;
  level: number;
};

export function parseDictationSnapshot(value: unknown): DictationSnapshot {
  if (!isRecord(value)) {
    throw new Error("Invalid dictation status payload");
  }
  const error = value.error;
  if (
    !isDictationState(value.state) ||
    !(typeof value.operationId === "string" || value.operationId === null) ||
    !(value.operationKind === null || isOperationKind(value.operationKind)) ||
    typeof value.shortcut !== "string" ||
    typeof value.rewriteShortcut !== "string" ||
    !(value.rewriteStage === null || isRewriteStage(value.rewriteStage)) ||
    !isMicrophonePermissionState(value.microphonePermission) ||
    !isAccessibilityPermissionState(value.accessibilityPermission) ||
    !(error === null || isDictationError(error)) ||
    !(
      typeof value.partialTranscript === "string" ||
      value.partialTranscript === null
    ) ||
    !(
      typeof value.finalTranscript === "string" ||
      value.finalTranscript === null
    ) ||
    !isTranscriptFinality(value.transcriptFinality) ||
    typeof value.degraded !== "boolean" ||
    !(value.insertion === null || isInsertionOutcome(value.insertion)) ||
    !(value.rewrite === null || isRewriteOutcome(value.rewrite)) ||
    !(typeof value.projectId === "string" || value.projectId === null) ||
    !(typeof value.projectName === "string" || value.projectName === null) ||
    !(typeof value.voiceId === "string" || value.voiceId === null) ||
    !(typeof value.voiceName === "string" || value.voiceName === null) ||
    !(typeof value.modeId === "string" || value.modeId === null) ||
    !(typeof value.modeName === "string" || value.modeName === null) ||
    !(
      value.transformation === null ||
      isTransformationStatus(value.transformation)
    )
  ) {
    throw new Error("Invalid dictation status payload");
  }
  return {
    state: value.state,
    operationId: value.operationId,
    operationKind: value.operationKind,
    shortcut: value.shortcut,
    rewriteShortcut: value.rewriteShortcut,
    rewriteStage: value.rewriteStage,
    microphonePermission: value.microphonePermission,
    accessibilityPermission: value.accessibilityPermission,
    error,
    partialTranscript: value.partialTranscript,
    finalTranscript: value.finalTranscript,
    transcriptFinality: value.transcriptFinality,
    degraded: value.degraded,
    insertion: value.insertion,
    rewrite: value.rewrite,
    projectId: value.projectId,
    projectName: value.projectName,
    voiceId: value.voiceId,
    voiceName: value.voiceName,
    modeId: value.modeId,
    modeName: value.modeName,
    transformation: value.transformation,
  };
}

function isOperationKind(value: unknown): value is OperationKind {
  return operationKinds.some((kind) => kind === value);
}

function isRewriteStage(value: unknown): value is RewriteStage {
  return rewriteStages.some((stage) => stage === value);
}

export function parseAudioLevelEvent(value: unknown): AudioLevelEvent {
  if (
    !isRecord(value) ||
    typeof value.operationId !== "string" ||
    typeof value.level !== "number" ||
    !Number.isFinite(value.level)
  ) {
    throw new Error("Invalid audio level payload");
  }
  return {
    operationId: value.operationId,
    level: Math.min(1, Math.max(0, value.level)),
  };
}

function isDictationState(value: unknown): value is DictationState {
  return dictationStates.some((state) => state === value);
}

function isMicrophonePermissionState(
  value: unknown,
): value is MicrophonePermissionState {
  return microphonePermissionStates.some((state) => state === value);
}

function isAccessibilityPermissionState(
  value: unknown,
): value is AccessibilityPermissionState {
  return accessibilityPermissionStates.some((state) => state === value);
}

function isInsertionOutcome(value: unknown): value is InsertionOutcome {
  return (
    isRecord(value) &&
    insertionOutcomeKinds.some((kind) => kind === value.kind) &&
    insertionStrategies.some((strategy) => strategy === value.strategy) &&
    (typeof value.targetApplication === "string" ||
      value.targetApplication === null) &&
    typeof value.durationMs === "number" &&
    Number.isFinite(value.durationMs) &&
    typeof value.clipboardRestored === "boolean" &&
    typeof value.fallbackUsed === "boolean" &&
    typeof value.degradedTranscript === "boolean" &&
    (value.error === null || isDictationError(value.error))
  );
}

function isTransformationStatus(value: unknown): value is TransformationStatus {
  return (
    isRecord(value) &&
    typeof value.provider === "string" &&
    typeof value.fallbackUsed === "boolean"
  );
}

function isRewriteOutcome(value: unknown): value is RewriteOutcome {
  return (
    isRecord(value) &&
    rewriteOutcomeKinds.some((kind) => kind === value.kind) &&
    (typeof value.targetApplication === "string" ||
      value.targetApplication === null) &&
    typeof value.durationMs === "number" &&
    Number.isFinite(value.durationMs) &&
    typeof value.clipboardRestored === "boolean" &&
    typeof value.fallbackUsed === "boolean" &&
    (value.error === null || isDictationError(value.error))
  );
}

function isTranscriptFinality(value: unknown): value is TranscriptFinality {
  return transcriptFinalities.some((finality) => finality === value);
}

function isDictationError(value: unknown): value is DictationError {
  return (
    isRecord(value) &&
    typeof value.code === "string" &&
    typeof value.message === "string" &&
    typeof value.retryable === "boolean"
  );
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}
