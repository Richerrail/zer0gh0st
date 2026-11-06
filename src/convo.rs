use anyhow::Result;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

use crate::agent::history::ChatMessage;
use crate::config;

pub fn path() -> PathBuf {
    config::data_dir().join("conversation.jsonl")
}

pub fn load(max: usize) -> Vec<ChatMessage> {
    load_from_path(&path(), max)
}

pub fn load_repaired(max: usize) -> Vec<ChatMessage> {
    let msgs = load_from_path(&path(), 0);
    take_last(repair(msgs), max)
}

pub fn rewrite(msgs: &[ChatMessage]) -> Result<()> {
    let p = path();
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = p.with_extension("jsonl.tmp");
    {
        let mut f = std::fs::File::create(&tmp)?;
        for m in msgs {
            writeln!(f, "{}", serde_json::to_string(m)?)?;
        }
        f.sync_all()?;
    }
    std::fs::rename(&tmp, &p)?;
    Ok(())
}

pub fn append(msg: &ChatMessage) -> Result<()> {
    let p = path();
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut f = OpenOptions::new().create(true).append(true).open(&p)?;
    writeln!(f, "{}", serde_json::to_string(msg)?)?;
    Ok(())
}

pub fn load_from_path(p: &std::path::Path, max: usize) -> Vec<ChatMessage> {
    let Ok(s) = std::fs::read_to_string(p) else {
        return Vec::new();
    };
    let all: Vec<ChatMessage> = s
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();
    take_last(all, max)
}

fn take_last(msgs: Vec<ChatMessage>, max: usize) -> Vec<ChatMessage> {
    if max == 0 || msgs.len() <= max {
        msgs
    } else {
        msgs[msgs.len() - max..].to_vec()
    }
}

pub fn repair(msgs: Vec<ChatMessage>) -> Vec<ChatMessage> {
    let mut out: Vec<ChatMessage> = Vec::with_capacity(msgs.len());
    let mut pending: Vec<String> = Vec::new();

    for m in msgs {
        match m.role.as_str() {
            "tool" => {
                let Some(id) = m.tool_call_id.as_ref() else {
                    continue;
                };
                if pending.iter().any(|p| p == id) {
                    pending.retain(|p| p != id);
                    out.push(m);
                }
            }
            "assistant" => {
                pending.clear();
                if let Some(calls) = &m.tool_calls {
                    pending = calls.iter().map(|c| c.id.clone()).collect();
                }
                out.push(m);
            }
            _ => {
                pending.clear();
                out.push(m);
            }
        }
    }

    while matches!(out.last().map(|m| m.role.as_str()), Some("assistant")) {
        let has_calls = out.last().and_then(|m| m.tool_calls.as_ref()).is_some();
        if has_calls {
            out.pop();
        } else {
            break;
        }
    }

    if let Some(first_user) = out.iter().position(|m| m.role == "user") {
        out.drain(..first_user);
    } else {
        out.clear();
    }

    out.retain(|m| m.role != "notice");
    out
}

pub fn clear() -> Result<()> {
    let p = path();
    if p.exists() {
        let msgs = load(0);
        if !msgs.is_empty() {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let archive = p.with_file_name(format!("conversation_{now}.jsonl"));
            let _ = std::fs::rename(&p, &archive);
        } else {
            let _ = std::fs::remove_file(&p);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::history::{FunctionCall, ToolCall};

    fn user(t: &str) -> ChatMessage {
        ChatMessage::user(t)
    }

    fn assistant_calls() -> ChatMessage {
        ChatMessage::assistant(
            String::new(),
            vec![ToolCall {
                id: "call_1".into(),
                kind: "function".into(),
                function: FunctionCall {
                    name: "code_exec".into(),
                    arguments: "{}".into(),
                },
            }],
        )
    }

    #[test]
    fn repair_drops_orphan_tool_result() {
        let msgs = vec![
            ChatMessage::tool_result("call_1", "sortie".into()),
            user("bonjour"),
        ];
        let out = repair(msgs);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].role, "user");
    }

    #[test]
    fn repair_keeps_a_complete_exchange() {
        let msgs = vec![
            user("fais-le"),
            assistant_calls(),
            ChatMessage::tool_result("call_1", "ok".into()),
            ChatMessage::assistant("c'est fait".into(), vec![]),
        ];
        let out = repair(msgs);
        assert_eq!(out.len(), 4);
        assert_eq!(out[0].role, "user");
    }

    #[test]
    fn repair_drops_assistant_with_unanswered_calls() {
        let msgs = vec![user("fais-le"), assistant_calls()];
        let out = repair(msgs);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].role, "user");
    }

    #[test]
    fn repair_starts_on_user_even_after_truncation() {
        let msgs = vec![
            ChatMessage::assistant("réponse précédente".into(), vec![]),
            user("nouvelle question"),
        ];
        let out = repair(msgs);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].role, "user");
    }

    #[test]
    fn repair_without_user_is_empty() {
        let msgs = vec![ChatMessage::assistant("seul".into(), vec![])];
        assert!(repair(msgs).is_empty());
    }

    #[test]
    fn take_last_bounds_the_window() {
        let msgs: Vec<_> = (0..10).map(|i| user(&format!("m{i}"))).collect();
        assert_eq!(take_last(msgs.clone(), 3).len(), 3);
        assert_eq!(take_last(msgs.clone(), 0).len(), 10);
        assert_eq!(take_last(msgs, 50).len(), 10);
    }

    #[test]
    fn rewrite_roundtrips_through_disk() {
        let dir = std::env::temp_dir().join(format!("zer0-convo-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("c.jsonl");
        let msgs = vec![user("un"), ChatMessage::assistant("deux".into(), vec![])];
        let mut s = String::new();
        for m in &msgs {
            s.push_str(&serde_json::to_string(m).unwrap());
            s.push('\n');
        }
        std::fs::write(&p, s).unwrap();
        let back = load_from_path(&p, 0);
        assert_eq!(back.len(), 2);
        assert_eq!(back[1].content.as_deref(), Some("deux"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
