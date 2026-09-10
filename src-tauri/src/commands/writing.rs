use tauri::State;

use super::CommandError;
use crate::{
    projects::Project,
    state::AppState,
    writing::{Mode, Voice, VoiceInput},
};

#[tauri::command]
pub fn list_voices(
    state: State<'_, AppState>,
    project_id: String,
) -> Result<Vec<Voice>, CommandError> {
    state.projects.list_voices(&project_id).map_err(Into::into)
}

#[tauri::command]
pub fn get_voice(
    state: State<'_, AppState>,
    project_id: String,
    voice_id: String,
) -> Result<Voice, CommandError> {
    state
        .projects
        .get_voice(&project_id, &voice_id)
        .map_err(Into::into)
}

#[tauri::command]
pub fn create_voice(state: State<'_, AppState>, input: VoiceInput) -> Result<Voice, CommandError> {
    state.projects.create_voice(input).map_err(Into::into)
}

#[tauri::command]
pub fn update_voice(
    state: State<'_, AppState>,
    voice_id: String,
    input: VoiceInput,
) -> Result<Voice, CommandError> {
    state
        .projects
        .update_voice(&voice_id, input)
        .map_err(Into::into)
}

#[tauri::command]
pub fn delete_voice(
    state: State<'_, AppState>,
    project_id: String,
    voice_id: String,
) -> Result<(), CommandError> {
    state
        .projects
        .delete_voice(&project_id, &voice_id)
        .map_err(Into::into)
}

#[tauri::command]
pub fn list_modes(state: State<'_, AppState>) -> Result<Vec<Mode>, CommandError> {
    state.projects.list_modes().map_err(Into::into)
}

#[tauri::command]
pub fn set_project_voice(
    state: State<'_, AppState>,
    project_id: String,
    voice_id: String,
) -> Result<Project, CommandError> {
    state
        .projects
        .set_project_voice(&project_id, &voice_id)
        .map_err(Into::into)
}

#[tauri::command]
pub fn set_project_mode(
    state: State<'_, AppState>,
    project_id: String,
    mode_id: String,
) -> Result<Project, CommandError> {
    state
        .projects
        .set_project_mode(&project_id, &mode_id)
        .map_err(Into::into)
}
