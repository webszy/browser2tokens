//! Isolated ChatGPT Web network-observation spike.
//!
//! This experiment intentionally stays outside the production provider and
//! transport modules. It observes one manually submitted message through the
//! CDP Network domain and reconstructs assistant text from the response stream.

use std::collections::HashMap;
use std::time::Duration;

use anyhow::{Context, anyhow, bail};
use chromiumoxide::Binary;
use chromiumoxide::cdp::browser_protocol::network::ResourceType;
use chromiumoxide::cdp::browser_protocol::network::{
    self, EventDataReceived, EventLoadingFinished, EventRequestWillBeSent, EventResponseReceived,
    EventWebSocketClosed, EventWebSocketCreated, EventWebSocketFrameReceived,
    EventWebSocketFrameSent,
};
use chromiumoxide::page::Page;
use futures::StreamExt;
use serde_json::Value;

use crate::test_cdp::{chrome_executable, profile_dir, wait_for_chatgpt_page};

const CHATGPT_URL: &str = "https://chatgpt.com";
const TARGET_PROMPT: &str = "Reply with exactly: B2T_TEST_OK";
const OBSERVATION_TIMEOUT: Duration = Duration::from_secs(180);
const MAX_NETWORK_BUFFER: i64 = 32 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ObservedStreamKind {
    Sse,
    ChunkedFetch,
    WebSocket,
    Other,
}

impl ObservedStreamKind {
    fn label(self) -> &'static str {
        match self {
            Self::Sse => "SSE",
            Self::ChunkedFetch => "chunked fetch",
            Self::WebSocket => "WebSocket",
            Self::Other => "other",
        }
    }
}

#[derive(Debug, Clone)]
struct ObservedRequest {
    request_id: String,
    method: String,
    host: String,
    path: String,
    resource_type: Option<String>,
    conversation_id: Option<String>,
    message_id: Option<String>,
    parent_message_id: Option<String>,
    response_id: Option<String>,
    model: Option<String>,
    stream_kind: Option<ObservedStreamKind>,
    response_status: Option<i64>,
    content_type: Option<String>,
    response_started: bool,
    stream_enabled: bool,
    finished: bool,
    sse_buffer: String,
    assistant_text: String,
    saw_incremental_data: bool,
    saw_done_marker: bool,
    reported_response_start: bool,
}

impl ObservedRequest {
    fn http(event: &EventRequestWillBeSent) -> Self {
        let url = &event.request.url;
        let body = event
            .request
            .post_data_entries
            .as_ref()
            .and_then(|entries| {
                let joined = entries
                    .iter()
                    .filter_map(|entry| entry.bytes.as_ref().map(binary_as_str))
                    .collect::<String>();
                (!joined.is_empty()).then_some(joined)
            });
        let body_value = body
            .as_deref()
            .and_then(|body| serde_json::from_str(body).ok());

        Self {
            request_id: event.request_id.as_ref().to_owned(),
            method: event.request.method.clone(),
            host: url_host(url).to_owned(),
            path: url_path(url),
            resource_type: event.r#type.as_ref().map(|kind| kind.as_ref().to_owned()),
            conversation_id: body_value
                .as_ref()
                .and_then(|value| find_string(value, &["conversation_id", "conversationId"])),
            message_id: body_value
                .as_ref()
                .and_then(|value| find_string(value, &["message_id", "messageId"])),
            parent_message_id: body_value
                .as_ref()
                .and_then(|value| find_string(value, &["parent_message_id", "parentMessageId"])),
            response_id: body_value
                .as_ref()
                .and_then(|value| find_string(value, &["response_id", "responseId"])),
            model: body_value
                .as_ref()
                .and_then(|value| find_string(value, &["model", "model_slug", "modelSlug"])),
            stream_kind: None,
            response_status: None,
            content_type: None,
            response_started: false,
            stream_enabled: false,
            finished: false,
            sse_buffer: String::new(),
            assistant_text: String::new(),
            saw_incremental_data: false,
            saw_done_marker: false,
            reported_response_start: false,
        }
    }

    fn websocket(request_id: String, url: &str) -> Self {
        Self {
            request_id,
            method: "CONNECT".to_owned(),
            host: url_host(url).to_owned(),
            path: url_path(url),
            resource_type: Some("WebSocket".to_owned()),
            conversation_id: None,
            message_id: None,
            parent_message_id: None,
            response_id: None,
            model: None,
            stream_kind: Some(ObservedStreamKind::WebSocket),
            response_status: None,
            content_type: None,
            response_started: false,
            stream_enabled: false,
            finished: false,
            sse_buffer: String::new(),
            assistant_text: String::new(),
            saw_incremental_data: false,
            saw_done_marker: false,
            reported_response_start: false,
        }
    }

