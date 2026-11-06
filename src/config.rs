use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::providers::{self, Provider};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalConfig {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub keys: BTreeMap<String, String>,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub routes: Vec<String>,
    #[serde(default)]
    pub roles: BTreeMap<String, String>,
    #[serde(default = "default_true")]
    pub roles_enabled: bool,
    #[serde(default)]
    pub gateway: Option<bool>,
    #[serde(default)]
    pub custom: BTreeMap<String, CustomProvider>,
    #[serde(default)]
    pub embeddings: Option<EmbeddingConfig>,
    #[serde(default)]
    pub context_window: Option<u64>,
    #[serde(default)]
    pub telegram_token: Option<String>,
    #[serde(default)]
    pub telegram_allow: Vec<i64>,
    #[serde(default)]
    pub theme: Option<String>,
    #[serde(default)]
    pub sound: Option<bool>,
    #[serde(default)]
    pub system_prompt: Option<String>,
    #[serde(default)]
    pub system_prompt_local: Option<String>,
    #[serde(default)]
    pub stream: Option<bool>,
    #[serde(default)]
    pub zero_box_fallbacks: Option<Vec<String>>,
    #[serde(default)]
    pub github_client_id: Option<String>,
    #[serde(default)]
    pub github_prompt_count: u32,
    #[serde(default)]
    pub github_prompted: bool,
    #[serde(default)]
    pub persona: Option<String>,
    #[serde(default)]
    pub thinking: bool,
    #[serde(default)]
    pub code_exec: bool,
    #[serde(default)]
    pub setup_done: bool,
    #[serde(default)]
    pub setup_step: u8,
    #[serde(default)]
    pub gw_admin_token: Option<String>,
}

pub const SCHEMA_VERSION: u32 = 1;

fn default_schema_version() -> u32 {
    SCHEMA_VERSION
}

