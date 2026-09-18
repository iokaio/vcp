// VCP modification: capture observed body bytes before SSE parsing; host admission supplies the exact request body.
use crate::auth::SharedAuthProvider;
use crate::common::ResponseStream;
use crate::common::ResponsesApiRequest;
use crate::endpoint::session::EndpointSession;
use crate::error::ApiError;
use crate::provider::Provider;
use crate::requests::headers::build_session_headers;
use crate::requests::headers::insert_header;
use crate::requests::headers::subagent_header;
use crate::requests::Compression;
use crate::sse::spawn_response_stream;
use crate::telemetry::SseTelemetry;
use codex_client::EncodedJsonBody;
use codex_client::HttpTransport;
use codex_client::RequestCompression;
use codex_client::RequestTelemetry;
use codex_protocol::protocol::SessionSource;
use futures::StreamExt;
use http::HeaderMap;
use http::HeaderValue;
use http::Method;
use serde_json::Value;
use std::sync::Arc;
use std::sync::OnceLock;
use tracing::instrument;

type ResponseCapture = Arc<dyn Fn(&[u8]) -> Result<(), String> + Send + Sync>;

pub struct ResponsesClient<T: HttpTransport> {
    session: EndpointSession<T>,
    sse_telemetry: Option<Arc<dyn SseTelemetry>>,
    response_capture: Option<ResponseCapture>,
    response_deadline: Option<std::time::Instant>,
}

#[derive(Default)]
pub struct ResponsesOptions {
    pub session_id: Option<String>,
    pub thread_id: Option<String>,
    pub session_source: Option<SessionSource>,
    pub extra_headers: HeaderMap,
    pub compression: Compression,
    pub turn_state: Option<Arc<OnceLock<String>>>,
}

impl<T: HttpTransport> ResponsesClient<T> {
    pub fn new(transport: T, provider: Provider, auth: SharedAuthProvider) -> Self {
        Self {
            session: EndpointSession::new(transport, provider, auth),
            sse_telemetry: None,
            response_capture: None,
            response_deadline: None,
        }
    }

    pub fn with_telemetry(
        self,
        request: Option<Arc<dyn RequestTelemetry>>,
        sse: Option<Arc<dyn SseTelemetry>>,
    ) -> Self {
        Self {
            session: self.session.with_request_telemetry(request),
            sse_telemetry: sse,
            response_capture: self.response_capture,
            response_deadline: self.response_deadline,
        }
    }

    pub fn with_response_capture(mut self, capture: Option<ResponseCapture>) -> Self {
        self.response_capture = capture;
        self
    }
    pub fn with_response_deadline(mut self, deadline: Option<std::time::Instant>) -> Self {
        self.response_deadline = deadline;
        self
    }