    fn is_http_message_candidate(&self, body: &str) -> bool {
        self.method == "POST" && is_chatgpt_host(&self.host) && body.contains(TARGET_PROMPT)
    }

    fn record_value_metadata(&mut self, value: &Value) {
        self.conversation_id = self
            .conversation_id
            .take()
            .or_else(|| find_string(value, &["conversation_id", "conversationId"]));
        self.message_id = self
            .message_id
            .take()
            .or_else(|| find_string(value, &["message_id", "messageId"]));
        self.parent_message_id = self
            .parent_message_id
            .take()
            .or_else(|| find_string(value, &["parent_message_id", "parentMessageId"]));
        self.response_id = self
            .response_id
            .take()
            .or_else(|| find_string(value, &["response_id", "responseId"]));
        self.model = self
            .model
            .take()
            .or_else(|| find_string(value, &["model", "model_slug", "modelSlug"]));
    }
}

/// Launch managed Chrome, wait for ChatGPT, and passively observe one message.
pub async fn run_chatgpt_test() -> anyhow::Result<()> {
    tracing::info!("Browser2Tokens ChatGPT Spike");

    let chrome = chrome_executable()?;
    let profile = profile_dir()?;
    tokio::fs::create_dir_all(&profile).await.with_context(|| {
        format!(
            "failed to create Chrome profile directory {}",
            profile.display()
        )
    })?;

    let config = chromiumoxide::browser::BrowserConfig::builder()
        .chrome_executable(&chrome)
        .user_data_dir(&profile)
        .with_head()
        .viewport(None)
        .respect_https_errors()
        .build()
        .map_err(|error| anyhow!("failed to build Chrome launch config: {error}"))?;

    let (mut browser, mut handler) = chromiumoxide::browser::Browser::launch(config)
        .await
        .context("failed to launch managed Chrome")?;

    let mut handler_task = tokio::spawn(async move {
        while let Some(event) = handler.next().await {
            if let Err(error) = event {
                tracing::warn!(%error, "CDP handler terminated unexpectedly");
                break;
            }
        }
    });

    tracing::info!("[chrome] connected");
    browser
        .new_page(CHATGPT_URL)
        .await
        .with_context(|| format!("failed to open {CHATGPT_URL}"))?;

    let page = tokio::select! {
        result = wait_for_chatgpt_page(&browser) => result?,
        join = &mut handler_task => return handler_join_error(join),
    };
    tracing::info!("[target] ChatGPT found");

    let result = observe_page(&page, &mut handler_task).await;

    if let Err(error) = browser.close().await {
        tracing::warn!(%error, "failed to close managed Chrome cleanly");
    }
    handler_task.abort();

    result
}

