use tauri::State;

use super::CommandError;
use crate::{
    projects::{Project, ProjectInput},
    state::AppState,
    vocabulary::{VocabularyEntry, VocabularyInput},
};

#[tauri::command]
pub fn list_projects(
    state: State<'_, AppState>,
    include_archived: Option<bool>,
) -> Result<Vec<Project>, CommandError> {
    state
        .projects
        .list_projects(include_archived.unwrap_or(true))
        .map_err(Into::into)
}

#[tauri::command]
pub fn get_project(
    state: State<'_, AppState>,
    project_id: String,
) -> Result<Project, CommandError> {
    state.projects.get_project(&project_id).map_err(Into::into)
}

#[tauri::command]
pub fn create_project(
    state: State<'_, AppState>,
    input: ProjectInput,
) -> Result<Project, CommandError> {
    state.projects.create_project(input).map_err(Into::into)
}

#[tauri::command]
pub fn update_project(
    state: State<'_, AppState>,
    project_id: String,
    input: ProjectInput,
) -> Result<Project, CommandError> {
    state
        .projects
        .update_project(&project_id, input)
        .map_err(Into::into)
}

#[tauri::command]
pub fn set_active_project(
    state: State<'_, AppState>,
    project_id: String,
) -> Result<Project, CommandError> {
    state
        .projects
        .set_active_project(&project_id)
        .map_err(Into::into)
}

#[tauri::command]
pub fn archive_project(
    state: State<'_, AppState>,
    project_id: String,
) -> Result<Project, CommandError> {
    state
        .projects
        .archive_project(&project_id)
        .map_err(Into::into)
}

#[tauri::command]
pub fn unarchive_project(
    state: State<'_, AppState>,
    project_id: String,
) -> Result<Project, CommandError> {
    state
        .projects
        .unarchive_project(&project_id)
        .map_err(Into::into)
}

#[tauri::command]
pub fn delete_project(state: State<'_, AppState>, project_id: String) -> Result<(), CommandError> {
    state
        .projects
        .delete_project(&project_id)
        .map_err(Into::into)
}

#[tauri::command]
pub fn list_vocabulary(
    state: State<'_, AppState>,
    project_id: String,
) -> Result<Vec<VocabularyEntry>, CommandError> {
    state
        .projects
        .list_vocabulary(&project_id)
        .map_err(Into::into)
}

#[tauri::command]
pub fn get_vocabulary_entry(
    state: State<'_, AppState>,
    project_id: String,
    entry_id: String,
) -> Result<VocabularyEntry, CommandError> {
    state
        .projects
        .get_vocabulary_entry(&project_id, &entry_id)
        .map_err(Into::into)
}

#[tauri::command]
pub fn create_vocabulary_entry(
    state: State<'_, AppState>,
    project_id: String,
    input: VocabularyInput,
) -> Result<VocabularyEntry, CommandError> {
    state
        .projects
        .create_vocabulary_entry(&project_id, input)
        .map_err(Into::into)
}

#[tauri::command]
pub fn update_vocabulary_entry(
    state: State<'_, AppState>,
    project_id: String,
    entry_id: String,
    input: VocabularyInput,
) -> Result<VocabularyEntry, CommandError> {
    state
        .projects
        .update_vocabulary_entry(&project_id, &entry_id, input)
        .map_err(Into::into)
}

#[tauri::command]
pub fn delete_vocabulary_entry(
    state: State<'_, AppState>,
    project_id: String,
    entry_id: String,
) -> Result<(), CommandError> {
    state
        .projects
        .delete_vocabulary_entry(&project_id, &entry_id)
        .map_err(Into::into)
}
