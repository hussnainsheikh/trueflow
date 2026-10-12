use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use async_trait::async_trait;
use reqwest::Client;
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::json;
use thiserror::Error;
use tracing::{info, warn};
use uuid::Uuid;

use crate::{
    audio::valid_device_field,
    config::AppConfig,
    credentials::ProviderCredentials,
    dictation::{DictationSnapshot, DictationState},
    rewrite::OperationKind,
    storage::Database,
};

const HISTORY_PAGE_LIMIT: u32 = 50;
const HISTORY_PREVIEW_CHARACTERS: u32 = 280;
const HISTORY_RETENTION_LIMIT: u32 = 1_000;

#[derive(Debug, Error)]
pub enum ProductError {
    #[error("local product data is unavailable: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("local product data is locked")]
    DatabaseLock,
    #[error("invalid product setting: {0}")]
    InvalidSetting(&'static str),
    #[error("requested history item was not found")]
    HistoryNotFound,
    #[error("authentication is not configured")]
    AuthUnconfigured,
    #[error("authentication request failed")]
    AuthNetwork,
    #[error("authentication response was invalid")]
    AuthInvalidResponse,
    #[error("authentication request expired")]
    AuthExpired,
    #[error("authentication is still pending")]
    AuthPending,
    #[error("secure session storage is unavailable")]
    SecureStorage,
}

impl ProductError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Database(_) | Self::DatabaseLock => "DATABASE_UNAVAILABLE",
            Self::InvalidSetting(_) => "SETTING_INVALID",
            Self::HistoryNotFound => "HISTORY_NOT_FOUND",
            Self::AuthUnconfigured => "AUTHENTICATION_NOT_CONFIGURED",
            Self::AuthNetwork => "NETWORK_UNAVAILABLE",
            Self::AuthInvalidResponse => "AUTHENTICATION_INVALID_RESPONSE",
            Self::AuthExpired => "AUTHENTICATION_EXPIRED",
            Self::AuthPending => "AUTHENTICATION_PENDING",
            Self::SecureStorage => "SECURE_STORAGE_UNAVAILABLE",
        }
    }

    pub const fn user_message(&self) -> &'static str {
        match self {
            Self::Database(_) | Self::DatabaseLock => {
                "Trueflow could not access local product data."
            }
            Self::InvalidSetting(_) => "That setting is not valid.",
            Self::HistoryNotFound => "That history item no longer exists.",
            Self::AuthUnconfigured => "Account sign-in is not configured for this build.",
            Self::AuthNetwork => "Trueflow could not reach the account service.",
            Self::AuthInvalidResponse => "The account service returned an invalid response.",
            Self::AuthExpired => "The sign-in request expired. Start again.",
            Self::AuthPending => "Complete sign-in in your browser.",
            Self::SecureStorage => "Trueflow could not access secure session storage.",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OnboardingState {
    FirstRun,
    InProgress,
    Ready,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OnboardingStep {
    Welcome,
    Microphone,
    Accessibility,
    Account,
    Shortcut,
    FirstDictation,
    Behavior,
    Complete,
}

impl OnboardingState {
    const fn as_str(self) -> &'static str {
        match self {
            Self::FirstRun => "first_run",
            Self::InProgress => "in_progress",
            Self::Ready => "ready",
        }
    }
    fn parse(value: &str) -> Result<Self, ProductError> {
        match value {
            "first_run" => Ok(Self::FirstRun),
            "in_progress" => Ok(Self::InProgress),
            "ready" => Ok(Self::Ready),
            _ => Err(ProductError::InvalidSetting("onboarding state")),
        }
    }
}

impl OnboardingStep {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Welcome => "welcome",
            Self::Microphone => "microphone",
            Self::Accessibility => "accessibility",
            Self::Account => "account",
            Self::Shortcut => "shortcut",
            Self::FirstDictation => "first_dictation",
            Self::Behavior => "behavior",
            Self::Complete => "complete",
        }
    }
    fn parse(value: &str) -> Result<Self, ProductError> {
        match value {
            "welcome" => Ok(Self::Welcome),
            "microphone" => Ok(Self::Microphone),
            "accessibility" => Ok(Self::Accessibility),
            "account" => Ok(Self::Account),
            "shortcut" => Ok(Self::Shortcut),
            "first_dictation" => Ok(Self::FirstDictation),
            "behavior" => Ok(Self::Behavior),
            "complete" => Ok(Self::Complete),
            _ => Err(ProductError::InvalidSetting("onboarding step")),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductSettings {
    pub onboarding_state: OnboardingState,
    pub onboarding_step: OnboardingStep,
    pub history_enabled: bool,
    pub analytics_enabled: bool,
    pub show_overlay: bool,
    pub dictation_shortcut: String,
    pub rewrite_shortcut: String,
    pub dictation_paused: bool,
    pub microphone_device_id: Option<String>,
    pub microphone_device_name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsUpdate {
    pub history_enabled: bool,
    pub analytics_enabled: bool,
    pub show_overlay: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryItem {
    pub id: String,
    pub kind: String,
    pub project_name: Option<String>,
    pub voice_name: Option<String>,
    pub mode_name: Option<String>,
    pub source_app: Option<String>,
    pub final_text: String,
    pub truncated: bool,
    pub status: String,
    pub error_code: Option<String>,
    pub created_at: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryPage {
    pub items: Vec<HistoryItem>,
    pub total: u32,
    pub has_more: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountStatus {
    pub state: &'static str,
    pub configured: bool,
    pub signed_in: bool,
    pub email: Option<String>,
    pub provider: Option<String>,
    pub offline: bool,
    pub cloud_ready: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthStart {
    pub verification_url: String,
    pub user_code: String,
    pub expires_in_seconds: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AuthStartResponse {
    request_id: String,
    verification_url: String,
    user_code: String,
    expires_in_seconds: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AuthPollResponse {
    status: String,
    user_id: Option<String>,
    email: Option<String>,
    session_token: Option<String>,
    provider: Option<String>,
    #[serde(default)]
    transcription_token: Option<String>,
    #[serde(default)]
    transformation_token: Option<String>,
}

struct PendingAuth {
    request_id: String,
    expires_at: Instant,
}

#[async_trait]
trait AuthProvider: Send + Sync {
    fn configured(&self) -> bool;
    async fn begin(&self, installation_id: &str) -> Result<AuthStartResponse, ProductError>;
    async fn poll(&self, request_id: &str) -> Result<AuthPollResponse, ProductError>;
    async fn restore(&self, session_token: &str) -> Result<AuthPollResponse, ProductError>;
}

struct HttpAuthProvider {
    base_url: Option<String>,
    http: Option<Client>,
}

impl HttpAuthProvider {
    fn new(config: &AppConfig) -> Self {
        Self {
            base_url: config.auth_base_url.clone(),
            http: config.auth_base_url.as_ref().map(|_| Client::new()),
        }
    }
    fn parts(&self) -> Result<(&str, &Client), ProductError> {
        let base = self
            .base_url
            .as_deref()
            .ok_or(ProductError::AuthUnconfigured)?
            .trim_end_matches('/');
        if !base.starts_with("https://")
            && !(cfg!(debug_assertions)
                && (base.starts_with("http://localhost") || base.starts_with("http://127.0.0.1")))
        {
            return Err(ProductError::AuthInvalidResponse);
        }
        Ok((
            base,
            self.http.as_ref().ok_or(ProductError::AuthUnconfigured)?,
        ))
    }
}

#[async_trait]
impl AuthProvider for HttpAuthProvider {
    fn configured(&self) -> bool {
        self.parts().is_ok()
    }
    async fn begin(&self, installation_id: &str) -> Result<AuthStartResponse, ProductError> {
        let (base, http) = self.parts()?;
        let response = http
            .post(format!("{base}/desktop-auth/start"))
            .timeout(Duration::from_secs(8))
            .json(&json!({ "installationId": installation_id, "application": "trueflow" }))
            .send()
            .await
            .map_err(|_| ProductError::AuthNetwork)?;
        if !response.status().is_success() {
            return Err(ProductError::AuthNetwork);
        }
        response
            .json()
            .await
            .map_err(|_| ProductError::AuthInvalidResponse)
    }
    async fn poll(&self, request_id: &str) -> Result<AuthPollResponse, ProductError> {
        let (base, http) = self.parts()?;
        let response = http
            .post(format!("{base}/desktop-auth/poll"))
            .timeout(Duration::from_secs(8))
            .json(&json!({ "requestId": request_id }))
            .send()
            .await
            .map_err(|_| ProductError::AuthNetwork)?;
        if !response.status().is_success() {
            return Err(ProductError::AuthNetwork);
        }
        response
            .json()
            .await
            .map_err(|_| ProductError::AuthInvalidResponse)
    }
    async fn restore(&self, session_token: &str) -> Result<AuthPollResponse, ProductError> {
        let (base, http) = self.parts()?;
        let response = http
            .get(format!("{base}/desktop-auth/session"))
            .timeout(Duration::from_secs(6))
            .bearer_auth(session_token)
            .send()
            .await
            .map_err(|_| ProductError::AuthNetwork)?;
        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Ok(AuthPollResponse {
                status: "expired".to_owned(),
                user_id: None,
                email: None,
                session_token: None,
                provider: None,
                transcription_token: None,
                transformation_token: None,
            });
        }
        if !response.status().is_success() {
            return Err(ProductError::AuthNetwork);
        }
        response
            .json()
            .await
            .map_err(|_| ProductError::AuthInvalidResponse)
    }
}

trait SessionStore: Send + Sync {
    fn load(&self) -> Result<Option<String>, ProductError>;
    fn save(&self, token: &str) -> Result<(), ProductError>;
    fn clear(&self) -> Result<(), ProductError>;
}

struct KeychainSessionStore;

impl SessionStore for KeychainSessionStore {
    fn load(&self) -> Result<Option<String>, ProductError> {
        Ok(load_keychain_session().and_then(|value| String::from_utf8(value).ok()))
    }
    fn save(&self, token: &str) -> Result<(), ProductError> {
        store_keychain_session(token)
    }
    fn clear(&self) -> Result<(), ProductError> {
        delete_keychain_session();
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostics {
    pub application_version: String,
    pub os: String,
    pub architecture: String,
    pub environment: String,
    pub schema_version: u32,
    pub transcription_provider: &'static str,
    pub transformation_provider: &'static str,
    pub authentication_configured: bool,
    pub analytics_configured: bool,
    pub crash_reporting_configured: bool,
    pub microphone_permission: String,
    pub accessibility_permission: String,
    pub last_error_category: Option<String>,
}

trait AnalyticsSink: Send + Sync {
    fn capture(
        &self,
        enabled: bool,
        installation_id: String,
        event: &'static str,
        properties: serde_json::Value,
    ) -> Result<(), ()>;
}

#[derive(Clone)]
struct AnalyticsClient {
    http: Option<Client>,
    endpoint: Option<String>,
    key: Option<String>,
}

impl AnalyticsClient {
    fn new(config: &AppConfig) -> Self {
        let needs_http = config.auth_base_url.is_some() || config.analytics_endpoint.is_some();
        Self {
            http: needs_http.then(Client::new),
            endpoint: config.analytics_endpoint.clone(),
            key: config.analytics_key.clone(),
        }
    }
}

impl AnalyticsSink for AnalyticsClient {
    fn capture(
        &self,
        enabled: bool,
        installation_id: String,
        event: &'static str,
        properties: serde_json::Value,
    ) -> Result<(), ()> {
        if !enabled {
            return Ok(());
        }
        let (Some(endpoint), Some(key)) = (self.endpoint.clone(), self.key.clone()) else {
            return Ok(());
        };
        let Some(http) = self.http.clone() else {
            return Ok(());
        };
        tauri::async_runtime::spawn(async move {
            let payload = json!({ "api_key": key, "event": event, "properties": { "distinct_id": installation_id, "source": "desktop", "metadata": properties } });
            let result = http
                .post(endpoint)
                .timeout(std::time::Duration::from_secs(3))
                .json(&payload)
                .send()
                .await;
            if let Err(error) = result {
                warn!(subsystem = "analytics", event = "capture_failed", error_kind = ?error.status(), "product analytics delivery failed");
            }
        });
        Ok(())
    }
}

pub struct ProductService {
    database: Arc<Database>,
    config: AppConfig,
    observed_operations: Mutex<HashSet<String>>,
    analytics: Arc<dyn AnalyticsSink>,
    settings_cache: Mutex<ProductSettings>,
    pending_auth: Mutex<Option<PendingAuth>>,
    auth: Arc<dyn AuthProvider>,
    sessions: Arc<dyn SessionStore>,
    credentials: ProviderCredentials,
}

impl ProductService {
    pub fn new(database: Arc<Database>, config: AppConfig) -> Result<Arc<Self>, ProductError> {
        let auth: Arc<dyn AuthProvider> = Arc::new(HttpAuthProvider::new(&config));
        let analytics: Arc<dyn AnalyticsSink> = Arc::new(AnalyticsClient::new(&config));
        Self::new_with_components(
            database,
            config,
            auth,
            Arc::new(KeychainSessionStore),
            analytics,
        )
    }

    fn new_with_components(
        database: Arc<Database>,
        config: AppConfig,
        auth: Arc<dyn AuthProvider>,
        sessions: Arc<dyn SessionStore>,
        analytics: Arc<dyn AnalyticsSink>,
    ) -> Result<Arc<Self>, ProductError> {
        let settings = load_settings(&database)?;
        let credentials = ProviderCredentials::new(
            config.deepgram_api_key.clone(),
            config.openai_api_key.clone(),
        );
        Ok(Arc::new(Self {
            database,
            analytics,
            config,
            observed_operations: Mutex::new(HashSet::new()),
            settings_cache: Mutex::new(settings),
            pending_auth: Mutex::new(None),
            auth,
            sessions,
            credentials,
        }))
    }

    pub fn provider_credentials(&self) -> ProviderCredentials {
        self.credentials.clone()
    }

    pub fn settings(&self) -> Result<ProductSettings, ProductError> {
        self.settings_cache
            .lock()
            .map(|settings| settings.clone())
            .map_err(|_| ProductError::DatabaseLock)
    }

    pub fn record_app_launched(&self) {
        if let Ok(settings) = self.settings() {
            let _ = self.analytics.capture(
                settings.analytics_enabled,
                self.installation_id().unwrap_or_default(),
                "app_launched",
                json!({ "environment": self.config.environment.to_string() }),
            );
        }
    }

    fn capture_product_event(&self, event: &'static str, properties: serde_json::Value) {
        if let Ok(settings) = self.settings() {
            let _ = self.analytics.capture(
                settings.analytics_enabled,
                self.installation_id().unwrap_or_default(),
                event,
                properties,
            );
        }
    }

    pub fn update_settings(&self, update: SettingsUpdate) -> Result<ProductSettings, ProductError> {
        let connection = self
            .database
            .connection()
            .map_err(|_| ProductError::DatabaseLock)?;
        connection.execute("UPDATE product_settings SET history_enabled=?1, analytics_enabled=?2, show_overlay=?3, updated_at=CURRENT_TIMESTAMP WHERE singleton=1", params![update.history_enabled, update.analytics_enabled, update.show_overlay])?;
        drop(connection);
        let settings = load_settings(&self.database)?;
        *self
            .settings_cache
            .lock()
            .map_err(|_| ProductError::DatabaseLock)? = settings.clone();
        Ok(settings)
    }

    /// Persists the preferred input device. `None` follows the system default.
    /// Callers must confirm the device is currently connected before saving it.
    pub fn set_microphone(
        &self,
        device: Option<(&str, &str)>,
    ) -> Result<ProductSettings, ProductError> {
        if let Some((id, name)) = device
            && !(valid_device_field(id) && valid_device_field(name))
        {
            return Err(ProductError::InvalidSetting("microphone"));
        }
        let (id, name) = device.unzip();
        let connection = self
            .database
            .connection()
            .map_err(|_| ProductError::DatabaseLock)?;
        connection.execute("UPDATE product_settings SET microphone_device_id=?1, microphone_device_name=?2, updated_at=CURRENT_TIMESTAMP WHERE singleton=1", params![id, name])?;
        drop(connection);
        let settings = load_settings(&self.database)?;
        *self
            .settings_cache
            .lock()
            .map_err(|_| ProductError::DatabaseLock)? = settings.clone();
        Ok(settings)
    }

    pub fn toggle_pause(&self) -> Result<ProductSettings, ProductError> {
        let paused = !self.settings()?.dictation_paused;
        let connection = self
            .database
            .connection()
            .map_err(|_| ProductError::DatabaseLock)?;
        connection.execute("UPDATE product_settings SET dictation_paused=?1, updated_at=CURRENT_TIMESTAMP WHERE singleton=1", [paused])?;
        drop(connection);
        let settings = load_settings(&self.database)?;
        *self
            .settings_cache
            .lock()
            .map_err(|_| ProductError::DatabaseLock)? = settings.clone();
        Ok(settings)
    }

    pub fn update_shortcuts(
        &self,
        dictation: &str,
        rewrite: &str,
    ) -> Result<ProductSettings, ProductError> {
        if !crate::hotkey::shortcut_pair_supported(dictation, rewrite) {
            return Err(ProductError::InvalidSetting("shortcut"));
        }
        let connection = self
            .database
            .connection()
            .map_err(|_| ProductError::DatabaseLock)?;
        connection.execute("UPDATE product_settings SET dictation_shortcut=?1,rewrite_shortcut=?2,updated_at=CURRENT_TIMESTAMP WHERE singleton=1", params![dictation, rewrite])?;
        drop(connection);
        let settings = load_settings(&self.database)?;
        *self
            .settings_cache
            .lock()
            .map_err(|_| ProductError::DatabaseLock)? = settings.clone();
        Ok(settings)
    }

    pub fn set_onboarding(
        &self,
        state: OnboardingState,
        step: OnboardingStep,
    ) -> Result<ProductSettings, ProductError> {
        let previous = self.settings()?;
        if state == OnboardingState::Ready && step != OnboardingStep::Complete {
            return Err(ProductError::InvalidSetting("ready onboarding step"));
        }
        let connection = self
            .database
            .connection()
            .map_err(|_| ProductError::DatabaseLock)?;
        connection.execute("UPDATE product_settings SET onboarding_state=?1, onboarding_step=?2, updated_at=CURRENT_TIMESTAMP WHERE singleton=1", params![state.as_str(), step.as_str()])?;
        drop(connection);
        let settings = load_settings(&self.database)?;
        *self
            .settings_cache
            .lock()
            .map_err(|_| ProductError::DatabaseLock)? = settings.clone();
        let event = if state == OnboardingState::Ready {
            Some("onboarding_completed")
        } else if previous.onboarding_state == OnboardingState::FirstRun
            && state == OnboardingState::InProgress
        {
            Some("onboarding_started")
        } else {
            None
        };
        if let Some(event) = event {
            let _ = self.analytics.capture(
                settings.analytics_enabled,
                self.installation_id().unwrap_or_default(),
                event,
                json!({ "step": step.as_str() }),
            );
        }
        Ok(settings)
    }

    pub fn history(&self, offset: u32, limit: u32) -> Result<HistoryPage, ProductError> {
        let limit = limit.clamp(1, HISTORY_PAGE_LIMIT);
        let connection = self
            .database
            .connection()
            .map_err(|_| ProductError::DatabaseLock)?;
        let total = connection.query_row("SELECT COUNT(*) FROM dictation_history", [], |row| {
            row.get::<_, u32>(0)
        })?;
        let mut statement = connection.prepare("SELECT id, kind, project_name, voice_name, mode_name, source_app, substr(final_text,1,?1), length(final_text)>?1, status, error_code, created_at FROM dictation_history ORDER BY created_at DESC, id DESC LIMIT ?2 OFFSET ?3")?;
        let rows =
            statement.query_map(params![HISTORY_PREVIEW_CHARACTERS, limit, offset], |row| {
                Ok(HistoryItem {
                    id: row.get(0)?,
                    kind: row.get(1)?,
                    project_name: row.get(2)?,
                    voice_name: row.get(3)?,
                    mode_name: row.get(4)?,
                    source_app: row.get(5)?,
                    final_text: row.get(6)?,
                    truncated: row.get(7)?,
                    status: row.get(8)?,
                    error_code: row.get(9)?,
                    created_at: row.get(10)?,
                })
            })?;
        let items = rows.collect::<Result<Vec<_>, _>>()?;
        Ok(HistoryPage {
            has_more: offset.saturating_add(items.len() as u32) < total,
            items,
            total,
        })
    }

    pub fn history_text(&self, id: &str) -> Result<String, ProductError> {
        let connection = self
            .database
            .connection()
            .map_err(|_| ProductError::DatabaseLock)?;
        connection
            .query_row(
                "SELECT final_text FROM dictation_history WHERE id=?1",
                [id],
                |row| row.get(0),
            )
            .optional()?
            .ok_or(ProductError::HistoryNotFound)
    }

    pub fn delete_history(&self, id: &str) -> Result<(), ProductError> {
        let connection = self
            .database
            .connection()
            .map_err(|_| ProductError::DatabaseLock)?;
        if connection.execute("DELETE FROM dictation_history WHERE id=?1", [id])? == 0 {
            return Err(ProductError::HistoryNotFound);
        }
        Ok(())
    }

    pub fn clear_history(&self) -> Result<u32, ProductError> {
        let connection = self
            .database
            .connection()
            .map_err(|_| ProductError::DatabaseLock)?;
        Ok(
            u32::try_from(connection.execute("DELETE FROM dictation_history", [])?)
                .unwrap_or(u32::MAX),
        )
    }

    pub fn account_status(&self) -> Result<AccountStatus, ProductError> {
        let connection = self
            .database
            .connection()
            .map_err(|_| ProductError::DatabaseLock)?;
        let row = connection
            .query_row(
                "SELECT provider, email FROM account_metadata WHERE singleton=1",
                [],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?;
        let signed_in = row.is_some() && self.sessions.load()?.is_some();
        Ok(AccountStatus {
            state: if signed_in {
                "signedIn"
            } else if row.is_some() {
                "sessionExpired"
            } else {
                "signedOut"
            },
            configured: self.auth.configured(),
            signed_in,
            email: row.as_ref().map(|value| value.1.clone()),
            provider: row.map(|value| value.0),
            offline: false,
            cloud_ready: self.credentials.cloud_ready(),
        })
    }

    pub async fn begin_auth(&self) -> Result<AuthStart, ProductError> {
        let installation_id = self.installation_id()?;
        let payload = match self.auth.begin(&installation_id).await {
            Ok(payload) => payload,
            Err(error) => {
                self.capture_product_event("sign_in_failed", json!({ "error_code": error.code() }));
                return Err(error);
            }
        };
        if payload.request_id.is_empty()
            || payload.user_code.is_empty()
            || !payload.verification_url.starts_with("https://")
            || payload.expires_in_seconds == 0
        {
            return Err(ProductError::AuthInvalidResponse);
        }
        *self
            .pending_auth
            .lock()
            .map_err(|_| ProductError::SecureStorage)? = Some(PendingAuth {
            request_id: payload.request_id,
            expires_at: Instant::now() + Duration::from_secs(payload.expires_in_seconds),
        });
        self.capture_product_event("sign_in_started", json!({}));
        Ok(AuthStart {
            verification_url: payload.verification_url,
            user_code: payload.user_code,
            expires_in_seconds: payload.expires_in_seconds,
        })
    }

    pub async fn poll_auth(&self) -> Result<AccountStatus, ProductError> {
        let request_id = {
            let pending = self
                .pending_auth
                .lock()
                .map_err(|_| ProductError::SecureStorage)?;
            let pending = pending.as_ref().ok_or(ProductError::AuthExpired)?;
            if Instant::now() >= pending.expires_at {
                return Err(ProductError::AuthExpired);
            }
            pending.request_id.clone()
        };
        let payload = match self.auth.poll(&request_id).await {
            Ok(payload) => payload,
            Err(ProductError::AuthPending) => return Err(ProductError::AuthPending),
            Err(error) => {
                self.capture_product_event("sign_in_failed", json!({ "error_code": error.code() }));
                return Err(error);
            }
        };
        match payload.status.as_str() {
            "pending" => Err(ProductError::AuthPending),
            "expired" => Err(ProductError::AuthExpired),
            "complete" => {
                let user_id = payload
                    .user_id
                    .filter(|value| !value.is_empty())
                    .ok_or(ProductError::AuthInvalidResponse)?;
                let email = payload
                    .email
                    .filter(|value| value.contains('@'))
                    .ok_or(ProductError::AuthInvalidResponse)?;
                let token = payload
                    .session_token
                    .filter(|value| !value.is_empty())
                    .ok_or(ProductError::AuthInvalidResponse)?;
                self.sessions.save(&token)?;
                let provider = payload.provider.unwrap_or_else(|| "managed".to_owned());
                self.credentials
                    .apply_session(payload.transcription_token, payload.transformation_token);
                let connection = self
                    .database
                    .connection()
                    .map_err(|_| ProductError::DatabaseLock)?;
                connection.execute("INSERT INTO account_metadata (singleton,provider,user_id,email) VALUES (1,?1,?2,?3) ON CONFLICT(singleton) DO UPDATE SET provider=excluded.provider,user_id=excluded.user_id,email=excluded.email,updated_at=CURRENT_TIMESTAMP", params![provider, user_id, email])?;
                drop(connection);
                *self
                    .pending_auth
                    .lock()
                    .map_err(|_| ProductError::SecureStorage)? = None;
                info!(
                    subsystem = "authentication",
                    event = "signed_in",
                    provider,
                    "desktop account session established"
                );
                self.capture_product_event("sign_in_completed", json!({ "provider": provider }));
                self.account_status()
            }
            _ => Err(ProductError::AuthInvalidResponse),
        }
    }

    pub async fn refresh_account(&self) -> Result<AccountStatus, ProductError> {
        if !self.auth.configured() {
            return self.account_status();
        }
        let Some(token) = self.sessions.load()? else {
            return self.account_status();
        };
        let payload = match self.auth.restore(&token).await {
            Ok(value) => value,
            Err(ProductError::AuthNetwork) => {
                let mut status = self.account_status()?;
                status.offline = true;
                status.state = "offline";
                return Ok(status);
            }
            Err(error) => return Err(error),
        };
        if payload.status == "expired" {
            self.sign_out_local()?;
            return Ok(AccountStatus {
                state: "sessionExpired",
                configured: true,
                signed_in: false,
                email: None,
                provider: None,
                offline: false,
                cloud_ready: self.credentials.cloud_ready(),
            });
        }
        let user_id = payload.user_id.ok_or(ProductError::AuthInvalidResponse)?;
        let email = payload.email.ok_or(ProductError::AuthInvalidResponse)?;
        let provider = payload.provider.unwrap_or_else(|| "managed".to_owned());
        self.credentials
            .apply_session(payload.transcription_token, payload.transformation_token);
        let connection = self
            .database
            .connection()
            .map_err(|_| ProductError::DatabaseLock)?;
        connection.execute("INSERT INTO account_metadata (singleton,provider,user_id,email) VALUES (1,?1,?2,?3) ON CONFLICT(singleton) DO UPDATE SET provider=excluded.provider,user_id=excluded.user_id,email=excluded.email,updated_at=CURRENT_TIMESTAMP", params![provider, user_id, email])?;
        drop(connection);
        self.account_status()
    }

    pub fn sign_out_local(&self) -> Result<(), ProductError> {
        self.sessions.clear()?;
        self.credentials.clear_session();
        let connection = self
            .database
            .connection()
            .map_err(|_| ProductError::DatabaseLock)?;
        connection.execute("DELETE FROM account_metadata WHERE singleton=1", [])?;
        drop(connection);
        self.capture_product_event("sign_out", json!({}));
        Ok(())
    }

    pub fn diagnostics(
        &self,
        microphone_permission: String,
        accessibility_permission: String,
        last_error_category: Option<String>,
    ) -> Result<Diagnostics, ProductError> {
        Ok(Diagnostics {
            application_version: env!("CARGO_PKG_VERSION").to_owned(),
            os: std::env::consts::OS.to_owned(),
            architecture: std::env::consts::ARCH.to_owned(),
            environment: self.config.environment.to_string(),
            schema_version: self
                .database
                .schema_version()
                .map_err(|_| ProductError::DatabaseLock)?,
            transcription_provider: "deepgram",
            transformation_provider: "openai",
            authentication_configured: self.config.auth_base_url.is_some(),
            analytics_configured: self.config.analytics_endpoint.is_some()
                && self.config.analytics_key.is_some(),
            crash_reporting_configured: self.config.crash_report_endpoint.is_some(),
            microphone_permission,
            accessibility_permission,
            last_error_category,
        })
    }

    pub fn report_safe_error(&self, category: &str, feature: &str) {
        let payload = safe_report_payload(category, feature);
        let category = payload["category"].as_str().unwrap_or("invalid");
        let feature = payload["feature"].as_str().unwrap_or("invalid");
        warn!(
            subsystem = "diagnostics",
            event = "client_error",
            category,
            feature,
            "privacy-safe client error reported"
        );
        let Some(endpoint) = self.config.crash_report_endpoint.clone() else {
            return;
        };
        let http = Client::new();
        tauri::async_runtime::spawn(async move {
            let _ = http
                .post(endpoint)
                .timeout(std::time::Duration::from_secs(3))
                .json(&payload)
                .send()
                .await;
        });
    }

    pub fn observe_dictation(&self, snapshot: &DictationSnapshot) {
        let Some(operation_id) = snapshot.operation_id.as_ref() else {
            return;
        };
        if !matches!(
            snapshot.state,
            DictationState::Completed
                | DictationState::RecoverableError
                | DictationState::FatalError
        ) {
            return;
        }
        let Ok(mut observed) = self.observed_operations.lock() else {
            return;
        };
        if !observed.insert(operation_id.clone()) {
            return;
        }
        drop(observed);

        let settings = match self.settings() {
            Ok(value) => value,
            Err(error) => {
                warn!(
                    subsystem = "history",
                    event = "settings_unavailable",
                    error_code = error.code(),
                    "history preference unavailable"
                );
                return;
            }
        };
        let text = snapshot
            .final_transcript
            .as_deref()
            .filter(|value| !value.trim().is_empty());
        if settings.history_enabled {
            if let Some(text) = text {
                if let Err(error) = self.insert_history(snapshot, text) {
                    warn!(
                        operation_id,
                        subsystem = "history",
                        event = "write_failed",
                        error_code = error.code(),
                        "completed operation could not be added to history"
                    );
                }
            }
        }
        let event = match (snapshot.operation_kind, snapshot.state) {
            (Some(OperationKind::SelectionRewrite), DictationState::Completed) => {
                "rewrite_completed"
            }
            (Some(OperationKind::SelectionRewrite), _) => "rewrite_failed",
            (_, DictationState::Completed) => "dictation_completed",
            _ => "dictation_failed",
        };
        let installation_id = self.installation_id().unwrap_or_default();
        let _ = self.analytics.capture(settings.analytics_enabled, installation_id, event, json!({ "status": history_status(snapshot), "error_code": snapshot.error.as_ref().map(|error| error.code) }));

        if snapshot.operation_kind == Some(OperationKind::Dictation)
            && snapshot.state == DictationState::Completed
            && settings.onboarding_step == OnboardingStep::FirstDictation
        {
            let _ = self.set_onboarding(OnboardingState::InProgress, OnboardingStep::Behavior);
        }
    }

    fn insert_history(&self, snapshot: &DictationSnapshot, text: &str) -> Result<(), ProductError> {
        let connection = self
            .database
            .connection()
            .map_err(|_| ProductError::DatabaseLock)?;
        let kind = if snapshot.operation_kind == Some(OperationKind::SelectionRewrite) {
            "selection_rewrite"
        } else {
            "dictation"
        };
        let source_app = snapshot
            .insertion
            .as_ref()
            .and_then(|value| value.target_application.clone())
            .or_else(|| {
                snapshot
                    .rewrite
                    .as_ref()
                    .and_then(|value| value.target_application.clone())
            });
        let transaction = connection.unchecked_transaction()?;
        transaction.execute("INSERT OR IGNORE INTO dictation_history (id, operation_id, kind, project_id, project_name, voice_id, voice_name, mode_id, mode_name, source_app, final_text, status, error_code) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)", params![Uuid::new_v4().to_string(), snapshot.operation_id, kind, snapshot.project_id, snapshot.project_name, snapshot.voice_id, snapshot.voice_name, snapshot.mode_id, snapshot.mode_name, source_app, text, history_status(snapshot), snapshot.error.as_ref().map(|error| error.code)])?;
        transaction.execute(
            "DELETE FROM dictation_history WHERE id IN (SELECT id FROM dictation_history ORDER BY created_at DESC, id DESC LIMIT -1 OFFSET ?1)",
            [HISTORY_RETENTION_LIMIT],
        )?;
        transaction.commit()?;
        Ok(())
    }

    fn installation_id(&self) -> Result<String, ProductError> {
        let connection = self
            .database
            .connection()
            .map_err(|_| ProductError::DatabaseLock)?;
        Ok(connection.query_row(
            "SELECT installation_id FROM product_settings WHERE singleton=1",
            [],
            |row| row.get(0),
        )?)
    }
}

#[cfg(target_os = "macos")]
fn store_keychain_session(token: &str) -> Result<(), ProductError> {
    security_framework::passwords::set_generic_password(
        "com.trueflow.desktop",
        "account-session",
        token.as_bytes(),
    )
    .map_err(|_| ProductError::SecureStorage)
}

#[cfg(not(target_os = "macos"))]
fn store_keychain_session(_token: &str) -> Result<(), ProductError> {
    Err(ProductError::SecureStorage)
}

#[cfg(target_os = "macos")]
fn load_keychain_session() -> Option<Vec<u8>> {
    security_framework::passwords::generic_password(
        security_framework::passwords::PasswordOptions::new_generic_password(
            "com.trueflow.desktop",
            "account-session",
        ),
    )
    .ok()
}

#[cfg(not(target_os = "macos"))]
fn load_keychain_session() -> Option<Vec<u8>> {
    None
}

#[cfg(not(target_os = "macos"))]
fn keychain_session_exists() -> bool {
    false
}

#[cfg(target_os = "macos")]
fn delete_keychain_session() {
    let _ = security_framework::passwords::delete_generic_password(
        "com.trueflow.desktop",
        "account-session",
    );
}

#[cfg(not(target_os = "macos"))]
fn delete_keychain_session() {}

fn history_status(snapshot: &DictationSnapshot) -> &'static str {
    if snapshot.state == DictationState::Completed {
        "completed"
    } else if snapshot.insertion.as_ref().is_some_and(|value| {
        matches!(
            value.kind,
            crate::insertion::InsertionOutcomeKind::CopiedToClipboard
        )
    }) {
        "copied"
    } else if snapshot.final_transcript.is_some() {
        "recovered"
    } else {
        "failed"
    }
}

fn safe_token(value: &str) -> String {
    if !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
    {
        value.to_owned()
    } else {
        "invalid".to_owned()
    }
}

fn safe_report_payload(category: &str, feature: &str) -> serde_json::Value {
    json!({
        "category": safe_token(category),
        "feature": safe_token(feature),
        "platform": "macos"
    })
}

fn load_settings(database: &Database) -> Result<ProductSettings, ProductError> {
    let connection = database
        .connection()
        .map_err(|_| ProductError::DatabaseLock)?;
    connection.query_row("SELECT onboarding_state, onboarding_step, history_enabled, analytics_enabled, show_overlay, dictation_shortcut, rewrite_shortcut, dictation_paused, microphone_device_id, microphone_device_name FROM product_settings WHERE singleton=1", [], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, bool>(2)?, row.get::<_, bool>(3)?, row.get::<_, bool>(4)?, row.get(5)?, row.get(6)?, row.get::<_, bool>(7)?, row.get(8)?, row.get(9)?))
    }).map_err(ProductError::from).and_then(|row| Ok(ProductSettings { onboarding_state: OnboardingState::parse(&row.0)?, onboarding_step: OnboardingStep::parse(&row.1)?, history_enabled: row.2, analytics_enabled: row.3, show_overlay: row.4, dictation_shortcut: row.5, rewrite_shortcut: row.6, dictation_paused: row.7, microphone_device_id: row.8, microphone_device_name: row.9 }))
}

pub fn install_crash_reporting(config: &AppConfig) {
    let endpoint = config.crash_report_endpoint.clone();
    let _ = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |_panic| {
        tracing::error!(
            subsystem = "crash_reporting",
            event = "rust_panic",
            error_code = "RUST_PANIC",
            "a Rust panic was captured without content"
        );
        if let Some(endpoint) = endpoint.clone() {
            tauri::async_runtime::spawn(async move {
                let _ = Client::new()
                    .post(endpoint)
                    .timeout(Duration::from_secs(3))
                    .json(&json!({ "category": "rust_panic", "platform": "macos" }))
                    .send()
                    .await;
            });
        }
    }));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use tempfile::tempdir;

    fn service() -> Arc<ProductService> {
        let directory = tempdir().expect("temporary directory").keep();
        let database =
            Arc::new(Database::open(&directory.join("product.sqlite3")).expect("database"));
        let config = AppConfig::for_tests();
        ProductService::new(database, config).expect("product service")
    }

    struct MockAuth {
        polls: Mutex<VecDeque<Result<AuthPollResponse, ProductError>>>,
        restores: Mutex<VecDeque<Result<AuthPollResponse, ProductError>>>,
    }
    #[async_trait]
    impl AuthProvider for MockAuth {
        fn configured(&self) -> bool {
            true
        }
        async fn begin(&self, _installation_id: &str) -> Result<AuthStartResponse, ProductError> {
            Ok(AuthStartResponse {
                request_id: "request-one".to_owned(),
                verification_url: "https://accounts.example.test/device".to_owned(),
                user_code: "TRUE-FLOW".to_owned(),
                expires_in_seconds: 300,
            })
        }
        async fn poll(&self, _request_id: &str) -> Result<AuthPollResponse, ProductError> {
            self.polls
                .lock()
                .expect("poll queue")
                .pop_front()
                .unwrap_or(Err(ProductError::AuthPending))
        }
        async fn restore(&self, _session_token: &str) -> Result<AuthPollResponse, ProductError> {
            self.restores
                .lock()
                .expect("restore queue")
                .pop_front()
                .unwrap_or(Err(ProductError::AuthNetwork))
        }
    }
    #[derive(Default)]
    struct MemorySessions(Mutex<Option<String>>);
    impl SessionStore for MemorySessions {
        fn load(&self) -> Result<Option<String>, ProductError> {
            Ok(self.0.lock().expect("session").clone())
        }
        fn save(&self, token: &str) -> Result<(), ProductError> {
            *self.0.lock().expect("session") = Some(token.to_owned());
            Ok(())
        }
        fn clear(&self) -> Result<(), ProductError> {
            *self.0.lock().expect("session") = None;
            Ok(())
        }
    }
    #[derive(Default)]
    struct RecordingAnalytics {
        events: Mutex<Vec<(&'static str, serde_json::Value)>>,
        fail: bool,
    }
    impl AnalyticsSink for RecordingAnalytics {
        fn capture(
            &self,
            enabled: bool,
            _installation_id: String,
            event: &'static str,
            properties: serde_json::Value,
        ) -> Result<(), ()> {
            if enabled {
                self.events
                    .lock()
                    .expect("analytics events")
                    .push((event, properties));
            }
            if self.fail { Err(()) } else { Ok(()) }
        }
    }

    fn service_with_auth(
        polls: Vec<Result<AuthPollResponse, ProductError>>,
        restores: Vec<Result<AuthPollResponse, ProductError>>,
        sessions: Arc<MemorySessions>,
    ) -> Arc<ProductService> {
        let directory = tempdir().expect("temporary directory").keep();
        let database = Arc::new(Database::open(&directory.join("auth.sqlite3")).expect("database"));
        ProductService::new_with_components(
            database,
            AppConfig::for_tests(),
            Arc::new(MockAuth {
                polls: Mutex::new(polls.into()),
                restores: Mutex::new(restores.into()),
            }),
            sessions,
            Arc::new(AnalyticsClient {
                http: None,
                endpoint: None,
                key: None,
            }),
        )
        .expect("product service")
    }

    fn service_with_analytics(analytics: Arc<RecordingAnalytics>) -> Arc<ProductService> {
        let directory = tempdir().expect("temporary directory").keep();
        let database =
            Arc::new(Database::open(&directory.join("analytics.sqlite3")).expect("database"));
        ProductService::new_with_components(
            database,
            AppConfig::for_tests(),
            Arc::new(HttpAuthProvider {
                base_url: None,
                http: None,
            }),
            Arc::new(MemorySessions::default()),
            analytics,
        )
        .expect("product service")
    }

    #[test]
    fn fresh_state_and_partial_onboarding_persist() {
        let service = service();
        assert_eq!(
            service.settings().expect("settings").onboarding_state,
            OnboardingState::FirstRun
        );
        service
            .set_onboarding(OnboardingState::InProgress, OnboardingStep::Accessibility)
            .expect("update");
        assert_eq!(
            service.settings().expect("settings").onboarding_step,
            OnboardingStep::Accessibility
        );
    }

    #[test]
    fn settings_reject_unsupported_shortcuts_and_onboarding_reentry_preserves_data() {
        let service = service();
        assert!(matches!(
            service.update_shortcuts("Option+AnyKey", "Option+Shift+Space"),
            Err(ProductError::InvalidSetting(_))
        ));
        let before = service
            .database
            .connection()
            .expect("connection")
            .query_row("SELECT COUNT(*) FROM projects", [], |row| {
                row.get::<_, u32>(0)
            })
            .expect("projects");
        service
            .set_onboarding(OnboardingState::Ready, OnboardingStep::Complete)
            .expect("ready");
        service
            .set_onboarding(OnboardingState::InProgress, OnboardingStep::Welcome)
            .expect("reenter");
        let after = service
            .database
            .connection()
            .expect("connection")
            .query_row("SELECT COUNT(*) FROM projects", [], |row| {
                row.get::<_, u32>(0)
            })
            .expect("projects");
        assert_eq!(before, after);
    }

    #[test]
    fn history_paginates_and_deletes_independently() {
        let service = service();
        let connection = service.database.connection().expect("connection");
        for index in 0..3 {
            connection.execute("INSERT INTO dictation_history (id,operation_id,kind,final_text,status) VALUES (?1,?2,'dictation',?3,'completed')", params![format!("id-{index}"), format!("op-{index}"), format!("text {index}")]).expect("insert");
        }
        drop(connection);
        let page = service.history(0, 2).expect("page");
        assert_eq!(page.items.len(), 2);
        assert!(page.has_more);
        service.delete_history(&page.items[0].id).expect("delete");
        assert_eq!(service.history(0, 50).expect("remaining").total, 2);
        assert_eq!(
            service
                .database
                .connection()
                .expect("database")
                .query_row("SELECT COUNT(*) FROM projects", [], |row| row
                    .get::<_, u32>(0))
                .expect("projects"),
            1
        );
    }

    #[test]
    fn history_round_trips_large_unicode_content() {
        let service = service();
        let text = "🖋️ café اردو\n".repeat(8_000);
        service.database.connection().expect("connection").execute("INSERT INTO dictation_history (id,operation_id,kind,final_text,status) VALUES ('large','large-op','dictation',?1,'completed')", [&text]).expect("insert large history");
        assert_eq!(service.history_text("large").expect("history text"), text);
        let preview = service.history(0, 1).expect("preview").items.remove(0);
        assert!(preview.truncated);
        assert_eq!(preview.final_text.chars().count(), 280);
    }

    #[test]
    fn history_retention_is_bounded_and_clear_preserves_other_local_state() {
        let service = service();
        let connection = service.database.connection().expect("connection");
        for index in 0..=HISTORY_RETENTION_LIMIT {
            connection.execute("INSERT INTO dictation_history (id,operation_id,kind,final_text,status) VALUES (?1,?2,'dictation','text','completed')", params![format!("id-{index:04}"), format!("op-{index:04}")]).expect("history seed");
        }
        connection.execute("INSERT INTO account_metadata (singleton,provider,user_id,email) VALUES (1,'managed','user','writer@example.test')", []).expect("account metadata");
        let tables = [
            "projects",
            "vocabulary_entries",
            "voices",
            "modes",
            "product_settings",
            "account_metadata",
        ];
        let before = tables.map(|table| {
            connection
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                    row.get::<_, u32>(0)
                })
                .expect("initial table count")
        });
        drop(connection);
        let snapshot = completed_snapshot("retention-trigger", "newest");
        service
            .insert_history(&snapshot, "newest")
            .expect("bounded insert");
        assert_eq!(
            service.history(0, 1).expect("history").total,
            HISTORY_RETENTION_LIMIT
        );
        assert_eq!(
            service.clear_history().expect("clear"),
            HISTORY_RETENTION_LIMIT
        );
        let connection = service.database.connection().expect("connection");
        for (table, expected) in tables.into_iter().zip(before) {
            let count: u32 = connection
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                    row.get(0)
                })
                .expect("preserved table");
            assert_eq!(count, expected, "{table} should be preserved");
        }
    }

    #[test]
    fn disabling_history_survives_restart_and_clear_is_immediate() {
        let directory = tempdir().expect("temporary directory");
        let path = directory.path().join("restart.sqlite3");
        let service = ProductService::new(
            Arc::new(Database::open(&path).expect("database")),
            AppConfig::for_tests(),
        )
        .expect("product service");
        service
            .update_settings(SettingsUpdate {
                history_enabled: false,
                analytics_enabled: false,
                show_overlay: true,
            })
            .expect("update");
        assert!(!service.settings().expect("settings").history_enabled);
        assert_eq!(service.clear_history().expect("clear"), 0);
        drop(service);
        let restarted = ProductService::new(
            Arc::new(Database::open(&path).expect("reopened database")),
            AppConfig::for_tests(),
        )
        .expect("restarted product service");
        assert!(!restarted.settings().expect("settings").history_enabled);
    }

    #[test]
    fn microphone_preference_persists_across_restart_and_clears_to_default() {
        let directory = tempdir().expect("temporary directory");
        let path = directory.path().join("microphone.sqlite3");
        let open = || {
            ProductService::new(
                Arc::new(Database::open(&path).expect("database")),
                AppConfig::for_tests(),
            )
            .expect("product service")
        };
        let service = open();
        assert_eq!(
            service.settings().expect("settings").microphone_device_id,
            None
        );
        let saved = service
            .set_microphone(Some(("coreaudio:USB-Mic-UID", "Studio USB Mic")))
            .expect("select microphone");
        assert_eq!(
            saved.microphone_device_id.as_deref(),
            Some("coreaudio:USB-Mic-UID")
        );
        assert_eq!(
            saved.microphone_device_name.as_deref(),
            Some("Studio USB Mic")
        );
        drop(service);

        let restarted = open();
        let settings = restarted.settings().expect("settings");
        assert_eq!(
            settings.microphone_device_id.as_deref(),
            Some("coreaudio:USB-Mic-UID")
        );
        assert_eq!(
            settings.microphone_device_name.as_deref(),
            Some("Studio USB Mic")
        );

        let cleared = restarted.set_microphone(None).expect("system default");
        assert_eq!(cleared.microphone_device_id, None);
        assert_eq!(cleared.microphone_device_name, None);
    }

    #[test]
    fn invalid_microphone_preference_is_rejected_without_changing_settings() {
        let service = service();
        service
            .set_microphone(Some(("coreaudio:built-in", "MacBook Microphone")))
            .expect("valid microphone");
        for (id, name) in [
            ("", "Name"),
            ("coreaudio:x", " "),
            ("coreaudio:\nx", "Name"),
        ] {
            assert!(matches!(
                service.set_microphone(Some((id, name))),
                Err(ProductError::InvalidSetting("microphone"))
            ));
        }
        assert_eq!(
            service
                .settings()
                .expect("settings")
                .microphone_device_id
                .as_deref(),
            Some("coreaudio:built-in")
        );
    }

    #[test]
    fn diagnostics_never_contains_configured_secrets() {
        let service = service();
        let serialized = serde_json::to_string(
            &service
                .diagnostics("granted".to_owned(), "granted".to_owned(), None)
                .expect("diagnostics"),
        )
        .expect("serialize");
        assert!(!serialized.contains("api_key"));
        assert!(!serialized.contains("token"));
    }

    #[test]
    fn completed_operation_is_recorded_once_and_advances_real_dictation_step() {
        let analytics = Arc::new(RecordingAnalytics {
            events: Mutex::new(Vec::new()),
            fail: true,
        });
        let service = service_with_analytics(analytics.clone());
        service
            .update_settings(SettingsUpdate {
                history_enabled: true,
                analytics_enabled: true,
                show_overlay: true,
            })
            .expect("analytics preference");
        service
            .set_onboarding(OnboardingState::InProgress, OnboardingStep::FirstDictation)
            .expect("onboarding");
        analytics.events.lock().expect("events").clear();
        let snapshot = DictationSnapshot {
            state: DictationState::Completed,
            operation_id: Some("operation-one".to_owned()),
            operation_kind: Some(OperationKind::Dictation),
            shortcut: crate::dictation::DEFAULT_SHORTCUT_LABEL,
            rewrite_shortcut: crate::dictation::REWRITE_SHORTCUT_LABEL,
            rewrite_stage: None,
            microphone_permission: crate::audio::MicrophonePermissionState::Granted,
            accessibility_permission: crate::insertion::AccessibilityPermissionState::Granted,
            error: None,
            partial_transcript: None,
            final_transcript: Some("Unicode survives: café, اردو, 🖋️".to_owned()),
            transcript_finality: crate::dictation::TranscriptFinality::Final,
            degraded: false,
            insertion: None,
            rewrite: None,
            project_id: None,
            project_name: Some("General".to_owned()),
            voice_id: None,
            voice_name: Some("My Voice".to_owned()),
            mode_id: None,
            mode_name: Some("Clean Dictation".to_owned()),
            transformation: None,
        };
        service.observe_dictation(&snapshot);
        service.observe_dictation(&snapshot);
        let history = service.history(0, 50).expect("history");
        assert_eq!(history.total, 1);
        assert_eq!(
            history.items[0].final_text,
            "Unicode survives: café, اردو, 🖋️"
        );
        assert_eq!(
            service.settings().expect("settings").onboarding_step,
            OnboardingStep::Behavior
        );
        let events = analytics.events.lock().expect("events");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].0, "dictation_completed");
        let payload = events[0].1.to_string();
        assert!(!payload.contains("Unicode"));
        assert!(!payload.contains("café"));
    }

    #[test]
    fn history_write_failure_does_not_change_completion_or_analytics() {
        let analytics = Arc::new(RecordingAnalytics::default());
        let service = service_with_analytics(analytics.clone());
        service
            .update_settings(SettingsUpdate {
                history_enabled: true,
                analytics_enabled: true,
                show_overlay: true,
            })
            .expect("preferences");
        service
            .set_onboarding(OnboardingState::InProgress, OnboardingStep::FirstDictation)
            .expect("onboarding");
        analytics.events.lock().expect("events").clear();
        service
            .database
            .connection()
            .expect("connection")
            .execute("DROP TABLE dictation_history", [])
            .expect("simulate history failure");
        service.observe_dictation(&completed_snapshot("history-failure", "still inserted"));
        assert_eq!(
            service.settings().expect("settings").onboarding_step,
            OnboardingStep::Behavior
        );
        assert_eq!(
            analytics.events.lock().expect("events")[0].0,
            "dictation_completed"
        );
    }

    #[test]
    fn analytics_opt_out_emits_nothing() {
        let analytics = Arc::new(RecordingAnalytics::default());
        let service = service_with_analytics(analytics.clone());
        let _ = service
            .set_onboarding(OnboardingState::InProgress, OnboardingStep::Microphone)
            .expect("onboarding");
        assert!(analytics.events.lock().expect("events").is_empty());
    }

    #[test]
    fn diagnostic_tokens_are_allowlisted_and_content_is_rejected() {
        assert_eq!(safe_token("react_render_error"), "react_render_error");
        assert_eq!(
            safe_token("the user dictated a private sentence"),
            "invalid"
        );
        assert_eq!(safe_token("secret/token"), "invalid");
        let payload =
            safe_report_payload("the user dictated a private sentence", "secret/token").to_string();
        assert_eq!(
            payload,
            r#"{"category":"invalid","feature":"invalid","platform":"macos"}"#
        );
    }

    #[tokio::test]
    async fn auth_success_pending_retry_restore_expiry_and_signout_preserve_local_data() {
        let sessions = Arc::new(MemorySessions::default());
        let service = service_with_auth(
            vec![
                Err(ProductError::AuthPending),
                Ok(AuthPollResponse {
                    status: "complete".to_owned(),
                    user_id: Some("user-1".to_owned()),
                    email: Some("writer@example.test".to_owned()),
                    session_token: Some("secret-session".to_owned()),
                    provider: Some("managed".to_owned()),
                    transcription_token: Some("session-stt".to_owned()),
                    transformation_token: Some("session-ai".to_owned()),
                }),
            ],
            vec![
                Ok(AuthPollResponse {
                    status: "complete".to_owned(),
                    user_id: Some("user-1".to_owned()),
                    email: Some("writer@example.test".to_owned()),
                    session_token: None,
                    provider: Some("managed".to_owned()),
                    transcription_token: Some("restored-stt".to_owned()),
                    transformation_token: Some("restored-ai".to_owned()),
                }),
                Ok(AuthPollResponse {
                    status: "expired".to_owned(),
                    user_id: None,
                    email: None,
                    session_token: None,
                    provider: None,
                    transcription_token: None,
                    transformation_token: None,
                }),
            ],
            sessions.clone(),
        );
        assert!(!service.account_status().expect("signed out").signed_in);
        assert_eq!(
            service.begin_auth().await.expect("begin").user_code,
            "TRUE-FLOW"
        );
        assert!(matches!(
            service.poll_auth().await,
            Err(ProductError::AuthPending)
        ));
        assert!(service.poll_auth().await.expect("completed").signed_in);
        assert!(service.provider_credentials().cloud_ready());
        assert_eq!(
            sessions.load().expect("session").as_deref(),
            Some("secret-session")
        );
        assert!(service.refresh_account().await.expect("restored").signed_in);
        assert!(!service.refresh_account().await.expect("expired").signed_in);
        assert!(
            service
                .database
                .connection()
                .expect("database")
                .query_row("SELECT COUNT(*) FROM projects", [], |row| row
                    .get::<_, u32>(0))
                .expect("projects")
                > 0
        );
        service.sign_out_local().expect("idempotent sign out");
        assert!(sessions.load().expect("session").is_none());
        assert!(!service.provider_credentials().cloud_ready());
    }

    #[tokio::test]
    async fn auth_network_failure_is_retryable_without_local_data_loss() {
        let sessions = Arc::new(MemorySessions::default());
        let service = service_with_auth(vec![Err(ProductError::AuthNetwork)], vec![], sessions);
        service.begin_auth().await.expect("begin");
        assert!(matches!(
            service.poll_auth().await,
            Err(ProductError::AuthNetwork)
        ));
        assert_eq!(
            service
                .database
                .connection()
                .expect("database")
                .query_row("SELECT name FROM projects LIMIT 1", [], |row| row
                    .get::<_, String>(0))
                .expect("project"),
            "General"
        );
    }

    #[tokio::test]
    async fn invalid_auth_completion_never_creates_a_session() {
        let sessions = Arc::new(MemorySessions::default());
        let service = service_with_auth(
            vec![Ok(AuthPollResponse {
                status: "complete".to_owned(),
                user_id: Some("user".to_owned()),
                email: Some("not-an-email".to_owned()),
                session_token: Some("token".to_owned()),
                provider: Some("managed".to_owned()),
                transcription_token: Some("stt".to_owned()),
                transformation_token: Some("ai".to_owned()),
            })],
            vec![],
            sessions.clone(),
        );
        service.begin_auth().await.expect("begin");
        assert!(matches!(
            service.poll_auth().await,
            Err(ProductError::AuthInvalidResponse)
        ));
        assert!(sessions.load().expect("session").is_none());
        assert!(!service.provider_credentials().cloud_ready());
    }

    fn completed_snapshot(operation_id: &str, text: &str) -> DictationSnapshot {
        DictationSnapshot {
            state: DictationState::Completed,
            operation_id: Some(operation_id.to_owned()),
            operation_kind: Some(OperationKind::Dictation),
            shortcut: crate::dictation::DEFAULT_SHORTCUT_LABEL,
            rewrite_shortcut: crate::dictation::REWRITE_SHORTCUT_LABEL,
            rewrite_stage: None,
            microphone_permission: crate::audio::MicrophonePermissionState::Granted,
            accessibility_permission: crate::insertion::AccessibilityPermissionState::Granted,
            error: None,
            partial_transcript: None,
            final_transcript: Some(text.to_owned()),
            transcript_finality: crate::dictation::TranscriptFinality::Final,
            degraded: false,
            insertion: None,
            rewrite: None,
            project_id: None,
            project_name: Some("General".to_owned()),
            voice_id: None,
            voice_name: Some("My Voice".to_owned()),
            mode_id: None,
            mode_name: Some("Clean Dictation".to_owned()),
            transformation: None,
        }
    }
}
