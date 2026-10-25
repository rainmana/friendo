//! Streaming wire adapters. Tool arguments become executable only after a complete reply.
use anyhow::{Result, bail, ensure};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::{
    collections::{BTreeMap, HashSet},
    time::Duration,
};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

const MAX_BODY: usize = 16 * 1024 * 1024;
const MAX_EVENT: usize = 1024 * 1024;
const MAX_CALLS: usize = 64;
const MAX_EVENTS: usize = 100_000;
const OPENAI_OPTIONS: &[&str] = &[
    "temperature",
    "top_p",
    "seed",
    "stop",
    "presence_penalty",
    "frequency_penalty",
    "reasoning_effort",
    "response_format",
    "parallel_tool_calls",
    "include_usage",
    "output_token_field",
];
const ANTHROPIC_OPTIONS: &[&str] = &[
    "temperature",
    "top_p",
    "top_k",
    "stop_sequences",
    "thinking",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ProviderConfig {
    pub protocol: String,
    pub endpoint: String,
    pub model: String,
    pub api_key_env: Option<String>,
    pub max_tokens: u32,
    pub options: Value,
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self {
            protocol: "openai".into(),
            endpoint: "http://127.0.0.1:8080/v1".into(),
            model: String::new(),
            api_key_env: None,
            max_tokens: 4096,
            options: json!({}),
        }
    }
}

