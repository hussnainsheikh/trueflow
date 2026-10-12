import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useState } from "react";

import type { DictationSnapshot } from "../types/dictation";
import {
  parseAccountStatus,
  parseAuthStart,
  parseDiagnostics,
  parseHistoryPage,
  parseMicrophoneOptions,
  parseProductSettings,
  type AccountStatus,
  type Diagnostics,
  type HistoryItem,
  type MicrophoneOptions,
  type ProductSettings,
} from "../types/product";

export type Destination =
  "home" | "projects" | "writing" | "history" | "settings" | "account";

export function useProductSettings() {
  const [settings, setSettings] = useState<ProductSettings | null>(null);
  const [error, setError] = useState<string | null>(null);
  const refresh = useCallback(async () => {
    try {
      setSettings(parseProductSettings(await invoke("product_settings")));
      setError(null);
    } catch (reason) {
      setError(message(reason));
    }
  }, []);
  useEffect(() => {
    void refresh();
  }, [refresh]);
  useEffect(() => {
    let dispose: (() => void) | undefined;
    void listen("product://changed", () => void refresh()).then((value) => {
      dispose = value;
    });
    return () => dispose?.();
  }, [refresh]);
  return { settings, setSettings, error, refresh };
}

export function Navigation({
  destination,
  onNavigate,
}: {
  destination: Destination;
  onNavigate: (value: Destination) => void;
}) {
  const destinations: Destination[] = [
    "home",
    "projects",
    "writing",
    "history",
    "settings",
    "account",
  ];
  return (
    <nav className="product-nav" aria-label="Trueflow sections">
      {destinations.map((item) => (
        <button
          type="button"
          aria-current={destination === item ? "page" : undefined}
          onClick={() => onNavigate(item)}
          key={item}
        >
          {item === "writing" ? "Voices & Modes" : title(item)}
        </button>
      ))}
    </nav>
  );
}

