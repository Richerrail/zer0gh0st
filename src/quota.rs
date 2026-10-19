//! Suivi d'usage des modèles — **cette session, sans devinette**.
//!
//! Les API ne publient pas leur quota restant : Cline n'expose aucun champ ni
//! en-tête, et OpenRouter ne révèle ses limites (`X-RateLimit-Limit` /
//! `X-RateLimit-Remain`) que dans le **corps des réponses 429**. Zer0 ne peut
//! donc pas afficher un quota qu'on ne lui donne pas — il comptabilise ce qu'il
//! **observe lui-même** : appels réussis (200), refus réels (429), et les limites
//! quand un fournisseur les révèle au passage.
//!
//! Ce qui n'est pas observé n'est pas affiché (principe d'honnêteté du projet).
//! Les compteurs vivent en mémoire : ils repartent à zéro à chaque lancement
//! (comme la mémoire d'échecs de `route.rs`).

use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

/// Un point d'observation : un endpoint `provider/modèle` réellement appelé.
#[derive(Default, Clone)]
struct Usage {
    /// Appels HTTP réussis (2xx) — zéro n'est pas « indisponible », c'est « pas appelé ».
    ok_calls: u64,
    last_ok: Option<Instant>,
    /// Dernier 429 observé ; effacé par un succès ultérieur.
    last_429: Option<Instant>,
    /// Limites révélées par le fournisseur dans le corps d'un 429 (OpenRouter).
    limit: Option<u64>,
    remain: Option<u64>,
    /// Message d'erreur tronqué du dernier 429 (ex. `free-models-per-day`).
    note: Option<String>,
}

fn registry() -> &'static Mutex<BTreeMap<String, Usage>> {
    static R: OnceLock<Mutex<BTreeMap<String, Usage>>> = OnceLock::new();
    R.get_or_init(|| Mutex::new(BTreeMap::new()))
}

/// Label canonique d'un endpoint, identique à `route::Endpoint::label`
/// (`provider_id/model`). L'hôte sert de repli pour un endpoint hors catalogue.
pub fn label_for(base_url: &str, model: &str) -> String {
    let base = base_url.trim_end_matches('/');
    let pid = crate::providers::all()
        .into_iter()
        .find(|p| p.base_url.trim_end_matches('/') == base)
        .map(|p| p.id)
        .unwrap_or_else(|| host_of(base));
    format!("{pid}/{model}")
}

fn host_of(url: &str) -> String {
    url.trim_start_matches("https://")
        .trim_start_matches("http://")
        .split('/')
        .next()
        .unwrap_or("inconnu")
        .to_string()
}

/// À appeler sur chaque réponse 2xx : l'endpoint a consommé de l'usage.
pub fn record_success(label: &str) {
    if let Ok(mut m) = registry().lock() {
        let u = m.entry(label.to_string()).or_default();
        u.ok_calls += 1;
        u.last_ok = Some(Instant::now());
        // Un succès après un 429 prouve que le quota n'est pas épuisé.
        u.last_429 = None;
    }
}

/// À appeler sur chaque 429. `body` est le corps brut de la réponse ; les
/// limites n'en sont extraites que si le fournisseur les y a mises.
pub fn record_rate_limited(label: &str, body: &str) {
    let (limit, remain, note) = parse_429_body(body);
    if let Ok(mut m) = registry().lock() {
        let u = m.entry(label.to_string()).or_default();
        u.last_429 = Some(Instant::now());
        u.limit = limit;
        u.remain = remain;
        u.note = note;
    }
}

/// Extrait les limites du corps d'un 429 façon OpenRouter :
/// `{"error":{"message":"…","metadata":{"headers":{"X-RateLimit-Limit":"50","X-RateLimit-Remain":"0"}}}}`
/// Inconnu ou dans un autre format ⇒ `(None, None, None)` — pas de valeur inventée.
fn parse_429_body(body: &str) -> (Option<u64>, Option<u64>, Option<String>) {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(body) else {
        return (None, None, None);
    };
    let err = &v["error"];
    let hdrs = &err["metadata"]["headers"];
    let limit = hdrs["X-RateLimit-Limit"].as_str().and_then(|s| s.parse().ok());
    let remain = hdrs["X-RateLimit-Remain"].as_str().and_then(|s| s.parse().ok());
    let note = err["message"]
        .as_str()
        .map(|s| s.chars().take(80).collect::<String>());
    (limit, remain, note)
}