async fn observe_page(
    page: &Page,
    handler_task: &mut tokio::task::JoinHandle<()>,
) -> anyhow::Result<()> {
    let mut requests = page
        .event_listener::<EventRequestWillBeSent>()
        .await
        .context("failed to subscribe to Network.requestWillBeSent")?;
    let mut responses = page
        .event_listener::<EventResponseReceived>()
        .await
        .context("failed to subscribe to Network.responseReceived")?;
    let mut data_received = page
        .event_listener::<EventDataReceived>()
        .await
        .context("failed to subscribe to Network.dataReceived")?;
    let mut loading_finished = page
        .event_listener::<EventLoadingFinished>()
        .await
        .context("failed to subscribe to Network.loadingFinished")?;
    let mut websocket_created = page
        .event_listener::<EventWebSocketCreated>()
        .await
        .context("failed to subscribe to Network.webSocketCreated")?;
    let mut websocket_sent = page
        .event_listener::<EventWebSocketFrameSent>()
        .await
        .context("failed to subscribe to Network.webSocketFrameSent")?;
    let mut websocket_received = page
        .event_listener::<EventWebSocketFrameReceived>()
        .await
        .context("failed to subscribe to Network.webSocketFrameReceived")?;
    let mut websocket_closed = page
        .event_listener::<EventWebSocketClosed>()
        .await
        .context("failed to subscribe to Network.webSocketClosed")?;

    page.execute(
        network::EnableParams::builder()
            .max_total_buffer_size(MAX_NETWORK_BUFFER)
            .max_resource_buffer_size(MAX_NETWORK_BUFFER)
            .max_post_data_size(MAX_NETWORK_BUFFER)
            .build(),
    )
    .await
    .context("failed to enable CDP Network observation")?;

    tracing::info!("[network] observation enabled");
    tracing::info!("[action] In the ChatGPT browser tab, manually send:\n         {TARGET_PROMPT}");

    let mut transactions = HashMap::<String, ObservedRequest>::new();
    let outcome = tokio::time::timeout(OBSERVATION_TIMEOUT, async {
        loop {
            tokio::select! {
                event = requests.next() => {
                    let event = event.context("CDP disconnected while waiting for network request")?;
                    let body = observed_request_body(page, &event).await;
                    let observed = ObservedRequest::http(&event);
                    if observed.is_http_message_candidate(&body) {
                        let request_id = observed.request_id.clone();
                        tracing::info!(
                            request_id = %request_id,
                            method = %observed.method,
                            host = %observed.host,
                            path = %observed.path,
                            resource_type = ?observed.resource_type,
                            "[chatgpt] message request detected"
                        );
                        log_metadata(&observed);
                        transactions.insert(request_id, observed);
                    }
                }
                event = responses.next() => {
                    let event = event.context("CDP disconnected while waiting for response event")?;
                    handle_response(page, &mut transactions, &event).await?;
                }
                event = data_received.next() => {
                    let event = event.context("CDP disconnected while waiting for response data")?;
                    if let Some(transaction) = transactions.get_mut(event.request_id.as_ref())
                        && let Some(data) = &event.data
                    {
                        transaction.saw_incremental_data = true;
                        consume_stream_data(transaction, binary_as_str(data).as_bytes());
                    }
                }
                event = loading_finished.next() => {
                    let event = event.context("CDP disconnected while waiting for stream completion")?;
                    let request_id = event.request_id.as_ref();
                    let needs_body_fallback = transactions
                        .get(request_id)
                        .is_some_and(|transaction| transaction.assistant_text.is_empty());
                    if needs_body_fallback
                        && let Ok(body) = page
                            .execute(network::GetResponseBodyParams::new(event.request_id.clone()))
                            .await
                        && !body.base64_encoded
                        && let Some(transaction) = transactions.get_mut(request_id)
                    {
                        consume_stream_data(transaction, body.body.as_bytes());
                        parse_sse_buffer(transaction, true);
                    }
                    let completed = if let Some(transaction) = transactions.get_mut(request_id) {
                        transaction.finished = true;
                        parse_sse_buffer(transaction, true);
                        (!transaction.assistant_text.is_empty()).then_some(transaction.clone())
                    } else {
                        None
                    };
                    if let Some(transaction) = completed {
                        return Ok::<ObservedRequest, anyhow::Error>(transaction);
                    }
                    if transactions.contains_key(request_id) {
                        bail!("message request detected but response stream could not be parsed");
                    }
                }
                event = websocket_created.next() => {
                    let event = event.context("CDP disconnected while waiting for WebSocket traffic")?;
                    if is_chatgpt_host(url_host(&event.url)) {
                        transactions.entry(event.request_id.as_ref().to_owned()).or_insert_with(|| {
                            ObservedRequest::websocket(event.request_id.as_ref().to_owned(), &event.url)
                        });
                    }
                }
                event = websocket_sent.next() => {
                    let event = event.context("CDP disconnected while waiting for WebSocket request")?;
                    let payload = event.response.payload_data.as_str();
                    if payload.contains(TARGET_PROMPT) {
                        let transaction = transactions.entry(event.request_id.as_ref().to_owned()).or_insert_with(|| {
                            ObservedRequest::websocket(event.request_id.as_ref().to_owned(), "wss://chatgpt.com")
                        });
                        tracing::info!(request_id = %transaction.request_id, path = %transaction.path, "[chatgpt] WebSocket message request detected");
                        log_metadata(transaction);
                    }
                }
                event = websocket_received.next() => {
                    let event = event.context("CDP disconnected while waiting for WebSocket response")?;
                    if let Some(transaction) = transactions.get_mut(event.request_id.as_ref()) {
                        transaction.response_started = true;
                        if !transaction.reported_response_start {
                            tracing::info!("[stream] response started");
                            transaction.reported_response_start = true;
                        }
                        consume_stream_data(transaction, event.response.payload_data.as_bytes());
                        if transaction.finished {
                            return Ok::<ObservedRequest, anyhow::Error>(transaction.clone());
                        }
                    }
                }
                event = websocket_closed.next() => {
                    let event = event.context("CDP disconnected while waiting for WebSocket completion")?;
                    if let Some(transaction) = transactions.get_mut(event.request_id.as_ref()) {
                        transaction.finished = true;
                        if !transaction.assistant_text.is_empty() {
                            return Ok::<ObservedRequest, anyhow::Error>(transaction.clone());
                        }
                    }
                }
                join = &mut *handler_task => {
                    return Err::<ObservedRequest, anyhow::Error>(handler_join_error(join).unwrap_err());
                }
            }
        }
    }).await
    .map_err(|_| anyhow!("timed out waiting for ChatGPT request/response after {} seconds", OBSERVATION_TIMEOUT.as_secs()))??;

    report_result(&outcome);
    Ok(())
}