export function Onboarding({
  settings,
  snapshot,
  refresh,
  onAccount,
}: {
  settings: ProductSettings;
  snapshot: DictationSnapshot | null;
  refresh: () => Promise<void>;
  onAccount: () => void;
}) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [account, setAccount] = useState<AccountStatus | null>(null);
  useEffect(() => {
    if (
      snapshot?.state === "completed" &&
      settings.onboardingStep === "first_dictation"
    )
      void refresh();
  }, [snapshot?.state, settings.onboardingStep, refresh]);
  useEffect(() => {
    if (settings.onboardingStep !== "account") return;
    void invoke("account_status")
      .then((value) => setAccount(parseAccountStatus(value)))
      .catch((reason) => setError(message(reason)));
  }, [settings.onboardingStep]);
  const move = async (
    step: ProductSettings["onboardingStep"],
    state: ProductSettings["onboardingState"] = "in_progress",
  ) => {
    setBusy(true);
    setError(null);
    try {
      await invoke("update_onboarding", { update: { state, step } });
      await refresh();
    } catch (reason) {
      setError(message(reason));
    } finally {
      setBusy(false);
    }
  };
  const step = settings.onboardingStep;
  return (
    <main className="onboarding-shell">
      <section className="onboarding-card" aria-live="polite">
        <p className="eyebrow">First-run setup</p>
        {step === "welcome" && (
          <>
            <h1>Write wherever you already work.</h1>
            <p>
              Hold the shortcut. Speak naturally. Release, and Trueflow inserts
              polished text in the focused app.
            </p>
            <button disabled={busy} onClick={() => void move("microphone")}>
              Get started
            </button>
          </>
        )}
        {step === "microphone" && (
          <>
            <h2>Enable your microphone</h2>
            <p>
              Audio is captured only while you hold the shortcut. It streams
              directly from Rust to the configured speech provider and is never
              saved to disk.
            </p>
            <p className="permission-state">
              Current status: {snapshot?.microphonePermission ?? "checking"}
            </p>
            <button
              disabled={busy}
              onClick={async () => {
                setBusy(true);
                try {
                  const permission = await invoke<unknown>(
                    "request_microphone_permission",
                  );
                  if (permission !== "granted") {
                    setError(
                      "Microphone access is still unavailable. Enable Trueflow in System Settings, then retry.",
                    );
                    return;
                  }
                  await move("accessibility");
                } catch (reason) {
                  setError(message(reason));
                } finally {
                  setBusy(false);
                }
              }}
            >
              Enable and recheck
            </button>
            {snapshot?.microphonePermission === "denied" && (
              <button
                disabled={busy}
                onClick={async () => {
                  setBusy(true);
                  try {
                    await invoke("open_microphone_settings_command");
                  } catch (reason) {
                    setError(message(reason));
                  } finally {
                    setBusy(false);
                  }
                }}
              >
                Open Microphone settings
              </button>
            )}
          </>
        )}
        {step === "accessibility" && (
          <>
            <h2>Allow text insertion</h2>
            <p>
              Accessibility lets Trueflow insert at the focused cursor. It does
              not grant document or filesystem access.
            </p>
            <p className="permission-state">
              Current status: {snapshot?.accessibilityPermission ?? "checking"}
            </p>
            <button
              disabled={busy}
              onClick={async () => {
                await invoke("open_accessibility_settings");
                await refresh();
              }}
            >
              Open System Settings
            </button>
            <button
              disabled={busy || snapshot?.accessibilityPermission !== "granted"}
              onClick={() => void move("account")}
            >
              Permission granted — continue
            </button>
          </>
        )}
        {step === "account" && (
          <>
            <h2>Your account</h2>
            <p>
              Sign in through your browser when account service endpoints are
              configured. Development builds with locally configured provider
              credentials can continue without an account; production builds
              require service-issued credentials. Billing is not part of this
              milestone.
            </p>
            <button onClick={onAccount}>Open account</button>
            <button
              disabled={busy || !account?.cloudReady}
              onClick={() => void move("shortcut")}
            >
              Continue to shortcuts
            </button>
            {account && !account.cloudReady && (
              <p className="permission-guidance">
                Sign in is required because transcription credentials are not
                available in this build.
              </p>
            )}
          </>
        )}
        {step === "shortcut" && (
          <>
            <h2>Learn the shortcuts</h2>
            <div className="shortcut-grid">
              <kbd>{settings.dictationShortcut}</kbd>
              <span>Hold to dictate</span>
              <kbd>{settings.rewriteShortcut}</kbd>
              <span>Hold to rewrite a selection</span>
            </div>
            <p>
              Trueflow stays in the menu bar and the overlay never takes focus.
            </p>
            <button
              disabled={busy}
              onClick={() => void move("first_dictation")}
            >
              Try a real dictation
            </button>
          </>
        )}
        {step === "first_dictation" && (
          <>
            <h2>Make your first dictation</h2>
            <p>
              Focus TextEdit, Notes, or another editable app. Hold{" "}
              <kbd>⌥ Space</kbd>, say “This is my first Trueflow dictation,” and
              release.
            </p>
            <p className="permission-state">
              {snapshot?.state === "completed"
                ? "Success recorded."
                : `Pipeline: ${snapshot?.state ?? "checking"}`}
            </p>
            <button disabled={busy} onClick={() => void refresh()}>
              Recheck result
            </button>
          </>
        )}
        {step === "behavior" && (
          <>
            <h2>Choose your default behavior</h2>
            <p>
              Clean Dictation stays close to your words. Draft reshapes speech
              into readable prose.
            </p>
            <div className="button-row">
              <button
                disabled={busy || !snapshot?.projectId}
                onClick={async () => {
                  await invoke("set_project_mode", {
                    projectId: snapshot?.projectId,
                    modeId: "20000000-0000-4000-8000-000000000001",
                  });
                  await move("complete", "ready");
                }}
              >
                Clean Dictation
              </button>
              <button
                disabled={busy || !snapshot?.projectId}
                onClick={async () => {
                  await invoke("set_project_mode", {
                    projectId: snapshot?.projectId,
                    modeId: "20000000-0000-4000-8000-000000000002",
                  });
                  await move("complete", "ready");
                }}
              >
                Draft
              </button>
            </div>
          </>
        )}
        {step === "complete" && (
          <>
            <h2>Trueflow is ready</h2>
            <button onClick={() => void move("complete", "ready")}>
              Enter Trueflow
            </button>
          </>
        )}
        {error && (
          <p className="management-error" role="alert">
            {error}
          </p>
        )}
      </section>
    </main>
  );
}

