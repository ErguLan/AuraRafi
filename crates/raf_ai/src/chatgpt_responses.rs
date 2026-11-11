//! Responses transport for a user-authorized plan, using the native tool loop.

use std::io::{BufRead, BufReader};
use serde_json::{json, Value};
use crate::openai_client::{FunctionCall, OpenAiConfig, OpenAiMessage, OpenAiTool, StreamDelta, ToolCall};

const ENDPOINT: &str = "https://api.openai.com/v1/responses";
const MAX_STREAM_LINE: usize = 8 * 1024 * 1024;

fn input_items(messages: &[OpenAiMessage]) -> (String, Vec<Value>) {
    let mut instructions = String::new();
    let mut input = Vec::new();
    for message in messages {
        if message.role == "system" {
            if let Some(text) = message.content.as_str() {
                if !instructions.is_empty() { instructions.push('\n'); }
                instructions.push_str(text);
            }
            continue;
        }
        if message.role == "tool" {
            if let Some(id) = &message.tool_call_id {
                input.push(json!({"type":"function_call_output", "call_id":id, "output":message.content.as_str().unwrap_or("")}));
            }
            continue;
        }
        if message.role == "assistant" {
            input.extend(message.reasoning_details.iter().filter(|item| item.get("type").and_then(Value::as_str) == Some("reasoning")).cloned());
        }
        let content = if let Some(parts) = message.content.as_array() {
            Value::Array(parts.iter().filter_map(|part| match part.get("type").and_then(Value::as_str) {
                Some("text") => Some(json!({"type":"input_text", "text":part.get("text")?.as_str()?})),
                Some("image_url") => Some(json!({"type":"input_image", "image_url":part.get("image_url")?.get("url")?.as_str()?})),
                _ => None,
            }).collect())
        } else { message.content.clone() };
        let has_content = content.as_str().is_none_or(|s| !s.trim().is_empty());
        if message.role != "assistant" || has_content {
            input.push(json!({"type":"message", "role":message.role, "content":content}));
        }
        if let Some(calls) = &message.tool_calls {
            input.extend(calls.iter().map(|call| json!({"type":"function_call", "namespace":"raf", "call_id":call.id,
                "name":call.function.name, "arguments":call.function.arguments})));
        }
    }
    (instructions, input)
}

fn request_body(config: &OpenAiConfig, messages: &[OpenAiMessage], tools: Option<&[OpenAiTool]>) -> Value {
    let (instructions, input) = input_items(messages);
    let mut body = json!({"model":config.model, "input":input, "store":false, "stream":true,
        "include":["reasoning.encrypted_content"]});
    if !instructions.is_empty() { body["instructions"] = instructions.into(); }
    if !config.reasoning_effort.is_empty() {
        body["reasoning"] = json!({"effort":config.reasoning_effort, "summary":"auto"});
    }
    if let Some(tools) = tools.filter(|tools| !tools.is_empty()) {
        let functions = tools.iter().map(|tool| json!({"type":"function", "name":tool.function.name,
            "description":tool.function.description, "parameters":tool.function.parameters, "strict":false})).collect::<Vec<_>>();
        body["tools"] = json!([{"type":"namespace", "name":"raf", "description":"Native project inspection and authoring tools.", "tools":functions}]);
    }
    body
}

pub(crate) fn chat<F: FnMut(StreamDelta)>(config: &OpenAiConfig, messages: &[OpenAiMessage], tools: Option<&[OpenAiTool]>, on_delta: &mut F) -> Result<OpenAiMessage, String> {
    let registration = config.chatgpt_registration.as_deref().ok_or("Connect a ChatGPT account in AI settings.")?;
    let token = crate::chatgpt_auth::access_token_for(registration)?;
    let agent = ureq::AgentBuilder::new().timeout(std::time::Duration::from_secs(600)).redirects(0).build();
    let response = agent.post(ENDPOINT).set("Authorization", &format!("Bearer {token}"))
        .set("Content-Type", "application/json").set("Accept", "text/event-stream")
        .send_json(request_body(config, messages, tools)).map_err(|error| match error {
            ureq::Error::Status(status, response) => {
                let body: Value = serde_json::from_reader(response.into_reader()).unwrap_or(Value::Null);
                let code = body.pointer("/error/code").and_then(Value::as_str).unwrap_or("account_request_failed");
                format!("ChatGPT plan request failed (HTTP {status}, {code}). Review account access and usage in settings.")
            },
            ureq::Error::Transport(_) => "ChatGPT plan service is unavailable; credentials were kept.".into(),
        })?;
    parse_stream(BufReader::new(response.into_reader()), on_delta)
}

