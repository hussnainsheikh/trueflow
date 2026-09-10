use std::sync::{Arc, RwLock};

use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::{
    error::AppError,
    storage::Database,
    vocabulary::{
        ValidatedVocabularyInput, VocabularyEntry, VocabularyError, VocabularyInput, VocabularyKind,
    },
    writing::{
        CLEAN_DICTATION_MODE_ID, MY_VOICE_ID, Mode, ValidatedVoiceInput, Voice, VoiceInput,
        WritingError,
    },
};

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub name: String,
    pub description: String,
    pub default_voice_id: Option<String>,
    pub default_mode_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub archived_at: Option<String>,
    pub active: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectInput {
    pub name: String,
    #[serde(default)]
    pub description: String,
}

impl ProjectInput {
    fn validate(self) -> Result<Self, ProjectError> {
        let name = self.name.trim().to_owned();
        if name.is_empty() {
            return Err(ProjectError::ValidationFailed(
                "Project name cannot be empty.".to_owned(),
            ));
        }
        if name.chars().count() > 120 {
            return Err(ProjectError::ValidationFailed(
                "Project name cannot exceed 120 characters.".to_owned(),
            ));
        }
        Ok(Self {
            name,
            description: self.description.trim().to_owned(),
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectVocabularySnapshot {
    pub project_id: String,
    pub project_name: String,
    pub project_description: String,
    pub vocabulary: Vec<VocabularyEntry>,
    pub voice: Voice,
    pub mode: Mode,
}

pub trait ProjectContextSource: Send + Sync {
    fn active_snapshot(&self) -> Result<Arc<ProjectVocabularySnapshot>, ProjectError>;
}

pub struct ProjectService {
    database: Arc<Database>,
    active: RwLock<Arc<ProjectVocabularySnapshot>>,
}

impl ProjectService {
    pub fn new(database: Arc<Database>) -> Result<Arc<Self>, ProjectError> {
        let snapshot = {
            let connection = database.connection()?;
            load_active_snapshot(&connection)?
        };
        Ok(Arc::new(Self {
            database,
            active: RwLock::new(Arc::new(snapshot)),
        }))
    }

    pub fn list_projects(&self, include_archived: bool) -> Result<Vec<Project>, ProjectError> {
        let connection = self.database.connection()?;
        let sql = if include_archived {
            PROJECT_SELECT.to_owned() + " ORDER BY p.archived_at IS NOT NULL, p.name, p.id"
        } else {
            PROJECT_SELECT.to_owned() + " WHERE p.archived_at IS NULL ORDER BY p.name, p.id"
        };
        let mut statement = connection.prepare(&sql).map_err(read_failed)?;
        statement
            .query_map([], map_project)
            .map_err(read_failed)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(read_failed)
    }

    pub fn get_project(&self, project_id: &str) -> Result<Project, ProjectError> {
        let connection = self.database.connection()?;
        find_project(&connection, project_id)?.ok_or(ProjectError::NotFound)
    }

    pub fn create_project(&self, input: ProjectInput) -> Result<Project, ProjectError> {
        let input = input.validate()?;
        let _guard = self
            .active
            .write()
            .map_err(|_| ProjectError::StateUnavailable)?;
        let connection = self.database.connection()?;
        let id = Uuid::new_v4().to_string();
        connection
            .execute(
                "INSERT INTO projects (id, name, description, default_voice_id, default_mode_id) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![id, input.name, input.description, MY_VOICE_ID, CLEAN_DICTATION_MODE_ID],
            )
            .map_err(write_failed)?;
        find_project(&connection, &id)?.ok_or(ProjectError::NotFound)
    }

    pub fn update_project(
        &self,
        project_id: &str,
        input: ProjectInput,
    ) -> Result<Project, ProjectError> {
        let input = input.validate()?;
        let mut active = self
            .active
            .write()
            .map_err(|_| ProjectError::StateUnavailable)?;
        let connection = self.database.connection()?;
        let changed = connection
            .execute(
                "UPDATE projects SET name=?2, description=?3, updated_at=CURRENT_TIMESTAMP WHERE id=?1",
                params![project_id, input.name, input.description],
            )
            .map_err(write_failed)?;
        if changed == 0 {
            return Err(ProjectError::NotFound);
        }
        let project = find_project(&connection, project_id)?.ok_or(ProjectError::NotFound)?;
        if active.project_id == project_id {
            *active = Arc::new(load_active_snapshot(&connection)?);
        }
        Ok(project)
    }

    pub fn set_active_project(&self, project_id: &str) -> Result<Project, ProjectError> {
        let mut active = self
            .active
            .write()
            .map_err(|_| ProjectError::StateUnavailable)?;
        let mut connection = self.database.connection()?;
        let transaction = connection.transaction().map_err(write_failed)?;
        let project = find_project(&transaction, project_id)?.ok_or(ProjectError::NotFound)?;
        if project.archived_at.is_some() {
            return Err(ProjectError::Archived);
        }
        transaction
            .execute(
                "UPDATE active_project SET project_id=?1 WHERE singleton=1",
                [project_id],
            )
            .map_err(write_failed)?;
        let snapshot = load_active_snapshot(&transaction)?;
        let project = find_project(&transaction, project_id)?.ok_or(ProjectError::NotFound)?;
        transaction.commit().map_err(write_failed)?;
        *active = Arc::new(snapshot);
        Ok(project)
    }

    pub fn archive_project(&self, project_id: &str) -> Result<Project, ProjectError> {
        self.set_archived(project_id, true)
    }

    pub fn unarchive_project(&self, project_id: &str) -> Result<Project, ProjectError> {
        self.set_archived(project_id, false)
    }

    fn set_archived(&self, project_id: &str, archived: bool) -> Result<Project, ProjectError> {
        let mut active = self
            .active
            .write()
            .map_err(|_| ProjectError::StateUnavailable)?;
        let mut connection = self.database.connection()?;
        let transaction = connection.transaction().map_err(write_failed)?;
        let project = find_project(&transaction, project_id)?.ok_or(ProjectError::NotFound)?;
        if archived && project.archived_at.is_some() || !archived && project.archived_at.is_none() {
            return Ok(project);
        }
        if archived && active.project_id == project_id {
            let replacement = replacement_project_id(&transaction, project_id)?
                .ok_or(ProjectError::CannotRemoveLastUsableProject)?;
            transaction
                .execute(
                    "UPDATE active_project SET project_id=?1 WHERE singleton=1",
                    [&replacement],
                )
                .map_err(write_failed)?;
        }
        let archived_at = if archived {
            Some("CURRENT_TIMESTAMP")
        } else {
            None
        };
        if archived_at.is_some() {
            transaction
                .execute(
                    "UPDATE projects SET archived_at=CURRENT_TIMESTAMP, updated_at=CURRENT_TIMESTAMP WHERE id=?1",
                    [project_id],
                )
                .map_err(write_failed)?;
        } else {
            transaction
                .execute(
                    "UPDATE projects SET archived_at=NULL, updated_at=CURRENT_TIMESTAMP WHERE id=?1",
                    [project_id],
                )
                .map_err(write_failed)?;
        }
        let snapshot = load_active_snapshot(&transaction)?;
        let project = find_project(&transaction, project_id)?.ok_or(ProjectError::NotFound)?;
        transaction.commit().map_err(write_failed)?;
        *active = Arc::new(snapshot);
        Ok(project)
    }

    pub fn delete_project(&self, project_id: &str) -> Result<(), ProjectError> {
        let mut active = self
            .active
            .write()
            .map_err(|_| ProjectError::StateUnavailable)?;
        let mut connection = self.database.connection()?;
        let transaction = connection.transaction().map_err(write_failed)?;
        let project = find_project(&transaction, project_id)?.ok_or(ProjectError::NotFound)?;
        if active.project_id == project_id {
            let replacement = replacement_project_id(&transaction, project_id)?
                .ok_or(ProjectError::CannotRemoveLastUsableProject)?;
            transaction
                .execute(
                    "UPDATE active_project SET project_id=?1 WHERE singleton=1",
                    [&replacement],
                )
                .map_err(write_failed)?;
        } else if project.archived_at.is_none() && usable_project_count(&transaction)? <= 1 {
            return Err(ProjectError::CannotRemoveLastUsableProject);
        }
        transaction
            .execute("DELETE FROM projects WHERE id=?1", [project_id])
            .map_err(write_failed)?;
        let snapshot = load_active_snapshot(&transaction)?;
        transaction.commit().map_err(write_failed)?;
        *active = Arc::new(snapshot);
        Ok(())
    }

    pub fn list_vocabulary(&self, project_id: &str) -> Result<Vec<VocabularyEntry>, ProjectError> {
        let connection = self.database.connection()?;
        ensure_project(&connection, project_id, false)?;
        list_vocabulary(&connection, project_id)
    }

    pub fn get_vocabulary_entry(
        &self,
        project_id: &str,
        entry_id: &str,
    ) -> Result<VocabularyEntry, ProjectError> {
        let connection = self.database.connection()?;
        let entry =
            find_vocabulary(&connection, entry_id)?.ok_or(VocabularyError::EntryNotFound)?;
        if entry.project_id != project_id {
            return Err(VocabularyError::ProjectMismatch.into());
        }
        Ok(entry)
    }

    pub fn create_vocabulary_entry(
        &self,
        project_id: &str,
        input: VocabularyInput,
    ) -> Result<VocabularyEntry, ProjectError> {
        let input = input.validate()?;
        let mut active = self
            .active
            .write()
            .map_err(|_| ProjectError::StateUnavailable)?;
        let mut connection = self.database.connection()?;
        let transaction = connection.transaction().map_err(write_failed)?;
        ensure_project(&transaction, project_id, true)?;
        ensure_unique_term(&transaction, project_id, &input.term, None)?;
        let id = Uuid::new_v4().to_string();
        write_vocabulary(&transaction, &id, project_id, &input, false)?;
        let entry = find_vocabulary(&transaction, &id)?.ok_or(VocabularyError::EntryNotFound)?;
        let snapshot = if active.project_id == project_id {
            Some(load_active_snapshot(&transaction)?)
        } else {
            None
        };
        transaction.commit().map_err(write_failed)?;
        if let Some(snapshot) = snapshot {
            *active = Arc::new(snapshot);
        }
        Ok(entry)
    }

    pub fn update_vocabulary_entry(
        &self,
        project_id: &str,
        entry_id: &str,
        input: VocabularyInput,
    ) -> Result<VocabularyEntry, ProjectError> {
        let input = input.validate()?;
        let mut active = self
            .active
            .write()
            .map_err(|_| ProjectError::StateUnavailable)?;
        let mut connection = self.database.connection()?;
        let transaction = connection.transaction().map_err(write_failed)?;
        ensure_project(&transaction, project_id, true)?;
        let existing =
            find_vocabulary(&transaction, entry_id)?.ok_or(VocabularyError::EntryNotFound)?;
        if existing.project_id != project_id {
            return Err(VocabularyError::ProjectMismatch.into());
        }
        ensure_unique_term(&transaction, project_id, &input.term, Some(entry_id))?;
        write_vocabulary(&transaction, entry_id, project_id, &input, true)?;
        let entry =
            find_vocabulary(&transaction, entry_id)?.ok_or(VocabularyError::EntryNotFound)?;
        let snapshot = if active.project_id == project_id {
            Some(load_active_snapshot(&transaction)?)
        } else {
            None
        };
        transaction.commit().map_err(write_failed)?;
        if let Some(snapshot) = snapshot {
            *active = Arc::new(snapshot);
        }
        Ok(entry)
    }

    pub fn delete_vocabulary_entry(
        &self,
        project_id: &str,
        entry_id: &str,
    ) -> Result<(), ProjectError> {
        let mut active = self
            .active
            .write()
            .map_err(|_| ProjectError::StateUnavailable)?;
        let mut connection = self.database.connection()?;
        let transaction = connection.transaction().map_err(write_failed)?;
        ensure_project(&transaction, project_id, true)?;
        let existing =
            find_vocabulary(&transaction, entry_id)?.ok_or(VocabularyError::EntryNotFound)?;
        if existing.project_id != project_id {
            return Err(VocabularyError::ProjectMismatch.into());
        }
        transaction
            .execute("DELETE FROM vocabulary_entries WHERE id=?1", [entry_id])
            .map_err(write_failed)?;
        let snapshot = if active.project_id == project_id {
            Some(load_active_snapshot(&transaction)?)
        } else {
            None
        };
        transaction.commit().map_err(write_failed)?;
        if let Some(snapshot) = snapshot {
            *active = Arc::new(snapshot);
        }
        Ok(())
    }

    pub fn list_voices(&self, project_id: &str) -> Result<Vec<Voice>, ProjectError> {
        let connection = self.database.connection()?;
        ensure_project(&connection, project_id, false)?;
        list_voices(&connection, project_id)
    }

    pub fn get_voice(&self, project_id: &str, voice_id: &str) -> Result<Voice, ProjectError> {
        let connection = self.database.connection()?;
        ensure_project(&connection, project_id, false)?;
        let voice = find_voice(&connection, voice_id)?.ok_or(WritingError::VoiceNotFound)?;
        ensure_voice_available(&voice, project_id)?;
        Ok(voice)
    }

    pub fn create_voice(&self, input: VoiceInput) -> Result<Voice, ProjectError> {
        let input = input.validate()?;
        let project_id = input.project_id.as_deref().ok_or_else(|| {
            WritingError::ValidationFailed(
                "User-created Voices must belong to a project.".to_owned(),
            )
        })?;
        let mut active = self
            .active
            .write()
            .map_err(|_| ProjectError::StateUnavailable)?;
        let mut connection = self.database.connection()?;
        let transaction = connection.transaction().map_err(write_failed)?;
        ensure_project(&transaction, project_id, true)?;
        let id = Uuid::new_v4().to_string();
        write_voice(&transaction, &id, &input, false)?;
        let voice = find_voice(&transaction, &id)?.ok_or(WritingError::VoiceNotFound)?;
        let snapshot = (active.project_id == project_id)
            .then(|| load_active_snapshot(&transaction))
            .transpose()?;
        transaction.commit().map_err(write_failed)?;
        if let Some(snapshot) = snapshot {
            *active = Arc::new(snapshot);
        }
        Ok(voice)
    }

    pub fn update_voice(&self, voice_id: &str, input: VoiceInput) -> Result<Voice, ProjectError> {
        let input = input.validate()?;
        let mut active = self
            .active
            .write()
            .map_err(|_| ProjectError::StateUnavailable)?;
        let mut connection = self.database.connection()?;
        let transaction = connection.transaction().map_err(write_failed)?;
        let existing = find_voice(&transaction, voice_id)?.ok_or(WritingError::VoiceNotFound)?;
        if existing.is_builtin {
            return Err(WritingError::BuiltinVoiceProtected.into());
        }
        if existing.project_id != input.project_id {
            return Err(WritingError::VoiceProjectMismatch.into());
        }
        if let Some(project_id) = input.project_id.as_deref() {
            ensure_project(&transaction, project_id, true)?;
        }
        write_voice(&transaction, voice_id, &input, true)?;
        let voice = find_voice(&transaction, voice_id)?.ok_or(WritingError::VoiceNotFound)?;
        let snapshot = (active.voice.id == voice_id)
            .then(|| load_active_snapshot(&transaction))
            .transpose()?;
        transaction.commit().map_err(write_failed)?;
        if let Some(snapshot) = snapshot {
            *active = Arc::new(snapshot);
        }
        Ok(voice)
    }

    pub fn delete_voice(&self, project_id: &str, voice_id: &str) -> Result<(), ProjectError> {
        let mut active = self
            .active
            .write()
            .map_err(|_| ProjectError::StateUnavailable)?;
        let mut connection = self.database.connection()?;
        let transaction = connection.transaction().map_err(write_failed)?;
        ensure_project(&transaction, project_id, true)?;
        let voice = find_voice(&transaction, voice_id)?.ok_or(WritingError::VoiceNotFound)?;
        ensure_voice_available(&voice, project_id)?;
        if voice.is_builtin {
            return Err(WritingError::BuiltinVoiceProtected.into());
        }
        transaction
            .execute(
                "UPDATE projects SET default_voice_id=?1, updated_at=CURRENT_TIMESTAMP WHERE default_voice_id=?2",
                params![MY_VOICE_ID, voice_id],
            )
            .map_err(write_failed)?;
        transaction
            .execute("DELETE FROM voices WHERE id=?1", [voice_id])
            .map_err(write_failed)?;
        let snapshot = (active.project_id == project_id)
            .then(|| load_active_snapshot(&transaction))
            .transpose()?;
        transaction.commit().map_err(write_failed)?;
        if let Some(snapshot) = snapshot {
            *active = Arc::new(snapshot);
        }
        Ok(())
    }

    pub fn list_modes(&self) -> Result<Vec<Mode>, ProjectError> {
        let connection = self.database.connection()?;
        list_modes(&connection)
    }

    pub fn set_project_voice(
        &self,
        project_id: &str,
        voice_id: &str,
    ) -> Result<Project, ProjectError> {
        self.set_writing_selection(project_id, Some(voice_id), None)
    }

    pub fn set_project_mode(
        &self,
        project_id: &str,
        mode_id: &str,
    ) -> Result<Project, ProjectError> {
        self.set_writing_selection(project_id, None, Some(mode_id))
    }

    fn set_writing_selection(
        &self,
        project_id: &str,
        voice_id: Option<&str>,
        mode_id: Option<&str>,
    ) -> Result<Project, ProjectError> {
        let mut active = self
            .active
            .write()
            .map_err(|_| ProjectError::StateUnavailable)?;
        let mut connection = self.database.connection()?;
        let transaction = connection.transaction().map_err(write_failed)?;
        let project = ensure_project(&transaction, project_id, true)?;
        if let Some(voice_id) = voice_id {
            let voice = find_voice(&transaction, voice_id)?.ok_or(WritingError::VoiceNotFound)?;
            ensure_voice_available(&voice, project_id)?;
            transaction
                .execute(
                    "UPDATE projects SET default_voice_id=?2, updated_at=CURRENT_TIMESTAMP WHERE id=?1",
                    params![project_id, voice_id],
                )
                .map_err(write_failed)?;
        }
        if let Some(mode_id) = mode_id {
            let mode = find_mode(&transaction, mode_id)?.ok_or(WritingError::ModeNotFound)?;
            if mode.supports_selection {
                return Err(WritingError::SelectionModeUnavailable.into());
            }
            transaction
                .execute(
                    "UPDATE projects SET default_mode_id=?2, updated_at=CURRENT_TIMESTAMP WHERE id=?1",
                    params![project_id, mode_id],
                )
                .map_err(write_failed)?;
        }
        let snapshot = (active.project_id == project_id)
            .then(|| load_active_snapshot(&transaction))
            .transpose()?;
        let project = find_project(&transaction, project_id)?.unwrap_or(project);
        transaction.commit().map_err(write_failed)?;
        if let Some(snapshot) = snapshot {
            *active = Arc::new(snapshot);
        }
        Ok(project)
    }
}

impl ProjectContextSource for ProjectService {
    fn active_snapshot(&self) -> Result<Arc<ProjectVocabularySnapshot>, ProjectError> {
        self.active
            .read()
            .map(|snapshot| snapshot.clone())
            .map_err(|_| ProjectError::StateUnavailable)
    }
}

#[derive(Debug, Error)]
pub enum ProjectError {
    #[error("project validation failed: {0}")]
    ValidationFailed(String),
    #[error("project was not found")]
    NotFound,
    #[error("project is archived")]
    Archived,
    #[error("at least one usable project is required")]
    CannotRemoveLastUsableProject,
    #[error("active project is unavailable")]
    ActiveProjectUnavailable,
    #[error("project state lock is unavailable")]
    StateUnavailable,
    #[error(transparent)]
    Vocabulary(#[from] VocabularyError),
    #[error(transparent)]
    Writing(#[from] WritingError),
    #[error(transparent)]
    Storage(#[from] AppError),
    #[error("project database read failed")]
    DatabaseReadFailed,
    #[error("project database write failed")]
    DatabaseWriteFailed,
}

impl ProjectError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::ValidationFailed(_) => "PROJECT_VALIDATION_FAILED",
            Self::NotFound => "PROJECT_NOT_FOUND",
            Self::Archived => "PROJECT_ARCHIVED",
            Self::CannotRemoveLastUsableProject => "PROJECT_LAST_USABLE",
            Self::ActiveProjectUnavailable => "ACTIVE_PROJECT_UNAVAILABLE",
            Self::StateUnavailable => "PROJECT_STATE_UNAVAILABLE",
            Self::Vocabulary(error) => error.code(),
            Self::Writing(error) => error.code(),
            Self::Storage(_) | Self::DatabaseReadFailed => "DATABASE_READ_FAILED",
            Self::DatabaseWriteFailed => "DATABASE_WRITE_FAILED",
        }
    }

    pub const fn user_message(&self) -> &'static str {
        match self {
            Self::ValidationFailed(_) => "Check the project fields and try again.",
            Self::NotFound => "That project no longer exists.",
            Self::Archived => "Unarchive this project before changing its vocabulary.",
            Self::CannotRemoveLastUsableProject => {
                "Create or unarchive another project before removing this one."
            }
            Self::ActiveProjectUnavailable | Self::StateUnavailable => {
                "Trueflow could not load the active project."
            }
            Self::Vocabulary(error) => error.user_message(),
            Self::Writing(error) => error.user_message(),
            Self::Storage(_) | Self::DatabaseReadFailed | Self::DatabaseWriteFailed => {
                "Trueflow could not update its project database."
            }
        }
    }
}

const PROJECT_SELECT: &str = "SELECT p.id, p.name, p.description, p.default_voice_id, \
    p.default_mode_id, p.created_at, p.updated_at, p.archived_at, \
    EXISTS(SELECT 1 FROM active_project a WHERE a.project_id=p.id) AS active FROM projects p";

fn map_project(row: &rusqlite::Row<'_>) -> rusqlite::Result<Project> {
    Ok(Project {
        id: row.get(0)?,
        name: row.get(1)?,
        description: row.get(2)?,
        default_voice_id: row.get(3)?,
        default_mode_id: row.get(4)?,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
        archived_at: row.get(7)?,
        active: row.get(8)?,
    })
}

fn find_project(
    connection: &Connection,
    project_id: &str,
) -> Result<Option<Project>, ProjectError> {
    connection
        .query_row(
            &(PROJECT_SELECT.to_owned() + " WHERE p.id=?1"),
            [project_id],
            map_project,
        )
        .optional()
        .map_err(read_failed)
}

fn ensure_project(
    connection: &Connection,
    project_id: &str,
    require_usable: bool,
) -> Result<Project, ProjectError> {
    let project = find_project(connection, project_id)?.ok_or(ProjectError::NotFound)?;
    if require_usable && project.archived_at.is_some() {
        Err(ProjectError::Archived)
    } else {
        Ok(project)
    }
}

fn replacement_project_id(
    transaction: &Transaction<'_>,
    excluded: &str,
) -> Result<Option<String>, ProjectError> {
    transaction
        .query_row(
            "SELECT id FROM projects WHERE archived_at IS NULL AND id<>?1 ORDER BY created_at, id LIMIT 1",
            [excluded],
            |row| row.get(0),
        )
        .optional()
        .map_err(read_failed)
}

fn usable_project_count(connection: &Connection) -> Result<i64, ProjectError> {
    connection
        .query_row(
            "SELECT COUNT(*) FROM projects WHERE archived_at IS NULL",
            [],
            |row| row.get(0),
        )
        .map_err(read_failed)
}

fn load_active_snapshot(
    connection: &Connection,
) -> Result<ProjectVocabularySnapshot, ProjectError> {
    let project = connection
        .query_row(
            &(PROJECT_SELECT.to_owned()
                + " JOIN active_project a ON a.project_id=p.id WHERE a.singleton=1 AND p.archived_at IS NULL"),
            [],
            map_project,
        )
        .optional()
        .map_err(read_failed)?
        .ok_or(ProjectError::ActiveProjectUnavailable)?;
    Ok(ProjectVocabularySnapshot {
        vocabulary: list_vocabulary(connection, &project.id)?,
        voice: find_voice(
            connection,
            project.default_voice_id.as_deref().unwrap_or(MY_VOICE_ID),
        )?
        .ok_or(WritingError::VoiceNotFound)?,
        mode: find_mode(
            connection,
            project
                .default_mode_id
                .as_deref()
                .unwrap_or(CLEAN_DICTATION_MODE_ID),
        )?
        .ok_or(WritingError::ModeNotFound)?,
        project_id: project.id,
        project_name: project.name,
        project_description: project.description,
    })
}

fn list_voices(connection: &Connection, project_id: &str) -> Result<Vec<Voice>, ProjectError> {
    let mut statement = connection
        .prepare(
            "SELECT id FROM voices WHERE project_id IS NULL OR project_id=?1 ORDER BY is_builtin DESC, lower(name), id",
        )
        .map_err(read_failed)?;
    let ids = statement
        .query_map([project_id], |row| row.get::<_, String>(0))
        .map_err(read_failed)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(read_failed)?;
    ids.into_iter()
        .map(|id| find_voice(connection, &id)?.ok_or(WritingError::VoiceNotFound.into()))
        .collect()
}

fn find_voice(connection: &Connection, voice_id: &str) -> Result<Option<Voice>, ProjectError> {
    let voice = connection
        .query_row(
            "SELECT id, project_id, name, description, instructions, avoidances, is_default, is_builtin, created_at, updated_at FROM voices WHERE id=?1",
            [voice_id],
            |row| {
                Ok(Voice {
                    id: row.get(0)?,
                    project_id: row.get(1)?,
                    name: row.get(2)?,
                    description: row.get(3)?,
                    instructions: row.get(4)?,
                    examples: Vec::new(),
                    avoidances: row.get(5)?,
                    is_default: row.get(6)?,
                    is_builtin: row.get(7)?,
                    created_at: row.get(8)?,
                    updated_at: row.get(9)?,
                })
            },
        )
        .optional()
        .map_err(read_failed)?;
    let Some(mut voice) = voice else {
        return Ok(None);
    };
    let mut statement = connection
        .prepare("SELECT example FROM voice_examples WHERE voice_id=?1 ORDER BY position")
        .map_err(read_failed)?;
    voice.examples = statement
        .query_map([voice_id], |row| row.get(0))
        .map_err(read_failed)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(read_failed)?;
    Ok(Some(voice))
}

fn ensure_voice_available(voice: &Voice, project_id: &str) -> Result<(), ProjectError> {
    if voice
        .project_id
        .as_deref()
        .is_none_or(|owner| owner == project_id)
    {
        Ok(())
    } else {
        Err(WritingError::VoiceProjectMismatch.into())
    }
}

fn write_voice(
    transaction: &Transaction<'_>,
    voice_id: &str,
    input: &ValidatedVoiceInput,
    update: bool,
) -> Result<(), ProjectError> {
    if update {
        transaction
            .execute(
                "UPDATE voices SET name=?2, description=?3, instructions=?4, avoidances=?5, updated_at=CURRENT_TIMESTAMP WHERE id=?1",
                params![voice_id, input.name, input.description, input.instructions, input.avoidances],
            )
            .map_err(write_failed)?;
        transaction
            .execute("DELETE FROM voice_examples WHERE voice_id=?1", [voice_id])
            .map_err(write_failed)?;
    } else {
        transaction
            .execute(
                "INSERT INTO voices (id, project_id, name, description, instructions, avoidances) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![voice_id, input.project_id, input.name, input.description, input.instructions, input.avoidances],
            )
            .map_err(write_failed)?;
    }
    for (position, example) in input.examples.iter().enumerate() {
        transaction
            .execute(
                "INSERT INTO voice_examples (voice_id, position, example) VALUES (?1, ?2, ?3)",
                params![
                    voice_id,
                    i64::try_from(position).unwrap_or(i64::MAX),
                    example
                ],
            )
            .map_err(write_failed)?;
    }
    Ok(())
}

fn list_modes(connection: &Connection) -> Result<Vec<Mode>, ProjectError> {
    let mut statement = connection
        .prepare("SELECT id, name, description, system_instructions, requires_voice, supports_selection, is_builtin, position, created_at, updated_at FROM modes ORDER BY position, id")
        .map_err(read_failed)?;
    statement
        .query_map([], map_mode)
        .map_err(read_failed)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(read_failed)
}

fn find_mode(connection: &Connection, mode_id: &str) -> Result<Option<Mode>, ProjectError> {
    connection
        .query_row(
            "SELECT id, name, description, system_instructions, requires_voice, supports_selection, is_builtin, position, created_at, updated_at FROM modes WHERE id=?1",
            [mode_id],
            map_mode,
        )
        .optional()
        .map_err(read_failed)
}

fn map_mode(row: &rusqlite::Row<'_>) -> rusqlite::Result<Mode> {
    Ok(Mode {
        id: row.get(0)?,
        name: row.get(1)?,
        description: row.get(2)?,
        system_instructions: row.get(3)?,
        requires_voice: row.get(4)?,
        supports_selection: row.get(5)?,
        is_builtin: row.get(6)?,
        position: row.get(7)?,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
    })
}

fn list_vocabulary(
    connection: &Connection,
    project_id: &str,
) -> Result<Vec<VocabularyEntry>, ProjectError> {
    let mut statement = connection
        .prepare(
            "SELECT id, project_id, term, pronunciation_hint, kind, description, case_sensitive, created_at, updated_at \
             FROM vocabulary_entries WHERE project_id=?1 ORDER BY lower(term), id",
        )
        .map_err(read_failed)?;
    let ids = statement
        .query_map([project_id], |row| row.get::<_, String>(0))
        .map_err(read_failed)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(read_failed)?;
    ids.into_iter()
        .map(|id| {
            find_vocabulary(connection, &id)?
                .ok_or_else(|| ProjectError::Vocabulary(VocabularyError::EntryNotFound))
        })
        .collect()
}

fn find_vocabulary(
    connection: &Connection,
    entry_id: &str,
) -> Result<Option<VocabularyEntry>, ProjectError> {
    let base = connection
        .query_row(
            "SELECT id, project_id, term, pronunciation_hint, kind, description, case_sensitive, created_at, updated_at \
             FROM vocabulary_entries WHERE id=?1",
            [entry_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, bool>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, String>(8)?,
                ))
            },
        )
        .optional()
        .map_err(read_failed)?;
    let Some((
        id,
        project_id,
        term,
        pronunciation_hint,
        kind,
        description,
        case_sensitive,
        created_at,
        updated_at,
    )) = base
    else {
        return Ok(None);
    };
    Ok(Some(VocabularyEntry {
        aliases: related_list(connection, "vocabulary_aliases", &id)?,
        forbidden_variants: related_list(connection, "vocabulary_forbidden_variants", &id)?,
        id,
        project_id,
        term,
        pronunciation_hint,
        kind: VocabularyKind::parse(&kind)?,
        description,
        case_sensitive,
        created_at,
        updated_at,
    }))
}

fn related_list(
    connection: &Connection,
    table: &str,
    entry_id: &str,
) -> Result<Vec<String>, ProjectError> {
    let sql = format!("SELECT value FROM {table} WHERE vocabulary_entry_id=?1 ORDER BY id");
    let mut statement = connection.prepare(&sql).map_err(read_failed)?;
    statement
        .query_map([entry_id], |row| row.get(0))
        .map_err(read_failed)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(read_failed)
}

fn ensure_unique_term(
    connection: &Connection,
    project_id: &str,
    term: &str,
    except_id: Option<&str>,
) -> Result<(), ProjectError> {
    let entries = list_vocabulary(connection, project_id)?;
    let folded = term
        .chars()
        .flat_map(char::to_lowercase)
        .collect::<String>();
    if entries.iter().any(|entry| {
        Some(entry.id.as_str()) != except_id
            && entry
                .term
                .chars()
                .flat_map(char::to_lowercase)
                .collect::<String>()
                == folded
    }) {
        Err(VocabularyError::DuplicateEntry.into())
    } else {
        Ok(())
    }
}

fn write_vocabulary(
    transaction: &Transaction<'_>,
    entry_id: &str,
    project_id: &str,
    input: &ValidatedVocabularyInput,
    update: bool,
) -> Result<(), ProjectError> {
    if update {
        transaction
            .execute(
                "UPDATE vocabulary_entries SET term=?3, pronunciation_hint=?4, kind=?5, description=?6, case_sensitive=?7, updated_at=CURRENT_TIMESTAMP WHERE id=?1 AND project_id=?2",
                params![entry_id, project_id, input.term, input.pronunciation_hint, input.kind.as_str(), input.description, input.case_sensitive],
            )
            .map_err(write_failed)?;
        transaction
            .execute(
                "DELETE FROM vocabulary_aliases WHERE vocabulary_entry_id=?1",
                [entry_id],
            )
            .map_err(write_failed)?;
        transaction
            .execute(
                "DELETE FROM vocabulary_forbidden_variants WHERE vocabulary_entry_id=?1",
                [entry_id],
            )
            .map_err(write_failed)?;
    } else {
        transaction
            .execute(
                "INSERT INTO vocabulary_entries (id, project_id, term, pronunciation_hint, kind, description, case_sensitive) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![entry_id, project_id, input.term, input.pronunciation_hint, input.kind.as_str(), input.description, input.case_sensitive],
            )
            .map_err(write_failed)?;
    }
    for alias in &input.aliases {
        transaction
            .execute(
                "INSERT INTO vocabulary_aliases (vocabulary_entry_id, value) VALUES (?1, ?2)",
                params![entry_id, alias],
            )
            .map_err(write_failed)?;
    }
    for variant in &input.forbidden_variants {
        transaction
            .execute(
                "INSERT INTO vocabulary_forbidden_variants (vocabulary_entry_id, value) VALUES (?1, ?2)",
                params![entry_id, variant],
            )
            .map_err(write_failed)?;
    }
    Ok(())
}

fn read_failed(_error: rusqlite::Error) -> ProjectError {
    ProjectError::DatabaseReadFailed
}

fn write_failed(_error: rusqlite::Error) -> ProjectError {
    ProjectError::DatabaseWriteFailed
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    fn service() -> Arc<ProjectService> {
        let directory = tempdir().expect("temporary directory");
        let path = directory.keep().join("projects.sqlite3");
        ProjectService::new(Arc::new(Database::open(&path).expect("database")))
            .expect("project service")
    }

    fn project(name: &str) -> ProjectInput {
        ProjectInput {
            name: name.to_owned(),
            description: String::new(),
        }
    }

    fn vocabulary(term: &str) -> VocabularyInput {
        VocabularyInput {
            term: term.to_owned(),
            pronunciation_hint: Some("spoken hint".to_owned()),
            kind: VocabularyKind::Person,
            description: Some("description".to_owned()),
            aliases: vec![format!("{term} alias")],
            forbidden_variants: vec![format!("{term} wrong")],
            case_sensitive: false,
        }
    }

    fn voice(project_id: &str, name: &str) -> VoiceInput {
        VoiceInput {
            project_id: Some(project_id.to_owned()),
            name: name.to_owned(),
            description: format!("{name} description"),
            instructions: format!("Write consistently as {name}."),
            examples: vec![format!("A representative {name} line.")],
            avoidances: "Avoid generic phrasing.".to_owned(),
        }
    }

    #[test]
    fn default_project_is_unique_active_and_persistent() {
        let service = service();
        let projects = service.list_projects(true).expect("projects");
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].name, "General");
        assert!(projects[0].active);
        assert_eq!(
            service.active_snapshot().expect("active").project_id,
            projects[0].id
        );
    }

    #[test]
    fn project_crud_preserves_a_valid_active_project() {
        let service = service();
        let general = service.list_projects(false).expect("projects")[0].clone();
        let created = service.create_project(project("Novel")).expect("create");
        service.set_active_project(&created.id).expect("activate");
        let renamed = service
            .update_project(&created.id, project("Novel Two"))
            .expect("rename");
        assert_eq!(renamed.name, "Novel Two");
        service.archive_project(&created.id).expect("archive");
        assert_eq!(
            service.active_snapshot().expect("active").project_id,
            general.id
        );
        service.unarchive_project(&created.id).expect("unarchive");
        service.delete_project(&created.id).expect("delete");
        assert_eq!(service.list_projects(true).expect("projects").len(), 1);
        assert_eq!(
            service.delete_project(&general.id).unwrap_err().code(),
            "PROJECT_LAST_USABLE"
        );
    }

    #[test]
    fn vocabulary_crud_is_isolated_and_cascades_with_project() {
        let service = service();
        let project_a = service.create_project(project("A")).expect("project A");
        let project_b = service.create_project(project("B")).expect("project B");
        let entry = service
            .create_vocabulary_entry(&project_a.id, vocabulary("Élara"))
            .expect("entry");
        assert_eq!(entry.aliases, vec!["Élara alias"]);
        assert!(
            service
                .list_vocabulary(&project_b.id)
                .expect("B entries")
                .is_empty()
        );
        assert_eq!(
            service
                .get_vocabulary_entry(&project_b.id, &entry.id)
                .unwrap_err()
                .code(),
            "VOCABULARY_PROJECT_MISMATCH"
        );
        let mut update = vocabulary("Élara Updated");
        update.case_sensitive = true;
        let updated = service
            .update_vocabulary_entry(&project_a.id, &entry.id, update)
            .expect("update");
        assert!(updated.case_sensitive);
        service
            .delete_project(&project_a.id)
            .expect("cascade delete");
        assert!(matches!(
            service.get_vocabulary_entry(&project_a.id, &entry.id),
            Err(ProjectError::Vocabulary(VocabularyError::EntryNotFound))
        ));
    }

    #[test]
    fn duplicate_terms_are_rejected_case_insensitively() {
        let service = service();
        let project = service.list_projects(false).expect("projects")[0].clone();
        service
            .create_vocabulary_entry(&project.id, vocabulary("Aeryndor"))
            .expect("first");
        assert_eq!(
            service
                .create_vocabulary_entry(&project.id, vocabulary("AERYNDOR"))
                .unwrap_err()
                .code(),
            "VOCABULARY_DUPLICATE_ENTRY"
        );
    }

    #[test]
    fn switching_and_updates_replace_the_active_snapshot() {
        let service = service();
        let general = service.list_projects(false).expect("projects")[0].clone();
        let novel = service.create_project(project("Novel")).expect("novel");
        service
            .create_vocabulary_entry(&general.id, vocabulary("GeneralTerm"))
            .expect("general term");
        let before = service.active_snapshot().expect("snapshot before");
        service.set_active_project(&novel.id).expect("switch");
        let during = before;
        service
            .create_vocabulary_entry(&novel.id, vocabulary("NovelTerm"))
            .expect("novel term");
        let after = service.active_snapshot().expect("snapshot after");
        assert_eq!(during.project_id, general.id);
        assert_eq!(during.vocabulary[0].term, "GeneralTerm");
        assert_eq!(after.project_id, novel.id);
        assert_eq!(after.vocabulary[0].term, "NovelTerm");
    }

    #[test]
    fn voice_crud_and_selection_are_project_isolated() {
        let service = service();
        let project_a = service
            .create_project(project("Voice A"))
            .expect("project A");
        let project_b = service
            .create_project(project("Voice B"))
            .expect("project B");
        service
            .set_active_project(&project_a.id)
            .expect("activate A");

        let created = service
            .create_voice(voice(&project_a.id, "Kael"))
            .expect("create Voice");
        assert_eq!(created.examples, vec!["A representative Kael line."]);
        assert!(
            service
                .list_voices(&project_b.id)
                .expect("project B Voices")
                .iter()
                .all(|voice| voice.id != created.id)
        );
        assert_eq!(
            service
                .get_voice(&project_b.id, &created.id)
                .unwrap_err()
                .code(),
            "VOICE_PROJECT_MISMATCH"
        );

        let selected = service
            .set_project_voice(&project_a.id, &created.id)
            .expect("select Voice");
        assert_eq!(
            selected.default_voice_id.as_deref(),
            Some(created.id.as_str())
        );
        assert_eq!(
            service.active_snapshot().expect("snapshot").voice.name,
            "Kael"
        );

        let updated = service
            .update_voice(&created.id, voice(&project_a.id, "Kael Revised"))
            .expect("update Voice");
        assert_eq!(updated.name, "Kael Revised");
        assert_eq!(
            service.active_snapshot().expect("snapshot").voice.name,
            "Kael Revised"
        );

        service
            .delete_voice(&project_a.id, &created.id)
            .expect("delete Voice");
        assert_eq!(
            service
                .get_project(&project_a.id)
                .expect("project")
                .default_voice_id
                .as_deref(),
            Some(MY_VOICE_ID)
        );
        assert_eq!(
            service
                .delete_voice(&project_a.id, MY_VOICE_ID)
                .unwrap_err()
                .code(),
            "VOICE_BUILTIN_PROTECTED"
        );
        let mut builtin_update = voice(&project_a.id, "Changed Built-in");
        builtin_update.project_id = None;
        assert_eq!(
            service
                .update_voice(MY_VOICE_ID, builtin_update)
                .unwrap_err()
                .code(),
            "VOICE_BUILTIN_PROTECTED"
        );
    }

    #[test]
    fn built_in_modes_are_ordered_and_rewrite_stays_out_of_normal_dictation() {
        let service = service();
        let project = service.list_projects(false).expect("projects")[0].clone();
        let modes = service.list_modes().expect("modes");
        assert_eq!(
            modes
                .iter()
                .map(|mode| mode.name.as_str())
                .collect::<Vec<_>>(),
            vec![
                "Clean Dictation",
                "Draft",
                "Dialogue",
                "Brainstorm",
                "Rewrite Selection"
            ]
        );
        let dialogue = modes
            .iter()
            .find(|mode| mode.name == "Dialogue")
            .expect("Dialogue");
        let selected = service
            .set_project_mode(&project.id, &dialogue.id)
            .expect("select Dialogue");
        assert_eq!(
            selected.default_mode_id.as_deref(),
            Some(dialogue.id.as_str())
        );

        let rewrite = modes
            .iter()
            .find(|mode| mode.name == "Rewrite Selection")
            .expect("Rewrite Selection");
        assert_eq!(
            service
                .set_project_mode(&project.id, &rewrite.id)
                .unwrap_err()
                .code(),
            "MODE_SELECTION_REWRITE_UNAVAILABLE"
        );
    }
}