impl Default for GlobalConfig {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
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
            theme: None,
            sound: None,
            system_prompt: None,
            system_prompt_local: None,
            stream: None,
            github_client_id: None,
            github_prompt_count: 0,
            github_prompted: false,
            persona: None,
            thinking: false,
            code_exec: false,
            setup_done: false,
            setup_step: 0,
            gw_admin_token: None,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EmbeddingConfig {
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomProvider {
    pub name: String,
    pub base_url: String,
    #[serde(default)]
    pub env_key: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProjectConfig {
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub roles: BTreeMap<String, String>,
    #[serde(default)]
    pub routes: Vec<String>,
    #[serde(default)]
    pub system_prompt: Option<String>,
    #[serde(default)]
    pub prompt_file: Option<String>,
}

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

pub fn data_dir() -> PathBuf {
    static DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    DIR.get_or_init(|| {
        let dir = std::env::var("ZER0_DATA_DIR")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
            .or_else(|| {
                directories::ProjectDirs::from("", "", "zer0gh0st")
                    .map(|d| d.data_dir().to_path_buf())
            })
            .unwrap_or_else(|| PathBuf::from(".zer0-data"));
        let _ = fs::create_dir_all(&dir);
        migrate_checkout_data(&crate::mods::project_root(), &dir);
        dir
    })
    .clone()
}

fn copy_if_missing_or_newer(from: &Path, to: &Path) {
    let Ok(src_meta) = fs::metadata(from) else {
        return;
    };
    if !src_meta.is_file() {
        return;
    }
    let newer = match fs::metadata(to).and_then(|m| m.modified()) {
        Ok(dst_time) => src_meta
            .modified()
            .map(|src_time| src_time > dst_time)
            .unwrap_or(false),
        Err(_) => true,
    };
    if newer {
        if let Some(parent) = to.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::copy(from, to);
    }
}

fn copy_if_missing(from: &Path, to: &Path) {
    if from.is_file() && !to.exists() {
        if let Some(parent) = to.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::copy(from, to);
    }
}

fn copy_dir_if_missing(from: &Path, to: &Path) {
    let Ok(entries) = fs::read_dir(from) else {
        return;
    };
    for e in entries.filter_map(Result::ok) {
        let src = e.path();
        if src.is_file() {
            copy_if_missing(&src, &to.join(e.file_name()));
        }
    }
}

pub(crate) fn migrate_checkout_data(root: &Path, target: &Path) {
    if root == target || !root.is_dir() {
        return;
    }
    let old = root.join("data");
    for name in [
        "conversation.jsonl",
        "memory.jsonl",
        "audit.jsonl",
        "knowledge.log",
    ] {
        copy_if_missing_or_newer(&old.join(name), &target.join(name));
    }
    copy_if_missing(&root.join("knowledge.db"), &target.join("knowledge.db"));
    copy_if_missing(&root.join("knowledge.log"), &target.join("knowledge.log"));
    copy_dir_if_missing(&old.join("personas"), &target.join("personas"));
}

type ConfigCache = std::sync::Mutex<Option<(PathBuf, Option<std::time::SystemTime>, GlobalConfig)>>;

fn cache() -> &'static ConfigCache {
    static C: std::sync::OnceLock<ConfigCache> = std::sync::OnceLock::new();
    C.get_or_init(|| std::sync::Mutex::new(None))
}

fn apply_setup_migration(cfg: &mut GlobalConfig) {
    if !cfg.setup_done && (cfg.provider.is_some() || cfg.model.is_some()) {
        cfg.setup_done = true;
    }
}

pub fn load_global() -> GlobalConfig {
    let path = global_path();
    let stamp = fs::metadata(&path).and_then(|m| m.modified()).ok();
    if let Ok(guard) = cache().lock() {
        if let Some((p, s, cfg)) = guard.as_ref() {
            if p == &path && s == &stamp {
                return cfg.clone();
            }
        }
    }
    let mut cfg: GlobalConfig = read_toml(&path).unwrap_or_default();
    apply_setup_migration(&mut cfg);
    if cfg.schema_version == 0 {
        cfg.schema_version = SCHEMA_VERSION;
    } else if cfg.schema_version > SCHEMA_VERSION {
        static WARNED: std::sync::OnceLock<()> = std::sync::OnceLock::new();
        WARNED.get_or_init(|| {
            eprintln!(
                "⚠ {} : schéma {} > {} (écrit par un binaire plus récent) — \
                 mets à jour ZER0|GH0ST avant de modifier la configuration.",
                path.display(),
                cfg.schema_version,
                SCHEMA_VERSION
            );
        });
    }
    if let Ok(mut guard) = cache().lock() {
        *guard = Some((path, stamp, cfg.clone()));
    }
    cfg
}

pub fn save_global(c: &GlobalConfig) -> Result<()> {
    let path = global_path();
    if is_corrupt(&path) {
        return Err(anyhow::anyhow!(
            "écriture refusée : {} est illisible. Une copie a été conservée \
             (voir `zer0 provider keys`). Répare ou supprime le fichier avant \
             de réessayer — sinon tes clés seraient écrasées.",
            path.display()
        ));
    }
    write_toml(&path, c)?;
    let stamp = fs::metadata(&path).and_then(|m| m.modified()).ok();
    if let Ok(mut guard) = cache().lock() {
        *guard = Some((path, stamp, c.clone()));
    }
    Ok(())
}

pub fn load_project() -> ProjectConfig {
    read_toml(&project_path()).unwrap_or_default()
}

pub fn save_project(c: &ProjectConfig) -> Result<()> {
    let path = project_path();
    if is_corrupt(&path) {
        return Err(anyhow::anyhow!(
            "écriture refusée : {} est illisible (copie conservée).",
            path.display()
        ));
    }
    write_toml(&path, c)
}

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

pub fn endpoint_for(p: &Provider) -> (String, Option<String>) {
    if p.id == "local" {
        return (p.base_url.to_string(), None);
    }
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

    let model = if provider.id == "local" {
        let path = std::path::Path::new(&model);
        if path.is_absolute() {
            path.file_stem()
                .and_then(|s| s.to_str())
                .map(str::to_string)
                .unwrap_or(model)
        } else {
            model
        }
    } else {
        model
    };

    let (base_url, api_key) = endpoint_for(&provider);

    Settings {
        provider_id: provider.id.to_string(),
        provider_name: provider.name.to_string(),
        base_url,
        model,
        api_key,
    }
}

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

pub fn effective_roles() -> BTreeMap<String, String> {
    let g = load_global();
    let p = load_project();
    let mut out = g.roles;
    for (k, v) in p.roles {
        out.insert(k, v);
    }
    out
}

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

fn default_true() -> bool {
    true
}

pub fn roles_enabled() -> bool {
    load_global().roles_enabled
}

pub fn set_roles_enabled(on: bool) -> Result<()> {
    let mut g = load_global();
    g.roles_enabled = on;
    save_global(&g)
}

pub fn gateway_enabled() -> bool {
    load_global()
        .gateway
        .unwrap_or_else(crate::gwclient::available)
}

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
    load_global().system_prompt.filter(|s| !s.trim().is_empty())
}

pub fn stream_enabled() -> bool {
    load_global().stream.unwrap_or(true)
}

pub fn local_system_prompt() -> String {
    load_global()
        .system_prompt_local
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(crate::local::system_local)
}

pub fn set_system_prompt_local(text: Option<String>) -> Result<()> {
    let mut g = load_global();
    g.system_prompt_local = text.filter(|s| !s.trim().is_empty());
    save_global(&g)
}

pub fn set_system_prompt_cloud(text: Option<String>) -> Result<()> {
    let mut g = load_global();
    g.system_prompt = text.filter(|s| !s.trim().is_empty());
    save_global(&g)
}

type CorruptionSlot = std::sync::Mutex<Option<(PathBuf, String, Option<PathBuf>)>>;

static CORRUPTION: std::sync::OnceLock<CorruptionSlot> = std::sync::OnceLock::new();

fn corruption_slot() -> &'static CorruptionSlot {
    CORRUPTION.get_or_init(|| std::sync::Mutex::new(None))
}

