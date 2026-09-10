mod openai;
mod prompt;

use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

#[cfg(test)]
use std::{collections::VecDeque, sync::Mutex};

use async_trait::async_trait;
use serde::Serialize;
use thiserror::Error;
use tokio::sync::Notify;

pub use openai::OpenAiTransformationProvider;
pub use prompt::{build_prompt, build_rewrite_prompt};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransformationRequest {
    pub operation_id: String,
    pub instructions: String,
    pub input: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransformationResult {
    pub text: String,
    pub provider_request_id: Option<String>,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum TransformationError {
    #[error("transformation is not configured")]
    ConfigurationMissing,
    #[error("transformation authentication failed")]
    AuthenticationFailed,
    #[error("transformation request timed out")]
    Timeout,
    #[error("transformation provider is unavailable: {0}")]
    ProviderUnavailable(String),
    #[error("transformation provider rate limit reached")]
    RateLimited,
    #[error("transformation provider returned an invalid response: {0}")]
    InvalidResponse(String),
    #[error("transformation was cancelled")]
    Cancelled,
}

impl TransformationError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::ConfigurationMissing => "TRANSFORM_CONFIGURATION_MISSING",
            Self::AuthenticationFailed => "TRANSFORM_AUTHENTICATION_FAILED",
            Self::Timeout => "TRANSFORM_TIMEOUT",
            Self::ProviderUnavailable(_) => "TRANSFORM_PROVIDER_UNAVAILABLE",
            Self::RateLimited => "TRANSFORM_RATE_LIMITED",
            Self::InvalidResponse(_) => "TRANSFORM_INVALID_RESPONSE",
            Self::Cancelled => "TRANSFORM_CANCELLED",
        }
    }

    pub const fn user_message(&self) -> &'static str {
        match self {
            Self::ConfigurationMissing => {
                "AI transformation is not configured. The normalized transcript was used."
            }
            Self::AuthenticationFailed => {
                "AI transformation authentication failed. The normalized transcript was used."
            }
            Self::Timeout => "AI transformation timed out. The normalized transcript was used.",
            Self::ProviderUnavailable(_) | Self::RateLimited => {
                "AI transformation is temporarily unavailable. The normalized transcript was used."
            }
            Self::InvalidResponse(_) => {
                "AI transformation returned no usable text. The normalized transcript was used."
            }
            Self::Cancelled => {
                "AI transformation was cancelled. The normalized transcript was preserved."
            }
        }
    }

    pub const fn retryable(&self) -> bool {
        matches!(self, Self::ProviderUnavailable(_) | Self::RateLimited)
    }
}

#[async_trait]
pub trait TransformationProvider: Send + Sync {
    fn name(&self) -> &'static str;
    async fn transform(
        &self,
        request: TransformationRequest,
    ) -> Result<TransformationResult, TransformationError>;
}

pub type SharedTransformationProvider = Arc<dyn TransformationProvider>;

#[derive(Clone, Copy, Debug)]
pub struct TransformationConfig {
    pub timeout: Duration,
    pub max_retries: u32,
}

impl Default for TransformationConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(12),
            max_retries: 1,
        }
    }
}

#[derive(Clone, Default)]
pub struct TransformationControl {
    cancelled: Arc<AtomicBool>,
    notify: Arc<Notify>,
}

impl TransformationControl {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
        self.notify.notify_waiters();
    }
}

pub struct TransformationExecution {
    pub result: Result<TransformationResult, TransformationError>,
    pub retry_count: u32,
}

pub async fn execute(
    provider: SharedTransformationProvider,
    request: TransformationRequest,
    config: TransformationConfig,
    control: TransformationControl,
) -> TransformationExecution {
    let mut retry_count = 0;
    loop {
        if control.cancelled.load(Ordering::Acquire) {
            return TransformationExecution {
                result: Err(TransformationError::Cancelled),
                retry_count,
            };
        }
        let attempt = provider.transform(request.clone());
        let result = tokio::select! {
            _ = control.notify.notified() => Err(TransformationError::Cancelled),
            result = tokio::time::timeout(config.timeout, attempt) => {
                result.unwrap_or(Err(TransformationError::Timeout))
            }
        };
        if result.as_ref().is_err_and(TransformationError::retryable)
            && retry_count < config.max_retries
        {
            retry_count += 1;
            continue;
        }
        return TransformationExecution {
            result,
            retry_count,
        };
    }
}

