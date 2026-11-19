//! Client minimaliste pour la passerelle `zer0-gw` (std only — utilisable
//! depuis le TUI synchrone comme depuis la CLI, sans runtime tokio).
//!
//! Sécurité : l'endpoint admin est **loopback + écriture seule**. On peut
//! *déposer* une clé, jamais la lire — le flux va toujours agent → passerelle.
//!
//! Comportement de `provider add <id> <clé>` :
//! - `zer0-gw` joignable → la clé est déposée sur la passerelle (persistée
//!   dans son fichier `keys.managed`), le provider `gw-<id>` est créé/activé,
//!   **aucune clé n'est stockée dans `~/.config/zer0`** ;
//! - `zer0-gw` absent → repli : stockage local historique, avec avertissement.
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use crate::config::{self, GlobalConfig};

/// URL de la passerelle (surchargeable : `ZER0_GW_URL=http://127.0.0.1:8787`).
pub fn gw_base() -> String {
    std::env::var("ZER0_GW_URL").unwrap_or_else(|_| "http://127.0.0.1:8787".to_string())
}

/// Résultat d'un `provider add`.
pub enum AddRoute {
    /// Clé déposée sur la passerelle ; agent pointé sur `provider_id` (sans clé).
    ViaGateway { provider_id: String },
    /// Passerelle absente → stockage local historique.
    Local,
}

/// Identifiant **côté passerelle** : `gw-openrouter` → `openrouter`, sinon tel quel.
pub fn gw_target(id: &str) -> String {
    let id = id.trim().to_ascii_lowercase();
    id.strip_prefix("gw-").unwrap_or(&id).to_string()
}

/// `true` si la passerelle répond (mis en cache 5 s : `resolve()` est appelé souvent).
pub fn available() -> bool {
    use std::sync::{Mutex, OnceLock};
    static CACHE: OnceLock<Mutex<Option<(std::time::Instant, bool)>>> = OnceLock::new();
    let slot = CACHE.get_or_init(|| Mutex::new(None));
    // Mutex empoisonné (panic d'un autre thread) : on récupère la garde plutôt
    // que de paniquer — le cache n'est qu'une optimisation de `probe()`.
    let mut slot = slot.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((t, v)) = *slot {
        if t.elapsed() < Duration::from_secs(5) {
            return v;
        }
    }
    let v = probe();
    *slot = Some((std::time::Instant::now(), v));
    v
}

/// Requête HTTP/1.1 minimale sur loopback. Retourne `(status, corps)`.
/// Hôte loopback extrait d'une URL de passerelle (**pur** : testable sans
/// muter l'environnement — les tests tournent en parallèle).
fn loopback_host(base: &str) -> Result<&str, String> {
    let rest = base
        .strip_prefix("http://")
        .ok_or_else(|| format!("ZER0_GW_URL doit être http:// (loopback) : {base}"))?;
    Ok(rest.trim_end_matches('/'))
}

fn http_req(method: &str, path: &str, body: Option<&str>) -> Result<(u16, String), String> {
    let base = gw_base();
    let rest = loopback_host(&base)?;
    let host = rest.trim_end_matches('/');
    let addr = host
        .to_socket_addrs()
        .map_err(|e| e.to_string())?
        .next()
        .ok_or_else(|| "adresse zer0-gw invalide".to_string())?;
    if !addr.ip().is_loopback() {
        return Err("zer0-gw n'est joignable qu'en loopback".to_string());
    }

    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_millis(800))
        .map_err(|_| "zer0-gw injoignable (sudo systemctl start zer0-gw)".to_string())?;
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .map_err(|e| e.to_string())?;

    let body = body.unwrap_or("");
    // Jeton admin si configuré (env prioritaire, sinon config.toml). Sans effet
    // sur le proxy (chemin `/provider/v1/...`), requis sur `/admin/*`.
    let auth = admin_token()
        .map(|t| format!("x-zer0-admin: {t}\r\n"))
        .unwrap_or_default();
    let req = format!(
        "{method} {path} HTTP/1.1\r\nHost: {host}\r\nContent-Type: application/json\r\n\
         {auth}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream
        .write_all(req.as_bytes())
        .map_err(|e| format!("écriture vers zer0-gw : {e}"))?;

    let mut buf = String::new();
    stream
        .read_to_string(&mut buf)
        .map_err(|e| format!("lecture de zer0-gw : {e}"))?;
    let status: u16 = buf
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| "réponse zer0-gw illisible".to_string())?;
    let (head, body_raw) = buf.split_once("\r\n\r\n").unwrap_or(("", buf.as_str()));
    // Le proxy de zer0-gw streame (pas de Content-Length) ré => réponses
    // `Transfer-Encoding: chunked`. On dé-chunke pour pouvoir parser le JSON.
    let body = if head
        .to_ascii_lowercase()
        .contains("transfer-encoding: chunked")
    {
        dechunk(body_raw)
    } else {
        body_raw.to_string()
    };
    Ok((status, body))
}