pub fn corruption() -> Option<String> {
    corruption_slot().lock().ok().and_then(|g| {
        g.as_ref().map(|(p, err, bak)| match bak {
            Some(b) => format!(
                "{} illisible ({err}). Copie conservée : {}. \
                 Les écritures sont refusées jusqu'à réparation.",
                p.display(),
                b.display()
            ),
            None => format!("{} illisible ({err}).", p.display()),
        })
    })
}

fn record_corruption(p: &Path, err: &str, backup: Option<PathBuf>) {
    if let Ok(mut g) = corruption_slot().lock() {
        *g = Some((p.to_path_buf(), err.to_string(), backup));
    }
}

fn clear_corruption_for(p: &Path) {
    if let Ok(mut g) = corruption_slot().lock() {
        if g.as_ref().map(|(x, _, _)| x == p).unwrap_or(false) {
            *g = None;
        }
    }
}

fn is_corrupt(p: &Path) -> bool {
    corruption_slot()
        .lock()
        .ok()
        .and_then(|g| g.as_ref().map(|(x, _, _)| x == p))
        .unwrap_or(false)
}

fn backup_corrupt(p: &Path) -> Option<PathBuf> {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let name = p.file_name()?.to_string_lossy().to_string();
    let dest = p.with_file_name(format!("{name}.corrupt-{ts}.bak"));
    fs::copy(p, &dest).ok().map(|_| dest)
}

