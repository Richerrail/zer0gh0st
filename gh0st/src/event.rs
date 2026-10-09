use crate::agent::history::ChatMessage;

/// Events sent from background agent turns back to the TUI.
#[derive(Debug, Clone)]
pub enum UiEvent {
    /// A streamed token of the assistant response.
    Token(String),
    /// A streamed token of the model's internal reasoning (thinking models).
    Reasoning(String),
    /// The model is requesting a tool call.
    ToolCall { name: String, args: String },
    /// A tool finished and produced this output.
    ToolResult(String),
    /// The whole turn finished; carries the FULL conversation.
    TurnComplete(Vec<ChatMessage>),
    /// The conversation history was compacted (summary + recent messages).
    Compacted(Vec<ChatMessage>),
    /// The endpoint actually used (after any failover).
    Active(String),
    /// Something went wrong during the turn.
    Error(String),
    /// A local informational message.
    #[allow(dead_code)]
    Info(String),
    /// Zero-chan a transcrit une commande vocale.
    Voice(String),
}