impl ProviderConfig {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            matches!(self.protocol.as_str(), "openai" | "anthropic"),
            "provider protocol must be openai or anthropic"
        );
        ensure!(
            !self.model.trim().is_empty() && self.model.len() <= 512,
            "configure an explicit provider model ID"
        );
        ensure!(self.max_tokens > 0, "provider max_tokens must be positive");
        self.url()?;
        if let Some(name) = &self.api_key_env {
            ensure!(
                !name.is_empty()
                    && name.len() <= 256
                    && name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_'),
                "invalid credential environment reference"
            );
        }
        let options = self
            .options
            .as_object()
            .ok_or_else(|| anyhow::anyhow!("provider options must be an object"))?;
        let allowed = if self.protocol == "openai" {
            OPENAI_OPTIONS
        } else {
            ANTHROPIC_OPTIONS
        };
        ensure!(
            options.keys().all(|k| allowed.contains(&k.as_str())),
            "unsupported provider option; inspect provider capabilities for supported options"
        );
        for field in [
            "temperature",
            "top_p",
            "presence_penalty",
            "frequency_penalty",
        ] {
            if let Some(value) = options.get(field) {
                ensure!(
                    value.is_number(),
                    "numeric provider option has an invalid type"
                );
            }
        }
        for field in ["parallel_tool_calls", "include_usage"] {
            if let Some(value) = options.get(field) {
                ensure!(
                    value.is_boolean(),
                    "boolean provider option has an invalid type"
                );
            }
        }
        if let Some(field) = options.get("output_token_field") {
            ensure!(
                matches!(field.as_str(), Some("max_tokens" | "max_completion_tokens")),
                "output_token_field must be max_tokens or max_completion_tokens"
            );
        }
        if let Some(value) = options.get("response_format") {
            ensure!(value.is_object(), "response_format must be an object");
        }
        if let Some(value) = options.get("thinking") {
            ensure!(value.is_object(), "thinking must be an object");
        }
        Ok(())
    }

    fn url(&self) -> Result<reqwest::Url> {
        let mut url = reqwest::Url::parse(&self.endpoint)
            .map_err(|_| anyhow::anyhow!("invalid provider endpoint URL"))?;
        ensure!(
            matches!(url.scheme(), "http" | "https") && url.host_str().is_some(),
            "provider endpoint must be an HTTP or HTTPS URL"
        );
        ensure!(
            url.username().is_empty()
                && url.password().is_none()
                && url.query().is_none()
                && url.fragment().is_none(),
            "provider endpoint cannot contain credentials, query parameters, or fragments"
        );
        let path = url.path().trim_end_matches('/');
        let suffix = if self.protocol == "anthropic" {
            "/messages"
        } else {
            "/chat/completions"
        };
        let path = if path.ends_with(suffix) {
            path.to_owned()
        } else if path.is_empty() {
            format!("/v1{suffix}")
        } else {
            format!("{path}{suffix}")
        };
        url.set_path(&path);
        Ok(url)
    }

    /// Protocol support is separate from the configured model's (unprobed) support.
    pub fn capabilities(&self) -> Value {
        let anthropic = self.protocol == "anthropic";
        json!({
            "protocol": self.protocol, "streaming_text": true, "tools": true,
            "structured_output": !anthropic, "images": false,
            "reasoning_controls": true, "caching": false,
            "usage_reporting": "when supplied by endpoint", "continuation": true,
            "native_thinking_continuation": anthropic,
            "model_capabilities": "unknown; configured model and server must support requested options",
            "supported_options": if anthropic { ANTHROPIC_OPTIONS } else { OPENAI_OPTIONS },
            "unsupported": if anthropic { vec!["images", "structured_output", "cache_controls", "beta_headers"] }
                else { vec!["images", "cache_controls", "responses_api", "audio"] },
            "limits": {"request_bytes":MAX_BODY,"response_bytes":MAX_BODY,"event_bytes":MAX_EVENT,"tool_calls":MAX_CALLS,"total_timeout_seconds":600,"idle_timeout_seconds":90}
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderReply {
    pub text: String,
    pub tool_calls: Vec<ToolCall>,
    pub continuation: Value,
    pub usage: Option<Value>,
}

/// Build the exact non-secret outbound body used by the transport and context inspector.
pub fn request_body(config: &ProviderConfig, messages: &[Message], tools: &Value) -> Result<Value> {
    config.validate()?;
    ensure!(messages.len() <= 4096, "provider message limit exceeded");
    let tool_defs = tools
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("tools must be an array"))?;
    ensure!(
        tool_defs.len() <= MAX_CALLS,
        "provider tool definition limit exceeded"
    );
    let mut names = HashSet::new();
    for tool in tool_defs {
        ensure!(
            tool["type"] == "function" && tool["function"].is_object(),
            "only function tool definitions are supported"
        );
        let name = required_str(&tool["function"], "name")?;
        ensure!(
            valid_name(name) && names.insert(name),
            "invalid or duplicate tool definition name"
        );
        ensure!(
            tool["function"]["parameters"].is_object(),
            "tool parameters must be a JSON schema object"
        );
    }
    let anthropic = config.protocol == "anthropic";
    let mut body = json!({"model":config.model,"max_tokens":config.max_tokens,"stream":true});
    if anthropic {
        let (system, messages) = anthropic_messages(messages)?;
        body["messages"] = Value::Array(messages);
        if !system.is_empty() {
            body["system"] = Value::Array(system);
        }
        if !tool_defs.is_empty() {
            body["tools"] = Value::Array(
                tool_defs
                    .iter()
                    .map(|tool| {
                        let f = &tool["function"];
                        let mut native = json!({"name":f["name"],"input_schema":f["parameters"]});
                        if let Some(description) = f.get("description") {
                            native["description"] = description.clone();
                        }
                        native
                    })
                    .collect(),
            );
        }
    } else {
        body["messages"] =
            Value::Array(messages.iter().map(openai_message).collect::<Result<_>>()?);
        if !tool_defs.is_empty() {
            body["tools"] = tools.clone();
        }
    }
    for (key, value) in config.options.as_object().expect("validated options") {
        match key.as_str() {
            "include_usage" => {
                if value == true {
                    body["stream_options"] = json!({"include_usage":true});
                }
            }
            "output_token_field" => {
                if value == "max_completion_tokens" {
                    body.as_object_mut().expect("object").remove("max_tokens");
                    body["max_completion_tokens"] = json!(config.max_tokens);
                }
            }
            _ => body[key] = value.clone(),
        }
    }
    ensure!(
        serde_json::to_vec(&body)?.len() <= MAX_BODY,
        "provider request size limit exceeded"
    );
    Ok(body)
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && name
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
}

fn required_str<'a>(value: &'a Value, field: &str) -> Result<&'a str> {
    value
        .get(field)
        .and_then(Value::as_str)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| anyhow::anyhow!("provider message is missing a required string field"))
}

