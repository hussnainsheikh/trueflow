import { invoke } from "@tauri-apps/api/core";
import { useEffect } from "react";

import { useDictation } from "../hooks/useDictation";
import type { DictationSnapshot } from "../types/dictation";

export function DictationOverlay() {
  const { snapshot, audioLevel } = useDictation();

  useEffect(() => {
    if (
      snapshot?.state !== "recoverableError" &&
      snapshot?.state !== "completed"
    )
      return;
    if (snapshot.insertion?.kind === "failed") return;
    const timeout = window.setTimeout(
      () => {
        void invoke("dismiss_dictation_status");
      },
      snapshot.state === "completed" ? 4000 : 5000,
    );
    return () => window.clearTimeout(timeout);
  }, [snapshot?.state]);

  if (!snapshot || snapshot.state === "idle") return null;
  return <OverlayView snapshot={snapshot} audioLevel={audioLevel} />;
}

export function OverlayView({
  snapshot,
  audioLevel,
}: {
  snapshot: DictationSnapshot;
  audioLevel: number;
}) {
  const message = overlayMessage(snapshot);
  const meterLevel = snapshot.state === "listening" ? audioLevel : 0;
  const transcript =
    snapshot.partialTranscript ?? snapshot.finalTranscript ?? null;

  return (
    <main className={`overlay-shell overlay-${snapshot.state}`}>
      <div className="overlay-indicator" aria-hidden="true" />
      <div className="overlay-content">
        <p className="overlay-state">{message}</p>
        {transcript && (
          <p className="overlay-transcript" aria-live="polite">
            {snapshot.transcriptFinality === "incomplete" && "Incomplete: "}
            {transcript}
          </p>
        )}
        {snapshot.state === "listening" && (
          <div
            className="audio-meter"
            role="meter"
            aria-label="Microphone level"
            aria-valuemin={0}
            aria-valuemax={100}
            aria-valuenow={Math.round(meterLevel * 100)}
          >
            <span style={{ transform: `scaleX(${meterLevel})` }} />
          </div>
        )}
      </div>
    </main>
  );
}

function overlayMessage(snapshot: DictationSnapshot): string {
  if (snapshot.error) return snapshot.error.message;
  if (snapshot.operationKind === "selectionRewrite") {
    switch (snapshot.rewriteStage) {
      case "capturingSelection":
        return "Capturing selection…";
      case "listeningForInstruction":
        return "Listening for rewrite instruction";
      case "finalizingInstruction":
        return "Finishing instruction…";
      case "transformingSelection":
        return "Rewriting selection…";
      case "revalidatingTarget":
        return "Checking selection…";
      case "replacingSelection":
        return "Replacing selection…";
      case null:
        if (snapshot.rewrite?.kind === "rewrittenDirectly") {
          return "Selection rewritten";
        }
        if (snapshot.rewrite?.kind === "rewrittenViaPaste") {
          return snapshot.rewrite.clipboardRestored
            ? "Selection rewritten"
            : "Selection rewritten — clipboard contains the result";
        }
        if (snapshot.rewrite?.kind === "rewriteResultCopiedToClipboard") {
          return "Rewrite copied to clipboard";
        }
        return snapshot.state === "completed"
          ? "Selection rewritten"
          : "Rewrite unavailable";
    }
  }
  switch (snapshot.state) {
    case "starting":
      return "Starting microphone…";
    case "listening":
      return "Listening";
    case "finalizing":
      return "Finishing…";
    case "completed":
      if (
        snapshot.insertion?.kind === "insertedDirectly" ||
        snapshot.insertion?.kind === "insertedByTyping"
      )
        return "Text inserted";
      if (snapshot.insertion?.kind === "insertedViaPaste") {
        return snapshot.insertion.clipboardRestored
          ? "Text inserted"
          : "Text inserted — clipboard kept as dictated text";
      }
      return snapshot.degraded ? "Transcript recovered" : "Transcript ready";
    case "fatalError":
    case "recoverableError":
      return "Dictation unavailable";
    case "inserting":
      return "Inserting…";
    case "transforming":
      return "Processing…";
    case "idle":
      return "Ready";
  }
}
