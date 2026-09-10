use std::{io, path::PathBuf};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("unsupported application environment: {0}")]
    InvalidEnvironment(String),
    #[error("failed to create database directory {path}: {source}")]
    CreateDatabaseDirectory { path: PathBuf, source: io::Error },
    #[error("database path has no parent directory: {0}")]
    MissingDatabaseParent(PathBuf),
    #[error("database operation failed: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("database migration {version} ({name}) failed: {source}")]
    Migration {
        version: u32,
        name: &'static str,
        source: rusqlite::Error,
    },
    #[error("database lock is unavailable")]
    DatabaseLock,
    #[error("structured logging initialization failed: {0}")]
    Logging(#[from] tracing_subscriber::util::TryInitError),
    #[error("TLS crypto provider initialization failed")]
    TlsCryptoProvider,
}

impl AppError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::InvalidEnvironment(_) => "CONFIG_INVALID_ENVIRONMENT",
            Self::CreateDatabaseDirectory { .. }
            | Self::MissingDatabaseParent(_)
            | Self::Database(_)
            | Self::DatabaseLock => "DATABASE_UNAVAILABLE",
            Self::Migration { .. } => "DATABASE_MIGRATION_FAILED",
            Self::Logging(_) => "LOGGING_INITIALIZATION_FAILED",
            Self::TlsCryptoProvider => "TLS_CRYPTO_PROVIDER_INITIALIZATION_FAILED",
        }
    }

    pub const fn user_message(&self) -> &'static str {
        match self {
            Self::InvalidEnvironment(_) => "The application environment is invalid.",
            Self::CreateDatabaseDirectory { .. }
            | Self::MissingDatabaseParent(_)
            | Self::Database(_)
            | Self::DatabaseLock => "Trueflow could not open its local database.",
            Self::Migration { .. } => "Trueflow could not upgrade its local database.",
            Self::Logging(_) => "Trueflow could not initialize diagnostic logging.",
            Self::TlsCryptoProvider => "Trueflow could not initialize secure provider connections.",
        }
    }
}
