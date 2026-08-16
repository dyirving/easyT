use std::sync::Arc;
use std::time::Duration;

use crate::config::AppConfig;
use crate::translation_backend::error::BackendError;
use crate::translation_backend::models::{BackendRequest, BackendResult};
use crate::translation_backend::web_gateway::credential_store::TicketSecret;
use crate::translation_backend::{TranslationPhase, TranslationProgressReporter};
use futures_util::StreamExt;

use super::adapter::{
    consume_qwen_sse_chunks, map_request_error, map_status_to_error, map_stream_error,
    prepare_qwen_request,
};

const DEFAULT_QWEN_API_URL: &str = "https://chat2.qianwen.com/api/v2/chat";

/// Inputs that determine exactly one Qwen protocol request.
pub(crate) struct QwenExecutionOptions<'a, F> {
    pub(crate) config: &'a AppConfig,
    pub(crate) request: &'a BackendRequest,
    pub(crate) ticket: &'a TicketSecret,
    pub(crate) progress: Arc<TranslationProgressReporter>,
    pub(crate) stream_output: bool,
    pub(crate) save_history: bool,
    pub(crate) timeout: Duration,
    pub(crate) before_send: F,
}

/// Protocol-only Qwen request executor. It never selects an account or retries a request.
pub struct QwenRequestExecutor {
    http_client: reqwest::Client,
    api_url: String,
}

impl QwenRequestExecutor {
    pub fn new(http_client: reqwest::Client) -> Self {
        Self {
            http_client,
            api_url: DEFAULT_QWEN_API_URL.to_string(),
        }
    }

    #[cfg(test)]
    pub(crate) fn with_api_url(http_client: reqwest::Client, api_url: String) -> Self {
        Self {
            http_client,
            api_url,
        }
    }

    pub async fn execute_once<F>(
        &self,
        options: QwenExecutionOptions<'_, F>,
    ) -> Result<BackendResult, BackendError>
    where
        F: FnOnce(),
    {
        let QwenExecutionOptions {
            config,
            request,
            ticket,
            progress,
            stream_output,
            save_history,
            timeout,
            before_send,
        } = options;
        let mut prepared = prepare_qwen_request(config, request, ticket, save_history)?;
        prepared.url = self.api_url.clone();

        progress.phase(TranslationPhase::ConnectingBackend, None);
        // Every fallible local step is complete. Cursor ownership changes at this boundary only.
        before_send();
        let response = if stream_output {
            tokio::time::timeout(
                timeout,
                self.http_client
                    .post(&prepared.url)
                    .headers(prepared.headers)
                    .query(&prepared.params)
                    .json(&prepared.body)
                    .send(),
            )
            .await
            .map_err(|_| BackendError::Qwen(super::error::QwenError::timeout()))?
            .map_err(map_request_error)?
        } else {
            self.http_client
                .post(&prepared.url)
                .headers(prepared.headers)
                .query(&prepared.params)
                .timeout(timeout)
                .json(&prepared.body)
                .send()
                .await
                .map_err(map_request_error)?
        };

        let status = response.status();
        if !status.is_success() {
            let response_body = response.text().await.unwrap_or_default();
            log::warn!(
                "Qwen upstream non-2xx: status={}, body_len={}",
                status.as_u16(),
                response_body.len()
            );
            return Err(map_status_to_error(status));
        }

        progress.phase(TranslationPhase::WaitingForContent, None);
        let chunks = response
            .bytes_stream()
            .map(|chunk| chunk.map_err(map_stream_error));
        consume_qwen_sse_chunks(
            chunks,
            &prepared.model,
            progress,
            stream_output,
            stream_output.then_some(timeout),
        )
        .await
        .map_err(|error| {
            super::error::QwenError::from_backend_error(&error)
                .map(BackendError::Qwen)
                .unwrap_or(error)
        })
    }
}
