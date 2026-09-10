import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { useCallback, useEffect, useState } from "react";

import {
  parseAudioLevelEvent,
  parseDictationSnapshot,
  type DictationSnapshot,
} from "../types/dictation";

type DictationViewState = {
  snapshot: DictationSnapshot | null;
  audioLevel: number;
  unavailable: boolean;
};

export function useDictation() {
  const [viewState, setViewState] = useState<DictationViewState>({
    snapshot: null,
    audioLevel: 0,
    unavailable: false,
  });

  const refresh = useCallback(async () => {
    const payload = await invoke<unknown>("dictation_status");
    const snapshot = parseDictationSnapshot(payload);
    setViewState((current) => ({
      snapshot,
      audioLevel: snapshot.state === "listening" ? current.audioLevel : 0,
      unavailable: false,
    }));
  }, []);

  useEffect(() => {
    let active = true;
    const unlisteners: UnlistenFn[] = [];

    void (async () => {
      try {
        unlisteners.push(
          await listen<unknown>("dictation://state", ({ payload }) => {
            if (!active) return;
            const snapshot = parseDictationSnapshot(payload);
            setViewState((current) => ({
              snapshot,
              audioLevel:
                snapshot.state === "listening" ? current.audioLevel : 0,
              unavailable: false,
            }));
          }),
        );
        unlisteners.push(
          await listen<unknown>("dictation://audio-level", ({ payload }) => {
            if (!active) return;
            const audio = parseAudioLevelEvent(payload);
            setViewState((current) => {
              if (current.snapshot?.operationId !== audio.operationId) {
                return current;
              }
              return { ...current, audioLevel: audio.level };
            });
          }),
        );
        await refresh();
      } catch {
        if (active) {
          setViewState({ snapshot: null, audioLevel: 0, unavailable: true });
        }
      }
    })();

    return () => {
      active = false;
      for (const unlisten of unlisteners) unlisten();
    };
  }, [refresh]);

  return { ...viewState, refresh };
}
