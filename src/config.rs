use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::providers::{self, Provider};

/// Global user config: API keys + last-used provider/model.
/// Stored at `~/.config/zer0/config.toml`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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
    /// User-defined providers (not in the built-in catalog).
    #[serde(default)]
    pub custom: BTreeMap<String, CustomProvider>,
    /// Optional embedding backend for semantic memory.
    #[serde(default)]
    pub embeddings: Option<EmbeddingConfig>,
    /// Contexte du modèle en tokens (override manuel ; abs. = auto).
    #[serde(default)]
    pub context_window: Option<u64>,
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

/// Data directory for persistent state (memory store, …).
pub fn data_dir() -> PathBuf {
    directories::ProjectDirs::from("", "", "zer0")
        .map(|d| d.data_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
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

    Settings {
        provider_id: provider.id.to_string(),
        provider_name: provider.name.to_string(),
        base_url: provider.base_url.to_string(),
        model,
        api_key: key_for(&provider),
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