async fn handle_response(
    page: &Page,
    transactions: &mut HashMap<String, ObservedRequest>,
    event: &EventResponseReceived,
) -> anyhow::Result<()> {
    let Some(transaction) = transactions.get_mut(event.request_id.as_ref()) else {
        return Ok(());
    };

    transaction.response_started = true;
    transaction.response_status = Some(event.response.status);
    transaction.content_type = response_content_type(event.response.headers.inner());
    transaction.stream_kind = Some(stream_kind(
        transaction.content_type.as_deref(),
        &event.r#type,
    ));

    if !transaction.reported_response_start {
        tracing::info!("[stream] response started");
        transaction.reported_response_start = true;
    }
    tracing::info!(
        status = event.response.status,
        content_type = ?transaction.content_type,
        stream = transaction.stream_kind.map(ObservedStreamKind::label),
        "[chatgpt] response observed"
    );

    if transaction.stream_kind != Some(ObservedStreamKind::WebSocket) && !transaction.stream_enabled
    {
        match page
            .execute(network::StreamResourceContentParams::new(
                event.request_id.clone(),
            ))
            .await
        {
            Ok(result) => {
                transaction.stream_enabled = true;
                let buffered_data = binary_as_str(&result.buffered_data);
                if !buffered_data.is_empty() {
                    transaction.saw_incremental_data = true;
                    consume_stream_data(transaction, buffered_data.as_bytes());
                }
            }
            Err(error) => {
                tracing::debug!(%error, request_id = %transaction.request_id, "response content streaming unavailable; will use data events");
            }
        }
    }
    Ok(())
}

fn consume_stream_data(transaction: &mut ObservedRequest, bytes: &[u8]) {
    transaction
        .sse_buffer
        .push_str(&String::from_utf8_lossy(bytes));
    parse_sse_buffer(transaction, false);
}

fn parse_sse_buffer(transaction: &mut ObservedRequest, flush: bool) {
    loop {
        let boundary = transaction
            .sse_buffer
            .find("\n\n")
            .or_else(|| transaction.sse_buffer.find("\r\n\r\n"));
        let Some(boundary) = boundary else {
            break;
        };
        let separator_len = if transaction.sse_buffer[boundary..].starts_with("\r\n") {
            4
        } else {
            2
        };
        let event = transaction.sse_buffer[..boundary].to_owned();
        transaction.sse_buffer.drain(..boundary + separator_len);
        parse_stream_event(transaction, &event);
    }

    if flush && !transaction.sse_buffer.trim().is_empty() {
        let event = std::mem::take(&mut transaction.sse_buffer);
        parse_stream_event(transaction, &event);
    }
}

fn parse_stream_event(transaction: &mut ObservedRequest, event: &str) {
    let data = event
        .lines()
        .filter_map(|line| line.strip_prefix("data:"))
        .map(str::trim_start)
        .collect::<Vec<_>>()
        .join("\n");
    if data.is_empty() {
        if let Ok(value) = serde_json::from_str::<Value>(event) {
            parse_stream_value(transaction, value);
        }
        return;
    }
    if data == "[DONE]" {
        transaction.saw_done_marker = true;
        transaction.finished = true;
        return;
    }

    let Ok(value) = serde_json::from_str::<Value>(&data) else {
        return;
    };
    parse_stream_value(transaction, value);
}

fn parse_stream_value(transaction: &mut ObservedRequest, value: Value) {
    transaction.record_value_metadata(&value);
    if let Some(text) = extract_full_text(&value) {
        update_assistant_text(transaction, text);
    } else if let Some(delta) = extract_delta(&value) {
        transaction.saw_incremental_data = true;
        transaction.assistant_text.push_str(&delta);
        tracing::info!("[delta] {delta}");
    }
    if is_stream_complete(&value) {
        transaction.finished = true;
    }
}

