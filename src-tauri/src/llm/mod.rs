pub mod jobs;

use crate::error::{AppError, AppResult};
use serde::Deserialize;
use serde_json::{json, Value};
use std::error::Error as StdError;
use std::sync::OnceLock;
use std::time::Duration;

const TOOL_NAME: &str = "emit_analysis";
const TIMEOUT: Duration = Duration::from_secs(180);
const BODY_LIMIT: usize = 800;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LlmProvider {
    OpenAi,
    Anthropic,
}

impl LlmProvider {
    pub fn parse(value: &str) -> AppResult<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "openai" => Ok(Self::OpenAi),
            "anthropic" => Ok(Self::Anthropic),
            _ => Err(AppError::msg(format!(
                "unsupported model provider: {value}"
            ))),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::OpenAi => "openai",
            Self::Anthropic => "anthropic",
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmCompleteRequest {
    pub provider: String,
    pub api_key: String,
    pub model: String,
    pub base_url: String,
    pub system: String,
    pub human: String,
    pub schema: Value,
}

pub fn endpoint(provider: LlmProvider, base_url: &str) -> String {
    let trimmed = base_url.trim().trim_end_matches('/');
    match provider {
        LlmProvider::OpenAi => {
            let base = if trimmed.is_empty() {
                "https://api.openai.com/v1"
            } else {
                trimmed
            };
            join_path(base, "chat/completions")
        }
        LlmProvider::Anthropic => {
            let base = if trimmed.is_empty() {
                "https://api.anthropic.com"
            } else {
                trimmed
            };
            if base.ends_with("/messages") {
                base.to_string()
            } else if base.ends_with("/v1") {
                format!("{base}/messages")
            } else {
                join_path(base, "v1/messages")
            }
        }
    }
}

pub fn request_body(provider: LlmProvider, request: &LlmCompleteRequest) -> Value {
    let schema = strip_schema(&request.schema);
    match provider {
        LlmProvider::OpenAi => json!({
            "model": request.model.trim(),
            "temperature": 0,
            "messages": [
                {"role": "system", "content": request.system},
                {"role": "user", "content": request.human}
            ],
            "tools": [{
                "type": "function",
                "function": {
                    "name": TOOL_NAME,
                    "description": "Return the analysis result as structured JSON.",
                    "parameters": schema
                }
            }],
            "tool_choice": {"type": "function", "function": {"name": TOOL_NAME}}
        }),
        LlmProvider::Anthropic => json!({
            "model": request.model.trim(),
            "max_tokens": 8192,
            "temperature": 0,
            "system": request.system,
            "messages": [{"role": "user", "content": request.human}],
            "tools": [{
                "name": TOOL_NAME,
                "description": "Return the analysis result as structured JSON.",
                "input_schema": schema
            }],
            "tool_choice": {"type": "tool", "name": TOOL_NAME}
        }),
    }
}

pub async fn complete(request: LlmCompleteRequest) -> AppResult<Value> {
    let provider = LlmProvider::parse(&request.provider)?;
    let api_key = request.api_key.trim();
    if api_key.is_empty() {
        return Err(AppError::msg("model API key is empty"));
    }
    if request.model.trim().is_empty() {
        return Err(AppError::msg("model name is empty"));
    }
    if !request.schema.is_object() {
        return Err(AppError::msg("model response schema must be a JSON object"));
    }

    let url = endpoint(provider, &request.base_url);
    let parsed = reqwest::Url::parse(&url)
        .map_err(|error| AppError::msg(format!("invalid model API URL: {error}")))?;
    log::info!(
        "llm request provider={} model={} host={}",
        provider.as_str(),
        request.model.trim(),
        parsed.host_str().unwrap_or("")
    );

    let body = request_body(provider, &request);
    let builder = http_client().post(parsed).json(&body);
    let builder = match provider {
        LlmProvider::OpenAi => builder.bearer_auth(api_key),
        LlmProvider::Anthropic => builder
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01"),
    };

    let response = builder.send().await.map_err(transport_error)?;
    let status = response.status();
    let request_id = header_value(response.headers(), "x-request-id")
        .or_else(|| header_value(response.headers(), "request-id"));
    let text = response.text().await.map_err(transport_error)?;
    if !status.is_success() {
        return Err(AppError::msg(http_failure(
            status.as_u16(),
            request_id.as_deref(),
            &text,
        )));
    }

    let payload: Value = serde_json::from_str(&text).map_err(|error| {
        AppError::msg(format!(
            "model returned invalid JSON: {error}\n{}",
            snippet(&text, BODY_LIMIT)
        ))
    })?;
    extract_result(provider, &payload).map_err(|error| match request_id {
        Some(id) if !error.to_string().contains("request-id") => {
            AppError::msg(format!("{error}\nrequest-id {id}"))
        }
        _ => error,
    })
}

pub fn extract_result(provider: LlmProvider, payload: &Value) -> AppResult<Value> {
    if let Some(message) = payload
        .get("error")
        .and_then(|error| error.get("message"))
        .and_then(Value::as_str)
    {
        let trimmed = message.trim();
        if !trimmed.is_empty() {
            return Err(AppError::msg(snippet(trimmed, BODY_LIMIT)));
        }
    }

    match provider {
        LlmProvider::OpenAi => extract_openai(payload),
        LlmProvider::Anthropic => extract_anthropic(payload),
    }
}

fn extract_openai(payload: &Value) -> AppResult<Value> {
    let message = payload.pointer("/choices/0/message").ok_or_else(|| {
        AppError::msg(format!(
            "model response has no message\n{}",
            compact(payload)
        ))
    })?;
    if let Some(refusal) = message.get("refusal").and_then(Value::as_str) {
        let trimmed = refusal.trim();
        if !trimmed.is_empty() {
            return Err(AppError::msg(snippet(trimmed, BODY_LIMIT)));
        }
    }
    if let Some(value) = tool_arguments(message.get("tool_calls")) {
        return parse_json_value(value);
    }
    if let Some(value) = message.pointer("/function_call/arguments") {
        return parse_json_value(value);
    }
    if let Some(text) = message_text(message.get("content")) {
        return parse_json_text(&text);
    }
    Err(AppError::msg(format!(
        "model response did not include a structured result\n{}",
        compact(payload)
    )))
}

fn extract_anthropic(payload: &Value) -> AppResult<Value> {
    let Some(blocks) = payload.get("content").and_then(Value::as_array) else {
        if let Some(text) = message_text(payload.get("content")) {
            return parse_json_text(&text);
        }
        return Err(AppError::msg(format!(
            "model response has no content\n{}",
            compact(payload)
        )));
    };
    if let Some(input) = blocks.iter().find_map(|block| {
        if block.get("type").and_then(Value::as_str) == Some("tool_use") {
            block.get("input")
        } else {
            None
        }
    }) {
        return parse_json_value(input);
    }
    let text = blocks
        .iter()
        .filter_map(|block| block.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("\n");
    if text.trim().is_empty() {
        return Err(AppError::msg(format!(
            "model response did not include a structured result\n{}",
            compact(payload)
        )));
    }
    parse_json_text(&text)
}

fn tool_arguments(calls: Option<&Value>) -> Option<&Value> {
    let calls = calls?.as_array()?;
    let matched = calls
        .iter()
        .find(|call| call.pointer("/function/name").and_then(Value::as_str) == Some(TOOL_NAME));
    let call = matched.or_else(|| calls.first())?;
    call.pointer("/function/arguments")
}

fn parse_json_value(value: &Value) -> AppResult<Value> {
    match value {
        Value::String(text) => parse_json_text(text),
        other => Ok(other.clone()),
    }
}

fn parse_json_text(text: &str) -> AppResult<Value> {
    let cleaned = strip_fence(text);
    serde_json::from_str(cleaned).map_err(|error| {
        AppError::msg(format!(
            "model result is not JSON: {error}\n{}",
            snippet(cleaned, BODY_LIMIT)
        ))
    })
}

fn message_text(content: Option<&Value>) -> Option<String> {
    match content? {
        Value::String(text) => Some(text.clone()),
        Value::Array(parts) => {
            let text = parts
                .iter()
                .filter_map(|part| {
                    part.get("text")
                        .and_then(Value::as_str)
                        .or_else(|| part.as_str())
                })
                .collect::<Vec<_>>()
                .join("\n");
            if text.trim().is_empty() {
                None
            } else {
                Some(text)
            }
        }
        _ => None,
    }
}

fn strip_schema(schema: &Value) -> Value {
    let mut copy = schema.clone();
    if let Some(object) = copy.as_object_mut() {
        object.remove("$schema");
    }
    copy
}

fn join_path(base: &str, path: &str) -> String {
    let base = base.trim_end_matches('/');
    if base.ends_with(&format!("/{path}")) {
        base.to_string()
    } else {
        format!("{base}/{path}")
    }
}

fn strip_fence(text: &str) -> &str {
    let trimmed = text.trim().trim_start_matches('\u{feff}').trim();
    let Some(rest) = trimmed.strip_prefix("```") else {
        return trimmed;
    };
    let rest = rest.trim_start();
    let rest = rest.strip_prefix("json").unwrap_or(rest);
    let rest = rest.trim_start_matches(['\r', '\n']);
    rest.trim_end().trim_end_matches("```").trim()
}

fn http_client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(TIMEOUT)
            .connect_timeout(Duration::from_secs(20))
            .build()
            .expect("failed to build model HTTP client")
    })
}

