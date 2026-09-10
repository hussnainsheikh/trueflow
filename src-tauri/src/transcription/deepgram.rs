use std::time::Duration;

use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt, stream::SplitSink, stream::SplitStream};
use serde::Deserialize;
use tokio::{net::TcpStream, time::timeout};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, connect_async,
    tungstenite::{
        Message,
        client::IntoClientRequest,
        http::{HeaderValue, StatusCode},
    },
};

use super::{
    AudioChunk, AudioEncoding, ProviderVocabularyCapabilities, SessionConfig, SessionParts,
    TranscriptionError, TranscriptionEvent, TranscriptionEvents, TranscriptionInput,
    TranscriptionProvider,
};
use crate::credentials::ProviderCredentials;

const ENDPOINT: &str = "wss://api.deepgram.com/v1/listen";

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;
type SocketWriter = SplitSink<Socket, Message>;
type SocketReader = SplitStream<Socket>;

#[derive(Clone)]
pub struct DeepgramProvider {
    credentials: ProviderCredentials,
    connection_timeout: Duration,
}

impl DeepgramProvider {
    pub fn new(api_key: Option<String>, connection_timeout: Duration) -> Self {
        Self::with_credentials(ProviderCredentials::new(api_key, None), connection_timeout)
    }

    pub const fn with_credentials(
        credentials: ProviderCredentials,
        connection_timeout: Duration,
    ) -> Self {
        Self {
            credentials,
            connection_timeout,
        }
    }
}

#[async_trait]
impl TranscriptionProvider for DeepgramProvider {
    fn name(&self) -> &'static str {
        "deepgram"
    }

    fn vocabulary_capabilities(&self) -> ProviderVocabularyCapabilities {
        ProviderVocabularyCapabilities {
            supports_hints: true,
            supports_weighted_terms: false,
            max_hint_count: 100,
            max_total_tokens: 500,
            max_hint_length: 256,
        }
    }

    async fn start_session(
        &self,
        config: SessionConfig,
    ) -> Result<SessionParts, TranscriptionError> {
        if config.encoding != AudioEncoding::Linear16 || config.channels != 1 {
            return Err(TranscriptionError::AudioRejected(
                "Deepgram streaming requires mono linear16 in this adapter".to_owned(),
            ));
        }
        let api_key = self
            .credentials
            .transcription()
            .filter(|key| !key.trim().is_empty())
            .ok_or(TranscriptionError::ConfigurationMissing)?;
        let mut url = format!(
            "{ENDPOINT}?model={}&language={}&encoding=linear16&sample_rate={}&channels=1&interim_results=true&smart_format=true&punctuate=true&vad_events=true&endpointing=300",
            config.model, config.language, config.sample_rate
        );
        for keyterm in &config.vocabulary.terms {
            url.push_str("&keyterm=");
            url.push_str(&percent_encode_query_value(keyterm));
        }
        let mut request = url
            .into_client_request()
            .map_err(|error| TranscriptionError::ProtocolError(error.to_string()))?;
        let authorization = HeaderValue::from_str(&format!("Token {api_key}"))
            .map_err(|_| TranscriptionError::ConfigurationMissing)?;
        request.headers_mut().insert("Authorization", authorization);

        let connection = timeout(self.connection_timeout, connect_async(request))
            .await
            .map_err(|_| TranscriptionError::ConnectionTimeout)?
            .map_err(map_websocket_error)?;
        let (writer, reader) = connection.0.split();
        Ok(SessionParts {
            input: Box::new(DeepgramInput { writer }),
            events: Box::new(DeepgramEvents {
                reader,
                session_started_pending: true,
            }),
        })
    }
}

fn percent_encode_query_value(value: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut encoded = String::with_capacity(value.len());
    for byte in value.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(char::from(*byte));
        } else {
            encoded.push('%');
            encoded.push(char::from(HEX[usize::from(byte >> 4)]));
            encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
        }
    }
    encoded
}

struct DeepgramInput {
    writer: SocketWriter,
}

