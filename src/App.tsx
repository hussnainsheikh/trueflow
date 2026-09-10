import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useState } from "react";

import "./App.css";
import { DictationOverlay } from "./components/DictationOverlay";
import { ProjectVocabularyManager } from "./components/ProjectVocabularyManager";
import { VoiceModeManager } from "./components/VoiceModeManager";
import {
  AccountScreen,
  HistoryScreen,
  Navigation,
  Onboarding,
  SettingsScreen,
  useProductSettings,
  useTrayNavigation,
  type Destination,
} from "./components/ProductShell";
import { useDictation } from "./hooks/useDictation";
import type { DictationSnapshot } from "./types/dictation";

type FoundationStatus = {
  application: string;
  environment: string;
  schemaVersion: number;
};

type LoadState =
  | { kind: "loading" }
  | { kind: "ready"; status: FoundationStatus }
  | { kind: "error" };

export function App() {
  const isOverlay =
    new URLSearchParams(window.location.search).get("window") === "overlay";
  return isOverlay ? <DictationOverlay /> : <MainWindow />;
}

export function MainWindow() {
  const [loadState, setLoadState] = useState<LoadState>({ kind: "loading" });
  const { snapshot, unavailable, refresh } = useDictation();
  const [requestingPermission, setRequestingPermission] = useState(false);
  const [openingAccessibility, setOpeningAccessibility] = useState(false);
  const [destination, setDestination] = useState<Destination>("home");
  const navigate = useCallback(
    (value: Destination) => setDestination(value),
    [],
  );
  const product = useProductSettings();
  useTrayNavigation(navigate);

  useEffect(() => {
    let active = true;
    void invoke<FoundationStatus>("foundation_status")
      .then((status) => {
        if (active) setLoadState({ kind: "ready", status });
      })
      .catch(() => {
        if (active) setLoadState({ kind: "error" });
      });
    return () => {
      active = false;
    };
  }, []);

  useEffect(() => {
    const recheckPermissions = () => void refresh();
    window.addEventListener("focus", recheckPermissions);
    return () => window.removeEventListener("focus", recheckPermissions);
  }, [refresh]);

  const requestPermission = async () => {
    setRequestingPermission(true);
    try {
      if (snapshot?.microphonePermission === "denied") {
        await invoke("open_microphone_settings_command");
      } else {
        await invoke("request_microphone_permission");
      }
      await refresh();
    } finally {
      setRequestingPermission(false);
    }
  };

  const openAccessibility = async () => {
    setOpeningAccessibility(true);
    try {
      await invoke("open_accessibility_settings");
      await refresh();
    } finally {
      setOpeningAccessibility(false);
    }
  };

  if (product.error) {
    return (
      <main className="app-shell">
        <section className="status-card">
          <h1>Trueflow</h1>
          <p className="dictation-error">{product.error}</p>
          <button
            className="permission-button"
            onClick={() => void product.refresh()}
          >
            Retry local setup
          </button>
        </section>
      </main>
    );
  }
  if (!product.settings)
    return (
      <main className="app-shell">
        <p>Loading Trueflow…</p>
      </main>
    );
  if (
    product.settings.onboardingState !== "ready" &&
    destination !== "account"
  ) {
    return (
      <Onboarding
        settings={product.settings}
        snapshot={snapshot}
        refresh={product.refresh}
        onAccount={() => setDestination("account")}
      />
    );
  }

  return (
    <main className="app-shell">
      <Navigation destination={destination} onNavigate={navigate} />
      {destination === "home" && (
        <section className="status-card" aria-labelledby="app-title">
          <p className="eyebrow">System-wide hold-to-talk</p>
          <h1 id="app-title">Trueflow</h1>
          <p className="summary">
            Put the cursor in another app, hold <kbd>⌥ Space</kbd>, speak, and
            release. Trueflow finalizes and inserts the transcript at the
            current cursor.
          </p>

          <div className="foundation-status" role="status" aria-live="polite">
            {loadState.kind === "loading" && "Checking local foundation…"}
            {loadState.kind === "error" &&
              "The local foundation could not initialize."}
            {loadState.kind === "ready" && (
              <>
                <span className="status-dot" aria-hidden="true" />
                <span>Foundation ready</span>
                <span className="status-detail">
                  {loadState.status.environment} · schema v
                  {loadState.status.schemaVersion}
                </span>
              </>
            )}
          </div>

          <DictationStatusCard
            snapshot={snapshot}
            unavailable={unavailable}
            shortcuts={{
              dictation: product.settings.dictationShortcut,
              rewrite: product.settings.rewriteShortcut,
            }}
          />

          {snapshot?.microphonePermission !== "granted" && (
            <button
              className="permission-button"
              type="button"
              disabled={requestingPermission}
              onClick={() => void requestPermission()}
            >
              {requestingPermission
                ? snapshot?.microphonePermission === "denied"
                  ? "Opening settings…"
                  : "Checking microphone…"
                : snapshot?.microphonePermission === "denied"
                  ? "Open Microphone settings"
                  : "Enable microphone"}
            </button>
          )}
          {snapshot?.accessibilityPermission !== "granted" && (
            <div className="permission-guidance">
              <p>
                Accessibility permission is required for automatic insertion.
                Transcription still works without it, and finalized text will be
                copied to the clipboard.
              </p>
              <button
                className="permission-button"
                type="button"
                disabled={openingAccessibility}
                onClick={() => void openAccessibility()}
              >
                {openingAccessibility
                  ? "Opening settings…"
                  : "Open Accessibility settings"}
              </button>
            </div>
          )}
        </section>
      )}
      {destination === "projects" && <ProjectVocabularyManager />}
      {destination === "writing" && <VoiceModeManager />}
      {destination === "history" && <HistoryScreen />}
      {destination === "settings" && (
        <SettingsScreen
          settings={product.settings}
          onSaved={product.setSettings}
        />
      )}
      {destination === "account" && <AccountScreen />}
    </main>
  );
}

