//! Standalone Anthropic → OpenAI translation proxy.
//!
//! A lightweight proxy that accepts Anthropic Messages API requests and
//! translates them to OpenAI chat completions format, forwarding to a
//! configurable backend (e.g. Ollama). No auth, no DB, no Docker.
//!
//! Usage:
//!   cargo run --example anthropic_proxy
//!   cargo run --example anthropic_proxy -- --listen 0.0.0.0:8082 --backend http://10.24.0.200:11434
//!
//! Then point Claude Code at it:
//!   ANTHROPIC_BASE_URL=http://localhost:8082 claude

use std::net::SocketAddr;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::State;
use axum::http::{Response, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// CLI args (manual parsing to avoid extra deps)
// ---------------------------------------------------------------------------

struct Args {
    listen: SocketAddr,
    backend: String,
    default_model: String,
}

fn parse_args() -> Args {
    let args: Vec<String> = std::env::args().collect();
    let mut listen: SocketAddr = "0.0.0.0:8082".parse().unwrap();
    let mut backend = "http://10.24.0.200:11434".to_string();
    let mut default_model = "llama3.1:8b".to_string();

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--listen" | "-l" => {
                i += 1;
                listen = args[i].parse().expect("Invalid listen address");
            }
            "--backend" | "-b" => {
                i += 1;
                backend = args[i].clone();
            }
            "--model" | "-m" => {
                i += 1;
                default_model = args[i].clone();
            }
            "--help" | "-h" => {
                eprintln!(
                    "Usage: anthropic_proxy [OPTIONS]\n\n\
                     Options:\n  \
                       --listen, -l <ADDR>    Listen address (default: 0.0.0.0:8082)\n  \
                       --backend, -b <URL>    Backend URL (default: http://10.24.0.200:11434)\n  \
                       --model, -m <MODEL>    Default model name (default: llama3.1:8b)\n"
                );
                std::process::exit(0);
            }
            _ => {
                eprintln!("Unknown argument: {}", args[i]);
                std::process::exit(1);
            }
        }
        i += 1;
    }

    Args {
        listen,
        backend,
        default_model,
    }
}

// ---------------------------------------------------------------------------
// Shared state
// ---------------------------------------------------------------------------

struct ProxyState {
    client: reqwest::Client,
    backend_url: String,
    #[allow(dead_code)]
    default_model: String,
}