fn update_assistant_text(transaction: &mut ObservedRequest, text: String) {
    if text == transaction.assistant_text {
        return;
    }
    if let Some(delta) = text.strip_prefix(&transaction.assistant_text) {
        if !delta.is_empty() {
            transaction.saw_incremental_data = true;
            tracing::info!("[delta] {delta}");
        }
    } else {
        transaction.saw_incremental_data = true;
    }
    transaction.assistant_text = text;
}

fn extract_full_text(value: &Value) -> Option<String> {
    let candidates = [
        value.pointer("/message/content/parts/0"),
        value.pointer("/message/content/text"),
        value.pointer("/response/message/content/parts/0"),
        value.pointer("/response/output_text"),
    ];
    candidates
        .into_iter()
        .flatten()
        .find_map(Value::as_str)
        .map(str::to_owned)
}

fn extract_delta(value: &Value) -> Option<String> {
    [
        value.pointer("/delta"),
        value.pointer("/delta/text"),
        value.pointer("/message/delta"),
        value.pointer("/message/content/delta"),
        value.pointer("/text"),
    ]
    .into_iter()
    .flatten()
    .find_map(Value::as_str)
    .map(str::to_owned)
}

fn is_stream_complete(value: &Value) -> bool {
    value.get("is_completion").and_then(Value::as_bool) == Some(true)
        || value.get("is_completion").and_then(Value::as_str) == Some("true")
        || value
            .get("type")
            .and_then(Value::as_str)
            .is_some_and(|kind| {
                let kind = kind.to_ascii_lowercase();
                kind.contains("complete") || kind.contains("done")
            })
        || value
            .get("event")
            .and_then(Value::as_str)
            .is_some_and(|kind| kind.to_ascii_lowercase().contains("complete"))
}

fn report_result(transaction: &ObservedRequest) {
    tracing::info!(
        method = %transaction.method,
        path = %transaction.path,
        stream = transaction.stream_kind.map(ObservedStreamKind::label).unwrap_or("unknown"),
        status = ?transaction.response_status,
        "[observed facts] request and response completed"
    );
    if let Some(id) = &transaction.conversation_id {
        tracing::info!(conversation_id = %id, "[chatgpt] conversation identifier");
    }
    if let Some(id) = &transaction.message_id {
        tracing::info!(message_id = %id, "[chatgpt] message identifier");
    }
    if let Some(id) = &transaction.parent_message_id {
        tracing::info!(parent_message_id = %id, "[chatgpt] parent message identifier");
    }
    if let Some(id) = &transaction.response_id {
        tracing::info!(response_id = %id, "[chatgpt] response identifier");
    }
    if let Some(model) = &transaction.model {
        tracing::info!(model = %model, "[chatgpt] model");
    }
    tracing::info!("[assistant] {}", transaction.assistant_text);
    tracing::info!("[stream] completed");
    if transaction.assistant_text == "B2T_TEST_OK" {
        tracing::info!("[result] ChatGPT network observation passed");
    } else {
        tracing::warn!(
            actual = %transaction.assistant_text,
            expected = "B2T_TEST_OK",
            "[result] ChatGPT response differed from expected text"
        );
    }
    tracing::info!(
        "[observed facts] request mechanism is {} and response stream is {}",
        if transaction.method == "CONNECT" {
            "WebSocket"
        } else {
            "fetch/XHR"
        },
        transaction
            .stream_kind
            .map(ObservedStreamKind::label)
            .unwrap_or("unknown")
    );
    tracing::info!(
        "[inferences] authenticated page context is the safest boundary for a future sending spike; no request replay was attempted"
    );
}

fn log_metadata(transaction: &ObservedRequest) {
    if let Some(id) = &transaction.conversation_id {
        tracing::info!(conversation_id = %id, "[chatgpt] conversation_id");
    }
    if let Some(id) = &transaction.message_id {
        tracing::info!(message_id = %id, "[chatgpt] message_id");
    }
    if let Some(model) = &transaction.model {
        tracing::info!(model = %model, "[chatgpt] model");
    }
}