    #[instrument(
        name = "responses.stream_request",
        level = "info",
        skip_all,
        fields(
            transport = "responses_http",
            http.method = "POST",
            api.path = "/responses"
        )
    )]
    pub async fn stream_request(
        &self,
        request: ResponsesApiRequest,
        options: ResponsesOptions,
    ) -> Result<ResponseStream, ApiError> {
        let body = serde_json::to_value(request)
            .map_err(|e| ApiError::Stream(format!("failed to encode responses request: {e}")))?;
        self.stream_body(body, options).await
    }

    pub async fn stream_body(
        &self,
        body: Value,
        options: ResponsesOptions,
    ) -> Result<ResponseStream, ApiError> {
        let ResponsesOptions {
            session_id,
            thread_id,
            session_source,
            extra_headers,
            compression,
            turn_state,
        } = options;
        let body = EncodedJsonBody::encode(&body)
            .map_err(|e| ApiError::Stream(format!("failed to encode responses request: {e}")))?;

        let mut headers = extra_headers;
        if let Some(ref thread_id) = thread_id {
            insert_header(&mut headers, "x-client-request-id", thread_id);
        }
        headers.extend(build_session_headers(session_id, thread_id));
        if let Some(subagent) = subagent_header(&session_source) {
            insert_header(&mut headers, "x-openai-subagent", &subagent);
        }

        self.stream_encoded(body, headers, compression, turn_state)
            .await
    }

    #[instrument(
        name = "responses.stream",
        level = "info",
        skip_all,
        fields(
            transport = "responses_http",
            http.method = "POST",
            api.path = "/responses",
            turn.has_state = turn_state.is_some()
        )
    )]
    pub async fn stream(
        &self,
        body: Value,
        extra_headers: HeaderMap,
        compression: Compression,
        turn_state: Option<Arc<OnceLock<String>>>,
    ) -> Result<ResponseStream, ApiError> {
        let body = EncodedJsonBody::encode(&body)
            .map_err(|e| ApiError::Stream(format!("failed to encode responses request: {e}")))?;
        self.stream_encoded(body, extra_headers, compression, turn_state)
            .await
    }

    async fn stream_encoded(
        &self,
        body: EncodedJsonBody,
        extra_headers: HeaderMap,
        compression: Compression,
        turn_state: Option<Arc<OnceLock<String>>>,
    ) -> Result<ResponseStream, ApiError> {
        let request_compression = match compression {
            Compression::None => RequestCompression::None,
            Compression::Zstd => RequestCompression::Zstd,
        };

        let mut stream_response = self
            .session
            .stream_encoded_json_with(
                Method::POST,
                "/responses",
                extra_headers,
                Some(body),
                |req| {
                    req.headers.insert(
                        http::header::ACCEPT,
                        HeaderValue::from_static("text/event-stream"),
                    );
                    req.compression = request_compression;
                },
            )
            .await?;

        if let Some(deadline) = self.response_deadline {
            stream_response.bytes = bounded_response_stream(stream_response.bytes, deadline);
        }
        if let Some(capture) = self.response_capture.clone() {
            stream_response.bytes = stream_response
                .bytes
                .map(move |item| {
                    item.and_then(|bytes| {
                        capture(&bytes).map_err(codex_client::TransportError::Build)?;
                        Ok(bytes)
                    })
                })
                .boxed();
        }

        Ok(spawn_response_stream(
            stream_response,
            self.session.provider().stream_idle_timeout,
            self.sse_telemetry.clone(),
            turn_state,
        ))
    }
}

// VCP: one absolute deadline cannot be extended by response keepalives.
fn bounded_response_stream(
    bytes: codex_client::ByteStream,
    deadline: std::time::Instant,
) -> codex_client::ByteStream {
    let deadline = tokio::time::Instant::from_std(deadline);
    futures::stream::unfold((bytes, false), move |(mut bytes, done)| async move {
        if done {
            return None;
        }
        // Check explicitly: an always-ready stream must not win timeout polling
        // forever after the deadline, including already buffered keepalives.
        let next = if tokio::time::Instant::now() >= deadline {
            Err(())
        } else {
            tokio::time::timeout_at(deadline, bytes.next()).await.map_err(|_| ())
        };
        match next {
            Ok(Some(item)) => Some((item, (bytes, false))),
            Ok(None) => None,
            Err(_) => Some((
                Err(codex_client::TransportError::Build(
                    "VCP response deadline elapsed".into(),
                )),
                (bytes, true),
            )),
        }
    })
    .boxed()
}

#[cfg(test)]
mod vcp_deadline_tests {
    use super::*;
    #[tokio::test]
    async fn absolute_deadline_stops_silent_and_keepalive_streams() {
        for mode in 0..3 {
            let bytes: codex_client::ByteStream = if mode == 2 {
                futures::stream::repeat_with(|| Ok(bytes::Bytes::from_static(b": buffered\n\n"))).boxed()
            } else if mode == 1 {
                futures::stream::unfold((), |_| async {
                    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                    Some((Ok(bytes::Bytes::from_static(b": keepalive\n\n")), ()))
                })
                .boxed()
            } else {
                futures::stream::pending().boxed()
            };
            let mut stream = bounded_response_stream(
                bytes,
                std::time::Instant::now() + std::time::Duration::from_millis(30),
            );
            tokio::time::timeout(std::time::Duration::from_secs(2), async {
                loop {
                    if stream
                        .next()
                        .await
                        .expect("deadline produces one error")
                        .is_err()
                    {
                        break;
                    }
                }
                assert!(stream.next().await.is_none());
            })
            .await
            .expect("absolute deadline must terminate the stream");
        }
    }
}
