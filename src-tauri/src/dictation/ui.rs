use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tracing::warn;

use super::{DictationSnapshot, UiSink};
use crate::platform::macos::{note_overlay_hidden, show_overlay_without_activation};
use crate::product::ProductService;

pub const STATE_EVENT: &str = "dictation://state";
pub const AUDIO_LEVEL_EVENT: &str = "dictation://audio-level";

#[derive(Clone)]
pub struct TauriUiSink {
    app: AppHandle,
    product: std::sync::Arc<ProductService>,
}

impl TauriUiSink {
    pub const fn new(app: AppHandle, product: std::sync::Arc<ProductService>) -> Self {
        Self { app, product }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AudioLevelPayload<'a> {
    operation_id: &'a str,
    level: f32,
}

impl UiSink for TauriUiSink {
    fn publish_state(&self, snapshot: &DictationSnapshot) {
        if let Err(error) = self.app.emit(STATE_EVENT, snapshot) {
            warn!(
                subsystem = "overlay",
                event = "state_emit_failed",
                error = %error,
                "failed to emit dictation state"
            );
        }

        if let Some(overlay) = self.app.get_webview_window("overlay") {
            let show_overlay = self
                .product
                .settings()
                .map_or(true, |settings| settings.show_overlay);
            let result = if snapshot.state.is_idle() || !show_overlay {
                note_overlay_hidden();
                overlay.hide()
            } else {
                show_overlay_without_activation(&overlay, snapshot.operation_id.as_deref())
            };
            if let Err(error) = result {
                warn!(
                    subsystem = "overlay",
                    event = "visibility_update_failed",
                    error = %error,
                    "failed to update overlay visibility"
                );
            }
        }

        if matches!(
            snapshot.state,
            super::DictationState::Completed
                | super::DictationState::RecoverableError
                | super::DictationState::FatalError
        ) {
            let product = self.product.clone();
            let snapshot = snapshot.clone();
            let app = self.app.clone();
            tauri::async_runtime::spawn_blocking(move || {
                product.observe_dictation(&snapshot);
                let _ = app.emit("product://changed", ());
            });
        }
    }

    fn publish_audio_level(&self, operation_id: &str, level: f32) {
        let payload = AudioLevelPayload {
            operation_id,
            level: level.clamp(0.0, 1.0),
        };
        if let Err(error) = self.app.emit(AUDIO_LEVEL_EVENT, payload) {
            warn!(
                subsystem = "overlay",
                event = "audio_level_emit_failed",
                error = %error,
                "failed to emit audio level"
            );
        }
    }
}
