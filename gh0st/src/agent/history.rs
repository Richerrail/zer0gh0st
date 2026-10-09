use serde::{Deserialize, Serialize};

/// A single conversation message in the OpenAI-compatible wire format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    /// `None` serializes to JSON `null` (needed for assistant tool-call turns).
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub function: FunctionCall,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String,
}

impl ChatMessage {
    #[allow(dead_code)]
    pub fn system(content: impl Into<String>) -> Self {
        Self { role: "system".into(), content: Some(content.into()), tool_calls: None, tool_call_id: None }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self { role: "user".into(), content: Some(content.into()), tool_calls: None, tool_call_id: None }
    }

    pub fn assistant(content: String, calls: Vec<ToolCall>) -> Self {
        Self {
            role: "assistant".into(),
            content: if content.is_empty() { None } else { Some(content) },
            tool_calls: if calls.is_empty() { None } else { Some(calls) },
            tool_call_id: None,
        }
    }

    pub fn tool_result(id: impl Into<String>, content: String) -> Self {
        Self {
            role: "tool".into(),
            content: Some(content),
            tool_calls: None,
            tool_call_id: Some(id.into()),
        }
    }

    /// A display-only message that is never sent to the model.
    pub fn notice(content: impl Into<String>) -> Self {
        Self { role: "notice".into(), content: Some(content.into()), tool_calls: None, tool_call_id: None }
    }
}
