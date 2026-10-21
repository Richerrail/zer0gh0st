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
    let mut slot = slot.lock().expect("mutex");
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
    let req = format!(
        "{method} {path} HTTP/1.1\r\nHost: {host}\r\nContent-Type: application/json\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
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
    let body = buf
        .split_once("\r\n\r\n")
        .map(|(_, b)| b.to_string())
        .unwrap_or_default();
    Ok((status, body))
}

/// `true` si la passerelle répond (`/health`).
pub fn probe() -> bool {
    matches!(http_req("GET", "/health", None), Ok((200..=299, _)))
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
                        info.get("has_key").and_then(serde_json::Value::as_bool).unwrap_or(false),
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
