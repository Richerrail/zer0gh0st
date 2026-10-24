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
    /// Version du schéma de configuration (migrations futures).
    ///
    /// Absente d'un ancien fichier → valeur courante (rien à migrer encore).
    /// Supérieure à la version connue → avertissement : le fichier vient d'un
    /// binaire plus récent, on ne l'écrase pas à l'aveugle.
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
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
    /// Combien de fois zer0 a déjà proposé la connexion GitHub (vote).
    /// Sert à ne pas harceler : le rappel devient discret après quelques fois.
    #[serde(default)]
    pub github_prompt_count: u32,
    /// L'utilisateur a déjà vu l'invitation de connexion GitHub.
    #[serde(default)]
    pub github_prompted: bool,
    /// Raisonnement (`think`) activé au démarrage (choix de l'assistant).
    #[serde(default)]
    pub thinking: bool,
    /// Shell (`code_exec`) activé au démarrage. `false` par défaut : c'est un
    /// choix explicite, jamais une conséquence d'une installation.
    #[serde(default)]
    pub code_exec: bool,
    /// Assistant de première connexion terminé ?
    #[serde(default)]
    pub setup_done: bool,
    /// Étape de reprise de l'assistant (0 = pas commencé, 8 = terminé).
    #[serde(default)]
    pub setup_step: u8,
    /// Jeton exigé par les endpoints `/admin/*` de zer0-gw (en-tête `x-zer0-admin`).
    /// La variable d'environnement `ZER0_GW_ADMIN_TOKEN` est prioritaire.
    #[serde(default)]
    pub gw_admin_token: Option<String>,
}

/// Version courante du format de configuration.
pub const SCHEMA_VERSION: u32 = 1;

fn default_schema_version() -> u32 {
    SCHEMA_VERSION
}

// `Default` manuel : `roles_enabled` doit valoir `true` sur une config neuve
// (le `#[serde(default = …)]` ne couvre que la désérialisation, pas `default()`).
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
            github_client_id: None,
            github_prompt_count: 0,
            github_prompted: false,
            thinking: false,
            code_exec: false,
            setup_done: false,
            setup_step: 0,
            gw_admin_token: None,
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
/// Dossier de données de l'utilisateur.
///
/// Toujours hors du dépôt (XDG : `~/.local/share/zer0gh0st`), surchargeable par
/// `ZER0_DATA_DIR`. Avant, un checkout source écrivait `data/`, `knowledge.db`
/// et `knowledge.log` **dans le dépôt** : données personnelles dans l'arbre Git,
/// et échec en checkout en lecture seule. Les données existantes sont migrées
/// une fois (copie sans écrasement).
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
        migrate_old_data(&dir);
        dir
    })
    .clone()
}

/// Copie si la cible manque, **ou** si la source est plus récente.
///
/// Règle nécessaire pour les journaux JSONL : une base XDG créée par un ancien
/// lancement bloquait la copie d'un `memory.jsonl` plus récent resté dans le
/// checkout (21 entrées invisibles). On ne recopie jamais une source plus
/// ancienne que la cible.
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
        Err(_) => true, // cible absente
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

/// Copie les fichiers d'un dossier s'ils n'existent pas déjà dans la cible.
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

/// Migration des données historiquement écrites dans le checkout.
///
/// Fonction pure vis-à-vis des chemins (testable avec des dossiers temporaires).
pub(crate) fn migrate_checkout_data(root: &Path, target: &Path) {
    if root == target || !root.is_dir() {
        return;
    }
    let old = root.join("data");
    // Journaux append-only : on prend la version la plus récente des deux.
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

/// Cache de la config globale, invalidé par le `mtime` du fichier.
///
/// `load_global` est appelé dans des chemins chauds (résolution de clé, routage,
/// vote, mémoire, TUI) : relire et re-parser le TOML à chaque appel était de
/// l'I/O disque synchrone dans la boucle async.
type ConfigCache = std::sync::Mutex<Option<(PathBuf, Option<std::time::SystemTime>, GlobalConfig)>>;

fn cache() -> &'static ConfigCache {
    static C: std::sync::OnceLock<ConfigCache> = std::sync::OnceLock::new();
    C.get_or_init(|| std::sync::Mutex::new(None))
}

/// Une config qui a déjà un provider/modèle vient d'un utilisateur existant :
/// on ne lui impose pas l'assistant de première connexion.
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
        // Ancien fichier : on adopte la version courante en mémoire ; elle sera
        // écrite au prochain `save_global`.
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
    // On installe directement la valeur écrite : le `mtime` peut avoir la même
    // seconde que l'entrée en cache (granularité du système de fichiers).
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
    load_global()
        .gateway
        .unwrap_or_else(crate::gwclient::available)
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

/// Dernière corruption détectée : (chemin, message, sauvegarde éventuelle).
///
/// Pourquoi : `read_toml` renvoyait `None` sur un TOML invalide, que les
/// appelants traitaient comme « configuration absente » → valeurs par défaut.
/// Le `save_global` suivant **écrasait alors le fichier** et détruisait les clés
/// API de l'utilisateur, sans un mot. On sauvegarde le fichier illisible et on
/// refuse les écritures tant qu'il n'est pas réparé.
type CorruptionSlot = std::sync::Mutex<Option<(PathBuf, String, Option<PathBuf>)>>;

static CORRUPTION: std::sync::OnceLock<CorruptionSlot> = std::sync::OnceLock::new();

fn corruption_slot() -> &'static CorruptionSlot {
    CORRUPTION.get_or_init(|| std::sync::Mutex::new(None))
}

/// Message lisible si la config globale est illisible (pour l'UI / la CLI).
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

/// Copie le fichier illisible à côté, horodatée (jamais de perte silencieuse).
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
        // Config may contain API keys: keep it private.
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

    /// Régression : les données vivaient dans le checkout (arbre Git) et un
    /// checkout en lecture seule cassait l'application.
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

        // Une seconde passe ne doit jamais écraser une donnée plus récente.
        std::fs::write(target.join("conversation.jsonl"), "plus récent").unwrap();
        // ... et une source plus récente doit au contraire remplacer la cible.
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

    /// Régression : un TOML invalide était traité comme « config absente »
    /// (valeurs par défaut), et le `save_global` suivant écrasait le fichier —
    /// donc les clés API de l'utilisateur, sans avertissement.
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

        // Un fichier redevenu valide lève l'état (les écritures repassent).
        std::fs::write(&path, "roles_enabled = true\n").unwrap();
        let good: Option<GlobalConfig> = read_toml(&path);
        assert!(good.is_some());
        assert!(!is_corrupt(&path));

        let _ = std::fs::remove_dir_all(&dir);
    }

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
