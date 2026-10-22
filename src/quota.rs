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
        if limit.is_some() {
            u.limit = limit;
        }
        if remain.is_some() {
            u.remain = remain;
        }
        u.note = note;
    }
}

/// Limites révélées par les **en-têtes** d'une réponse (200 ou 429) — quand le
/// fournisseur les envoie, c'est la valeur la plus fraîche possible.
pub fn record_limits(label: &str, limit: Option<u64>, remain: Option<u64>) {
    if limit.is_none() && remain.is_none() {
        return;
    }
    if let Ok(mut m) = registry().lock() {
        let u = m.entry(label.to_string()).or_default();
        if limit.is_some() {
            u.limit = limit;
        }
        if remain.is_some() {
            u.remain = remain;
        }
    }
}

/// Lit un compteur dans les en-têtes (plusieurs noms acceptés, insensible à la casse).
pub fn header_u64(headers: &reqwest::header::HeaderMap, names: &[&str]) -> Option<u64> {
    names.iter().find_map(|n| {
        headers
            .get(*n)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.trim().parse::<u64>().ok())
    })
}

pub const LIMIT_HEADERS: &[&str] = &["x-ratelimit-limit", "x-ratelimit-limit-requests"];
pub const REMAIN_HEADERS: &[&str] = &[
    "x-ratelimit-remaining",
    "x-ratelimit-remaining-requests",
    "x-ratelimit-remain",
];