/// Décode un corps HTTP `Transfer-Encoding: chunked` en texte brut.
fn dechunk(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some((size_str, after)) = rest.split_once("\r\n") {
        let size = usize::from_str_radix(size_str.trim(), 16).unwrap_or(0);
        if size == 0 {
            break;
        }
        if after.len() < size {
            out.push_str(after);
            break;
        }
        out.push_str(&after[..size]);
        rest = &after[size..];
        if let Some(stripped) = rest.strip_prefix("\r\n") {
            rest = stripped;
        }
    }
    out
}

/// `true` si la passerelle répond (`/health`).
pub fn probe() -> bool {
    matches!(http_req("GET", "/health", None), Ok((200..=299, _)))
}

/// Jeton admin : `ZER0_GW_ADMIN_TOKEN` (env, prioritaire) sinon `gw_admin_token`
/// de la config globale. `None` = passerelle sans jeton (loopback historique).
fn admin_token() -> Option<String> {
    if let Some(t) = std::env::var("ZER0_GW_ADMIN_TOKEN")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
    {
        return Some(t);
    }
    crate::config::load_global()
        .gw_admin_token
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Enregistre (ou met à jour) un provider sur la passerelle — écriture seule,
/// aucune clé. Idempotent : met à jour `base_url` si le provider existe déjà.
/// Retourne une erreur si la passerelle est absente ou refuse (id/base_url).
pub fn register_provider(id: &str, base_url: &str) -> Result<(), String> {
    if !probe() {
        return Err("zer0-gw injoignable (sudo systemctl start zer0-gw)".to_string());
    }
    let payload = serde_json::json!({ "id": id, "base_url": base_url }).to_string();
    let (status, body) = http_req("POST", "/admin/providers", Some(&payload))?;
    match status {
        200..=299 => Ok(()),
        s => Err(format!("zer0-gw a refusé le provider ({s}) : {body}")),
    }
}

/// Sonde `GET /{id}/v1/models` **à travers la passerelle** (le chemin réel).
/// `Err` = injoignable ; sinon `(status HTTP, corps)`.
pub fn probe_models(id: &str) -> Result<(u16, String), String> {
    http_req("GET", &format!("/{id}/v1/models"), None)
}

/// Sonde `POST /{id}/v1/chat/completions` (1 token) à travers la passerelle.
pub fn probe_chat(id: &str, model: &str) -> Result<(u16, String), String> {
    let body = serde_json::json!({
        "model": model,
        "messages": [{"role": "user", "content": "ping"}],
        "max_tokens": 1,
        "stream": false,
    })
    .to_string();
    http_req("POST", &format!("/{id}/v1/chat/completions"), Some(&body))
}

/// Jusqu'à `max` ids de modèles d'une réponse `GET /v1/models` (`data[i].id`).
/// Les premiers `[0..max]` suffisent pour sonder : un seul modèle en 404 ne
/// condamne pas le provider (listes d'agrégateurs comme NVIDIA/OpenRouter).
pub fn model_ids(models_body: &str, max: usize) -> Vec<String> {
    serde_json::from_str::<serde_json::Value>(models_body)
        .ok()
        .and_then(|v| v.get("data").and_then(|d| d.as_array()).cloned())
        .map(|arr| {
            arr.iter()
                .filter_map(|m| m.get("id")?.as_str().map(str::to_string))
                .take(max)
                .collect()
        })
        .unwrap_or_default()
}

/// Premier id de modèle d'une réponse `GET /v1/models` (`data[0].id`).
pub fn first_model_id(models_body: &str) -> Option<String> {
    model_ids(models_body, 1).into_iter().next()
}

/// Providers connus de la passerelle et présence d'une clé (jamais la valeur).
pub fn list_keys() -> Result<Vec<(String, bool)>, String> {
    let (status, body) = http_req("GET", "/admin/keys", None)?;
    if !(200..=299).contains(&status) {
        return Err(format!("zer0-gw ({status}) : {body}"));
    }
    let v: serde_json::Value = serde_json::from_str(&body).map_err(|e| e.to_string())?;
    let mut out: Vec<(String, bool)> = v
        .get("keys")
        .and_then(|k| k.as_object())
        .map(|map| {
            map.iter()
                .map(|(id, info)| {
                    (
                        id.clone(),
                        info.get("has_key")
                            .and_then(serde_json::Value::as_bool)
                            .unwrap_or(false),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    Ok(out)
}

/// Dépose une clé pour `gw_id` — écriture seule, la valeur n'est jamais relue.
fn post_key(gw_id: &str, key: &str) -> Result<(), String> {
    let payload = serde_json::json!({ "api_key": key }).to_string();
    let (status, body) = http_req("POST", &format!("/admin/keys/{gw_id}"), Some(&payload))?;
    match status {
        200..=299 => Ok(()),
        404 => Err(format!(
            "zer0-gw ne connaît pas le provider « {gw_id} » — ajoute [providers.{gw_id}] \
             dans /etc/zer0-gw/gw.toml puis redémarre-le"
        )),
        s => Err(format!("zer0-gw a refusé la clé ({s}) : {body}")),
    }
}

/// Dépose une clé pour `id` (résout l'id passerelle). Écriture seule.
pub fn set_key(id: &str, key: &str) -> Result<(), String> {
    post_key(&gw_target(id), key)
}

/// Demande à la passerelle d'oublier une clé (best-effort pour `provider remove`).
pub fn delete_key(id: &str) -> Result<(), String> {
    let gw_id = gw_target(id);
    match http_req("DELETE", &format!("/admin/keys/{gw_id}"), None) {
        Ok((200..=299, _)) | Err(_) => Ok(()), // passerelle absente : pas grave
        Ok((s, b)) => Err(format!("zer0-gw ({s}) : {b}")),
    }
}

/// Flux complet de `provider add <id> <clé>` quand une clé explicite est fournie.
pub fn add_key_flow(id: &str, key: &str) -> Result<AddRoute, String> {
    let id = id.trim().to_ascii_lowercase();
    if id == "local" {
        // llama-server n'a pas de clé : rien à déposer.
        return Ok(AddRoute::Local);
    }
    if !probe() {
        return Ok(AddRoute::Local);
    }
    let gw_id = gw_target(&id);
    post_key(&gw_id, key)?;
    // Un seul identifiant : le provider reste `openrouter`, `nvidia`… ; c'est
    // `config::resolve` qui décide du transport (passerelle ou direct).
    Ok(AddRoute::ViaGateway { provider_id: gw_id })
}

/// Active `provider_id` en mode passerelle : aucun provider `gw-*`, aucune clé
/// locale. Supprime aussi une éventuelle entrée héritée `[custom.gw-<id>]`.
/// Retourne `(nom, base_url)` pour l'état du TUI/CLI.
pub fn activate_provider(g: &mut GlobalConfig, provider_id: &str) -> (String, String) {
    g.custom.remove(&format!("gw-{provider_id}"));
    g.provider = Some(provider_id.to_string());
    let _ = config::save_global(g);
    let name = crate::providers::find(provider_id)
        .map(|p| p.name)
        .unwrap_or_else(|| provider_id.to_string());
    let base_url = format!("{}/{}/v1", gw_base().trim_end_matches('/'), provider_id);
    (name, base_url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dechunk_rebuilds_body() {
        // Deux chunks "Hello " (6) et "world" (5), fin 0.
        let raw = "6\r\nHello \r\n5\r\nworld\r\n0\r\n\r\n";
        assert_eq!(dechunk(raw), "Hello world");
    }

    #[test]
    fn dechunk_handles_json_chunk() {
        let body = r#"{"data":[{"id":"demo/model-1"}]}"#;
        let hex = format!("{:x}", body.len());
        let raw = format!("{hex}\r\n{body}\r\n0\r\n\r\n");
        assert_eq!(
            first_model_id(&dechunk(&raw)).as_deref(),
            Some("demo/model-1")
        );
    }

    #[test]
    fn first_model_id_reads_data() {
        assert_eq!(
            first_model_id(r#"{"data":[{"id":"a/b"}]}"#).as_deref(),
            Some("a/b")
        );
        assert_eq!(first_model_id("{}"), None);
        assert_eq!(first_model_id("pas json"), None);
    }

    #[test]
    fn gw_target_mapping() {
        assert_eq!(gw_target("openrouter"), "openrouter");
        assert_eq!(gw_target("GW-OpenRouter"), "openrouter");
        assert_eq!(gw_target("mymachine"), "mymachine");
    }

    #[test]
    fn gateway_url_uses_plain_provider_id() {
        // Un seul identifiant : plus de préfixe `gw-` côté agent.
        assert_eq!(gw_target("gw-openrouter"), "openrouter");
        assert!(gw_base().starts_with("http://"));
    }

    #[test]
    fn gw_url_rejects_non_loopback_scheme() {
        // Fonction **pure** : pas de mutation d'environnement (course en
        // parallèle → CI instable).
        let err = loopback_host("https://evil.example").err().unwrap();
        assert!(err.contains("http://"));
        assert!(loopback_host("http://127.0.0.1:8787").is_ok());
    }
}
