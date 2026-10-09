use serde_json::json;

use super::ToolSpec;
use crate::tasks;

pub fn spec() -> ToolSpec {
    ToolSpec {
        name: "task_list",
        description: "List the project's markdown tasks with their status.",
        parameters: json!({ "type": "object", "properties": {} }),
    }
}

pub fn run(_args: &str) -> String {
    tasks::format_list()
}
