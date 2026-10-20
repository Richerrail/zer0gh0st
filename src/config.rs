use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::providers::{self, Provider};

/// Global user config: API keys + last-used provider/model.
/// Stored at `~/.config/zer0/config.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalConfig {
    #[serde(default)]
    pub keys: BTreeMap<String, String>,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    /// Ordered failover chain, each entry `provider/model`.
    #[serde(default)]
    pub routes: Vec<String>,
    /// Named model presets: role -> `provider/model`.
    #[serde(default)]
    pub roles: BTreeMap<String, String>,
    /// Routage par rôle actif ? `/role off` garde le modèle du provider actif
    /// pour tout (utile pour tester un seul modèle). Défaut : `true`.
    #[serde(default = "default_true")]
    pub roles_enabled: bool,
    /// Passer par zer0-gw pour les providers intégrés.
    /// `None` = auto (on utilise la passerelle si elle répond).
    #[serde(default)]
    pub gateway: Option<bool>,
    /// User-defined providers (not in the built-in catalog).
    #[serde(default)]
    pub custom: BTreeMap<String, CustomProvider>,
    /// Optional embedding backend for semantic memory.
    #[serde(default)]
    pub embeddings: Option<EmbeddingConfig>,
    /// Contexte du modèle en tokens (override manuel ; abs. = auto).
    #[serde(default)]
    pub context_window: Option<u64>,
    /// Bot Telegram : token du bot (secret).
    #[serde(default)]
    pub telegram_token: Option<String>,
    /// Bot Telegram : identifiants (user id) autorisés à parler au bot.
    #[serde(default)]
    pub telegram_allow: Vec<i64>,
    /// Modèles de fallback spécifiques pour /0box (ajoutés en tête de chaîne).
    #[serde(default)]
    pub zero_box_fallbacks: Option<Vec<String>>,
    /// `client_id` de l'application OAuth GitHub (device flow du vote).
    /// Public par nature : ce n'est pas un secret.
    #[serde(default)]
    pub github_client_id: Option<String>,
}

// `Default` manuel : `roles_enabled` doit valoir `true` sur une config neuve
// (le `#[serde(default = …)]` ne couvre que la désérialisation, pas `default()`).
impl Default for GlobalConfig {
    fn default() -> Self {
        Self {
            keys: BTreeMap::new(),
            provider: None,
            model: None,
            routes: Vec::new(),
            roles: BTreeMap::new(),
            roles_enabled: true,
            gateway: None,
            custom: BTreeMap::new(),
            embeddings: None,
            context_window: None,
            telegram_token: None,
            telegram_allow: Vec::new(),
            zero_box_fallbacks: None,
            github_client_id: None,
        }
    }
}

/// Optional embedding backend, OpenAI-compatible (`/embeddings`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EmbeddingConfig {
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
}

/// A user-defined provider, added with `zer0 provider add-custom`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomProvider {
    pub name: String,
    /// Base URL including the version prefix (contains `/chat/completions`).
    pub base_url: String,
    #[serde(default)]
    pub env_key: Option<String>,
}

/// Per-project overrides: `.zer0/project.toml` in the working directory.
/// `roles` maps a role name (e.g. "main", "utility", "review") to `provider/model`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProjectConfig {
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub roles: BTreeMap<String, String>,
    /// Project-local ordered failover chain, each entry `provider/model`.
    #[serde(default)]
    pub routes: Vec<String>,
    /// Inline system-prompt override for this project.
    #[serde(default)]
    pub system_prompt: Option<String>,
    /// Path to a system-prompt file for this project (relative to cwd or absolute).
    #[serde(default)]
    pub prompt_file: Option<String>,
}

/// Fully resolved runtime settings.
#[derive(Debug, Clone)]
pub struct Settings {
    pub provider_id: String,
    pub provider_name: String,
    pub base_url: String,
    pub model: String,
    pub api_key: Option<String>,
}

