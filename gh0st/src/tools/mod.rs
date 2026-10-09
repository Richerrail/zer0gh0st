pub mod code_exec;
pub mod knowledge_search;
pub mod memory_save;
pub mod memory_search;
pub mod task_add;
pub mod task_list;
pub mod task_update;
pub mod text_editor;
pub mod web_search;

use serde_json::json;

/// A tool definition advertised to the model in OpenAI function format.
#[derive(Debug, Clone)]
pub struct ToolSpec {
    pub name: &'static str,
    pub description: &'static str,
    pub parameters: serde_json::Value,
}

impl ToolSpec {
    pub fn to_openai(&self) -> serde_json::Value {
        json!({
            "type": "function",
            "function": {
                "name": self.name,
                "description": self.description,
                "parameters": self.parameters,
            }
        })
    }
}

/// All tools known to Zer0, regardless of runtime toggles.
pub fn all() -> Vec<ToolSpec> {
    vec![
        code_exec::spec(),
        text_editor::spec(),
        memory_save::spec(),
        memory_search::spec(),
        knowledge_search::spec(),
        task_add::spec(),
        task_list::spec(),
        task_update::spec(),
        web_search::spec(),
    ]
}

/// Execute a tool by name. Errors are returned as text so the model can react.
pub async fn execute(name: &str, args: &str) -> String {
    match name {
        "code_exec" => code_exec::run(args),
        "text_editor" => text_editor::run(args),
        "memory_save" => memory_save::run(args).await,
        "memory_search" => memory_search::run(args).await,
        "knowledge_search" => knowledge_search::run(args).await,
        "task_add" => task_add::run(args),
        "task_list" => task_list::run(args),
        "task_update" => task_update::run(args),
        "web_search" => web_search::run(args).await,
        other => format!("Erreur: outil inconnu `{other}`"),
    }
}
