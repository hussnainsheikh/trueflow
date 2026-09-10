use tauri::State;

use super::CommandError;
use crate::{
    audio::MicrophonePermissionState, dictation::DictationSnapshot,
    insertion::AccessibilityPermissionState, platform::macos_insertion::open_microphone_settings,
    state::AppState,
};

#[tauri::command]
pub fn dictation_status(state: State<'_, AppState>) -> Result<DictationSnapshot, CommandError> {
    state.dictation.status().map_err(Into::into)
}

#[tauri::command]
pub fn open_microphone_settings_command() -> Result<(), CommandError> {
    open_microphone_settings().map_err(Into::into)
}

#[tauri::command]
pub fn dismiss_dictation_status(state: State<'_, AppState>) -> Result<(), CommandError> {
    state.dictation.dismiss_error().map_err(Into::into)
}

#[tauri::command]
pub async fn request_microphone_permission(
    state: State<'_, AppState>,
) -> Result<MicrophonePermissionState, CommandError> {
    let dictation = state.dictation.clone();
    tauri::async_runtime::spawn_blocking(move || dictation.request_microphone_permission())
        .await
        .map_err(|_| CommandError::from(crate::dictation::DictationError::StateUnavailable))?
        .map_err(Into::into)
}

#[tauri::command]
pub fn accessibility_permission_status(state: State<'_, AppState>) -> AccessibilityPermissionState {
    state.dictation.accessibility_permission_status()
}

#[tauri::command]
pub fn open_accessibility_settings(state: State<'_, AppState>) -> Result<(), CommandError> {
    state
        .dictation
        .open_accessibility_settings()
        .map_err(Into::into)
}
