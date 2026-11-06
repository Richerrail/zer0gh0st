pub mod box0;
pub mod code_exec;
pub mod delegate;
pub mod knowledge_add;
pub mod knowledge_delete;
pub mod knowledge_search;
pub mod memory_delete;
pub mod memory_save;
pub mod memory_search;
pub mod memory_update;
pub mod read_image;
pub mod task_add;
pub mod task_delete;
pub mod task_list;
pub mod task_update;
pub mod text_editor;
pub mod web_read;
pub mod web_search;

use serde_json::json;
use std::future::Future;
use std::pin::Pin;
use std::sync::{OnceLock, RwLock};

#[derive(Clone)]
pub struct Runtime {
    pub settings: crate::config::Settings,
    pub system: String,
    pub options: crate::llm::llama::ChatOptions,
    pub all_tools: Vec<ToolSpec>,
    pub read_write: bool,
    pub code_exec: bool,
}

static RUNTIME: OnceLock<RwLock<Option<Runtime>>> = OnceLock::new();

fn cell() -> &'static RwLock<Option<Runtime>> {
    RUNTIME.get_or_init(|| RwLock::new(None))
}

pub fn set_runtime(rt: Runtime) {
    if let Ok(mut slot) = cell().write() {
        *slot = Some(rt);
    }
}

pub fn runtime() -> Option<Runtime> {
    cell().read().ok().and_then(|g| g.clone())
}

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

pub fn all() -> Vec<ToolSpec> {
    vec![
        box0::spec(),
        code_exec::spec(),
        delegate::spec(),
        text_editor::spec(),
        memory_save::spec(),
        memory_search::spec(),
        memory_update::spec(),
        memory_delete::spec(),
        read_image::spec(),
        knowledge_add::spec(),
        knowledge_delete::spec(),
        knowledge_search::spec(),
        task_add::spec(),
        task_delete::spec(),
        task_list::spec(),
        task_update::spec(),
        web_read::spec(),
        web_search::spec(),
    ]
}

pub fn execute_boxed(name: &str, args: &str) -> Pin<Box<dyn Future<Output = String> + Send>> {
    let name = name.to_string();
    let args = args.to_string();
    Box::pin(async move { execute(&name, &args).await })
}

pub async fn execute(name: &str, args: &str) -> String {
    let result = dispatch(name, args).await;
    crate::audit::record(name, args, &result);
    result
}

async fn dispatch(name: &str, args: &str) -> String {
    match name {
        "0box" => box0::run(args).await,
        "code_exec" => code_exec::run(args).await,
        "delegate" => delegate::run(args).await,
        "web_read" => web_read::run(args).await,
        "text_editor" => text_editor::run(args),
        "memory_save" => memory_save::run(args).await,
        "memory_search" => memory_search::run(args).await,
        "memory_update" => memory_update::run(args).await,
        "memory_delete" => memory_delete::run(args),
        "read_image" => read_image::run(args).await,
        "knowledge_add" => knowledge_add::run(args).await,
        "knowledge_delete" => knowledge_delete::run(args),
        "knowledge_search" => knowledge_search::run(args).await,
        "task_add" => task_add::run(args),
        "task_delete" => task_delete::run(args),
        "task_list" => task_list::run(args),
        "task_update" => task_update::run(args),
        "web_search" => web_search::run(args).await,
        other => format!("Erreur: outil inconnu `{other}`"),
    }
}