// ---------------------------------------------------------------------------
// Anthropic Messages API types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, Default)]
#[allow(dead_code)]
struct AnthropicMetadata {
    #[serde(default)]
    user_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AnthropicTool {
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    input_schema: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct AnthropicMessage {
    role: String,
    content: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct AnthropicRequest {
    model: String,
    max_tokens: u64,
    #[serde(default)]
    messages: Vec<AnthropicMessage>,
    #[serde(default)]
    system: Option<serde_json::Value>,
    #[serde(default)]
    stream: bool,
    #[serde(default)]
    temperature: Option<f64>,
    #[serde(default)]
    top_p: Option<f64>,
    #[serde(default)]
    #[allow(dead_code)]
    top_k: Option<u64>,
    #[serde(default)]
    stop_sequences: Option<Vec<String>>,
    #[serde(default)]
    #[allow(dead_code)]
    metadata: Option<AnthropicMetadata>,
    #[serde(default)]
    tools: Option<Vec<AnthropicTool>>,
}

#[derive(Debug, Serialize, Clone)]
struct AnthropicUsage {
    input_tokens: i64,
    output_tokens: i64,
}

#[derive(Debug, Serialize)]
struct AnthropicResponse {
    id: String,
    #[serde(rename = "type")]
    response_type: &'static str,
    role: &'static str,
    content: Vec<serde_json::Value>,
    model: String,
    stop_reason: Option<String>,
    stop_sequence: Option<serde_json::Value>,
    usage: AnthropicUsage,
}

#[derive(Debug, Serialize)]
struct AnthropicError {
    #[serde(rename = "type")]
    response_type: &'static str,
    error: AnthropicErrorDetail,
}

#[derive(Debug, Serialize)]
struct AnthropicErrorDetail {
    #[serde(rename = "type")]
    error_type: String,
    message: String,
}

// ---------------------------------------------------------------------------
// OpenAI types (for deserializing backend responses)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct OpenAIUsage {
    prompt_tokens: Option<i64>,
    completion_tokens: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct OpenAIToolCallFunction {
    name: Option<String>,
    arguments: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAIToolCall {
    id: Option<String>,
    function: Option<OpenAIToolCallFunction>,
}

#[derive(Debug, Deserialize)]
struct OpenAIMessage {
    content: Option<String>,
    tool_calls: Option<Vec<OpenAIToolCall>>,
}

#[derive(Debug, Deserialize)]
struct OpenAIChoice {
    message: Option<OpenAIMessage>,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAIResponse {
    choices: Option<Vec<OpenAIChoice>>,
    usage: Option<OpenAIUsage>,
}

#[derive(Debug, Deserialize)]
struct OpenAIStreamDelta {
    content: Option<String>,
    tool_calls: Option<Vec<OpenAIStreamToolCallDelta>>,
}

#[derive(Debug, Deserialize)]
struct OpenAIStreamToolCallDelta {
    index: Option<usize>,
    id: Option<String>,
    function: Option<OpenAIStreamToolCallFunction>,
}

#[derive(Debug, Deserialize)]
struct OpenAIStreamToolCallFunction {
    name: Option<String>,
    arguments: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAIStreamChoice {
    delta: Option<OpenAIStreamDelta>,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAIStreamChunk {
    choices: Option<Vec<OpenAIStreamChoice>>,
    usage: Option<OpenAIUsage>,
}

// ---------------------------------------------------------------------------
// OpenAI model list types
// ---------------------------------------------------------------------------

// (Model list types removed — we pass through the backend response directly)

// ---------------------------------------------------------------------------
// Translation helpers
// ---------------------------------------------------------------------------

fn system_to_string(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Array(blocks) => {
            let mut text = String::new();
            for block in blocks {
                if block.get("type").and_then(|t| t.as_str()) == Some("text") {
                    if let Some(t) = block.get("text").and_then(|t| t.as_str()) {
                        if !text.is_empty() {
                            text.push('\n');
                        }
                        text.push_str(t);
                    }
                }
            }
            text
        }
        _ => String::new(),
    }
}

fn content_to_string(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Array(blocks) => {
            let mut text = String::new();
            for block in blocks {
                if block.get("type").and_then(|t| t.as_str()) == Some("text") {
                    if let Some(t) = block.get("text").and_then(|t| t.as_str()) {
                        text.push_str(t);
                    }
                }
            }
            text
        }
        _ => String::new(),
    }
}

fn content_has_tool_use(value: &serde_json::Value) -> bool {
    if let serde_json::Value::Array(blocks) = value {
        blocks
            .iter()
            .any(|b| b.get("type").and_then(|t| t.as_str()) == Some("tool_use"))
    } else {
        false
    }
}

fn content_has_tool_result(value: &serde_json::Value) -> bool {
    if let serde_json::Value::Array(blocks) = value {
        blocks
            .iter()
            .any(|b| b.get("type").and_then(|t| t.as_str()) == Some("tool_result"))
    } else {
        false
    }
}

fn translate_assistant_tool_use(content: &serde_json::Value) -> serde_json::Value {
    let blocks = match content {
        serde_json::Value::Array(b) => b,
        _ => {
            return serde_json::json!({
                "role": "assistant",
                "content": content_to_string(content),
            });
        }
    };

    let mut text_parts = String::new();
    let mut tool_calls = Vec::new();

    for block in blocks {
        match block.get("type").and_then(|t| t.as_str()) {
            Some("text") => {
                if let Some(t) = block.get("text").and_then(|t| t.as_str()) {
                    text_parts.push_str(t);
                }
            }
            Some("tool_use") => {
                let id = block
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let name = block
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let input = block.get("input").cloned().unwrap_or(serde_json::json!({}));
                let arguments = serde_json::to_string(&input).unwrap_or_default();

                tool_calls.push(serde_json::json!({
                    "id": id,
                    "type": "function",
                    "function": {
                        "name": name,
                        "arguments": arguments,
                    }
                }));
            }
            _ => {}
        }
    }

    let mut msg = serde_json::json!({
        "role": "assistant",
    });

    if !text_parts.is_empty() {
        msg["content"] = serde_json::Value::String(text_parts);
    } else {
        msg["content"] = serde_json::Value::Null;
    }

    if !tool_calls.is_empty() {
        msg["tool_calls"] = serde_json::Value::Array(tool_calls);
    }

    msg
}

fn translate_user_tool_result(content: &serde_json::Value) -> Vec<serde_json::Value> {
    let blocks = match content {
        serde_json::Value::Array(b) => b,
        _ => {
            return vec![serde_json::json!({
                "role": "user",
                "content": content_to_string(content),
            })];
        }
    };

    let mut messages = Vec::new();
    let mut text_parts = String::new();

    for block in blocks {
        match block.get("type").and_then(|t| t.as_str()) {
            Some("tool_result") => {
                if !text_parts.is_empty() {
                    messages.push(serde_json::json!({
                        "role": "user",
                        "content": text_parts,
                    }));
                    text_parts = String::new();
                }

                let tool_call_id = block
                    .get("tool_use_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                let result_content = if let Some(content_val) = block.get("content") {
                    content_to_string(content_val)
                } else {
                    String::new()
                };

                messages.push(serde_json::json!({
                    "role": "tool",
                    "tool_call_id": tool_call_id,
                    "content": result_content,
                }));
            }
            Some("text") => {
                if let Some(t) = block.get("text").and_then(|t| t.as_str()) {
                    text_parts.push_str(t);
                }
            }
            _ => {}
        }
    }

    if !text_parts.is_empty() {
        messages.push(serde_json::json!({
            "role": "user",
            "content": text_parts,
        }));
    }

    messages
}

fn translate_request(req: &AnthropicRequest) -> serde_json::Value {
    let mut openai_messages: Vec<serde_json::Value> = Vec::new();

    if let Some(ref system) = req.system {
        let system_text = system_to_string(system);
        if !system_text.is_empty() {
            openai_messages.push(serde_json::json!({
                "role": "system",
                "content": system_text,
            }));
        }
    }

    for msg in &req.messages {
        if msg.role == "assistant" && content_has_tool_use(&msg.content) {
            openai_messages.push(translate_assistant_tool_use(&msg.content));
        } else if msg.role == "user" && content_has_tool_result(&msg.content) {
            openai_messages.extend(translate_user_tool_result(&msg.content));
        } else {
            openai_messages.push(serde_json::json!({
                "role": msg.role,
                "content": content_to_string(&msg.content),
            }));
        }
    }

    let mut body = serde_json::json!({
        "model": req.model,
        "max_tokens": req.max_tokens,
        "messages": openai_messages,
        "stream": req.stream,
    });

    if let Some(temp) = req.temperature {
        body["temperature"] = serde_json::json!(temp);
    }
    if let Some(top_p) = req.top_p {
        body["top_p"] = serde_json::json!(top_p);
    }
    if let Some(ref stop) = req.stop_sequences {
        body["stop"] = serde_json::json!(stop);
    }

    if let Some(ref tools) = req.tools {
        let openai_tools: Vec<serde_json::Value> = tools
            .iter()
            .map(|t| {
                let mut func = serde_json::json!({
                    "name": t.name,
                });
                if let Some(ref desc) = t.description {
                    func["description"] = serde_json::json!(desc);
                }
                if let Some(ref schema) = t.input_schema {
                    func["parameters"] = schema.clone();
                }
                serde_json::json!({
                    "type": "function",
                    "function": func,
                })
            })
            .collect();
        body["tools"] = serde_json::json!(openai_tools);
    }

    if req.stream {
        body["stream_options"] = serde_json::json!({"include_usage": true});
    }

    body
}

fn translate_stop_reason(finish_reason: Option<&str>) -> String {
    match finish_reason {
        Some("stop") => "end_turn".to_string(),
        Some("length") => "max_tokens".to_string(),
        Some("tool_calls") => "tool_use".to_string(),
        Some(other) => other.to_string(),
        None => "end_turn".to_string(),
    }
}

fn generate_message_id() -> String {
    let id = Uuid::new_v4().simple().to_string();
    format!("msg_{}", &id[..24])
}

fn generate_tool_use_id() -> String {
    let id = Uuid::new_v4().simple().to_string();
    format!("toolu_{}", &id[..24])
}

fn translate_openai_response(
    openai_resp: &OpenAIResponse,
    requested_model: &str,
) -> AnthropicResponse {
    let choice = openai_resp.choices.as_ref().and_then(|c| c.first());

    let finish_reason = choice.and_then(|c| c.finish_reason.as_deref());
    let message = choice.and_then(|c| c.message.as_ref());

    let mut content_blocks: Vec<serde_json::Value> = Vec::new();

    if let Some(msg) = message {
        if let Some(ref text) = msg.content {
            if !text.is_empty() {
                content_blocks.push(serde_json::json!({
                    "type": "text",
                    "text": text,
                }));
            }
        }

        if let Some(ref tool_calls) = msg.tool_calls {
            for tc in tool_calls {
                let id = tc.id.as_deref().unwrap_or("").to_string();
                let tool_id = if id.is_empty() {
                    generate_tool_use_id()
                } else {
                    id
                };

                let name = tc
                    .function
                    .as_ref()
                    .and_then(|f| f.name.clone())
                    .unwrap_or_default();
                let arguments_str = tc
                    .function
                    .as_ref()
                    .and_then(|f| f.arguments.clone())
                    .unwrap_or_else(|| "{}".to_string());
                let input: serde_json::Value =
                    serde_json::from_str(&arguments_str).unwrap_or(serde_json::json!({}));

                content_blocks.push(serde_json::json!({
                    "type": "tool_use",
                    "id": tool_id,
                    "name": name,
                    "input": input,
                }));
            }
        }
    }

    if content_blocks.is_empty() {
        content_blocks.push(serde_json::json!({
            "type": "text",
            "text": "",
        }));
    }

    let (input_tokens, output_tokens) = openai_resp
        .usage
        .as_ref()
        .map(|u| {
            (
                u.prompt_tokens.unwrap_or(0),
                u.completion_tokens.unwrap_or(0),
            )
        })
        .unwrap_or((0, 0));

    AnthropicResponse {
        id: generate_message_id(),
        response_type: "message",
        role: "assistant",
        content: content_blocks,
        model: requested_model.to_string(),
        stop_reason: Some(translate_stop_reason(finish_reason)),
        stop_sequence: None,
        usage: AnthropicUsage {
            input_tokens,
            output_tokens,
        },
    }
}

// ---------------------------------------------------------------------------
// Error helpers
// ---------------------------------------------------------------------------

fn error_response(status: StatusCode, error_type: &str, message: String) -> Response<Body> {
    let body = AnthropicError {
        response_type: "error",
        error: AnthropicErrorDetail {
            error_type: error_type.to_string(),
            message,
        },
    };
    (status, Json(body)).into_response()
}

// ---------------------------------------------------------------------------
// SSE streaming helpers
// ---------------------------------------------------------------------------

fn sse_event(event: &str, data: &serde_json::Value) -> String {
    format!("event: {}\ndata: {}\n\n", event, data)
}

#[derive(Debug, Clone)]
struct StreamingToolCall {
    index: usize,
    id: String,
    name: String,
    #[allow(dead_code)]
    arguments: String,
    block_index: usize,
    started: bool,
}

fn transform_stream(
    openai_stream: impl futures::Stream<Item = Result<Bytes, reqwest::Error>> + Send + 'static,
    model: String,
    msg_id: String,
) -> Body {
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Bytes, std::io::Error>>(32);

    tokio::spawn(async move {
        use futures::StreamExt;

        let mut text_block_started = false;
        let mut buffer = String::new();
        let mut final_stop_reason: Option<String> = None;
        let mut final_usage = (0i64, 0i64);
        let mut next_block_index: usize = 0;
        let mut text_block_index: Option<usize> = None;
        let mut tool_calls: Vec<StreamingToolCall> = Vec::new();

        macro_rules! send_event {
            ($event:expr, $data:expr) => {
                if tx
                    .send(Ok(Bytes::from(sse_event($event, $data))))
                    .await
                    .is_err()
                {
                    return;
                }
            };
        }

        // message_start
        let start_event = serde_json::json!({
            "type": "message_start",
            "message": {
                "id": msg_id,
                "type": "message",
                "role": "assistant",
                "content": [],
                "model": model,
                "stop_reason": null,
                "stop_sequence": null,
                "usage": {"input_tokens": 0, "output_tokens": 0}
            }
        });
        send_event!("message_start", &start_event);

        // ping
        let ping = serde_json::json!({"type": "ping"});
        send_event!("ping", &ping);

        let mut pinned_stream = std::pin::pin!(openai_stream);

        while let Some(chunk_result) = pinned_stream.next().await {
            let chunk_bytes = match chunk_result {
                Ok(b) => b,
                Err(e) => {
                    eprintln!("[stream] Error reading chunk: {e}");
                    break;
                }
            };

            let chunk_str = String::from_utf8_lossy(&chunk_bytes);
            buffer.push_str(&chunk_str);

            while let Some(newline_pos) = buffer.find('\n') {
                let line = buffer[..newline_pos].trim().to_string();
                buffer = buffer[newline_pos + 1..].to_string();

                if line.is_empty() || line == "data: [DONE]" {
                    continue;
                }

                let data_str = if let Some(stripped) = line.strip_prefix("data: ") {
                    stripped
                } else {
                    continue;
                };

                let chunk: OpenAIStreamChunk = match serde_json::from_str(data_str) {
                    Ok(c) => c,
                    Err(_) => continue,
                };

                if let Some(ref u) = chunk.usage {
                    final_usage = (
                        u.prompt_tokens.unwrap_or(final_usage.0),
                        u.completion_tokens.unwrap_or(final_usage.1),
                    );
                }

                if let Some(ref choices) = chunk.choices {
                    for choice in choices {
                        if let Some(ref reason) = choice.finish_reason {
                            final_stop_reason = Some(translate_stop_reason(Some(reason)));
                        }

                        if let Some(ref delta) = choice.delta {
                            // Text deltas
                            if let Some(ref text) = delta.content {
                                if !text.is_empty() {
                                    if !text_block_started {
                                        text_block_started = true;
                                        let idx = next_block_index;
                                        text_block_index = Some(idx);
                                        next_block_index += 1;

                                        let block_start = serde_json::json!({
                                            "type": "content_block_start",
                                            "index": idx,
                                            "content_block": {"type": "text", "text": ""}
                                        });
                                        send_event!("content_block_start", &block_start);
                                    }

                                    let idx = text_block_index.unwrap_or(0);
                                    let delta_event = serde_json::json!({
                                        "type": "content_block_delta",
                                        "index": idx,
                                        "delta": {"type": "text_delta", "text": text}
                                    });
                                    send_event!("content_block_delta", &delta_event);
                                }
                            }

                            // Tool call deltas
                            if let Some(ref tc_deltas) = delta.tool_calls {
                                for tc_delta in tc_deltas {
                                    let tc_index = tc_delta.index.unwrap_or(0);

                                    let tc = if let Some(existing) =
                                        tool_calls.iter_mut().find(|t| t.index == tc_index)
                                    {
                                        existing
                                    } else {
                                        if text_block_started && tool_calls.is_empty() {
                                            let idx = text_block_index.unwrap_or(0);
                                            let block_stop = serde_json::json!({
                                                "type": "content_block_stop",
                                                "index": idx,
                                            });
                                            send_event!("content_block_stop", &block_stop);
                                            text_block_started = false;
                                        }

                                        let block_idx = next_block_index;
                                        next_block_index += 1;

                                        tool_calls.push(StreamingToolCall {
                                            index: tc_index,
                                            id: String::new(),
                                            name: String::new(),
                                            arguments: String::new(),
                                            block_index: block_idx,
                                            started: false,
                                        });
                                        tool_calls.last_mut().unwrap()
                                    };

                                    if let Some(ref id) = tc_delta.id {
                                        tc.id = id.clone();
                                    }
                                    if let Some(ref func) = tc_delta.function {
                                        if let Some(ref name) = func.name {
                                            tc.name.push_str(name);
                                        }
                                        if let Some(ref args) = func.arguments {
                                            tc.arguments.push_str(args);
                                        }
                                    }

                                    if !tc.started && !tc.name.is_empty() {
                                        tc.started = true;
                                        let id = if tc.id.is_empty() {
                                            generate_tool_use_id()
                                        } else {
                                            tc.id.clone()
                                        };
                                        tc.id = id.clone();

                                        let block_start = serde_json::json!({
                                            "type": "content_block_start",
                                            "index": tc.block_index,
                                            "content_block": {
                                                "type": "tool_use",
                                                "id": id,
                                                "name": tc.name,
                                                "input": {},
                                            }
                                        });
                                        send_event!("content_block_start", &block_start);
                                    }

                                    if tc.started {
                                        if let Some(ref func) = tc_delta.function {
                                            if let Some(ref args) = func.arguments {
                                                if !args.is_empty() {
                                                    let delta_event = serde_json::json!({
                                                        "type": "content_block_delta",
                                                        "index": tc.block_index,
                                                        "delta": {
                                                            "type": "input_json_delta",
                                                            "partial_json": args,
                                                        }
                                                    });
                                                    send_event!(
                                                        "content_block_delta",
                                                        &delta_event
                                                    );
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Close text block if still open
        if text_block_started {
            let idx = text_block_index.unwrap_or(0);
            let block_stop = serde_json::json!({
                "type": "content_block_stop",
                "index": idx,
            });
            send_event!("content_block_stop", &block_stop);
        }

        // Close any open tool call blocks
        for tc in &tool_calls {
            if tc.started {
                let block_stop = serde_json::json!({
                    "type": "content_block_stop",
                    "index": tc.block_index,
                });
                send_event!("content_block_stop", &block_stop);
            }
        }

        // If no content blocks were emitted, emit an empty text block
        if !text_block_started && tool_calls.is_empty() {
            let block_start = serde_json::json!({
                "type": "content_block_start",
                "index": 0,
                "content_block": {"type": "text", "text": ""}
            });
            send_event!("content_block_start", &block_start);

            let block_stop = serde_json::json!({
                "type": "content_block_stop",
                "index": 0,
            });
            send_event!("content_block_stop", &block_stop);
        }

        let stop_reason = final_stop_reason.unwrap_or_else(|| "end_turn".to_string());

        // message_delta
        let msg_delta = serde_json::json!({
            "type": "message_delta",
            "delta": {
                "stop_reason": stop_reason,
                "stop_sequence": null,
            },
            "usage": {"output_tokens": final_usage.1}
        });
        send_event!("message_delta", &msg_delta);

        // message_stop
        let msg_stop = serde_json::json!({"type": "message_stop"});
        send_event!("message_stop", &msg_stop);
    });

    let body_stream = tokio_stream::wrappers::ReceiverStream::new(rx);
    Body::from_stream(body_stream)
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// POST /v1/messages — Anthropic Messages API
async fn messages_handler(State(state): State<Arc<ProxyState>>, body: Bytes) -> impl IntoResponse {
    let parsed: AnthropicRequest = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(e) => {
            return error_response(
                StatusCode::BAD_REQUEST,
                "invalid_request_error",
                format!("Invalid request body: {}", e),
            );
        }
    };

    let num_messages = parsed.messages.len();
    let num_tools = parsed.tools.as_ref().map(|t| t.len()).unwrap_or(0);
    let has_system = parsed.system.is_some();

    eprintln!(
        "[proxy] POST /v1/messages model={} stream={} max_tokens={} msgs={} tools={} system={}",
        parsed.model, parsed.stream, parsed.max_tokens, num_messages, num_tools, has_system
    );

    // Translate to OpenAI format, remapping model to backend model
    let mut openai_body = translate_request(&parsed);
    openai_body["model"] = serde_json::json!(&state.default_model);

    // Cap max_tokens to avoid overwhelming small models
    if let Some(max) = openai_body.get("max_tokens").and_then(|v| v.as_u64()) {
        if max > 4096 {
            openai_body["max_tokens"] = serde_json::json!(4096);
            eprintln!("[proxy] capped max_tokens {} → 4096", max);
        }
    }

    let openai_bytes = Bytes::from(serde_json::to_vec(&openai_body).unwrap());

    eprintln!(
        "[proxy] → backend model={} openai_body_bytes={} stream={}",
        state.default_model,
        openai_bytes.len(),
        parsed.stream
    );

    let backend_url = format!("{}/v1/chat/completions", state.backend_url);
    let requested_model = parsed.model.clone();
    let is_streaming = parsed.stream;

    if !is_streaming {
        // Non-streaming
        let resp = match state
            .client
            .post(&backend_url)
            .header("content-type", "application/json")
            .body(openai_bytes)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                eprintln!("[proxy] Backend error: {e}");
                return error_response(
                    StatusCode::BAD_GATEWAY,
                    "api_error",
                    format!("Backend unavailable: {}", e),
                );
            }
        };

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            eprintln!("[proxy] Backend returned {status}: {body}");
            return error_response(
                StatusCode::BAD_GATEWAY,
                "api_error",
                format!("Backend error ({}): {}", status, body),
            );
        }

        let body_bytes = match resp.bytes().await {
            Ok(b) => b,
            Err(e) => {
                return error_response(
                    StatusCode::BAD_GATEWAY,
                    "api_error",
                    format!("Failed to read backend response: {}", e),
                );
            }
        };

        match serde_json::from_slice::<OpenAIResponse>(&body_bytes) {
            Ok(openai_resp) => {
                let anthropic_resp = translate_openai_response(&openai_resp, &requested_model);
                (StatusCode::OK, Json(anthropic_resp)).into_response()
            }
            Err(e) => {
                eprintln!(
                    "[proxy] Failed to parse OpenAI response: {e}\n  body: {}",
                    String::from_utf8_lossy(&body_bytes)
                );
                error_response(
                    StatusCode::BAD_GATEWAY,
                    "api_error",
                    "Backend returned unparseable response".to_string(),
                )
            }
        }
    } else {
        // Streaming
        let resp = match state
            .client
            .post(&backend_url)
            .header("content-type", "application/json")
            .body(openai_bytes)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                eprintln!("[proxy] Backend error: {e}");
                return error_response(
                    StatusCode::BAD_GATEWAY,
                    "api_error",
                    format!("Backend unavailable: {}", e),
                );
            }
        };

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            eprintln!("[proxy] Backend returned {status}: {body}");
            return error_response(
                StatusCode::BAD_GATEWAY,
                "api_error",
                format!("Backend error ({}): {}", status, body),
            );
        }

        let msg_id = generate_message_id();
        let body = transform_stream(resp.bytes_stream(), requested_model, msg_id);

        Response::builder()
            .status(StatusCode::OK)
            .header("content-type", "text/event-stream")
            .header("cache-control", "no-cache")
            .header("connection", "keep-alive")
            .body(body)
            .unwrap()
    }
}

/// GET /v1/models — proxy to backend's model list (Claude Code may query this)
async fn models_handler(State(state): State<Arc<ProxyState>>) -> impl IntoResponse {
    let url = format!("{}/v1/models", state.backend_url);
    match state.client.get(&url).send().await {
        Ok(resp) => {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            Response::builder()
                .status(status.as_u16())
                .header("content-type", "application/json")
                .body(Body::from(body))
                .unwrap()
        }
        Err(e) => {
            let body = serde_json::json!({"error": format!("Backend error: {}", e)});
            (StatusCode::BAD_GATEWAY, Json(body)).into_response()
        }
    }
}

/// GET / — health check
async fn health() -> &'static str {
    "anthropic-proxy ok\n"
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() {
    let args = parse_args();

    eprintln!("╔══════════════════════════════════════════════════╗");
    eprintln!("║  Anthropic → OpenAI Translation Proxy           ║");
    eprintln!("╠══════════════════════════════════════════════════╣");
    eprintln!("║  Listen:  {:<39}║", args.listen);
    eprintln!("║  Backend: {:<39}║", args.backend);
    eprintln!("║  Model:   {:<39}║", args.default_model);
    eprintln!("╠══════════════════════════════════════════════════╣");
    eprintln!("║  Claude Code usage:                             ║");
    eprintln!("║  ANTHROPIC_BASE_URL=http://{}  ║", args.listen);
    eprintln!("║  claude --model {}            ║", args.default_model);
    eprintln!("╚══════════════════════════════════════════════════╝");

    let state = Arc::new(ProxyState {
        client: reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .unwrap(),
        backend_url: args.backend.trim_end_matches('/').to_string(),
        default_model: args.default_model,
    });

    let app = Router::new()
        .route("/", get(health))
        .route("/v1/messages", post(messages_handler))
        .route("/v1/models", get(models_handler))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(args.listen).await.unwrap();
    eprintln!("\nListening on {}...", args.listen);
    axum::serve(listener, app).await.unwrap();
}