pub fn config_dir() -> PathBuf {
    directories::ProjectDirs::from("", "", "zer0")
        .map(|d| d.config_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn global_path() -> PathBuf {
    config_dir().join("config.toml")
}

pub fn project_path() -> PathBuf {
    PathBuf::from(".zer0").join("project.toml")
}

/// Data directory (conversations, mémoire, personas) : **dans le projet** (`<projet>/data`).
///
/// Autonome : tu copies le dossier du projet, tu gardes tout. Seule la **config/clés**
/// reste hors du projet (`~/.config/zer0`). Migration depuis les anciens emplacements XDG.
pub fn data_dir() -> PathBuf {
    let dir = crate::mods::project_root().join("data");
    migrate_old_data(&dir);
    dir
}

/// Migration unique depuis les anciens dossiers XDG (`…/zer0gh0st`, `…/zer0`).
fn migrate_old_data(new: &Path) {
    static ONCE: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    ONCE.get_or_init(|| {
        let _ = fs::create_dir_all(new);
        let olds: Vec<PathBuf> = ["zer0gh0st", "zer0"]
            .iter()
            .filter_map(|n| {
                directories::ProjectDirs::from("", "", n).map(|d| d.data_dir().to_path_buf())
            })
            .collect();
        for old in olds {
            if old == *new || !old.is_dir() {
                continue;
            }
            let copy = |name: &str| {
                let s = old.join(name);
                let d = new.join(name);
                if s.is_file() && !d.exists() {
                    let _ = fs::copy(&s, &d);
                }
            };
            copy("conversation.jsonl");
            copy("memory.jsonl");
            let op = old.join("personas");
            let np = new.join("personas");
            if op.is_dir() {
                let _ = fs::create_dir_all(&np);
                if let Ok(rd) = fs::read_dir(&op) {
                    for e in rd.flatten() {
                        let d = np.join(e.file_name());
                        if !d.exists() {
                            let _ = fs::copy(e.path(), d);
                        }
                    }
                }
            }
            if let Ok(rd) = fs::read_dir(&old) {
                for e in rd.flatten() {
                    let name = e.file_name();
                    let ns = name.to_string_lossy();
                    if ns.starts_with("conversation_") && ns.ends_with(".jsonl") {
                        let d = new.join(&name);
                        if !d.exists() {
                            let _ = fs::copy(e.path(), &d);
                        }
                    }
                }
            }
        }
    });
}

pub fn load_global() -> GlobalConfig {
    read_toml(&global_path()).unwrap_or_default()
}

pub fn save_global(c: &GlobalConfig) -> Result<()> {
    write_toml(&global_path(), c)
}

pub fn load_project() -> ProjectConfig {
    read_toml(&project_path()).unwrap_or_default()
}

#[allow(dead_code)]
pub fn save_project(c: &ProjectConfig) -> Result<()> {
    write_toml(&project_path(), c)
}

/// Resolve the API key: environment variable, then unlocked vault, then stored config.
pub fn key_for(provider: &Provider) -> Option<String> {
    if !provider.env_key.is_empty() {
        if let Ok(v) = std::env::var(&provider.env_key) {
            if !v.is_empty() {
                return Some(v);
            }
        }
    }
    if let Some(k) = crate::vault::get_unlocked(&provider.id) {
        return Some(k);
    }
    load_global()
        .keys
        .get(&provider.id)
        .cloned()
        .filter(|s| !s.is_empty())
}

/// Merge project over global config into effective settings.
/// URL + clé **effectives** d'un provider.
///
/// - passerelle activée (et provider intégré) : URL de `zer0-gw` et **aucune clé**
///   (elle reste dans la passerelle — l'agent ne la détient jamais) ;
/// - sinon : URL du provider et clé locale (env / coffre / config).
///
/// Utilisé par `resolve()` **et** par `route::parse_route` (routes et rôles) :
/// sinon le provider actif passait par la passerelle mais tous les replis
/// partaient en direct → 401 « clé absente ».
pub fn endpoint_for(p: &Provider) -> (String, Option<String>) {
    if gateway_enabled() && !p.custom {
        (
            format!(
                "{}/{}/v1",
                crate::gwclient::gw_base().trim_end_matches('/'),
                p.id
            ),
            None,
        )
    } else {
        (p.base_url.to_string(), key_for(p))
    }
}

pub fn resolve() -> Settings {
    let g = load_global();
    let p = load_project();

    let provider_id = p
        .provider
        .clone()
        .or_else(|| g.provider.clone())
        .unwrap_or_else(|| "local".to_string())
        .to_ascii_lowercase();

    let provider = providers::find(&provider_id)
        .or_else(|| providers::find("local"))
        .expect("local provider must exist");

    let model = p
        .model
        .clone()
        .or_else(|| g.model.clone())
        .or_else(|| providers::default_model(&provider.id).map(String::from))
        .unwrap_or_default();

    // Un seul identifiant par provider : si la passerelle tourne (ou si
    // `gateway = true`), les providers intégrés passent par elle — la clé
    // reste dans zer0-gw et n'est jamais résolue dans l'agent.
    let (base_url, api_key) = endpoint_for(&provider);

    Settings {
        provider_id: provider.id.to_string(),
        provider_name: provider.name.to_string(),
        base_url,
        model,
        api_key,
    }
}

/// Effective failover chain: project routes first, then global routes.
pub fn effective_routes() -> Vec<String> {
    let g = load_global();
    let p = load_project();
    let mut out = p.routes.clone();
    for r in g.routes {
        if !out.contains(&r) {
            out.push(r);
        }
    }
    out
}

/// Effective roles: global roles overridden by project roles.
pub fn effective_roles() -> BTreeMap<String, String> {
    let g = load_global();
    let p = load_project();
    let mut out = g.roles;
    for (k, v) in p.roles {
        out.insert(k, v);
    }
    out
}

/// Set a role preset. `project` writes to `.zer0/project.toml`, otherwise the global config.
pub fn set_role(role: &str, route: &str, project: bool) -> Result<()> {
    let role = role.to_ascii_lowercase();
    if project {
        let mut p = load_project();
        p.roles.insert(role, route.to_string());
        save_project(&p)
    } else {
        let mut g = load_global();
        g.roles.insert(role, route.to_string());
        save_global(&g)
    }
}

/// Remove a role preset.
pub fn unset_role(role: &str, project: bool) -> Result<()> {
    let role = role.to_ascii_lowercase();
    if project {
        let mut p = load_project();
        p.roles.remove(&role);
        save_project(&p)
    } else {
        let mut g = load_global();
        g.roles.remove(&role);
        save_global(&g)
    }
}

/// Valeur par défaut des booléens « opt-out » (configs anciennes = comportement
/// historique activé).
fn default_true() -> bool {
    true
}

/// `true` si le routage par rôle est actif. Les configs sans le champ
/// `roles_enabled` restent actives (compatibilité).
pub fn roles_enabled() -> bool {
    load_global().roles_enabled
}

/// Active/désactive le routage par rôle (persisté dans la config globale).
pub fn set_roles_enabled(on: bool) -> Result<()> {
    let mut g = load_global();
    g.roles_enabled = on;
    save_global(&g)
}

/// La passerelle doit-elle servir les providers intégrés ?
/// `gateway = true/false` force ; absent = auto (la passerelle répond).
pub fn gateway_enabled() -> bool {
    load_global().gateway.unwrap_or_else(crate::gwclient::available)
}

/// Migration de l'ancien schéma `gw-<id>` : un seul identifiant par provider.
/// Retire le préfixe dans `provider`/`routes`/`roles` et supprime les entrées
/// `[custom.gw-*]` devenues inutiles. Retourne `true` si quelque chose a changé.
pub fn migrate_gateway_prefix() -> bool {
    let mut g = load_global();
    let mut changed = false;

    if let Some(p) = g.provider.as_ref().and_then(|p| p.strip_prefix("gw-")) {
        g.provider = Some(p.to_string());
        changed = true;
    }
    for r in g.routes.iter_mut() {
        if let Some(rest) = r.strip_prefix("gw-") {
            *r = rest.to_string();
            changed = true;
        }
    }
    for v in g.roles.values_mut() {
        if let Some(rest) = v.strip_prefix("gw-") {
            *v = rest.to_string();
            changed = true;
        }
    }
    let legacy: Vec<String> = g
        .custom
        .keys()
        .filter(|k| k.starts_with("gw-"))
        .cloned()
        .collect();
    for k in legacy {
        g.custom.remove(&k);
        changed = true;
    }

    if changed {
        let _ = save_global(&g);
    }
    changed
}

/// System-prompt override from the project, if any.
pub fn system_prompt_override() -> Option<String> {
    let p = load_project();
    if let Some(s) = p.system_prompt {
        if !s.trim().is_empty() {
            return Some(s);
        }
    }
    if let Some(f) = p.prompt_file {
        return fs::read_to_string(f).ok();
    }
    None
}

fn read_toml<T: for<'de> Deserialize<'de>>(p: &Path) -> Option<T> {
    let s = fs::read_to_string(p).ok()?;
    toml::from_str(&s).ok()
}

fn write_toml<T: Serialize>(p: &Path, v: &T) -> Result<()> {
    if let Some(parent) = p.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    fs::write(p, toml::to_string_pretty(v)?)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        // Config may contain API keys: keep it private.
        let _ = fs::set_permissions(p, fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roles_enabled_defaults_true_and_parses_false() {
        // Config neuve (aucun fichier) → rôles actifs.
        assert!(GlobalConfig::default().roles_enabled);
        // Config ancienne (aucun champ) → rôles actifs, comportement historique.
        let g: GlobalConfig = toml::from_str("").unwrap();
        assert!(g.roles_enabled);
        // Désactivation explicite.
        let g: GlobalConfig = toml::from_str("roles_enabled = false").unwrap();
        assert!(!g.roles_enabled);
        // Réactivation explicite.
        let g: GlobalConfig = toml::from_str("roles_enabled = true").unwrap();
        assert!(g.roles_enabled);
    }
}
