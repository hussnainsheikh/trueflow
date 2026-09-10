use serde::Serialize;
use tauri::State;

use crate::{commands::CommandError, state::AppState};

#[derive(Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FoundationStatus {
    application: &'static str,
    environment: String,
    schema_version: u32,
}

#[tauri::command]
pub fn foundation_status(state: State<'_, AppState>) -> Result<FoundationStatus, CommandError> {
    Ok(FoundationStatus {
        application: "Trueflow",
        environment: state.config.environment.to_string(),
        schema_version: state.database.schema_version()?,
    })
}