export function HistoryScreen() {
  const [items, setItems] = useState<HistoryItem[]>([]);
  const [total, setTotal] = useState(0);
  const [more, setMore] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const load = useCallback(async (offset = 0) => {
    try {
      const page = parseHistoryPage(
        await invoke("list_history", { offset, limit: 25 }),
      );
      setItems((current) =>
        offset ? [...current, ...page.items] : page.items,
      );
      setTotal(page.total);
      setMore(page.hasMore);
      setError(null);
    } catch (reason) {
      setError(message(reason));
    }
  }, []);
  useEffect(() => {
    void load();
  }, [load]);
  return (
    <section className="management">
      <div className="management-header">
        <div>
          <p className="eyebrow">Local only</p>
          <h2>History</h2>
          <p className="muted">
            {total} saved operations. Audio and selected source text are never
            stored.
          </p>
        </div>
        <button
          className="danger"
          disabled={!total || busy !== null}
          onClick={async () => {
            if (window.confirm("Permanently clear all local history?")) {
              setBusy("clear");
              try {
                await invoke("clear_history");
                await load();
              } catch (reason) {
                setError(message(reason));
              } finally {
                setBusy(null);
              }
            }
          }}
        >
          {busy === "clear" ? "Clearing…" : "Clear all"}
        </button>
      </div>
      {error && <p className="management-error">{error}</p>}
      <ul className="history-list">
        {items.map((item) => (
          <li key={item.id}>
            <div className="history-meta">
              <span>{new Date(`${item.createdAt}Z`).toLocaleString()}</span>
              <span>{item.sourceApp ?? "Unknown app"}</span>
              <span>{item.projectName ?? "No project"}</span>
              <span>{item.status}</span>
            </div>
            <p>
              {item.finalText}
              {item.truncated ? "…" : ""}
            </p>
            <div className="record-actions">
              <button
                disabled={busy !== null}
                onClick={() =>
                  void invoke("copy_history_item", { id: item.id })
                }
              >
                Copy result
              </button>
              <button
                className="danger"
                disabled={busy !== null}
                onClick={async () => {
                  setBusy(item.id);
                  try {
                    await invoke("delete_history_item", { id: item.id });
                    await load();
                  } catch (reason) {
                    setError(message(reason));
                  } finally {
                    setBusy(null);
                  }
                }}
              >
                {busy === item.id ? "Deleting…" : "Delete"}
              </button>
            </div>
          </li>
        ))}
      </ul>
      {more && (
        <button
          disabled={busy !== null}
          onClick={() => void load(items.length)}
        >
          Load more
        </button>
      )}
    </section>
  );
}

const SYSTEM_DEFAULT_MICROPHONE = "";