fn transport_error(error: reqwest::Error) -> AppError {
    if error.is_timeout() {
        return AppError::msg(format!("model request timed out\n{}", chain(&error)));
    }
    AppError::msg(format!("model request failed\n{}", chain(&error)))
}

fn http_failure(status: u16, request_id: Option<&str>, body: &str) -> String {
    let mut parts = vec![format!("HTTP {status}")];
    if let Some(id) = request_id.map(str::trim).filter(|id| !id.is_empty()) {
        parts.push(format!("request-id {id}"));
    }
    let text = snippet(body, BODY_LIMIT);
    if !text.is_empty() {
        parts.push(text);
    }
    parts.join("\n")
}

fn header_value(headers: &reqwest::header::HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn compact(value: &Value) -> String {
    snippet(&value.to_string(), BODY_LIMIT)
}

fn snippet(text: &str, limit: usize) -> String {
    let trimmed = text.trim();
    if trimmed.chars().count() <= limit {
        return trimmed.to_string();
    }
    let end = trimmed
        .char_indices()
        .nth(limit)
        .map(|(index, _)| index)
        .unwrap_or(trimmed.len());
    format!("{}…", &trimmed[..end])
}

fn chain(error: &dyn StdError) -> String {
    let mut message = error.to_string();
    let mut current = error.source();
    while let Some(source) = current {
        let text = source.to_string();
        if !text.is_empty() && !message.contains(&text) {
            message.push('\n');
            message.push_str(&text);
        }
        current = source.source();
    }
    message
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(provider: &str) -> LlmCompleteRequest {
        LlmCompleteRequest {
            provider: provider.to_string(),
            api_key: "secret".to_string(),
            model: "demo".to_string(),
            base_url: String::new(),
            system: "system".to_string(),
            human: "human".to_string(),
            schema: json!({"type": "object", "properties": {"ok": {"type": "boolean"}}}),
        }
    }

    #[test]
    fn builds_official_and_proxy_urls() {
        assert_eq!(
            endpoint(LlmProvider::OpenAi, ""),
            "https://api.openai.com/v1/chat/completions"
        );
        assert_eq!(
            endpoint(LlmProvider::OpenAi, "https://proxy.example/v1/"),
            "https://proxy.example/v1/chat/completions"
        );
        assert_eq!(
            endpoint(
                LlmProvider::OpenAi,
                "https://proxy.example/v1/chat/completions"
            ),
            "https://proxy.example/v1/chat/completions"
        );
        assert_eq!(
            endpoint(LlmProvider::Anthropic, "https://api.anthropic.com"),
            "https://api.anthropic.com/v1/messages"
        );
        assert_eq!(
            endpoint(LlmProvider::Anthropic, "https://proxy.example/v1"),
            "https://proxy.example/v1/messages"
        );
    }

    #[test]
    fn request_body_uses_forced_tool_and_omits_the_key() {
        let body = request_body(LlmProvider::OpenAi, &request("openai"));
        let text = body.to_string();
        assert!(text.contains("emit_analysis"));
        assert!(!text.contains("secret"));
        assert!(body["messages"][0]["content"].as_str() == Some("system"));
        assert!(body["tools"][0]["function"]["parameters"]["properties"]["ok"].is_object());
    }

    #[test]
    fn reads_openai_tool_arguments_and_fenced_content() {
        let tool = json!({
            "choices": [{
                "message": {
                    "tool_calls": [{
                        "function": {
                            "name": "emit_analysis",
                            "arguments": "{\"comments\":[]}"
                        }
                    }]
                }
            }]
        });
        assert_eq!(
            extract_result(LlmProvider::OpenAi, &tool).expect("tool"),
            json!({"comments": []})
        );

        let fenced = json!({
            "choices": [{
                "message": {"content": "```json\n{\"relations\":[]}\n```"}
            }]
        });
        assert_eq!(
            extract_result(LlmProvider::OpenAi, &fenced).expect("fence"),
            json!({"relations": []})
        );
    }

    #[test]
    fn reads_anthropic_tool_input() {
        let payload = json!({
            "content": [
                {"type": "text", "text": "done"},
                {"type": "tool_use", "name": "emit_analysis", "input": {"comments": [{"id": "o1"}]}}
            ]
        });
        assert_eq!(
            extract_result(LlmProvider::Anthropic, &payload).expect("tool"),
            json!({"comments": [{"id": "o1"}]})
        );
    }

    #[test]
    fn http_failure_keeps_status_and_request_id() {
        let message = http_failure(
            422,
            Some("req_123"),
            "{\"error\":{\"message\":\"bad schema\"}}",
        );
        assert!(message.contains("HTTP 422"));
        assert!(message.contains("request-id req_123"));
        assert!(message.contains("bad schema"));
    }
}
