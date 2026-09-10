import { describe, expect, it } from "vitest";

import { parseAudioLevelEvent, parseDictationSnapshot } from "./dictation";

describe("dictation bridge validation", () => {
  it("accepts a valid Rust snapshot", () => {
    expect(
      parseDictationSnapshot({
        state: "starting",
        operationId: "operation-1",
        operationKind: "dictation",
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
      }),
    ).toMatchObject({ state: "starting", operationId: "operation-1" });
  });

  it("rejects malformed snapshots", () => {
    expect(() => parseDictationSnapshot({ state: "recording" })).toThrow(
      "Invalid dictation status payload",
    );
  });

  it("clamps scalar meter payloads", () => {
    expect(
      parseAudioLevelEvent({ operationId: "operation-1", level: 2 }).level,
    ).toBe(1);
  });

  it("validates normalized insertion outcomes", () => {
    const snapshot = parseDictationSnapshot({
      state: "completed",
      operationId: "operation-2",
      operationKind: "dictation",
      shortcut: "Option + Space",
      rewriteShortcut: "Option + Shift + Space",
      rewriteStage: null,
      microphonePermission: "granted",
      accessibilityPermission: "granted",
      error: null,
      partialTranscript: null,
      finalTranscript: "Hello",
      transcriptFinality: "final",
      degraded: false,
      insertion: {
        kind: "insertedDirectly",
        strategy: "directAccessibility",
        targetApplication: "Notes",
        durationMs: 4,
        clipboardRestored: false,
        fallbackUsed: false,
        degradedTranscript: false,
        error: null,
      },
      rewrite: null,
      projectId: "project-1",
      projectName: "General",
      voiceId: "voice-1",
      voiceName: "Narrator",
      modeId: "mode-1",
      modeName: "Draft",
      transformation: { provider: "openai", fallbackUsed: false },
    });

    expect(snapshot.insertion?.kind).toBe("insertedDirectly");
  });

  it("validates selection rewrite snapshots without selected source content", () => {
    const snapshot = parseDictationSnapshot({
      state: "inserting",
      operationId: "rewrite-1",
      operationKind: "selectionRewrite",
      shortcut: "Option + Space",
      rewriteShortcut: "Option + Shift + Space",
      rewriteStage: "revalidatingTarget",
      microphonePermission: "granted",
      accessibilityPermission: "granted",
      error: null,
      partialTranscript: null,
      finalTranscript: "Rewritten result",
      transcriptFinality: "final",
      degraded: false,
      insertion: null,
      rewrite: null,
      projectId: "project-1",
      projectName: "Novel",
      voiceId: "voice-1",
      voiceName: "Kael",
      modeId: "mode-1",
      modeName: "Draft",
      transformation: { provider: "openai", fallbackUsed: false },
    });
    expect(snapshot.operationKind).toBe("selectionRewrite");
    expect(snapshot).not.toHaveProperty("selectedText");
  });
});
