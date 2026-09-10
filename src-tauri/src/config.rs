use std::{env, fmt, str::FromStr};

use serde::Serialize;

use crate::error::AppError;

const DEFAULT_LOG_FILTER: &str = "info,trueflow=debug";
const DEFAULT_OPENAI_MODEL: &str = "gpt-5.6-luna";

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Environment {
    Development,
    Test,
    Staging,
    Production,
}

impl fmt::Display for Environment {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Development => "development",
            Self::Test => "test",
            Self::Staging => "staging",
            Self::Production => "production",
        };
        formatter.write_str(name)
    }
}

impl FromStr for Environment {
    type Err = AppError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "development" => Ok(Self::Development),
            "test" => Ok(Self::Test),
            "staging" => Ok(Self::Staging),
            "production" => Ok(Self::Production),
            invalid => Err(AppError::InvalidEnvironment(invalid.to_owned())),
        }
    }
}

#[derive(Clone)]
pub struct AppConfig {
    pub environment: Environment,
    pub log_filter: String,
    pub deepgram_api_key: Option<String>,
    pub openai_api_key: Option<String>,
    pub openai_model: String,
    pub auth_base_url: Option<String>,
    pub analytics_endpoint: Option<String>,
    pub analytics_key: Option<String>,
    pub crash_report_endpoint: Option<String>,
}

impl AppConfig {
    pub fn load() -> Result<Self, AppError> {
        let default_environment = if cfg!(debug_assertions) {
            Environment::Development
        } else {
            Environment::Production
        };

        Self::from_values(
            env::var("TRUEFLOW_ENV").ok().as_deref(),
            env::var("RUST_LOG").ok().as_deref(),
            env::var("TRUEFLOW_DEEPGRAM_API_KEY").ok().as_deref(),
            env::var("TRUEFLOW_OPENAI_API_KEY").ok().as_deref(),
            env::var("TRUEFLOW_OPENAI_MODEL").ok().as_deref(),
            env::var("TRUEFLOW_AUTH_BASE_URL").ok().as_deref(),
            env::var("TRUEFLOW_ANALYTICS_ENDPOINT").ok().as_deref(),
            env::var("TRUEFLOW_ANALYTICS_KEY").ok().as_deref(),
            env::var("TRUEFLOW_CRASH_REPORT_ENDPOINT").ok().as_deref(),
            default_environment,
        )
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "configuration sources stay explicit at the secret boundary"
    )]
    fn from_values(
        environment: Option<&str>,
        log_filter: Option<&str>,
        deepgram_api_key: Option<&str>,
        openai_api_key: Option<&str>,
        openai_model: Option<&str>,
        auth_base_url: Option<&str>,
        analytics_endpoint: Option<&str>,
        analytics_key: Option<&str>,
        crash_report_endpoint: Option<&str>,
        default_environment: Environment,
    ) -> Result<Self, AppError> {
        let environment = environment
            .map(Environment::from_str)
            .transpose()?
            .unwrap_or(default_environment);
        let log_filter = log_filter
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(DEFAULT_LOG_FILTER)
            .to_owned();
        let deepgram_api_key = deepgram_api_key
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned);
        let openai_api_key = openai_api_key
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned);
        let openai_model = openai_model
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(DEFAULT_OPENAI_MODEL)
            .to_owned();
        let optional = |value: Option<&str>| {
            value
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
        };

        Ok(Self {
            environment,
            log_filter,
            deepgram_api_key,
            openai_api_key,
            openai_model,
            auth_base_url: optional(auth_base_url),
            analytics_endpoint: optional(analytics_endpoint),
            analytics_key: optional(analytics_key),
            crash_report_endpoint: optional(crash_report_endpoint),
        })
    }

    #[cfg(test)]
    pub(crate) fn for_tests() -> Self {
        Self::from_values(
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            Environment::Test,
        )
        .expect("test configuration must be valid")
    }
}

#[cfg(test)]
mod tests {
    use super::{AppConfig, Environment};

    #[test]
    fn accepts_all_supported_environments() {
        for (input, expected) in [
            ("development", Environment::Development),
            ("test", Environment::Test),
            ("staging", Environment::Staging),
            ("production", Environment::Production),
        ] {
            let config = AppConfig::from_values(
                Some(input),
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                Environment::Production,
            )
            .expect("supported environment should parse");

            assert_eq!(config.environment, expected);
        }
    }

    #[test]
    fn rejects_unknown_environment() {
        let error = AppConfig::from_values(
            Some("preview"),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            Environment::Development,
        )
        .err()
        .expect("unknown environment must be rejected");

        assert_eq!(error.code(), "CONFIG_INVALID_ENVIRONMENT");
    }

    #[test]
    fn uses_safe_defaults_when_values_are_absent() {
        let config = AppConfig::from_values(
            None,
            Some(""),
            Some("  "),
            Some(" "),
            Some(" "),
            None,
            None,
            None,
            None,
            Environment::Test,
        )
        .expect("defaults should be valid");

        assert_eq!(config.environment, Environment::Test);
        assert_eq!(config.log_filter, "info,trueflow=debug");
        assert_eq!(config.deepgram_api_key, None);
        assert_eq!(config.openai_api_key, None);
        assert_eq!(config.openai_model, "gpt-5.6-luna");
    }

    #[test]
    fn keeps_nonempty_provider_key_rust_side() {
        let config = AppConfig::from_values(
            None,
            None,
            Some(" development-secret "),
            Some(" transform-secret "),
            Some(" gpt-5.6-luna "),
            None,
            None,
            None,
            None,
            Environment::Test,
        )
        .expect("configuration should load");

        assert_eq!(
            config.deepgram_api_key.as_deref(),
            Some("development-secret")
        );
        assert_eq!(config.openai_api_key.as_deref(), Some("transform-secret"));
    }
}