#[cfg(test)]
#[derive(Clone, Debug)]
pub struct MockTransformationPlan {
    pub delay: Duration,
    pub result: Result<TransformationResult, TransformationError>,
}

#[cfg(test)]
impl Default for MockTransformationPlan {
    fn default() -> Self {
        Self {
            delay: Duration::ZERO,
            result: Ok(TransformationResult {
                text: "transformed text".to_owned(),
                provider_request_id: Some("mock-request".to_owned()),
                input_tokens: Some(10),
                output_tokens: Some(4),
            }),
        }
    }
}

#[cfg(test)]
#[derive(Default)]
pub struct MockTransformationProvider {
    plans: Mutex<VecDeque<MockTransformationPlan>>,
    requests: Mutex<Vec<TransformationRequest>>,
}

#[cfg(test)]
impl MockTransformationProvider {
    pub fn new(plans: impl IntoIterator<Item = MockTransformationPlan>) -> Self {
        Self {
            plans: Mutex::new(plans.into_iter().collect()),
            requests: Mutex::new(Vec::new()),
        }
    }

    pub fn requests(&self) -> Vec<TransformationRequest> {
        self.requests
            .lock()
            .map(|requests| requests.clone())
            .unwrap_or_default()
    }
}

#[cfg(test)]
#[async_trait]
impl TransformationProvider for MockTransformationProvider {
    fn name(&self) -> &'static str {
        "mock"
    }

    async fn transform(
        &self,
        request: TransformationRequest,
    ) -> Result<TransformationResult, TransformationError> {
        if let Ok(mut requests) = self.requests.lock() {
            requests.push(request);
        }
        let plan = self
            .plans
            .lock()
            .ok()
            .and_then(|mut plans| plans.pop_front())
            .unwrap_or_default();
        if !plan.delay.is_zero() {
            tokio::time::sleep(plan.delay).await;
        }
        plan.result
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransformationStatus {
    pub provider: &'static str,
    pub fallback_used: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> TransformationRequest {
        TransformationRequest {
            operation_id: "operation".to_owned(),
            instructions: "instructions".to_owned(),
            input: "input".to_owned(),
        }
    }

    #[tokio::test]
    async fn retries_only_within_the_configured_bound() {
        let provider = Arc::new(MockTransformationProvider::new([
            MockTransformationPlan {
                result: Err(TransformationError::ProviderUnavailable("one".to_owned())),
                ..MockTransformationPlan::default()
            },
            MockTransformationPlan {
                result: Err(TransformationError::ProviderUnavailable("two".to_owned())),
                ..MockTransformationPlan::default()
            },
        ]));
        let execution = execute(
            provider.clone(),
            request(),
            TransformationConfig {
                timeout: Duration::from_secs(1),
                max_retries: 1,
            },
            TransformationControl::default(),
        )
        .await;

        assert_eq!(execution.retry_count, 1);
        assert_eq!(provider.requests().len(), 2);
        assert!(matches!(
            execution.result,
            Err(TransformationError::ProviderUnavailable(_))
        ));
    }

    #[tokio::test]
    async fn timeout_and_cancellation_are_bounded() {
        let provider = Arc::new(MockTransformationProvider::new([MockTransformationPlan {
            delay: Duration::from_secs(10),
            ..MockTransformationPlan::default()
        }]));
        let timed_out = execute(
            provider,
            request(),
            TransformationConfig {
                timeout: Duration::from_millis(5),
                max_retries: 0,
            },
            TransformationControl::default(),
        )
        .await;
        assert_eq!(timed_out.result, Err(TransformationError::Timeout));

        let control = TransformationControl::default();
        control.cancel();
        let cancelled = execute(
            Arc::new(MockTransformationProvider::default()),
            request(),
            TransformationConfig::default(),
            control,
        )
        .await;
        assert_eq!(cancelled.result, Err(TransformationError::Cancelled));
    }
}
