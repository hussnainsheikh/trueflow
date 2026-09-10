import { invoke } from "@tauri-apps/api/core";
import { Component, type ReactNode } from "react";

export class ErrorBoundary extends Component<
  { children: ReactNode },
  { failed: boolean }
> {
  override state = { failed: false };
  static getDerivedStateFromError() {
    return { failed: true };
  }
  override componentDidCatch() {
    void invoke("report_client_error", {
      category: "react_render_error",
      feature: "main_window",
    }).catch(() => undefined);
  }
  override render() {
    if (this.state.failed)
      return (
        <main className="app-shell">
          <section className="status-card">
            <h1>Trueflow</h1>
            <p className="dictation-error">
              This window hit an unexpected error. Your dictation service and
              local data were not stopped.
            </p>
            <button
              className="permission-button"
              onClick={() => window.location.reload()}
            >
              Reload window
            </button>
          </section>
        </main>
      );
    return this.props.children;
  }
}
