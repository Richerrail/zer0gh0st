use crate::agent::history::ChatMessage;

#[derive(Debug, Clone)]
pub enum UiEvent {
    Token(String),
    Reasoning(String),
    ToolCall {
        name: String,
        args: String,
    },
    ToolResult(String),
    TurnComplete(Vec<ChatMessage>),
    Compacted(Vec<ChatMessage>),
    Active(String),
    Error(String),
    #[allow(dead_code)]
    Info(String),
    TaskDue(String),
    Voice(String),
    VoteState {
        stars: Option<u64>,
        starred: Option<bool>,
    },
    ContextWindow(u64),
    ModelList(Vec<crate::models::ModelEntry>),
    WizardModels(Vec<crate::models::ModelEntry>),
    FreeModels(Vec<crate::models::ModelEntry>),
    TelegramShutdown,
}