/// Format demandé : « 0/50 (0 %) » — None si inconnu (on n'invente rien).
pub fn fmt_remaining(limit: Option<u64>, remain: Option<u64>) -> Option<String> {
    match (limit, remain) {
        (Some(l), Some(r)) if l > 0 => {
            let pct = (r * 100 + l / 2) / l;
            Some(format!("{r}/{l} ({pct} %)"))
        }
        _ => None,
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
    let limit = hdrs["X-RateLimit-Limit"]
        .as_str()
        .and_then(|s| s.parse().ok());
    let remain = hdrs["X-RateLimit-Remain"]
        .as_str()
        .and_then(|s| s.parse().ok());
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
    // Épuisé (429 réel ou reste == 0 révélé) — chiffre exact quand connu.
    let remain_str = fmt_remaining(u.limit, u.remain);
    if let Some(t) = u.last_429 {
        let mins = t.elapsed().as_secs() / 60;
        let when = if mins == 0 {
            "à l'instant".to_string()
        } else {
            format!("il y a {mins} min")
        };
        return Some(match remain_str {
            Some(r) => format!("  ⚠ {r} restant ({when})"),
            None => format!("  ⚠ 429 ({when})"),
        });
    }
    if let Some(r) = &remain_str {
        let empty = matches!(u.remain, Some(0));
        return Some(if empty {
            format!("  ⚠ {r} restant")
        } else {
            format!("  · {r} restant")
        });
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
        return "Aucun appel de modèle dans cette session.\n\
                (les compteurs vivent dans le process en cours — la TUI les \
                accumule pendant la session ; ce binaire lancé à part part d'une \
                page blanche)\n\n\
                /quota n'affiche que ce que Zer0 a réellement observé — appels \
                réussis, 429, limites révélées par le fournisseur. Les API ne \
                publient pas leur quota (Cline : rien ; OpenRouter : seulement \
                dans le corps des 429) : ce qui n'est pas observé n'est pas \
                inventé."
            .into();
    }
    let mut s = String::from("Usage observé par Zer0 (cette session) :\n");
    for (label, u) in m.iter() {
        s.push_str(&format!("  {label}\n"));
        let mut details = Vec::new();
        let remain_str = fmt_remaining(u.limit, u.remain);
        match u.last_429 {
            Some(t) => {
                let mins = t.elapsed().as_secs() / 60;
                let when = if mins == 0 {
                    "à l'instant".to_string()
                } else {
                    format!("il y a {mins} min")
                };
                details.push(format!("⚠ 429 {when}"));
                if let Some(r) = &remain_str {
                    details.push(format!("{r} restant"));
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
                }
                if let Some(r) = &remain_str {
                    let empty = matches!(u.remain, Some(0));
                    details.push(if empty {
                        format!("⚠ {r} restant")
                    } else {
                        format!("{r} restant")
                    });
                }
            }
        }
        s.push_str(&format!("      {}\n", details.join(" · ")));
    }
    s.push_str(
        "\nHonnêteté : compteurs locaux à la session (zéro au lancement). « restant » \
         n'apparaît que si le fournisseur le révèle (en-têtes ou corps d'un 429) — \
         sinon seul l'observé est affiché.",
    );
    s
}

// ----------------------------------------------------------------- solde live

/// Interroge le fournisseur pour un solde **réel et à jour** — pour l'instant
/// OpenRouter seulement (`GET /auth/key` ; Cline ne publie rien : 404 sur tous
/// les endpoints testés). Retourne None quand le provider n'expose rien.
pub async fn fetch_live(provider_id: &str, key: Option<&str>) -> Option<String> {
    if provider_id != "openrouter" {
        return None;
    }
    let k = key?;
    let resp = crate::http_client::HTTP_CLIENT
        .get("https://openrouter.ai/api/v1/auth/key")
        .bearer_auth(k)
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let v: serde_json::Value = resp.json().await.ok()?;
    Some(parse_auth_key(&v))
}

/// Parse la réponse `/auth/key` d'OpenRouter en lignes lisibles (testable).
pub fn parse_auth_key(v: &serde_json::Value) -> String {
    let d = &v["data"];
    let num = |x: &serde_json::Value| x.as_f64().unwrap_or(0.0);
    let usage = num(&d["usage"]);
    let daily = num(&d["usage_daily"]);
    let monthly = num(&d["usage_monthly"]);
    let tier = if d["is_free_tier"].as_bool().unwrap_or(false) {
        "free tier"
    } else {
        "payant"
    };
    let mut s = format!(
        "Solde de la clé OpenRouter ({tier}) :\n  · usage total : {usage:.2} $\n  · journalier : {daily:.2} $ · mensuel : {monthly:.2} $"
    );
    match (d["limit"].as_f64(), d["limit_remaining"].as_f64()) {
        (Some(l), r) if l > 0.0 => {
            let rem = r.unwrap_or(l - usage).max(0.0);
            let pct = (rem * 100.0 / l).round() as u64;
            s.push_str(&format!(
                "\n  · crédit : {rem:.2}/{l:.2} $ restants ({pct} %)"
            ));
        }
        _ => s.push_str("\n  · crédit : illimité (facturation à l'usage)"),
    }
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

    #[test]
    fn fmt_remaining_formats_the_user_wanted() {
        assert_eq!(fmt_remaining(Some(50), Some(0)), Some("0/50 (0 %)".into()));
        assert_eq!(
            fmt_remaining(Some(100), Some(0)),
            Some("0/100 (0 %)".into())
        );
        assert_eq!(
            fmt_remaining(Some(50), Some(47)),
            Some("47/50 (94 %)".into())
        );
        assert_eq!(
            fmt_remaining(Some(50), Some(25)),
            Some("25/50 (50 %)".into())
        );
        // Inconnu ⇒ rien (jamais de chiffre inventé)
        assert_eq!(fmt_remaining(None, Some(3)), None);
        assert_eq!(fmt_remaining(Some(50), None), None);
        assert_eq!(fmt_remaining(Some(0), Some(0)), None); // limite nulle : non interprétable
    }

    #[test]
    fn parse_auth_key_with_and_without_limit() {
        // Cas réel observé : free tier, limite de dépense illimitée.
        let v: serde_json::Value = serde_json::from_str(
            r#"{"data":{"usage":0.0,"usage_daily":0.0,"usage_monthly":0.0,
                 "limit":null,"limit_remaining":null,"is_free_tier":true}}"#,
        )
        .unwrap();
        let s = parse_auth_key(&v);
        assert!(s.contains("free tier"));
        assert!(s.contains("usage total : 0.00 $"));
        assert!(s.contains("illimité"), "{s}");

        // Clé avec budget : reste + pourcentage.
        let v: serde_json::Value = serde_json::from_str(
            r#"{"data":{"usage":7.5,"usage_daily":1.0,"usage_monthly":7.5,
                 "limit":10.0,"limit_remaining":2.5,"is_free_tier":false}}"#,
        )
        .unwrap();
        let s = parse_auth_key(&v);
        assert!(s.contains("2.50/10.00 $ restants (25 %)"), "{s}");
    }

    #[test]
    fn header_u64_reads_known_names() {
        let mut h = reqwest::header::HeaderMap::new();
        h.insert("x-ratelimit-remaining", "3".parse().unwrap());
        assert_eq!(header_u64(&h, REMAIN_HEADERS), Some(3));
        let empty = reqwest::header::HeaderMap::new();
        assert_eq!(header_u64(&empty, LIMIT_HEADERS), None);
    }
}
