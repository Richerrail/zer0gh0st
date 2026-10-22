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
    /// Image jointe, sous forme de data URL (`data:image/png;base64,…`).
    ///
    /// `serde(skip)` : elle n'est **jamais** écrite dans le JSONL de conversation
    /// (taille, et une image peut contenir des données sensibles). Elle n'existe
    /// que pour la requête en cours — la reprendre après `--resume` nécessite de
    /// refaire `/img`.
    #[serde(skip)]
    pub image: Option<String>,
    /// Sortie d'outil dépliée dans le fil ? (UI seule : jamais envoyée ni persistée)
    #[serde(skip)]
    pub expanded: bool,
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
        Self {
            role: "system".into(),
            content: Some(content.into()),
            tool_calls: None,
            tool_call_id: None,
            image: None,
            expanded: false,
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".into(),
            content: Some(content.into()),
            tool_calls: None,
            tool_call_id: None,
            image: None,
            expanded: false,
        }
    }

    pub fn assistant(content: String, calls: Vec<ToolCall>) -> Self {
        Self {
            role: "assistant".into(),
            content: if content.is_empty() {
                None
            } else {
                Some(content)
            },
            tool_calls: if calls.is_empty() { None } else { Some(calls) },
            tool_call_id: None,
            image: None,
            expanded: false,
        }
    }

    pub fn tool_result(id: impl Into<String>, content: String) -> Self {
        Self {
            role: "tool".into(),
            content: Some(content),
            tool_calls: None,
            tool_call_id: Some(id.into()),
            image: None,
            expanded: false,
        }
    }

    /// A display-only message that is never sent to the model.
    pub fn notice(content: impl Into<String>) -> Self {
        Self {
            role: "notice".into(),
            content: Some(content.into()),
            tool_calls: None,
            tool_call_id: None,
            image: None,
            expanded: false,
        }
    }

    /// JSON réellement envoyé au provider.
    ///
    /// Sans image : `content` est une **chaîne** (comme avant). Avec image :
    /// `content` devient un **tableau de parties** — `[{type:text}, {type:image_url}]`
    /// — le format attendu par l'API OpenAI-compatible pour la vision.
    pub fn to_wire_json(&self) -> serde_json::Value {
        let mut v = serde_json::json!({ "role": self.role });
        v["content"] = match (&self.content, &self.image) {
            (Some(text), Some(img)) => serde_json::json!([
                { "type": "text", "text": text },
                { "type": "image_url", "image_url": { "url": img } },
            ]),
            (Some(text), None) => serde_json::json!(text),
            (None, Some(img)) => serde_json::json!([
                { "type": "image_url", "image_url": { "url": img } },
            ]),
            (None, None) => serde_json::Value::Null,
        };
        if let Some(tc) = &self.tool_calls {
            v["tool_calls"] = serde_json::to_value(tc).unwrap_or(serde_json::Value::Null);
        }
        if let Some(id) = &self.tool_call_id {
            v["tool_call_id"] = serde_json::json!(id);
        }
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_json_without_image_is_a_plain_string() {
        let v = ChatMessage::user("salut").to_wire_json();
        assert_eq!(v["role"], "user");
        assert_eq!(v["content"], serde_json::json!("salut"));
    }

    #[test]
    fn wire_json_with_image_uses_content_parts() {
        let mut m = ChatMessage::user("décris");
        m.image = Some("data:image/png;base64,AAAA".to_string());
        let v = m.to_wire_json();
        let parts = v["content"]
            .as_array()
            .expect("content doit être un tableau");
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0]["type"], "text");
        assert_eq!(parts[0]["text"], "décris");
        assert_eq!(parts[1]["type"], "image_url");
        assert_eq!(parts[1]["image_url"]["url"], "data:image/png;base64,AAAA");
    }

    #[test]
    fn images_are_not_persisted_in_the_jsonl() {
        let mut m = ChatMessage::user("x");
        m.image = Some("data:image/png;base64,SENSIBLE".to_string());
        let s = serde_json::to_string(&m).unwrap();
        assert!(
            !s.contains("SENSIBLE"),
            "l'image ne doit pas être sérialisée dans l'historique"
        );
    }

    #[test]
    fn tool_turns_keep_null_content() {
        let m = ChatMessage::assistant(String::new(), vec![]);
        let v = m.to_wire_json();
        assert!(v["content"].is_null());
    }
}