fn content_text(content: &Value) -> Result<&str> {
    if let Some(text) = content.as_str() {
        return Ok(text);
    }
    content
        .get("text")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("only text message content is supported"))
}

fn canonical_calls(content: &Value) -> Result<Vec<ToolCall>> {
    match content.get("tool_calls") {
        Some(value) => serde_json::from_value(value.clone())
            .map_err(|_| anyhow::anyhow!("invalid retained tool calls")),
        None => Ok(vec![]),
    }
}

fn openai_message(message: &Message) -> Result<Value> {
    match message.role.as_str() {
        "system" | "developer" | "user" => {
            Ok(json!({"role":message.role,"content":content_text(&message.content)?}))
        }
        "assistant" => {
            let text = content_text(&message.content)?;
            let calls = canonical_calls(&message.content)?;
            let mut value = json!({"role":"assistant","content":text});
            if !calls.is_empty() {
                value["tool_calls"] = json!(calls.iter().map(|c| json!({"id":c.id,"type":"function","function":{"name":c.name,"arguments":c.arguments.to_string()}})).collect::<Vec<_>>());
            }
            Ok(value)
        }
        "tool" => Ok(
            json!({"role":"tool","tool_call_id":required_str(&message.content,"tool_call_id")?,"content":message.content.get("content").and_then(Value::as_str).ok_or_else(||anyhow::anyhow!("tool result must contain text"))?}),
        ),
        _ => bail!("unsupported message role"),
    }
}

fn anthropic_messages(messages: &[Message]) -> Result<(Vec<Value>, Vec<Value>)> {
    let mut system = Vec::new();
    let mut out: Vec<Value> = Vec::new();
    for message in messages {
        let (role, blocks) = match message.role.as_str() {
            "system" | "developer" => {
                system.push(json!({"type":"text","text":content_text(&message.content)?}));
                continue;
            }
            "user" => (
                "user",
                vec![json!({"type":"text","text":content_text(&message.content)?})],
            ),
            "tool" => (
                "user",
                vec![
                    json!({"type":"tool_result","tool_use_id":required_str(&message.content,"tool_call_id")?,"content":message.content.get("content").and_then(Value::as_str).ok_or_else(||anyhow::anyhow!("tool result must contain text"))?}),
                ],
            ),
            "assistant" => {
                if let Some(native) = message
                    .content
                    .get("native")
                    .filter(|native| native["protocol"] == "anthropic")
                {
                    let blocks = native
                        .get("content")
                        .and_then(Value::as_array)
                        .ok_or_else(|| anyhow::anyhow!("invalid native assistant continuation"))?;
                    ("assistant", blocks.clone())
                } else {
                    let text = content_text(&message.content)?;
                    let mut blocks = vec![];
                    if !text.is_empty() {
                        blocks.push(json!({"type":"text","text":text}));
                    }
                    for call in canonical_calls(&message.content)? {
                        blocks.push(json!({"type":"tool_use","id":call.id,"name":call.name,"input":call.arguments}));
                    }
                    ensure!(!blocks.is_empty(), "empty assistant content");
                    ("assistant", blocks)
                }
            }
            _ => bail!("unsupported message role"),
        };
        // Anthropic parallel tool results must share a single following user message.
        if let Some(previous) = out.last_mut().filter(|previous| previous["role"] == role) {
            previous["content"]
                .as_array_mut()
                .expect("content array")
                .extend(blocks);
        } else {
            out.push(json!({"role":role,"content":blocks}));
        }
    }
    Ok((system, out))
}