fn parse_stream<R: BufRead, F: FnMut(StreamDelta)>(mut reader: R, on_delta: &mut F) -> Result<OpenAiMessage, String> {
    let mut line = String::new();
    let mut text = String::new();
    let mut reasoning = String::new();
    let mut terminal = None;
    loop {
        line.clear();
        let bytes = reader.read_line(&mut line).map_err(|e| format!("Could not read provider stream: {e}"))?;
        if bytes == 0 { break; }
        if line.len() > MAX_STREAM_LINE { return Err("Provider stream event exceeds its safe limit.".into()); }
        let Some(data) = line.strip_prefix("data:").map(str::trim) else { continue };
        if data.is_empty() || data == "[DONE]" { continue; }
        let event: Value = serde_json::from_str(data).map_err(|_| "Invalid provider stream event.")?;
        match event.get("type").and_then(Value::as_str).unwrap_or("") {
            "response.output_text.delta" => if let Some(delta) = event.get("delta").and_then(Value::as_str) {
                if text.len().saturating_add(delta.len()) > MAX_STREAM_LINE { return Err("Provider output exceeds its safe limit.".into()); }
                text.push_str(delta); on_delta(StreamDelta::Text(delta.to_owned()));
            },
            "response.reasoning_summary_text.delta" => if let Some(delta) = event.get("delta").and_then(Value::as_str) {
                if reasoning.len().saturating_add(delta.len()) > MAX_STREAM_LINE { return Err("Provider reasoning exceeds its safe limit.".into()); }
                reasoning.push_str(delta); on_delta(StreamDelta::Reasoning(delta.to_owned()));
            },
            "response.failed" | "response.incomplete" | "error" => {
                let code = event.pointer("/response/error/code").or_else(|| event.pointer("/error/code")).and_then(Value::as_str).unwrap_or("incomplete");
                return Err(format!("ChatGPT plan response did not complete ({code}). Inspect saved state before continuing."));
            },
            "response.completed" => { terminal = event.get("response").cloned(); break; },
            _ => {}
        }
    }
    let response = terminal.ok_or("ChatGPT plan stream ended before response.completed.")?;
    if response.get("status").and_then(Value::as_str).is_some_and(|status| status != "completed") {
        return Err("ChatGPT plan response was incomplete.".into());
    }
    let mut calls = Vec::new();
    let mut reasoning_items = Vec::new();
    if let Some(output) = response.get("output").and_then(Value::as_array) {
        for item in output {
            match item.get("type").and_then(Value::as_str) {
                Some("function_call") => {
                    if item.get("namespace").and_then(Value::as_str).is_some_and(|namespace| namespace != "raf") {
                        return Err("The provider requested a tool outside the native namespace.".into());
                    }
                    let id = item.get("call_id").and_then(Value::as_str).filter(|s| !s.is_empty()).ok_or("Provider tool call has no ID.")?;
                    let name = item.get("name").and_then(Value::as_str).filter(|s| !s.is_empty()).ok_or("Provider tool call has no name.")?;
                    let arguments = item.get("arguments").and_then(Value::as_str).ok_or("Provider tool call has no arguments.")?;
                    calls.push(ToolCall { id:id.into(), call_type:"function".into(), function:FunctionCall { name:name.into(), arguments:arguments.into() } });
                },
                Some("reasoning") => {
                    if reasoning.is_empty() {
                        if let Some(summary) = item.get("summary").and_then(Value::as_array) {
                            for part in summary { if let Some(value) = part.get("text").and_then(Value::as_str) { reasoning.push_str(value); } }
                        }
                    }
                    reasoning_items.push(item.clone());
                },
                Some("message") if text.is_empty() => {
                    if let Some(parts) = item.get("content").and_then(Value::as_array) {
                        for part in parts { if part.get("type").and_then(Value::as_str) == Some("output_text") {
                            if let Some(value) = part.get("text").and_then(Value::as_str) { text.push_str(value); }
                        }}
                    }
                },
                _ => {}
            }
        }
    }
    Ok(OpenAiMessage { role:"assistant".into(), content:Value::String(text), tool_calls:(!calls.is_empty()).then_some(calls),
        tool_call_id:None, reasoning_content:(!reasoning.is_empty()).then_some(reasoning), reasoning:None, reasoning_details:reasoning_items })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn plan_request_uses_stateless_responses_and_namespace() {
        let config = OpenAiConfig { chatgpt_plan:true, ..OpenAiConfig::default() };
        let message = OpenAiMessage { role:"system".into(), content:json!("system"), tool_calls:None, tool_call_id:None,
            reasoning_content:None, reasoning:None, reasoning_details:Vec::new() };
        let body = request_body(&config, &[message], None);
        assert_eq!(body["store"], false); assert_eq!(body["stream"], true);
        assert_eq!(body["instructions"], "system"); assert!(body.get("max_output_tokens").is_none());
    }
    #[test]
    fn plan_stream_requires_completion_before_accepting_calls() {
        let incomplete = b"data: {\"type\":\"response.output_text.delta\",\"delta\":\"hello\"}\n\n";
        assert!(parse_stream(&incomplete[..], &mut |_| {}).is_err());
        let done = b"data: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\",\"output\":[{\"type\":\"function_call\",\"namespace\":\"raf\",\"call_id\":\"call1\",\"name\":\"scene_verify\",\"arguments\":\"{}\"}]}}\n\n";
        let reply = parse_stream(&done[..], &mut |_| {}).unwrap();
        assert_eq!(reply.tool_calls.unwrap()[0].function.name, "scene_verify");
    }
}