async fn observed_request_body(page: &Page, event: &EventRequestWillBeSent) -> String {
    let body = event
        .request
        .post_data_entries
        .as_ref()
        .and_then(|entries| {
            let body = entries
                .iter()
                .filter_map(|entry| entry.bytes.as_ref().map(binary_as_str))
                .collect::<String>();
            (!body.is_empty()).then_some(body)
        });
    if let Some(body) = body {
        return body;
    }

    let should_inspect = event.request.method == "POST"
        && is_chatgpt_host(url_host(&event.request.url))
        && url_path(&event.request.url)
            .to_ascii_lowercase()
            .contains("conversation");
    if !should_inspect {
        return String::new();
    }

    page.execute(network::GetRequestPostDataParams::new(
        event.request_id.clone(),
    ))
    .await
    .map(|response| response.post_data.clone())
    .unwrap_or_default()
}

fn response_content_type(headers: &Value) -> Option<String> {
    headers.as_object().and_then(|headers| {
        headers.iter().find_map(|(name, value)| {
            (name.eq_ignore_ascii_case("content-type"))
                .then(|| value.as_str().map(str::to_owned))
                .flatten()
        })
    })
}

fn stream_kind(content_type: Option<&str>, resource_type: &ResourceType) -> ObservedStreamKind {
    if content_type.is_some_and(|value| value.to_ascii_lowercase().contains("text/event-stream")) {
        ObservedStreamKind::Sse
    } else if matches!(resource_type, ResourceType::Fetch | ResourceType::Xhr) {
        ObservedStreamKind::ChunkedFetch
    } else {
        ObservedStreamKind::Other
    }
}

fn find_string(value: &Value, keys: &[&str]) -> Option<String> {
    match value {
        Value::Object(object) => {
            for key in keys {
                if let Some(found) = object.get(*key).and_then(Value::as_str) {
                    return Some(found.to_owned());
                }
            }
            object.values().find_map(|value| find_string(value, keys))
        }
        Value::Array(values) => values.iter().find_map(|value| find_string(value, keys)),
        _ => None,
    }
}

fn url_host(url: &str) -> &str {
    url.split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(url)
        .split(['/', '?', '#'])
        .next()
        .unwrap_or("")
}

fn url_path(url: &str) -> String {
    let rest = url.split_once("://").map(|(_, rest)| rest).unwrap_or(url);
    let Some((_, path)) = rest.split_once('/') else {
        return "/".to_owned();
    };
    path.split(['?', '#'])
        .next()
        .filter(|path| !path.is_empty())
        .map(|path| format!("/{path}"))
        .unwrap_or_else(|| "/".to_owned())
}

fn binary_as_str(binary: &Binary) -> &str {
    <Binary as AsRef<str>>::as_ref(binary)
}

fn is_chatgpt_host(host: &str) -> bool {
    host == "chatgpt.com" || host.ends_with(".chatgpt.com")
}

fn handler_join_error(join: Result<(), tokio::task::JoinError>) -> anyhow::Result<()> {
    match join {
        Ok(()) => bail!("CDP handler terminated unexpectedly"),
        Err(error) => bail!("CDP handler terminated unexpectedly: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ObservedStreamKind, find_string, is_chatgpt_host, stream_kind, url_host, url_path,
    };
    use chromiumoxide::cdp::browser_protocol::network::ResourceType;
    use serde_json::json;

    #[test]
    fn safe_url_parts_exclude_query_and_fragment() {
        assert_eq!(
            url_host("https://chatgpt.com/backend-api/conversation?x=1"),
            "chatgpt.com"
        );
        assert_eq!(
            url_path("https://chatgpt.com/backend-api/conversation?x=1#fragment"),
            "/backend-api/conversation"
        );
    }

    #[test]
    fn chatgpt_host_filter_is_scoped() {
        assert!(is_chatgpt_host("chatgpt.com"));
        assert!(is_chatgpt_host("ab.chatgpt.com"));
        assert!(!is_chatgpt_host("chatgpt.com.evil.example"));
    }

    #[test]
    fn nested_metadata_is_extracted_without_dumping_payload() {
        let value = json!({"message": {"id": "m1"}, "conversation_id": "c1", "model": "m"});
        assert_eq!(
            find_string(&value, &["conversation_id"]),
            Some("c1".to_owned())
        );
        assert_eq!(
            find_string(&value, &["message_id", "messageId", "id"]),
            Some("m1".to_owned())
        );
    }

    #[test]
    fn stream_kind_uses_content_type_before_resource_type() {
        assert_eq!(
            stream_kind(Some("text/event-stream"), &ResourceType::Fetch),
            ObservedStreamKind::Sse
        );
        assert_eq!(
            stream_kind(Some("application/json"), &ResourceType::Xhr),
            ObservedStreamKind::ChunkedFetch
        );
    }
}