pub async fn stream(
    config: &ProviderConfig,
    messages: &[Message],
    tools: &Value,
    cancel: CancellationToken,
    updates: mpsc::Sender<String>,
) -> Result<ProviderReply> {
    let body = request_body(config, messages, tools)?;
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(20))
        .timeout(Duration::from_secs(600))
        .build()
        .map_err(|_| anyhow::anyhow!("could not initialize provider transport"))?;
    let mut request = client
        .post(config.url()?)
        .header("accept", "text/event-stream");
    if config.protocol == "anthropic" {
        request = request.header("anthropic-version", "2023-06-01");
    }
    if let Some(name) = &config.api_key_env {
        let key = std::env::var(name).map_err(|_| {
            anyhow::anyhow!("provider credential environment reference is unset or invalid")
        })?;
        ensure!(
            !key.trim().is_empty(),
            "provider credential environment reference is empty"
        );
        // Never retain the resolved value in configuration, errors, or journalable payloads.
        let mut key = reqwest::header::HeaderValue::from_str(&key)
            .map_err(|_| anyhow::anyhow!("invalid provider credential value"))?;
        key.set_sensitive(true);
        if config.protocol == "anthropic" {
            request = request.header("x-api-key", key);
        } else {
            let mut authorization = reqwest::header::HeaderValue::from_str(&format!(
                "Bearer {}",
                key.to_str()
                    .map_err(|_| anyhow::anyhow!("invalid provider credential value"))?
            ))
            .map_err(|_| anyhow::anyhow!("invalid provider credential value"))?;
            authorization.set_sensitive(true);
            request = request.header(reqwest::header::AUTHORIZATION, authorization);
        }
    }
    let response = tokio::select! {
        biased;
        _ = cancel.cancelled() => bail!("provider request cancelled"),
        response = request.json(&body).send() => response.map_err(|_|anyhow::anyhow!("provider connection failed or timed out"))?,
    };
    ensure!(
        response.status().is_success(),
        "provider returned HTTP status {}",
        response.status().as_u16()
    );
    ensure!(
        response
            .content_length()
            .is_none_or(|length| length <= MAX_BODY as u64),
        "provider response size limit exceeded"
    );
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    ensure!(
        content_type
            .split(';')
            .next()
            .is_some_and(|value| value.trim().eq_ignore_ascii_case("text/event-stream")),
        "provider did not return a server-sent event stream"
    );
    let mut chunks = response.bytes_stream();
    let mut decoder = SseDecoder::default();
    let mut state = if config.protocol == "anthropic" {
        ReplyState::Anthropic(AnthropicState::default())
    } else {
        ReplyState::OpenAi(OpenAiState::default())
    };
    let deadline = tokio::time::Instant::now() + Duration::from_secs(600);
    let mut bytes = 0usize;
    let mut count = 0usize;
    loop {
        let chunk = tokio::select! {
            biased;
            _ = cancel.cancelled() => bail!("provider request cancelled"),
            _ = tokio::time::sleep_until(deadline) => bail!("provider response total timeout"),
            result = tokio::time::timeout(Duration::from_secs(90), chunks.next()) => result.map_err(|_|anyhow::anyhow!("provider stream idle timeout"))?,
        };
        let Some(chunk) = chunk else {
            break;
        };
        let chunk = chunk.map_err(|_| anyhow::anyhow!("provider stream transport failed"))?;
        bytes = bytes
            .checked_add(chunk.len())
            .ok_or_else(|| anyhow::anyhow!("provider response size limit exceeded"))?;
        ensure!(bytes <= MAX_BODY, "provider response size limit exceeded");
        for event in decoder.feed(&chunk)? {
            count += 1;
            ensure!(count <= MAX_EVENTS, "provider event count limit exceeded");
            if state.event(event, &updates)? {
                ensure!(!cancel.is_cancelled(), "provider request cancelled");
                let reply = state.finish()?;
                let definitions = tools.as_array().expect("validated tool definitions");
                ensure!(
                    reply.tool_calls.iter().all(|call| definitions
                        .iter()
                        .any(|definition| definition["function"]["name"] == call.name)),
                    "provider requested a tool that was not offered"
                );
                return Ok(reply);
            }
        }
    }
    bail!("provider stream ended before its completion boundary")
}

