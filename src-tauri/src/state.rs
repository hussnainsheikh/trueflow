use std::sync::Arc;

use crate::{
    audio::MicrophoneSelection, config::AppConfig, dictation::DictationService,
    product::ProductService, projects::ProjectService, storage::Database,
};

pub struct AppState {
    pub config: AppConfig,
    pub database: Arc<Database>,
    pub projects: Arc<ProjectService>,
    pub dictation: Arc<DictationService>,
    pub product: Arc<ProductService>,
    pub microphone: MicrophoneSelection,
}

impl AppState {
    pub const fn new(
        config: AppConfig,
        database: Arc<Database>,
        projects: Arc<ProjectService>,
        dictation: Arc<DictationService>,
        product: Arc<ProductService>,
        microphone: MicrophoneSelection,
    ) -> Self {
        Self {
            config,
            database,
            projects,
            dictation,
            product,
            microphone,
        }
    }
}