fn read_toml<T: for<'de> Deserialize<'de>>(p: &Path) -> Option<T> {
    let raw = fs::read_to_string(p).ok()?;
    match toml::from_str(&raw) {
        Ok(v) => {
            clear_corruption_for(p);
            Some(v)
        }
        Err(e) => {
            let backup = backup_corrupt(p);
            record_corruption(p, &e.to_string(), backup);
            None
        }
    }
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
        let _ = fs::set_permissions(p, fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn existing_config_skips_the_wizard() {
        let mut cfg = GlobalConfig::default();
        assert!(!cfg.setup_done);
        cfg.model = Some("minimax/foo".into());
        apply_setup_migration(&mut cfg);
        assert!(
            cfg.setup_done,
            "un utilisateur existant ne doit pas voir /setup"
        );

        let mut fresh = GlobalConfig::default();
        apply_setup_migration(&mut fresh);
        assert!(
            !fresh.setup_done,
            "une config vierge doit lancer l'assistant"
        );
    }

    #[test]
    fn schema_version_is_versioned() {
        let g: GlobalConfig = toml::from_str("").unwrap();
        assert_eq!(g.schema_version, SCHEMA_VERSION);
        let g2: GlobalConfig = toml::from_str("schema_version = 7").unwrap();
        assert_eq!(g2.schema_version, 7);
        assert_eq!(GlobalConfig::default().schema_version, SCHEMA_VERSION);
    }

    #[test]
    fn checkout_data_is_migrated_without_overwrite() {
        let base = std::env::temp_dir().join(format!("zer0-mig-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let root = base.join("projet");
        let target = base.join("xdg");
        std::fs::create_dir_all(root.join("data/personas")).unwrap();
        std::fs::write(root.join("data/conversation.jsonl"), "{\"role\":\"user\"}").unwrap();
        std::fs::write(root.join("data/personas/alice.md"), "je suis alice").unwrap();
        std::fs::write(root.join("knowledge.db"), "db").unwrap();
        std::fs::write(root.join("knowledge.log"), "log").unwrap();

        migrate_checkout_data(&root, &target);
        assert!(target.join("conversation.jsonl").is_file());
        assert!(target.join("personas/alice.md").is_file());
        assert!(target.join("knowledge.db").is_file());
        assert!(target.join("knowledge.log").is_file());

        std::fs::write(target.join("conversation.jsonl"), "plus récent").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1100));
        std::fs::write(root.join("data/memory.jsonl"), "{\"plus\":\"récent\"}").unwrap();
        migrate_checkout_data(&root, &target);
        assert_eq!(
            std::fs::read_to_string(target.join("conversation.jsonl")).unwrap(),
            "plus récent"
        );
        assert_eq!(
            std::fs::read_to_string(target.join("memory.jsonl")).unwrap(),
            "{\"plus\":\"récent\"}"
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn corrupt_config_is_backed_up_and_flagged() {
        let dir = std::env::temp_dir().join(format!("zer0-cfg-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        std::fs::write(&path, "ceci n'est pas = du toml [[[").unwrap();

        let bad: Option<GlobalConfig> = read_toml(&path);
        assert!(bad.is_none());
        assert!(is_corrupt(&path));
        let msg = corruption().expect("état de corruption attendu");
        assert!(msg.contains("illisible"), "{msg}");

        let backup = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .any(|e| e.file_name().to_string_lossy().contains(".corrupt-"));
        assert!(backup, "aucune sauvegarde .corrupt-* créée");

        std::fs::write(&path, "roles_enabled = true\n").unwrap();
        let good: Option<GlobalConfig> = read_toml(&path);
        assert!(good.is_some());
        assert!(!is_corrupt(&path));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn theme_is_optional_and_roundtrips() {
        let fresh: GlobalConfig = toml::from_str("").unwrap();
        assert_eq!(fresh.theme, None);
        let configured: GlobalConfig = toml::from_str("theme = \"noir-rouge\"").unwrap();
        assert_eq!(configured.theme.as_deref(), Some("noir-rouge"));
    }

    #[test]
    fn roles_enabled_defaults_true_and_parses_false() {
        assert!(GlobalConfig::default().roles_enabled);
        let g: GlobalConfig = toml::from_str("").unwrap();
        assert!(g.roles_enabled);
        let g: GlobalConfig = toml::from_str("roles_enabled = false").unwrap();
        assert!(!g.roles_enabled);
        let g: GlobalConfig = toml::from_str("roles_enabled = true").unwrap();
        assert!(g.roles_enabled);
    }
}