#[derive(Default)]
struct SseDecoder {
    line: Vec<u8>,
    data: String,
    event: String,
    bytes: usize,
}
struct SseEvent {
    name: String,
    data: String,
}
impl SseDecoder {
    fn feed(&mut self, chunk: &[u8]) -> Result<Vec<SseEvent>> {
        let mut events = Vec::new();
        for byte in chunk {
            self.bytes += 1;
            ensure!(
                self.bytes <= MAX_EVENT,
                "provider event size limit exceeded"
            );
            if *byte != b'\n' {
                self.line.push(*byte);
                continue;
            }
            let mut line = std::mem::take(&mut self.line);
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            let line = std::str::from_utf8(&line)
                .map_err(|_| anyhow::anyhow!("provider stream contained invalid UTF-8"))?;
            if line.is_empty() {
                if !self.data.is_empty() {
                    self.data.pop(); // Remove only the final SSE-added newline.
                    events.push(SseEvent {
                        name: std::mem::take(&mut self.event),
                        data: std::mem::take(&mut self.data),
                    });
                }
                self.event.clear();
                self.bytes = 0;
            } else if !line.starts_with(':') {
                let (field, value) = line.split_once(':').unwrap_or((line, ""));
                let value = value.strip_prefix(' ').unwrap_or(value);
                match field {
                    "data" => {
                        self.data.push_str(value);
                        self.data.push('\n');
                    }
                    "event" => self.event = value.to_owned(),
                    _ => {}
                }
            }
        }
        Ok(events)
    }
}

enum ReplyState {
    OpenAi(OpenAiState),
    Anthropic(AnthropicState),
}
impl ReplyState {
    fn event(&mut self, event: SseEvent, updates: &mpsc::Sender<String>) -> Result<bool> {
        match self {
            Self::OpenAi(state) => state.event(event, updates),
            Self::Anthropic(state) => state.event(event, updates),
        }
    }
    fn finish(self) -> Result<ProviderReply> {
        match self {
            Self::OpenAi(state) => state.finish(),
            Self::Anthropic(state) => state.finish(),
        }
    }
}