/// Badge court pour les listes de modèles. `None` = rien d'observé cette
/// session : on ne raconte pas ce qu'on ne sait pas.
pub fn badge(label: &str) -> Option<String> {
    let m = registry().lock().ok()?;
    let u = m.get(label)?;
    // 429 non suivi d'un succès ⇒ l'endpoint est à sec (pour le moment).
    if let Some(t) = u.last_429 {
        let mins = t.elapsed().as_secs() / 60;
        let when = if mins == 0 {
            "à l'instant".to_string()
        } else {
            format!("il y a {mins} min")
        };
        let quota = match (u.remain, u.limit) {
            (Some(r), Some(l)) => format!(" — {r}/{l} restantes"),
            _ => String::new(),
        };
        return Some(format!("  ⚠ 429 ({when}{quota})"));
    }
    // Reste connu et non nul (limite révélée par un 429 antérieur, depuis remontée).
    if let (Some(r), Some(l)) = (u.remain, u.limit) {
        if r > 0 {
            return Some(format!("  · ~{r}/{l} restantes"));
        }
    }
    if u.ok_calls > 0 {
        return Some(format!("  · ok ×{}", u.ok_calls));
    }
    None
}

/// Rapport complet pour `/quota` : tout ce que Zer0 a observé, avec ses limites.
pub fn report() -> String {
    let Ok(m) = registry().lock() else {
        return "Registre d'usage indisponible.".into();
    };
    if m.is_empty() {
        return "Aucun appel de modèle cette session.\n\n\
                /quota n'affiche que ce que Zer0 a réellement observé — appels \
                réussis, 429, limites révélées par le fournisseur. Les API ne \
                publient pas leur quota (Cline : rien ; OpenRouter : seulement \
                dans le corps des 429) : ce qui n'est pas observé n'est pas \
                inventé.".into();
    }
    let mut s = String::from("Usage observé par Zer0 (cette session) :\n");
    for (label, u) in m.iter() {
        s.push_str(&format!("  {label}\n"));
        let mut details = Vec::new();
        match u.last_429 {
            Some(t) => {
                let mins = t.elapsed().as_secs() / 60;
                let when = if mins == 0 { "à l'instant".to_string() } else { format!("il y a {mins} min") };
                details.push(format!("⚠ 429 {when}"));
                if let (Some(r), Some(l)) = (u.remain, u.limit) {
                    details.push(format!("{r}/{l} restantes"));
                }
                if let Some(n) = &u.note {
                    details.push(n.clone());
                }
            }
            None => {
                if u.ok_calls > 0 {
                    details.push(format!("✓ {} appel(s) ok", u.ok_calls));
                    if let Some(t) = u.last_ok {
                        let mins = t.elapsed().as_secs() / 60;
                        if mins > 0 {
                            details.push(format!("dernier il y a {mins} min"));
                        } else {
                            details.push("dernier à l'instant".into());
                        }
                    }
                    if let (Some(r), Some(l)) = (u.remain, u.limit) {
                        details.push(format!("~{r}/{l} restantes (dernier signal)"));
                    }
                }
            }
        }
        s.push_str(&format!("      {}\n", details.join(" · ")));
    }
    s.push_str(
        "\nHonnêteté : ces compteurs sont locaux à Zer0 (ils repartent à zéro au \
         lancement). « restantes » n'apparaît que si le fournisseur l'a révélé dans \
         un 429 — sinon, seul ce que Zer0 a observé est affiché.",
    );
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_429_extracts_openrouter_metadata() {
        let body = r#"{"error":{"message":"Rate limit exceeded: free-models-per-day","code":429,
            "metadata":{"headers":{"X-RateLimit-Limit":"50","X-RateLimit-Remain":"0"}}}}"#;
        let (l, r, note) = parse_429_body(body);
        assert_eq!(l, Some(50));
        assert_eq!(r, Some(0));
        assert!(note.unwrap().contains("free-models-per-day"));
    }

    #[test]
    fn parse_429_ignores_unknown_formats() {
        // Cline renvoie du texte brut ou un JSON sans metadata : rien n'est inventé.
        assert_eq!(parse_429_body("Too Many Requests"), (None, None, None));
        assert_eq!(
            parse_429_body(r#"{"error":{"message":"rate limited"}}"#),
            (None, None, Some("rate limited".into()))
        );
        assert_eq!(parse_429_body(""), (None, None, None));
    }

    #[test]
    fn label_uses_provider_catalog() {
        // Provider connu ⇒ son id ; sinon l'hôte fait foi.
        assert_eq!(label_for("http://127.0.0.1:8080/v1/", "qwen"), "local/qwen");
        assert_eq!(label_for("https://exemple.io/v1", "m"), "exemple.io/m");
    }
}
