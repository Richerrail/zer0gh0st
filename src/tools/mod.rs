pub mod box0;
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
use std::future::Future;
use std::pin::Pin;
use std::sync::OnceLock;

/// Contexte d'exécution, accessible aux outils.
///
/// Pourquoi : l'outil `0box` doit pouvoir lancer le **vrai** débat multi-agents
/// (le même que `/0box` et `zer0 0box`), pas en simuler un. Il lui faut donc
/// les réglages, le prompt système, les options de tour et la liste d'outils.
pub struct Runtime {
    pub settings: crate::config::Settings,
    pub system: String,
    pub options: crate::llm::llama::ChatOptions,
    pub all_tools: Vec<ToolSpec>,
    pub read_write: bool,
    pub code_exec: bool,
}

static RUNTIME: OnceLock<Runtime> = OnceLock::new();

/// Initialise le contexte (au démarrage de l'agent). Idempotent : le premier
/// appel gagne.
pub fn init_runtime(rt: Runtime) {
    let _ = RUNTIME.set(rt);
}

/// Contexte d'exécution, s'il a été initialisé.
pub fn runtime() -> Option<&'static Runtime> {
    RUNTIME.get()
}

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
        box0::spec(),
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

/// Variante à effacement de type de `execute`.
///
/// Nécessaire parce que l'outil `0box` relance un débat → sous-agent → appel
/// d'outils → … : sans `dyn Future`, le compilateur doit calculer le type
/// concret de la future et tourne en rond (E0391 « cycle detected »).
pub fn execute_boxed(
    name: &str,
    args: &str,
) -> Pin<Box<dyn Future<Output = String> + Send>> {
    // On possède les données (la future doit être 'static).
    let name = name.to_string();
    let args = args.to_string();
    Box::pin(async move { execute(&name, &args).await })
}

/// Execute a tool by name. Errors are returned as text so the model can react.
pub async fn execute(name: &str, args: &str) -> String {
    match name {
        "0box" => box0::run(args).await,
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