#[derive(Default)]
struct PendingCall {
    id: String,
    name: String,
    arguments: String,
}
#[derive(Default)]
struct OpenAiState {
    text: String,
    calls: BTreeMap<u64, PendingCall>,
    finish: Option<String>,
    usage: Option<Value>,
}
impl OpenAiState {
    fn event(&mut self, event: SseEvent, updates: &mpsc::Sender<String>) -> Result<bool> {
        if event.data == "[DONE]" {
            ensure!(
                self.finish.is_some(),
                "provider completion marker arrived without a finish reason"
            );
            return Ok(true);
        }
        let value: Value = serde_json::from_str(&event.data)
            .map_err(|_| anyhow::anyhow!("provider event contained malformed JSON"))?;
        ensure!(
            value.get("error").is_none() && event.name != "error",
            "provider reported a streaming error"
        );
        if let Some(usage) = value.get("usage").filter(|v| v.is_object()) {
            self.usage = Some(usage.clone());
        }
        let choices = value
            .get("choices")
            .and_then(Value::as_array)
            .ok_or_else(|| anyhow::anyhow!("provider event is missing choices"))?;
        ensure!(
            choices.len() <= 1,
            "multiple provider choices are unsupported"
        );
        for choice in choices {
            ensure!(
                choice["index"].as_u64() == Some(0),
                "unexpected provider choice index"
            );
            ensure!(
                self.finish.is_none(),
                "provider emitted content after finish reason"
            );
            let delta = choice
                .get("delta")
                .and_then(Value::as_object)
                .ok_or_else(|| anyhow::anyhow!("provider event is missing delta"))?;
            ensure!(
                !delta.contains_key("function_call"),
                "legacy function_call is unsupported; use tools"
            );
            if let Some(text) = delta.get("content").filter(|v| !v.is_null()) {
                let text = text
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("provider text delta is invalid"))?;
                self.text.push_str(text);
                let _ = updates.try_send(text.to_owned());
            }
            if let Some(calls) = delta.get("tool_calls") {
                for fragment in calls
                    .as_array()
                    .ok_or_else(|| anyhow::anyhow!("invalid provider tool-call delta"))?
                {
                    let index = fragment["index"]
                        .as_u64()
                        .ok_or_else(|| anyhow::anyhow!("provider tool-call index is missing"))?;
                    ensure!(
                        index < MAX_CALLS as u64,
                        "provider tool-call count limit exceeded"
                    );
                    if let Some(kind) = fragment.get("type") {
                        ensure!(kind == "function", "unsupported provider tool-call type");
                    }
                    let pending = self.calls.entry(index).or_default();
                    if let Some(id) = fragment.get("id") {
                        append_string(&mut pending.id, id)?;
                    }
                    if let Some(function) = fragment.get("function") {
                        ensure!(function.is_object(), "invalid provider tool-call function");
                        if let Some(name) = function.get("name") {
                            append_string(&mut pending.name, name)?;
                        }
                        if let Some(arguments) = function.get("arguments") {
                            append_string(&mut pending.arguments, arguments)?;
                        }
                    }
                }
            }
            if let Some(reason) = choice.get("finish_reason").filter(|v| !v.is_null()) {
                let reason = reason
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("invalid provider finish reason"))?;
                ensure!(
                    matches!(reason, "stop" | "tool_calls"),
                    "provider response was truncated, filtered, or stopped for an unsupported reason"
                );
                self.finish = Some(reason.to_owned());
            }
        }
        Ok(false)
    }

    fn finish(self) -> Result<ProviderReply> {
        let reason = self
            .finish
            .ok_or_else(|| anyhow::anyhow!("provider response is incomplete"))?;
        ensure!(
            (reason == "tool_calls") == !self.calls.is_empty(),
            "provider finish reason does not match tool calls"
        );
        let mut calls = Vec::new();
        for (expected, (index, pending)) in self.calls.into_iter().enumerate() {
            ensure!(
                index == expected as u64,
                "provider tool-call indexes are incomplete"
            );
            let arguments: Value = serde_json::from_str(&pending.arguments).map_err(|_| {
                anyhow::anyhow!("provider tool arguments are incomplete or malformed JSON")
            })?;
            calls.push(ToolCall {
                id: pending.id,
                name: pending.name,
                arguments,
            });
        }
        validate_calls(&calls)?;
        let continuation = json!({"protocol":"openai","content":self.text,"tool_calls":calls});
        Ok(ProviderReply {
            text: self.text,
            tool_calls: calls,
            continuation,
            usage: self.usage,
        })
    }
}

fn append_string(out: &mut String, value: &Value) -> Result<()> {
    out.push_str(
        value
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("provider stream string fragment is invalid"))?,
    );
    ensure!(out.len() <= MAX_EVENT, "provider field size limit exceeded");
    Ok(())
}

