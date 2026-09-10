mod migrations;

use std::{
    fs,
    path::Path,
    sync::{Mutex, MutexGuard},
};

use rusqlite::{Connection, OpenFlags};

use crate::error::AppError;

pub struct Database {
    connection: Mutex<Connection>,
}

impl Database {
    pub fn open(path: &Path) -> Result<Self, AppError> {
        let parent = path
            .parent()
            .ok_or_else(|| AppError::MissingDatabaseParent(path.to_path_buf()))?;
        fs::create_dir_all(parent).map_err(|source| AppError::CreateDatabaseDirectory {
            path: parent.to_path_buf(),
            source,
        })?;

        let mut connection = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
        )?;
        connection.pragma_update(None, "foreign_keys", true)?;
        migrations::run(&mut connection)?;

        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    pub fn schema_version(&self) -> Result<u32, AppError> {
        let connection = self.connection.lock().map_err(|_| AppError::DatabaseLock)?;
        migrations::current_version(&connection)
    }

    pub(crate) fn connection(&self) -> Result<MutexGuard<'_, Connection>, AppError> {
        self.connection.lock().map_err(|_| AppError::DatabaseLock)
    }
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::Database;

    #[test]
    fn creates_parent_directory_and_runs_migrations() {
        let temporary_directory = tempdir().expect("temporary directory should be available");
        let database_path = temporary_directory.path().join("nested/trueflow.sqlite3");

        let database = Database::open(&database_path).expect("database should initialize");

        assert!(database_path.exists());
        assert_eq!(database.schema_version().expect("version should load"), 4);
    }

    #[test]
    fn reopening_database_does_not_reapply_migrations() {
        let temporary_directory = tempdir().expect("temporary directory should be available");
        let database_path = temporary_directory.path().join("trueflow.sqlite3");

        Database::open(&database_path).expect("first initialization should succeed");
        let database =
            Database::open(&database_path).expect("second initialization should succeed");

        assert_eq!(database.schema_version().expect("version should load"), 4);

        let connection = rusqlite::Connection::open(&database_path).expect("inspect database");
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM projects", [], |row| row
                    .get::<_, i64>(0))
                .expect("project count"),
            1
        );
    }
}
