use rusqlite::{Connection, OptionalExtension};

use crate::error::AppError;

#[derive(Clone, Copy)]
struct Migration {
    version: u32,
    name: &'static str,
    sql: &'static str,
}

const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "foundation",
        sql: include_str!("../../migrations/0001_foundation.sql"),
    },
    Migration {
        version: 2,
        name: "projects_vocabulary",
        sql: include_str!("../../migrations/0002_projects_vocabulary.sql"),
    },
    Migration {
        version: 3,
        name: "voices_modes",
        sql: include_str!("../../migrations/0003_voices_modes.sql"),
    },
    Migration {
        version: 4,
        name: "productization",
        sql: include_str!("../../migrations/0004_productization.sql"),
    },
    Migration {
        version: 5,
        name: "microphone_selection",
        sql: include_str!("../../migrations/0005_microphone_selection.sql"),
    },
];

pub fn run(connection: &mut Connection) -> Result<(), AppError> {
    run_list(connection, MIGRATIONS)
}

fn run_list(connection: &mut Connection, migrations: &[Migration]) -> Result<(), AppError> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (\
            version INTEGER PRIMARY KEY NOT NULL, \
            name TEXT NOT NULL, \
            applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP\
        );",
    )?;
    for migration in migrations {
        let already_applied = connection
            .query_row(
                "SELECT 1 FROM schema_migrations WHERE version = ?1",
                [migration.version],
                |_| Ok(()),
            )
            .optional()?
            .is_some();

        if already_applied {
            continue;
        }

        let result = (|| {
            let transaction = connection.transaction()?;
            transaction.execute_batch(migration.sql)?;
            transaction.execute(
                "INSERT INTO schema_migrations (version, name) VALUES (?1, ?2)",
                (migration.version, migration.name),
            )?;
            transaction.commit()
        })();
        result.map_err(|source| AppError::Migration {
            version: migration.version,
            name: migration.name,
            source,
        })?;
    }

    Ok(())
}

pub fn current_version(connection: &Connection) -> Result<u32, AppError> {
    let version = connection.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
        [],
        |row| row.get(0),
    )?;
    Ok(version)
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;

    use super::{MIGRATIONS, Migration, current_version, run, run_list};

    #[test]
    fn upgrades_a_version_one_database_without_losing_existing_metadata() {
        let mut connection = Connection::open_in_memory().expect("database");
        run_list(&mut connection, &MIGRATIONS[..1]).expect("foundation migration");
        connection
            .execute(
                "INSERT INTO schema_metadata (key, value) VALUES ('preserved', 'yes')",
                [],
            )
            .expect("existing data");

        run(&mut connection).expect("latest migration");

        assert_eq!(current_version(&connection).expect("version"), 5);
        assert_eq!(
            connection
                .query_row(
                    "SELECT value FROM schema_metadata WHERE key = 'preserved'",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .expect("preserved row"),
            "yes"
        );
    }

    #[test]
    fn upgrades_version_two_and_seeds_builtins_once() {
        let mut connection = Connection::open_in_memory().expect("database");
        run_list(&mut connection, &MIGRATIONS[..2]).expect("version two");
        connection
            .execute(
                "INSERT INTO vocabulary_entries (id, project_id, term, kind) VALUES ('kept', '00000000-0000-4000-8000-000000000001', 'Élara', 'person')",
                [],
            )
            .expect("milestone four data");

        run(&mut connection).expect("version three");
        run(&mut connection).expect("idempotent restart");

        assert_eq!(current_version(&connection).expect("version"), 5);
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM voices WHERE is_builtin=1",
                    [],
                    |row| row.get::<_, i64>(0)
                )
                .expect("voice count"),
            2
        );
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM modes WHERE is_builtin=1", [], |row| {
                    row.get::<_, i64>(0)
                })
                .expect("mode count"),
            5
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT term FROM vocabulary_entries WHERE id='kept'",
                    [],
                    |row| row.get::<_, String>(0)
                )
                .expect("preserved vocabulary"),
            "Élara"
        );
    }

    #[test]
    fn upgrades_version_three_to_productization_without_losing_writing_data() {
        let mut connection = Connection::open_in_memory().expect("database");
        run_list(&mut connection, &MIGRATIONS[..3]).expect("version three");
        connection.execute("UPDATE projects SET name='Preserved manuscript' WHERE id='00000000-0000-4000-8000-000000000001'", []).expect("existing project");

        run(&mut connection).expect("productization migration");

        assert_eq!(current_version(&connection).expect("version"), 5);
        assert_eq!(
            connection
                .query_row(
                    "SELECT name FROM projects WHERE id='00000000-0000-4000-8000-000000000001'",
                    [],
                    |row| row.get::<_, String>(0)
                )
                .expect("project"),
            "Preserved manuscript"
        );
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM product_settings", [], |row| row
                    .get::<_, u32>(0))
                .expect("settings"),
            1
        );
    }

    #[test]
    fn upgrades_version_four_to_microphone_selection_preserving_settings() {
        let mut connection = Connection::open_in_memory().expect("database");
        run_list(&mut connection, &MIGRATIONS[..4]).expect("version four");
        connection
            .execute(
                "UPDATE product_settings SET show_overlay=0, dictation_shortcut='Control+Space' WHERE singleton=1",
                [],
            )
            .expect("existing settings");

        run(&mut connection).expect("microphone selection migration");

        assert_eq!(current_version(&connection).expect("version"), 5);
        let (show_overlay, shortcut, device_id, device_name) = connection
            .query_row(
                "SELECT show_overlay, dictation_shortcut, microphone_device_id, microphone_device_name FROM product_settings WHERE singleton=1",
                [],
                |row| {
                    Ok((
                        row.get::<_, bool>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, Option<String>>(3)?,
                    ))
                },
            )
            .expect("settings");
        assert!(!show_overlay);
        assert_eq!(shortcut, "Control+Space");
        assert_eq!(
            device_id, None,
            "existing installs keep following the system default"
        );
        assert_eq!(device_name, None);
        assert!(
            connection
                .execute(
                    "UPDATE product_settings SET microphone_device_id='' WHERE singleton=1",
                    [],
                )
                .is_err(),
            "empty device identifiers are rejected by the schema"
        );
    }

    #[test]
    fn failed_migration_rolls_back_its_schema_and_version() {
        let mut connection = Connection::open_in_memory().expect("database");
        run_list(&mut connection, &MIGRATIONS[..1]).expect("foundation migration");
        let failure = Migration {
            version: 2,
            name: "intentional_failure",
            sql: "CREATE TABLE should_rollback (id INTEGER); INVALID SQL;",
        };

        assert!(run_list(&mut connection, &[failure]).is_err());
        assert_eq!(current_version(&connection).expect("version"), 1);
        assert!(
            connection
                .query_row(
                    "SELECT 1 FROM sqlite_master WHERE type='table' AND name='should_rollback'",
                    [],
                    |_| Ok(()),
                )
                .is_err()
        );
    }
}
