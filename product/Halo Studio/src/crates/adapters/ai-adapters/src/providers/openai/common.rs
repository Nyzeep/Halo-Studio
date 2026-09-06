use crate::client::quirks::apply_openai_compatible_reasoning_fields;
use crate::client::utils::{dedupe_remote_models, normalize_base_url_for_discovery};
use crate::client::AIClient;
use crate::providers::shared;
use crate::types::{RemoteModelInfo, ToolDefinition};
use anyhow::Result;
use reqwest::RequestBuilder;
use serde::Deserialize;


#[derive(Debug, Deserialize)]
struct OpenAIModelsResponse {
    data: Vec<OpenAIModelEntry>,
}

#[derive(Debug, Deserialize)]
struct OpenAIModelEntry {
    id: String,
}

pub(crate) fn apply_headers(client: &AIClient, builder: RequestBuilder) -> RequestBuilder {
    shared::apply_header_policy(client, builder, |mut builder| {
        builder = builder
            .header("Content-Type", "application/json")
            .header("Authorization", format!("Bearer {}", client.config.api_key));

        if client.config.base_url.contains("openbitfun.com") {
            builder = builder.header("X-Verification-Code", "from_halo");
        }

        builder
    })
}

pub(crate) fn apply_reasoning_fields(
    request_body: &mut serde_json::Value,
    client: &AIClient,
    url: &str,
) {
    apply_openai_compatible_reasoning_fields(
        request_body,
        client.config.reasoning_mode,
        client.config.reasoning_effort.as_deref(),
        url,
        &client.config.model,
    );
}

pub(crate) fn resolve_models_url(client: &AIClient) -> String {
    let mut base = normalize_base_url_for_discovery(&client.config.base_url);

    for suffix in ["/chat/completions", "/responses", "/models"] {
        if base.ends_with(suffix) {
            base.truncate(base.len() - suffix.len());
            break;
        }
    }

    if base.is_empty() {
        return "models".to_string();
    }

    format!("{}/models", base)
}

pub(crate) async fn list_models(client: &AIClient) -> Result<Vec<RemoteModelInfo>> {
    let url = resolve_models_url(client);

    let response = apply_headers(client, client.client.get(&url))
        .send()
        .await?
        .error_for_status()?;

    let payload: OpenAIModelsResponse = response.json().await?;
    Ok(dedupe_remote_models(
        payload
            .data
            .into_iter()
            .map(|model| RemoteModelInfo {
                id: model.id,
                display_name: None,
            })
            .collect(),
    ))
}

pub(crate) fn extract_tool_name(tool: &serde_json::Value) -> String {
    tool.get("function")
        .and_then(|function| function.get("name"))
        .and_then(|name| name.as_str())
        .or_else(|| tool.get("name").and_then(|name| name.as_str()))
        .unwrap_or("unknown")
        .to_string()
}

pub(crate) fn attach_tools(
    request_body: &mut serde_json::Value,
    tools: Option<Vec<serde_json::Value>>,
    target: &str,
) {
    match tools {
        Some(tools) if !tools.is_empty() => {
            let tool_names = tools.iter().map(extract_tool_name).collect::<Vec<_>>();
            shared::log_tool_names(target, tool_names);
            request_body["tools"] = serde_json::Value::Array(tools);
            let has_tool_choice = request_body
                .get("tool_choice")
                .is_some_and(|value| !value.is_null());
            if !has_tool_choice {
                request_body["tool_choice"] = serde_json::Value::String("auto".to_string());
            }
        }
        _ => {
            if request_body
                .as_object_mut()
                .and_then(|object| object.remove("tool_choice"))
                .is_some()
            {
                log::debug!(
                    target: target,
                    "Removed tool_choice from OpenAI request because no tools are attached"
                );
            }
        }
    }
}

pub(crate) fn convert_tools_flat(
    tools: Option<Vec<ToolDefinition>>,
) -> Option<Vec<serde_json::Value>> {
    tools.map(|defs| {
        defs.into_iter()
            .map(|tool| {
                serde_json::json!({
                    "type": "function",
                    "name": tool.name,
                    "description": tool.description,
                    "parameters": tool.parameters,
                    "strict": false,
                })
            })
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::attach_tools;
    use serde_json::json;

    #[test]
    fn attach_tools_removes_tool_choice_without_tools() {
        let mut request_body = json!({
            "model": "test-model",
            "messages": [],
            "stream": true,
            "tool_choice": "none"
        });

        attach_tools(&mut request_body, None, "test");

        assert!(request_body.get("tools").is_none());
        assert!(request_body.get("tool_choice").is_none());
    }

    #[test]
    fn attach_tools_removes_tool_choice_for_empty_tools() {
        let mut request_body = json!({
            "model": "test-model",
            "messages": [],
            "stream": true,
            "tool_choice": "none"
        });

        attach_tools(&mut request_body, Some(vec![]), "test");

        assert!(request_body.get("tools").is_none());
        assert!(request_body.get("tool_choice").is_none());
    }

    #[test]
    fn attach_tools_preserves_explicit_tool_choice_with_tools() {
        let mut request_body = json!({
            "model": "test-model",
            "messages": [],
            "stream": true,
            "tool_choice": "none"
        });

        attach_tools(
            &mut request_body,
            Some(vec![json!({
                "type": "function",
                "function": {
                    "name": "example",
                    "description": "Example tool",
                    "parameters": { "type": "object" }
                }
            })]),
            "test",
        );

        assert_eq!(request_body["tool_choice"], json!("none"));
        assert_eq!(
            request_body["tools"][0]["function"]["name"],
            json!("example")
        );
    }
}
