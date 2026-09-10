mod audio;
mod commands;
mod config;
mod credentials;
mod dictation;
mod error;
mod hotkey;
mod insertion;
mod platform;
mod product;
mod projects;
mod rewrite;
mod state;
mod storage;
mod telemetry;
pub mod transcription;
mod transformation;
mod tray;
mod vocabulary;
mod writing;

use std::sync::Arc;

use tauri::Manager;
use tracing::info;

use crate::{
    audio::CpalAudioInput,
    config::AppConfig,
    dictation::{DictationProviders, DictationService, TauriUiSink},
    insertion::InsertionCoordinator,
    platform::{
        macos::{MacOsMicrophonePermission, create_and_configure_overlay_window},
        macos_insertion::MacOsInsertionBackend,
    },
    product::{ProductService, install_crash_reporting},
    projects::ProjectService,
    rewrite::RewriteCoordinator,
    state::AppState,
    storage::Database,
    transcription::{DeepgramProvider, SessionTimeouts, initialize_tls_crypto_provider},
    transformation::{OpenAiTransformationProvider, TransformationConfig},
};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let application = tauri::Builder::default()
        .plugin(hotkey::plugin())
        .setup(|app| {
            let config = AppConfig::load()?;
            telemetry::initialize(&config.log_filter)?;
            initialize_tls_crypto_provider()?;

            let database_path = app.path().app_data_dir()?.join("trueflow.sqlite3");
            let database = Arc::new(Database::open(&database_path)?);
            let schema_version = database.schema_version()?;
            let projects = ProjectService::new(database.clone())?;
            let product = ProductService::new(database.clone(), config.clone())?;
            install_crash_reporting(&config);

            info!(
                subsystem = "application",
                event = "foundation_ready",
                environment = %config.environment,
                schema_version,
                "Trueflow foundation initialized"
            );

            create_and_configure_overlay_window(app.handle())?;

            let credentials = product.provider_credentials();
            let provider = Arc::new(DeepgramProvider::with_credentials(
                credentials.clone(),
                SessionTimeouts::default().connection,
            ));
            let macos_text_backend = Arc::new(MacOsInsertionBackend::new());
            let inserter = Arc::new(InsertionCoordinator::new(macos_text_backend.clone()));
            let rewriter = Arc::new(RewriteCoordinator::new(macos_text_backend));
            let transformation_config = TransformationConfig::default();
            let transformation_provider = Arc::new(OpenAiTransformationProvider::with_credentials(
                credentials,
                config.openai_model.clone(),
                transformation_config.timeout,
            ));
            let dictation = DictationService::new(
                Box::new(MacOsMicrophonePermission),
                Box::new(CpalAudioInput),
                projects.clone(),
                DictationProviders {
                    transcription: provider,
                    transcription_timeouts: SessionTimeouts::default(),
                    transformation: transformation_provider,
                    transformation_config,
                },
                inserter,
                rewriter,
                Box::new(TauriUiSink::new(app.handle().clone(), product.clone())),
            );
            app.manage(AppState::new(
                config,
                database,
                projects,
                dictation.clone(),
                product.clone(),
            ));

            app.state::<AppState>().product.record_app_launched();
            tauri::async_runtime::spawn(async move {
                if let Err(error) = product.refresh_account().await {
                    tracing::warn!(
                        subsystem = "authentication",
                        event = "startup_restore_failed",
                        error_code = error.code(),
                        "account restoration did not block local startup"
                    );
                }
            });

            tray::create(app.handle())?;

            if let Err(error) = hotkey::register(app.handle()) {
                dictation.set_fatal_error(error);
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::dictation::dictation_status,
            commands::dictation::dismiss_dictation_status,
            commands::dictation::request_microphone_permission,
            commands::dictation::open_microphone_settings_command,
            commands::dictation::accessibility_permission_status,
            commands::dictation::open_accessibility_settings,
            commands::foundation::foundation_status,
            commands::projects::list_projects,
            commands::projects::get_project,
            commands::projects::create_project,
            commands::projects::update_project,
            commands::projects::set_active_project,
            commands::projects::archive_project,
            commands::projects::unarchive_project,
            commands::projects::delete_project,
            commands::projects::list_vocabulary,
            commands::projects::get_vocabulary_entry,
            commands::projects::create_vocabulary_entry,
            commands::projects::update_vocabulary_entry,
            commands::projects::delete_vocabulary_entry,
            commands::writing::list_voices,
            commands::writing::get_voice,
            commands::writing::create_voice,
            commands::writing::update_voice,
            commands::writing::delete_voice,
            commands::writing::list_modes,
            commands::writing::set_project_voice,
            commands::writing::set_project_mode,
            commands::product::product_settings,
            commands::product::update_product_settings,
            commands::product::update_shortcuts,
            commands::product::update_onboarding,
            commands::product::list_history,
            commands::product::delete_history_item,
            commands::product::clear_history,
            commands::product::copy_history_item,
            commands::product::account_status,
            commands::product::begin_sign_in,
            commands::product::poll_sign_in,
            commands::product::refresh_account,
            commands::product::sign_out,
            commands::product::diagnostics,
            commands::product::report_client_error,
        ])
        .on_window_event(|window, event| {
            if window.label() == "main"
                && let tauri::WindowEvent::CloseRequested { api, .. } = event
            {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .build(tauri::generate_context!())
        .expect("failed to build the Trueflow desktop application");

    application.run(|app_handle, event| {
        if matches!(event, tauri::RunEvent::Exit) {
            hotkey::unregister(app_handle);
            app_handle.state::<AppState>().dictation.shutdown();
        }
    });
}