#[async_trait]
impl TranscriptionInput for DeepgramInput {
    async fn send_audio(&mut self, chunk: AudioChunk) -> Result<(), TranscriptionError> {
        let mut bytes = Vec::with_capacity(chunk.samples.len().saturating_mul(2));
        for sample in chunk.samples.iter() {
            bytes.extend_from_slice(&sample.to_le_bytes());
        }
        self.writer
            .send(Message::Binary(bytes.into()))
            .await
            .map_err(map_websocket_error)
    }

    async fn finish(&mut self) -> Result<(), TranscriptionError> {
        self.writer
            .send(Message::Text(r#"{"type":"Finalize"}"#.into()))
            .await
            .map_err(map_websocket_error)
    }

    async fn close(&mut self) -> Result<(), TranscriptionError> {
        self.writer
            .send(Message::Text(r#"{"type":"CloseStream"}"#.into()))
            .await
            .map_err(map_websocket_error)
    }

    async fn cancel(&mut self) -> Result<(), TranscriptionError> {
        let _ = self
            .writer
            .send(Message::Text(r#"{"type":"CloseStream"}"#.into()))
            .await;
        self.writer.close().await.map_err(map_websocket_error)
    }
}

struct DeepgramEvents {
    reader: SocketReader,
    session_started_pending: bool,
}

#[async_trait]
impl TranscriptionEvents for DeepgramEvents {
    async fn next_event(&mut self) -> Result<TranscriptionEvent, TranscriptionError> {
        if self.session_started_pending {
            self.session_started_pending = false;
            return Ok(TranscriptionEvent::SessionStarted {
                provider_session_id: None,
            });
        }

        loop {
            let message = self
                .reader
                .next()
                .await
                .ok_or(TranscriptionError::SessionClosedUnexpectedly)?
                .map_err(map_websocket_error)?;
            match message {
                Message::Text(text) => return parse_server_message(text.as_ref()),
                Message::Close(_) => return Ok(TranscriptionEvent::SessionClosed),
                Message::Ping(_) | Message::Pong(_) | Message::Frame(_) => continue,
                Message::Binary(_) => {
                    return Err(TranscriptionError::InvalidResponse(
                        "unexpected binary provider response".to_owned(),
                    ));
                }
            }
        }
    }
}

#[derive(Debug, Deserialize)]
struct MessageType {
    #[serde(rename = "type")]
    kind: String,
}

#[derive(Debug, Deserialize)]
struct ResultsMessage {
    channel: ResultsChannel,
    #[serde(default)]
    is_final: bool,
    #[serde(default)]
    speech_final: bool,
    #[serde(default)]
    from_finalize: bool,
}

#[derive(Debug, Deserialize)]
struct ResultsChannel {
    alternatives: Vec<TranscriptAlternative>,
}

#[derive(Debug, Deserialize)]
struct TranscriptAlternative {
    transcript: String,
}

#[derive(Debug, Deserialize)]
struct MetadataMessage {
    request_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ErrorMessage {
    #[serde(default)]
    code: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    message: String,
}

fn parse_server_message(text: &str) -> Result<TranscriptionEvent, TranscriptionError> {
    let message_type: MessageType = serde_json::from_str(text)
        .map_err(|error| TranscriptionError::InvalidResponse(error.to_string()))?;
    match message_type.kind.as_str() {
        "Results" => {
            let result: ResultsMessage = serde_json::from_str(text)
                .map_err(|error| TranscriptionError::InvalidResponse(error.to_string()))?;
            let transcript = result
                .channel
                .alternatives
                .into_iter()
                .next()
                .map(|alternative| alternative.transcript)
                .unwrap_or_default();
            if result.is_final || result.from_finalize {
                Ok(TranscriptionEvent::FinalTranscript {
                    text: transcript,
                    operation_final: result.from_finalize,
                })
            } else if result.speech_final {
                Ok(TranscriptionEvent::SpeechEnded)
            } else {
                Ok(TranscriptionEvent::PartialTranscript { text: transcript })
            }
        }
        "SpeechStarted" => Ok(TranscriptionEvent::SpeechStarted),
        "UtteranceEnd" => Ok(TranscriptionEvent::SpeechEnded),
        "Metadata" => {
            let metadata: MetadataMessage = serde_json::from_str(text)
                .map_err(|error| TranscriptionError::InvalidResponse(error.to_string()))?;
            Ok(TranscriptionEvent::SessionStarted {
                provider_session_id: metadata.request_id,
            })
        }
        "Warning" => Ok(TranscriptionEvent::ProviderWarning {
            code: "DEEPGRAM_WARNING".to_owned(),
        }),
        "Error" => {
            let error: ErrorMessage = serde_json::from_str(text).map_err(|parse_error| {
                TranscriptionError::InvalidResponse(parse_error.to_string())
            })?;
            let detail = [error.code, error.description, error.message]
                .into_iter()
                .find(|value| !value.is_empty())
                .unwrap_or_else(|| "provider error".to_owned());
            Ok(TranscriptionEvent::ProviderError(
                TranscriptionError::ProtocolError(detail),
            ))
        }
        other => Err(TranscriptionError::InvalidResponse(format!(
            "unsupported Deepgram message type: {other}"
        ))),
    }
}

fn map_websocket_error(error: tokio_tungstenite::tungstenite::Error) -> TranscriptionError {
    use tokio_tungstenite::tungstenite::Error;

    match error {
        Error::Http(response) => match response.status() {
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
                TranscriptionError::AuthenticationFailed
            }
            StatusCode::TOO_MANY_REQUESTS => TranscriptionError::RateLimited,
            status if status.is_server_error() => {
                TranscriptionError::ProviderUnavailable(status.to_string())
            }
            status => TranscriptionError::ConnectionFailed(status.to_string()),
        },
        Error::Io(error) if is_network_unavailable(&error) => {
            TranscriptionError::NetworkUnavailable(error.to_string())
        }
        Error::Io(error) => TranscriptionError::ConnectionFailed(error.to_string()),
        Error::ConnectionClosed | Error::AlreadyClosed => {
            TranscriptionError::SessionClosedUnexpectedly
        }
        other => TranscriptionError::ProtocolError(other.to_string()),
    }
}

fn is_network_unavailable(error: &std::io::Error) -> bool {
    use std::io::ErrorKind;

    matches!(
        error.kind(),
        ErrorKind::NotConnected
            | ErrorKind::NetworkUnreachable
            | ErrorKind::HostUnreachable
            | ErrorKind::AddrNotAvailable
    )
}

#[cfg(test)]
mod tests {
    use super::{parse_server_message, percent_encode_query_value};
    use crate::transcription::{TranscriptionError, TranscriptionEvent};

    #[test]
    fn maps_interim_result_to_replacing_partial() {
        let event = parse_server_message(
            r#"{"type":"Results","is_final":false,"channel":{"alternatives":[{"transcript":"I think we"}]}}"#,
        )
        .expect("valid result");

        assert_eq!(
            event,
            TranscriptionEvent::PartialTranscript {
                text: "I think we".to_owned()
            }
        );
    }

    #[test]
    fn maps_finalize_result_to_operation_final() {
        let event = parse_server_message(
            r#"{"type":"Results","is_final":true,"from_finalize":true,"channel":{"alternatives":[{"transcript":"final words"}]}}"#,
        )
        .expect("valid result");

        assert_eq!(
            event,
            TranscriptionEvent::FinalTranscript {
                text: "final words".to_owned(),
                operation_final: true
            }
        );
    }

    #[test]
    fn rejects_malformed_provider_json() {
        assert!(matches!(
            parse_server_message("not json"),
            Err(TranscriptionError::InvalidResponse(_))
        ));
    }

    #[test]
    fn keyterms_are_encoded_as_independent_query_values() {
        assert_eq!(percent_encode_query_value("Vhal'Tor"), "Vhal%27Tor");
        assert_eq!(percent_encode_query_value("Ashen Coast"), "Ashen%20Coast");
        assert_eq!(percent_encode_query_value("Élara"), "%C3%89lara");
    }
}