export function MicrophoneSettings() {
  const [options, setOptions] = useState<MicrophoneOptions | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const refresh = useCallback(async () => {
    setBusy(true);
    try {
      setOptions(parseMicrophoneOptions(await invoke("list_microphones")));
      setError(null);
    } catch (reason) {
      setError(message(reason));
    } finally {
      setBusy(false);
    }
  }, []);
  useEffect(() => {
    void refresh();
  }, [refresh]);
  const select = async (value: string) => {
    if (busy) return;
    setBusy(true);
    try {
      setOptions(
        parseMicrophoneOptions(
          await invoke("select_microphone", {
            deviceId: value === SYSTEM_DEFAULT_MICROPHONE ? null : value,
          }),
        ),
      );
      setError(null);
    } catch (reason) {
      setError(message(reason));
    } finally {
      setBusy(false);
    }
  };
  const systemDefault = options?.devices.find((device) => device.isDefault);
  return (
    <>
      <label>
        Microphone
        <select
          value={options?.selectedId ?? SYSTEM_DEFAULT_MICROPHONE}
          disabled={busy || options === null}
          onChange={(event) => void select(event.target.value)}
        >
          <option value={SYSTEM_DEFAULT_MICROPHONE}>
            System default
            {systemDefault ? ` (${systemDefault.name})` : ""}
          </option>
          {options?.devices.map((device) => (
            <option key={device.id} value={device.id}>
              {device.name}
            </option>
          ))}
          {options && !options.selectedAvailable && options.selectedId && (
            <option value={options.selectedId} disabled>
              {options.selectedName ?? "Saved microphone"} (disconnected)
            </option>
          )}
        </select>
      </label>
      <button type="button" disabled={busy} onClick={() => void refresh()}>
        Refresh microphones
      </button>
      {options && !options.selectedAvailable && (
        <p role="status">
          {options.selectedName ?? "Your saved microphone"} is disconnected.
          Dictation uses the system default until it is reconnected.
        </p>
      )}
      {error && <p role="alert">{error}</p>}
      <p>
        Changes apply from the next dictation. The live meter appears during a
        real hold-to-talk test.
      </p>
    </>
  );
}

