use serde::Deserialize;
use tauri::{AppHandle, State};

use super::CommandError;
use crate::{
    audio::{InputDeviceInfo, MicrophoneOptions, list_input_devices},
    dictation::DictationError,
    hotkey,
    insertion::InsertionBackend,
    platform::macos_insertion::{MacOsInsertionBackend, open_external_https_url},
    product::{
        AccountStatus, AuthStart, Diagnostics, HistoryPage, OnboardingState, OnboardingStep,
        ProductSettings, SettingsUpdate,
    },
    state::AppState,
};

#[tauri::command]
pub fn product_settings(state: State<'_, AppState>) -> Result<ProductSettings, CommandError> {
    state.product.settings().map_err(Into::into)
}

#[tauri::command]
pub fn update_product_settings(
    update: SettingsUpdate,
    state: State<'_, AppState>,
) -> Result<ProductSettings, CommandError> {
    state.product.update_settings(update).map_err(Into::into)
}

async fn current_input_devices() -> Result<Vec<InputDeviceInfo>, CommandError> {
    tauri::async_runtime::spawn_blocking(list_input_devices)
        .await
        .map_err(|_| CommandError::from(DictationError::NoInputDevice))?
        .map_err(Into::into)
}

#[tauri::command]
pub async fn list_microphones(
    state: State<'_, AppState>,
) -> Result<MicrophoneOptions, CommandError> {
    let devices = current_input_devices().await?;
    let settings = state.product.settings()?;
    Ok(MicrophoneOptions::new(
        devices,
        settings.microphone_device_id,
        settings.microphone_device_name,
    ))
}

/// Selects an input device by its stable identifier, or the system default
/// when `device_id` is absent. Only currently connected devices can be chosen;
/// the new preference applies from the next recording.
#[tauri::command]
pub async fn select_microphone(
    device_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<MicrophoneOptions, CommandError> {
    let devices = current_input_devices().await?;
    let chosen = match device_id.as_deref() {
        None => None,
        Some(id) => Some(
            devices
                .iter()
                .find(|device| device.id == id)
                .ok_or(crate::product::ProductError::InvalidSetting("microphone"))?,
        ),
    };
    let settings = state
        .product
        .set_microphone(chosen.map(|device| (device.id.as_str(), device.name.as_str())))?;
    state.microphone.set(settings.microphone_device_id.clone());
    Ok(MicrophoneOptions::new(
        devices,
        settings.microphone_device_id,
        settings.microphone_device_name,
    ))
}

#[tauri::command]
pub fn update_shortcuts(
    dictation: String,
    rewrite: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<ProductSettings, CommandError> {
    hotkey::reconfigure(&app, &dictation, &rewrite)?;
    match state.product.update_shortcuts(&dictation, &rewrite) {
        Ok(settings) => Ok(settings),
        Err(error) => {
            let previous = state.product.settings()?;
            let _ = hotkey::reconfigure(
                &app,
                &previous.dictation_shortcut,
                &previous.rewrite_shortcut,
            );
            Err(error.into())
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OnboardingUpdate {
    state: OnboardingState,
    step: OnboardingStep,
}

#[tauri::command]
pub fn update_onboarding(
    update: OnboardingUpdate,
    state: State<'_, AppState>,
) -> Result<ProductSettings, CommandError> {
    if update.state == OnboardingState::Ready {
        let current = state.product.settings()?;
        let permissions = state.dictation.status()?;
        let completed_test = matches!(
            current.onboarding_step,
            OnboardingStep::Behavior | OnboardingStep::Complete
        );
        if !completed_test
            || permissions.microphone_permission != crate::audio::MicrophonePermissionState::Granted
            || permissions.accessibility_permission
                != crate::insertion::AccessibilityPermissionState::Granted
        {
            return Err(crate::product::ProductError::InvalidSetting("onboarding completion requires granted permissions and a successful real dictation").into());
        }
    }
    state
        .product
        .set_onboarding(update.state, update.step)
        .map_err(Into::into)
}

#[tauri::command]
pub fn list_history(
    offset: u32,
    limit: u32,
    state: State<'_, AppState>,
) -> Result<HistoryPage, CommandError> {
    state.product.history(offset, limit).map_err(Into::into)
}

#[tauri::command]
pub fn delete_history_item(id: String, state: State<'_, AppState>) -> Result<(), CommandError> {
    state.product.delete_history(&id).map_err(Into::into)
}

#[tauri::command]
pub fn clear_history(state: State<'_, AppState>) -> Result<u32, CommandError> {
    state.product.clear_history().map_err(Into::into)
}

#[tauri::command]
pub fn copy_history_item(id: String, state: State<'_, AppState>) -> Result<(), CommandError> {
    let text = state.product.history_text(&id)?;
    let backend = MacOsInsertionBackend::new();
    InsertionBackend::write_clipboard(&backend, &text)
        .map(|_| ())
        .map_err(Into::into)
}

#[tauri::command]
pub fn account_status(state: State<'_, AppState>) -> Result<AccountStatus, CommandError> {
    state.product.account_status().map_err(Into::into)
}

#[tauri::command]
pub async fn begin_sign_in(state: State<'_, AppState>) -> Result<AuthStart, CommandError> {
    let start = state.product.begin_auth().await?;
    open_external_https_url(&start.verification_url)?;
    Ok(start)
}

#[tauri::command]
pub async fn poll_sign_in(state: State<'_, AppState>) -> Result<AccountStatus, CommandError> {
    state.product.poll_auth().await.map_err(Into::into)
}

#[tauri::command]
pub async fn refresh_account(state: State<'_, AppState>) -> Result<AccountStatus, CommandError> {
    state.product.refresh_account().await.map_err(Into::into)
}

#[tauri::command]
pub fn sign_out(state: State<'_, AppState>) -> Result<(), CommandError> {
    state.product.sign_out_local().map_err(Into::into)
}

#[tauri::command]
pub fn diagnostics(state: State<'_, AppState>) -> Result<Diagnostics, CommandError> {
    let snapshot = state.dictation.status()?;
    let microphone = serde_json::to_value(snapshot.microphone_permission)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_else(|| "unavailable".to_owned());
    let accessibility = serde_json::to_value(snapshot.accessibility_permission)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_else(|| "unavailable".to_owned());
    let last_error = snapshot.error.map(|error| error.code.to_owned());
    state
        .product
        .diagnostics(microphone, accessibility, last_error)
        .map_err(Into::into)
}

#[tauri::command]
pub fn report_client_error(category: String, feature: String, state: State<'_, AppState>) {
    state.product.report_safe_error(&category, &feature);
}