function DictationStatusCard({
  snapshot,
  unavailable,
  shortcuts,
}: {
  snapshot: DictationSnapshot | null;
  unavailable: boolean;
  shortcuts: { dictation: string; rewrite: string };
}) {
  if (unavailable) {
    return <p className="dictation-error">Dictation status is unavailable.</p>;
  }
  if (!snapshot)
    return <p className="dictation-state">Loading dictation status…</p>;
  return (
    <div className="dictation-card">
      <div>
        <span className="label">State</span>
        <strong>{formatState(snapshot.state)}</strong>
      </div>
      <div>
        <span className="label">Accessibility</span>
        <strong>{formatState(snapshot.accessibilityPermission)}</strong>
      </div>
      <div>
        <span className="label">Shortcut</span>
        <strong>{shortcuts.dictation.replaceAll("+", " + ")}</strong>
      </div>
      <div>
        <span className="label">Rewrite shortcut</span>
        <strong>{shortcuts.rewrite.replaceAll("+", " + ")}</strong>
      </div>
      <div>
        <span className="label">Microphone</span>
        <strong>{formatState(snapshot.microphonePermission)}</strong>
      </div>
      {snapshot.voiceName && (
        <div>
          <span className="label">Voice</span>
          <strong>{snapshot.voiceName}</strong>
        </div>
      )}
      {snapshot.modeName && (
        <div>
          <span className="label">Mode</span>
          <strong>{snapshot.modeName}</strong>
        </div>
      )}
      {snapshot.error && (
        <p className="dictation-error">{snapshot.error.message}</p>
      )}
      {(snapshot.partialTranscript ?? snapshot.finalTranscript) && (
        <div className="transcript-result">
          <span className="label">
            {snapshot.transcriptFinality === "partial"
              ? "Live transcript"
              : snapshot.transcriptFinality === "incomplete"
                ? "Recovered transcript"
                : "Final transcript"}
          </span>
          <p>{snapshot.partialTranscript ?? snapshot.finalTranscript}</p>
        </div>
      )}
      {snapshot.insertion && (
        <div>
          <span className="label">Insertion</span>
          <strong>{formatState(snapshot.insertion.kind)}</strong>
          {snapshot.insertion.targetApplication && (
            <span className="status-detail">
              {snapshot.insertion.targetApplication}
            </span>
          )}
        </div>
      )}
      {snapshot.transformation && (
        <div>
          <span className="label">Transformation</span>
          <strong>
            {snapshot.transformation.fallbackUsed
              ? "Normalized transcript fallback"
              : snapshot.transformation.provider}
          </strong>
        </div>
      )}
    </div>
  );
}

function formatState(value: string): string {
  return value
    .replace(/([A-Z])/g, " $1")
    .replace(/^./, (letter) => letter.toUpperCase());
}