export function SettingsScreen({
  settings,
  onSaved,
}: {
  settings: ProductSettings;
  onSaved: (settings: ProductSettings) => void;
}) {
  const [draft, setDraft] = useState(settings);
  const [diagnostics, setDiagnostics] = useState<Diagnostics | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const loadDiagnostics = useCallback(async () => {
    setDiagnostics(parseDiagnostics(await invoke("diagnostics")));
  }, []);
  useEffect(() => {
    void loadDiagnostics().catch((reason) => setError(message(reason)));
  }, [loadDiagnostics]);
  const save = async () => {
    if (busy) return;
    setBusy("save");
    try {
      await invoke("update_shortcuts", {
        dictation: draft.dictationShortcut,
        rewrite: draft.rewriteShortcut,
      });
      const value = parseProductSettings(
        await invoke("update_product_settings", {
          update: {
            historyEnabled: draft.historyEnabled,
            analyticsEnabled: draft.analyticsEnabled,
            showOverlay: draft.showOverlay,
          },
        }),
      );
      setDraft(value);
      onSaved(value);
      setError(null);
    } catch (reason) {
      setError(message(reason));
    } finally {
      setBusy(null);
    }
  };
  return (
    <section className="management">
      <h2>Settings</h2>
      <div className="settings-grid">
        <fieldset>
          <legend>General & privacy</legend>
          <label>
            <input
              type="checkbox"
              checked={draft.showOverlay}
              onChange={(event) =>
                setDraft({ ...draft, showOverlay: event.target.checked })
              }
            />{" "}
            Show non-focus overlay
          </label>
          <label>
            <input
              type="checkbox"
              checked={draft.historyEnabled}
              onChange={(event) =>
                setDraft({ ...draft, historyEnabled: event.target.checked })
              }
            />{" "}
            Save local text history
          </label>
          <label>
            <input
              type="checkbox"
              checked={draft.analyticsEnabled}
              onChange={(event) =>
                setDraft({ ...draft, analyticsEnabled: event.target.checked })
              }
            />{" "}
            Share content-free product analytics
          </label>
          <small>
            Analytics never includes dictated, transformed, selected, project,
            Voice, vocabulary, clipboard, prompt, or token content.
          </small>
          <button
            type="button"
            disabled={busy !== null}
            onClick={async () => {
              setBusy("onboarding");
              try {
                const value = parseProductSettings(
                  await invoke("update_onboarding", {
                    update: { state: "in_progress", step: "welcome" },
                  }),
                );
                onSaved(value);
              } catch (reason) {
                setError(message(reason));
              } finally {
                setBusy(null);
              }
            }}
          >
            Run onboarding again
          </button>
        </fieldset>
        <fieldset>
          <legend>Shortcuts & audio</legend>
          <label>
            Hold to dictate
            <select
              value={draft.dictationShortcut}
              onChange={(event) =>
                setDraft({ ...draft, dictationShortcut: event.target.value })
              }
            >
              <option>Option+Space</option>
              <option>Control+Space</option>
              <option>Command+Shift+Space</option>
              <option>Control+Option+Space</option>
            </select>
          </label>
          <label>
            Hold to rewrite
            <select
              value={draft.rewriteShortcut}
              onChange={(event) =>
                setDraft({ ...draft, rewriteShortcut: event.target.value })
              }
            >
              <option>Option+Shift+Space</option>
              <option>Control+Shift+Space</option>
              <option>Command+Option+Space</option>
            </select>
          </label>
          <p>
            Registration is validated before saving; a conflict rolls back to
            the working shortcuts.
          </p>
          <MicrophoneSettings />
        </fieldset>
        <fieldset>
          <legend>Permissions</legend>
          <p>Microphone: {diagnostics?.microphonePermission ?? "checking"}</p>
          <button
            type="button"
            disabled={busy !== null}
            onClick={async () => {
              setBusy("microphone");
              try {
                await invoke("request_microphone_permission");
                await loadDiagnostics();
              } catch (reason) {
                setError(message(reason));
              } finally {
                setBusy(null);
              }
            }}
          >
            {busy === "microphone"
              ? "Checking microphone…"
              : "Request/recheck microphone"}
          </button>
          {diagnostics?.microphonePermission === "denied" && (
            <button
              type="button"
              disabled={busy !== null}
              onClick={async () => {
                setBusy("microphone-settings");
                try {
                  await invoke("open_microphone_settings_command");
                } catch (reason) {
                  setError(message(reason));
                } finally {
                  setBusy(null);
                }
              }}
            >
              {busy === "microphone-settings"
                ? "Opening settings…"
                : "Open Microphone settings"}
            </button>
          )}
          <p>
            Accessibility: {diagnostics?.accessibilityPermission ?? "checking"}
          </p>
          <button
            type="button"
            onClick={async () => {
              await invoke("open_accessibility_settings");
            }}
          >
            Open Accessibility settings
          </button>
          <button
            type="button"
            disabled={busy !== null}
            onClick={async () => {
              setBusy("permissions");
              try {
                await loadDiagnostics();
              } catch (reason) {
                setError(message(reason));
              } finally {
                setBusy(null);
              }
            }}
          >
            {busy === "permissions"
              ? "Checking permissions…"
              : "Recheck permissions"}
          </button>
        </fieldset>
        <fieldset>
          <legend>Diagnostics</legend>
          {diagnostics ? (
            <dl>
              <dt>Version</dt>
              <dd>{diagnostics.applicationVersion}</dd>
              <dt>System</dt>
              <dd>
                {diagnostics.os} / {diagnostics.architecture}
              </dd>
              <dt>Database</dt>
              <dd>schema v{diagnostics.schemaVersion}</dd>
              <dt>Providers</dt>
              <dd>
                {diagnostics.transcriptionProvider} /{" "}
                {diagnostics.transformationProvider}
              </dd>
              <dt>Permissions</dt>
              <dd>
                microphone {diagnostics.microphonePermission}; Accessibility{" "}
                {diagnostics.accessibilityPermission}
              </dd>
              <dt>Last error</dt>
              <dd>{diagnostics.lastErrorCategory ?? "none"}</dd>
              <dt>Reporting</dt>
              <dd>
                analytics{" "}
                {diagnostics.analyticsConfigured
                  ? "configured"
                  : "not configured"}
                ; crash reports{" "}
                {diagnostics.crashReportingConfigured
                  ? "configured"
                  : "not configured"}
              </dd>
            </dl>
          ) : (
            "Loading…"
          )}
        </fieldset>
      </div>
      <button
        className="permission-button"
        disabled={busy !== null}
        onClick={() => void save()}
      >
        {busy === "save" ? "Saving settings…" : "Save settings"}
      </button>
      {error && <p className="management-error">{error}</p>}
    </section>
  );
}