fn validate_calls(calls: &[ToolCall]) -> Result<()> {
    let mut ids = HashSet::new();
    ensure!(
        calls.len() <= MAX_CALLS,
        "provider tool-call count limit exceeded"
    );
    for call in calls {
        ensure!(
            !call.id.is_empty() && call.id.len() <= 512 && ids.insert(&call.id),
            "invalid or duplicate provider tool-call ID"
        );
        ensure!(valid_name(&call.name), "invalid provider tool name");
        ensure!(
            call.arguments.is_object(),
            "provider tool arguments must be a JSON object"
        );
    }
    Ok(())
}

struct NativeBlock {
    content: Value,
    arguments: String,
    stopped: bool,
}
#[derive(Default)]
struct AnthropicState {
    started: bool,
    blocks: BTreeMap<u64, NativeBlock>,
    text: String,
    stop_reason: Option<String>,
    usage: Map<String, Value>,
}
impl AnthropicState {
    fn event(&mut self, event: SseEvent, updates: &mpsc::Sender<String>) -> Result<bool> {
        let value: Value = serde_json::from_str(&event.data)
            .map_err(|_| anyhow::anyhow!("provider event contained malformed JSON"))?;
        let kind = value
            .get("type")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow::anyhow!("provider event type is missing"))?;
        ensure!(
            event.name.is_empty() || event.name == kind,
            "provider SSE event and data types disagree"
        );
        match kind {
            "error" => bail!("provider reported a streaming error"),
            "ping" => {}
            "message_start" => {
                ensure!(!self.started, "provider emitted duplicate message_start");
                ensure!(
                    value["message"]["role"] == "assistant"
                        && value["message"]["content"]
                            .as_array()
                            .is_some_and(Vec::is_empty),
                    "invalid native message_start"
                );
                self.started = true;
                self.merge_usage(&value["message"]["usage"]);
            }
            "content_block_start" => {
                ensure!(
                    self.started && self.stop_reason.is_none(),
                    "provider content started outside message boundary"
                );
                let index = event_index(&value)?;
                ensure!(
                    index < 256 && !self.blocks.contains_key(&index),
                    "invalid or duplicate provider content index"
                );
                let content = value["content_block"].clone();
                let kind = required_str(&content, "type")?;
                ensure!(
                    matches!(kind, "text" | "tool_use" | "thinking" | "redacted_thinking"),
                    "unsupported native assistant content block"
                );
                if kind == "text" {
                    let text = content["text"]
                        .as_str()
                        .ok_or_else(|| anyhow::anyhow!("invalid native text block"))?;
                    self.text.push_str(text);
                    if !text.is_empty() {
                        let _ = updates.try_send(text.to_owned());
                    }
                }
                self.blocks.insert(
                    index,
                    NativeBlock {
                        content,
                        arguments: String::new(),
                        stopped: false,
                    },
                );
            }
            "content_block_delta" => {
                ensure!(
                    self.stop_reason.is_none(),
                    "provider emitted content after stop reason"
                );
                let block = self
                    .blocks
                    .get_mut(&event_index(&value)?)
                    .filter(|b| !b.stopped)
                    .ok_or_else(|| {
                        anyhow::anyhow!("provider delta referenced an inactive content block")
                    })?;
                let delta = &value["delta"];
                match required_str(delta, "type")? {
                    "text_delta" => {
                        ensure!(
                            block.content["type"] == "text",
                            "provider delta type does not match content block"
                        );
                        let text = delta["text"]
                            .as_str()
                            .ok_or_else(|| anyhow::anyhow!("invalid text delta"))?;
                        append_value_string(&mut block.content, "text", &delta["text"])?;
                        self.text.push_str(text);
                        let _ = updates.try_send(text.to_owned());
                    }
                    "input_json_delta" => {
                        ensure!(
                            block.content["type"] == "tool_use",
                            "provider delta type does not match content block"
                        );
                        append_string(&mut block.arguments, &delta["partial_json"])?;
                    }
                    "thinking_delta" | "signature_delta" => {
                        ensure!(
                            block.content["type"] == "thinking",
                            "provider delta type does not match content block"
                        );
                        let field = if delta["type"] == "thinking_delta" {
                            "thinking"
                        } else {
                            "signature"
                        };
                        append_value_string(&mut block.content, field, &delta[field])?;
                    }
                    _ => bail!("unsupported native assistant content delta"),
                }
            }
            "content_block_stop" => {
                ensure!(
                    self.stop_reason.is_none(),
                    "provider block stopped after message stop reason"
                );
                let block = self
                    .blocks
                    .get_mut(&event_index(&value)?)
                    .filter(|b| !b.stopped)
                    .ok_or_else(|| anyhow::anyhow!("provider stopped an inactive content block"))?;
                if block.content["type"] == "tool_use" {
                    if !block.arguments.is_empty() {
                        block.content["input"] =
                            serde_json::from_str(&block.arguments).map_err(|_| {
                                anyhow::anyhow!(
                                    "provider tool arguments are incomplete or malformed JSON"
                                )
                            })?;
                    }
                    ensure!(
                        block.content["input"].is_object(),
                        "provider tool arguments must be a JSON object"
                    );
                }
                if block.content["type"] == "thinking" {
                    required_str(&block.content, "signature")?;
                }
                block.stopped = true;
            }
            "message_delta" => {
                ensure!(
                    self.started && self.blocks.values().all(|block| block.stopped),
                    "provider message delta arrived before content completion"
                );
                if let Some(reason) = value["delta"].get("stop_reason").filter(|v| !v.is_null()) {
                    let reason = reason
                        .as_str()
                        .ok_or_else(|| anyhow::anyhow!("invalid native stop reason"))?;
                    ensure!(
                        self.stop_reason.is_none()
                            && matches!(reason, "end_turn" | "stop_sequence" | "tool_use"),
                        "provider response was truncated or stopped for an unsupported reason"
                    );
                    self.stop_reason = Some(reason.to_owned());
                }
                self.merge_usage(&value["usage"]);
            }
            "message_stop" => {
                ensure!(
                    self.started
                        && self.stop_reason.is_some()
                        && self.blocks.values().all(|block| block.stopped),
                    "provider message stopped before completion"
                );
                return Ok(true);
            }
            // The protocol explicitly permits new non-content event types.
            _ => {}
        }
        Ok(false)
    }

    fn merge_usage(&mut self, value: &Value) {
        if let Some(values) = value.as_object() {
            self.usage
                .extend(values.iter().map(|(k, v)| (k.clone(), v.clone())));
        }
    }

    fn finish(self) -> Result<ProviderReply> {
        let mut content = Vec::new();
        let mut calls = Vec::new();
        for (expected, (index, block)) in self.blocks.into_iter().enumerate() {
            ensure!(
                index == expected as u64 && block.stopped,
                "provider content blocks are incomplete"
            );
            if block.content["type"] == "tool_use" {
                calls.push(ToolCall {
                    id: required_str(&block.content, "id")?.into(),
                    name: required_str(&block.content, "name")?.into(),
                    arguments: block.content["input"].clone(),
                });
            }
            content.push(block.content);
        }
        validate_calls(&calls)?;
        ensure!(
            (self.stop_reason.as_deref() == Some("tool_use")) == !calls.is_empty(),
            "provider stop reason does not match tool calls"
        );
        Ok(ProviderReply {
            text: self.text,
            tool_calls: calls,
            continuation: json!({"protocol":"anthropic","content":content}),
            usage: if self.usage.is_empty() {
                None
            } else {
                Some(Value::Object(self.usage))
            },
        })
    }
}

fn append_value_string(value: &mut Value, field: &str, fragment: &Value) -> Result<()> {
    let mut text = value[field].as_str().unwrap_or("").to_owned();
    append_string(&mut text, fragment)?;
    value[field] = Value::String(text);
    Ok(())
}
fn event_index(value: &Value) -> Result<u64> {
    value["index"]
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("provider content index is missing"))
}
