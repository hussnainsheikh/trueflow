use std::time::Duration;

use async_trait::async_trait;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};

use super::{
    TransformationError, TransformationProvider, TransformationRequest, TransformationResult,
};
use crate::credentials::ProviderCredentials;

pub struct OpenAiTransformationProvider {
    client: reqwest::Client,
    credentials: ProviderCredentials,
    model: String,
    endpoint: String,
}

impl OpenAiTransformationProvider {
    #[cfg(test)]
    fn new(api_key: Option<String>, model: String, timeout: Duration) -> Self {
        Self::with_credentials(ProviderCredentials::new(None, api_key), model, timeout)
    }

    pub fn with_credentials(
        credentials: ProviderCredentials,
        model: String,
        timeout: Duration,
    ) -> Self {
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self {
            client,
            credentials,
            model,
            endpoint: "https://api.openai.com/v1/responses".to_owned(),
        }
    }

    #[cfg(test)]
    fn with_endpoint(mut self, endpoint: String) -> Self {
        self.endpoint = endpoint;
        self
    }
}

#[derive(Serialize)]
struct OpenAiRequest<'a> {
    model: &'a str,
    instructions: &'a str,
    input: &'a str,
    max_output_tokens: u32,
    store: bool,
}

#[derive(Deserialize)]
struct OpenAiResponse {
    id: Option<String>,
    status: Option<String>,
    output: Vec<OutputItem>,
    usage: Option<Usage>,
}

#[derive(Deserialize)]
struct OutputItem {
    #[serde(default)]
    content: Vec<OutputContent>,
}

#[derive(Deserialize)]
struct OutputContent {
    #[serde(rename = "type")]
    kind: String,
    text: Option<String>,
}

#[derive(Deserialize)]
struct Usage {
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
}

#[async_trait]
impl TransformationProvider for OpenAiTransformationProvider {
    fn name(&self) -> &'static str {
        "openai"
    }

    async fn transform(
        &self,
        request: TransformationRequest,
    ) -> Result<TransformationResult, TransformationError> {
        let api_key = self
            .credentials
            .transformation()
            .ok_or(TransformationError::ConfigurationMissing)?;
        let response = self
            .client
            .post(&self.endpoint)
            .bearer_auth(&api_key)
            .header("X-Client-Request-Id", &request.operation_id)
            .json(&OpenAiRequest {
                model: &self.model,
                instructions: &request.instructions,
                input: &request.input,
                max_output_tokens: 2_048,
                store: false,
            })
            .send()
            .await
            .map_err(|error| {
                if error.is_timeout() {
                    TransformationError::Timeout
                } else {
                    TransformationError::ProviderUnavailable(error.to_string())
                }
            })?;
        let status = response.status();
        if !status.is_success() {
            return Err(map_status(status));
        }
        let response: OpenAiResponse = response
            .json()
            .await
            .map_err(|error| TransformationError::InvalidResponse(error.to_string()))?;
        if response
            .status
            .as_deref()
            .is_some_and(|status| status != "completed")
        {
            return Err(TransformationError::InvalidResponse(
                "response was not completed".to_owned(),
            ));
        }
        let text = response
            .output
            .iter()
            .flat_map(|item| &item.content)
            .filter(|content| content.kind == "output_text")
            .filter_map(|content| content.text.as_deref())
            .collect::<Vec<_>>()
            .join("");
        let text = text.trim().to_owned();
        if text.is_empty() {
            return Err(TransformationError::InvalidResponse(
                "response contained no output text".to_owned(),
            ));
        }
        Ok(TransformationResult {
            text,
            provider_request_id: response.id,
            input_tokens: response.usage.as_ref().and_then(|usage| usage.input_tokens),
            output_tokens: response.usage.and_then(|usage| usage.output_tokens),
        })
    }
}

fn map_status(status: StatusCode) -> TransformationError {
    match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
            TransformationError::AuthenticationFailed
        }
        StatusCode::TOO_MANY_REQUESTS => TransformationError::RateLimited,
        status if status.is_server_error() => {
            TransformationError::ProviderUnavailable(format!("HTTP {}", status.as_u16()))
        }
        status => TransformationError::InvalidResponse(format!("HTTP {}", status.as_u16())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_provider_statuses_to_stable_errors() {
        assert_eq!(
            map_status(StatusCode::UNAUTHORIZED),
            TransformationError::AuthenticationFailed
        );
        assert_eq!(
            map_status(StatusCode::TOO_MANY_REQUESTS),
            TransformationError::RateLimited
        );
        assert!(matches!(
            map_status(StatusCode::SERVICE_UNAVAILABLE),
            TransformationError::ProviderUnavailable(_)
        ));
    }

    #[test]
    fn request_serialization_uses_the_responses_contract_without_storage() {
        let body = serde_json::to_value(OpenAiRequest {
            model: "gpt-5.6-luna",
            instructions: "Keep it terse.",
            input: "Draft this.",
            max_output_tokens: 2_048,
            store: false,
        })
        .expect("request JSON");

        assert_eq!(body["model"], "gpt-5.6-luna");
        assert_eq!(body["instructions"], "Keep it terse.");
        assert_eq!(body["input"], "Draft this.");
        assert_eq!(body["max_output_tokens"], 2_048);
        assert_eq!(body["store"], false);
    }

    #[tokio::test]
    async fn missing_key_fails_before_network_access() {
        let provider = OpenAiTransformationProvider::new(
            None,
            "gpt-5.6-luna".to_owned(),
            Duration::from_secs(1),
        )
        .with_endpoint("http://127.0.0.1:1".to_owned());
        let result = provider
            .transform(TransformationRequest {
                operation_id: "operation".to_owned(),
                instructions: "instructions".to_owned(),
                input: "input".to_owned(),
            })
            .await;
        assert_eq!(result, Err(TransformationError::ConfigurationMissing));
    }
}