export function AccountScreen() {
  const [account, setAccount] = useState<AccountStatus | null>(null);
  const [code, setCode] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const refresh = useCallback(async () => {
    try {
      setAccount(parseAccountStatus(await invoke("refresh_account")));
      setError(null);
    } catch (reason) {
      setError(message(reason));
    }
  }, []);
  useEffect(() => {
    void refresh();
  }, [refresh]);
  const begin = async () => {
    setBusy(true);
    try {
      const start = parseAuthStart(await invoke("begin_sign_in"));
      setCode(start.userCode);
      setError(null);
    } catch (reason) {
      setError(message(reason));
    } finally {
      setBusy(false);
    }
  };
  const poll = async () => {
    setBusy(true);
    try {
      setAccount(parseAccountStatus(await invoke("poll_sign_in")));
      setCode(null);
      setError(null);
    } catch (reason) {
      setError(message(reason));
    } finally {
      setBusy(false);
    }
  };
  return (
    <section className="management">
      <h2>Account</h2>
      {account?.signedIn ? (
        <>
          <p>
            Signed in as <strong>{account.email}</strong>
            {account.offline ? " (offline — cached session)" : ""}.
          </p>
          <p>
            Signing out does not delete local projects, Voices, vocabulary,
            settings, or history.
          </p>
          {!account.cloudReady && (
            <p className="permission-guidance">
              Your account is active, but this deployment has not supplied
              transcription credentials. Retry shortly or contact support.
            </p>
          )}
          <button
            onClick={async () => {
              await invoke("sign_out");
              await refresh();
            }}
          >
            Sign out
          </button>
        </>
      ) : (
        <>
          {account?.state === "sessionExpired" && (
            <p className="permission-guidance">
              Your account session expired. Sign in again; local projects,
              settings, and history are unchanged.
            </p>
          )}
          {account?.state === "offline" && (
            <p className="permission-guidance">
              The account service is offline. Your cached local data remains
              available.
            </p>
          )}
          <p>
            Authentication happens in your system browser. The resulting session
            token is kept in macOS Keychain, never frontend storage.
          </p>
          {!account?.cloudReady && account?.configured && (
            <p className="permission-guidance">
              Sign in is required before cloud transcription can start in this
              build.
            </p>
          )}
          {!account?.configured && (
            <p className="permission-guidance">
              Account service is not configured for this build. Local dictation
              remains available.
            </p>
          )}
          <button
            disabled={busy || !account?.configured}
            onClick={() => void begin()}
          >
            Sign in or create account
          </button>
          {code && (
            <div className="auth-code">
              <span>Browser code</span>
              <strong>{code}</strong>
              <button disabled={busy} onClick={() => void poll()}>
                I completed sign-in
              </button>
            </div>
          )}
        </>
      )}
      {error && <p className="management-error">{error}</p>}
    </section>
  );
}

export function useTrayNavigation(
  onNavigate: (destination: Destination) => void,
) {
  useEffect(() => {
    let dispose: (() => void) | undefined;
    void listen<string>("product://navigate", ({ payload }) => {
      if (["home", "history", "settings"].includes(payload))
        onNavigate(payload as Destination);
    }).then((value) => {
      dispose = value;
    });
    return () => dispose?.();
  }, [onNavigate]);
}

function title(value: string) {
  return value[0]?.toUpperCase() + value.slice(1);
}
export function message(reason: unknown): string {
  if (typeof reason === "string") return reason;
  if (
    reason &&
    typeof reason === "object" &&
    "message" in reason &&
    typeof reason.message === "string"
  )
    return reason.message;
  return "The operation could not be completed.";
}
