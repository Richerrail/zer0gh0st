//! Module: 0box — débat multi-agents (3 perspectives + MAIN qui synthétise).
//!
//! L'outil `0box` est **réel** : il lance exactement le même débat que
//! `/0box` (TUI) et `zer0 0box` (CLI), via `tui::run_0box_debate`, et renvoie
//! au modèle la transcription du débat.
//!
//! Avant, cet outil renvoyait une chaîne prefabricée (« 3 agents confinés ont
//! débattu ») alors qu'aucun débat n'avait lieu : le modèle croyait à un
//! résultat qui n'existait pas. C'est exactement le genre de mensonge que la
//! philosophie d'honnêteté du projet refuse.

use super::ToolSpec;
use serde_json::json;

/// Perspectives par défaut : `code` → `reasoning` → `main` (MAIN toujours en
/// dernier : il synthétise et exécute).
pub const DEFAULT_PERSPECTIVES: [&str; 3] = ["code", "reasoning", "main"];

/// Taille maximale de la transcription renvoyée au modèle.
const MAX_REPORT: usize = 8_000;

pub fn spec() -> ToolSpec {
    ToolSpec {
        name: "0box",
        description: "Débat multi-agents (0box) : plusieurs perspectives IA débattent \
                      sur un sujet, puis MAIN synthétise et peut exécuter les actions \
                      (tâches, fichiers). Renvoie la transcription du débat.",
        parameters: json!({
            "type": "object",
            "properties": {
                "topic": { "type": "string", "description": "Sujet du débat." },
                "perspectives": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Perspectives (rôles), ex. [\"code\",\"reasoning\",\"main\"]. Défaut : code, reasoning, main."
                }
            },
            "required": ["topic"]
        }),
    }
}

/// Analyse les arguments : sujet + perspectives optionnelles.
pub fn parse_args(args: &str) -> Result<(String, Vec<String>), String> {
    let v: serde_json::Value =
        serde_json::from_str(args).map_err(|e| format!("arguments JSON invalides ({e})"))?;
    let topic = v
        .get("topic")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if topic.is_empty() {
        return Err("topic requis".to_string());
    }
    let perspectives: Vec<String> = v
        .get("perspectives")
        .and_then(|x| x.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .filter(|p: &Vec<String>| !p.is_empty())
        .unwrap_or_else(|| DEFAULT_PERSPECTIVES.iter().map(|s| s.to_string()).collect());
    Ok((topic, perspectives))
}

pub async fn run(args: &str) -> String {
    let (topic, perspectives) = match parse_args(args) {
        Ok(v) => v,
        Err(e) => return format!("Erreur 0box : {e}"),
    };

    let Some(rt) = super::runtime() else {
        return "Erreur 0box : contexte d'exécution absent (l'agent n'est pas initialisé). \
                Lance le débat avec `/0box <sujet>` dans la TUI ou `zer0 0box <sujet>`."
            .to_string();
    };

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<crate::event::UiEvent>();
    let setup = crate::tui::DebateSetup {
        settings: rt.settings.clone(),
        system: rt.system.clone(),
        options: rt.options.clone(),
        read_write: rt.read_write,
        code_exec: rt.code_exec,
        all_tools: rt.all_tools.clone(),
        ui: tx,
    };

    let perspectives_for_prompt = perspectives.clone();
    // Le sujet part dans la tâche ; on garde une copie pour le message d'erreur
    // ci-dessous (M3 : il doit nommer le vrai sujet, pas « (sujet inconnu) »).
    let topic_for_report = topic.clone();
    tokio::spawn(async move {
        crate::tui::run_0box_debate(topic, perspectives, setup).await;
    });

    // Collecte la transcription du débat (les autres events sont ignorés :
    // ce sont des notifications d'UI, pas du contenu).
    let mut report = String::new();
    while let Some(ev) = rx.recv().await {
        if let crate::event::UiEvent::Info(t) = ev {
            report.push_str(&t);
            report.push('\n');
        }
    }

    if report.trim().is_empty() {
        // M3 : le sujet est connu ici (`topic` survit au `clone` passé au spawn).
        // Avant, `topic_for_report` ignorait son argument et affichait toujours
        // « (sujet inconnu) » — le message mentait sur le sujet du débat.
        return format!(
            "0box : débat terminé sans sortie exploitable sur « {topic_for_report} » (perspectives : {}).",
            perspectives_for_prompt.join(", ")
        );
    }
    truncate_report(report)
}

/// Tronque la transcription à `MAX_REPORT` octets en gardant la fin.
///
/// B2 : `report.len() - MAX_REPORT` tombait au milieu d'un caractère accentué
/// et `&report[cut..]` paniquait ("byte index is not a char boundary") dans la
/// tâche de génération, laissant la TUI bloquée en « streaming ». On recule
/// sur une frontière UTF-8 avant de découper.
fn truncate_report(report: String) -> String {
    if report.len() <= MAX_REPORT {
        return report;
    }
    let cut = crate::util::floor_char_boundary(&report, report.len() - MAX_REPORT);
    format!("… (début tronqué)\n{}", &report[cut..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_defaults_to_three_perspectives() {
        let (topic, p) = parse_args(r#"{"topic":"conception d'un CLI"}"#).unwrap();
        assert_eq!(topic, "conception d'un CLI");
        assert_eq!(p, vec!["code", "reasoning", "main"]);
    }

    #[test]
    fn parse_accepts_custom_perspectives() {
        let (_, p) = parse_args(r#"{"topic":"x","perspectives":["main","writing"]}"#).unwrap();
        assert_eq!(p, vec!["main", "writing"]);
    }

    #[test]
    fn parse_rejects_empty_topic_and_bad_json() {
        assert!(parse_args(r#"{"topic":"  "}"#).is_err());
        assert!(parse_args("pas du json").is_err());
    }

    #[test]
    fn parse_ignores_empty_perspectives_list() {
        let (_, p) = parse_args(r#"{"topic":"x","perspectives":[]}"#).unwrap();
        assert_eq!(p.len(), 3);
    }

    #[test]
    fn short_report_is_untouched() {
        let r = "débat court".to_string();
        assert_eq!(truncate_report(r.clone()), r);
    }

    /// B2 : la coupe tombait au milieu d'un `é` -> panic, TUI bloquée.
    #[test]
    fn truncate_report_never_panics_on_multibyte() {
        // 'é' = 2 octets. On construit un rapport dont la coupe à
        // len - MAX_REPORT tombe pile sur le 2e octet d'un 'é'.
        for offset in 0..4 {
            let mut r = String::from("é");
            r.push_str(&"a".repeat(MAX_REPORT - 1 + offset));
            let out = truncate_report(r);
            assert!(out.starts_with("… (début tronqué)"));
            // le contenu conservé est valide et de taille bornée
            assert!(out.len() <= MAX_REPORT + 32);
        }
    }

    #[test]
    fn truncate_report_keeps_the_tail() {
        let mut r = String::from("DEBUT");
        r.push_str(&"x".repeat(MAX_REPORT));
        r.push_str("FIN");
        let out = truncate_report(r);
        assert!(out.ends_with("FIN"));
        assert!(!out.contains("DEBUT"));
    }
}
