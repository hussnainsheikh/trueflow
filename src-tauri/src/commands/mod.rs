pub(crate) mod dictation;
pub(crate) mod foundation;
pub(crate) mod product;
pub(crate) mod projects;
pub(crate) mod writing;

use serde::Serialize;

use crate::{
    dictation::DictationError, error::AppError, insertion::InsertionError, product::ProductError,
    projects::ProjectError,
};

#[derive(Debug, Eq, PartialEq, Serialize)]
pub struct CommandError {
    code: &'static str,
    message: &'static str,
}

impl From<AppError> for CommandError {
    fn from(error: AppError) -> Self {
        Self {
            code: error.code(),
            message: error.user_message(),
        }
    }
}

impl From<DictationError> for CommandError {
    fn from(error: DictationError) -> Self {
        Self {
            code: error.code(),
            message: error.user_message(),
        }
    }
}

impl From<InsertionError> for CommandError {
    fn from(error: InsertionError) -> Self {
        Self {
            code: error.code,
            message: error.message,
        }
    }
}

impl From<ProjectError> for CommandError {
    fn from(error: ProjectError) -> Self {
        Self {
            code: error.code(),
            message: error.user_message(),
        }
    }
}

impl From<ProductError> for CommandError {
    fn from(error: ProductError) -> Self {
        Self {
            code: error.code(),
            message: error.user_message(),
        }
    }
}
