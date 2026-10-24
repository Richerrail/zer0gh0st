//! Tests d'intégration du noyau.
//!
//! Ils n'existaient pas avant l'extraction de `src/lib.rs` : le projet était un
//! binaire monolithique, donc rien n'était importable depuis `tests/`. C'est le
//! point de départ du filet de sécurité (boucle d'agent, persistance, outils).

use zer0::agent::history::ChatMessage;
use zer0::convo::repair;
use zer0::util::{floor_char_boundary, truncate_utf8};

fn call(id: &str) -> ChatMessage {
    ChatMessage::assistant(
        String::new(),
        vec![zer0::agent::history::ToolCall {
            id: id.to_string(),
            kind: "function".into(),
            function: zer0::agent::history::FunctionCall {
                name: "code_exec".into(),
                arguments: "{}".into(),
            },
        }],
    )
}

#[test]
fn a_truncated_session_can_be_replayed() {
    // JSONL coupé au milieu d'un échange d'outils : c'est ce que produisait
    // `--resume` avec `load(200)`, et les providers répondaient 400.
    let msgs = vec![
        ChatMessage::tool_result("call_9", "orphelin".into()),
        ChatMessage::user("écris le fichier"),
        call("call_1"),
        ChatMessage::tool_result("call_1", "ok".into()),
        ChatMessage::assistant("fait".into(), vec![]),
        call("call_2"),
    ];
    let fixed = repair(msgs);
    assert_eq!(fixed.first().map(|m| m.role.as_str()), Some("user"));
    assert_eq!(fixed.last().map(|m| m.role.as_str()), Some("assistant"));
    assert!(fixed
        .iter()
        .all(|m| m.role != "tool" || m.tool_call_id.as_deref() == Some("call_1")));
}

#[test]
fn binary_tool_output_is_truncated_without_panicking() {
    let mut s = String::from("a");
    s.push_str(&"é".repeat(20_000));
    assert!(truncate_utf8(&mut s, 16_000));
    assert!(s.len() <= 16_000);
    assert!(s.chars().all(|c| c == 'a' || c == 'é'));
}

#[test]
fn boundaries_never_split_a_character() {
    let s = "🦀é🦀é";
    for n in 0..=s.len() {
        assert!(s.is_char_boundary(floor_char_boundary(s, n)));
    }
}

#[test]
fn task_classification_is_stable() {
    use zer0::select::{classify, Category};
    assert_eq!(classify("corrige ce bug rust"), Category::Code);
    assert_eq!(
        classify("explique et compare ces deux options"),
        Category::Reasoning
    );
}
