//! Mode local : démarrer et surveiller `llama-server`, lister les GGUF.
//!
//! Le provider `local` ne fonctionne que si un serveur OpenAI-compatible écoute
//! sur `127.0.0.1:8080`. Jusqu'ici, l'utilisateur devait lancer `scripts/serve.sh`
//! lui-même — s'il l'oubliait, tout échouait avec des messages peu clairs.
//!
//! Ce module :
//! - liste les modèles réellement présents dans `<projet>/models` (liste utile
//!   même quand le serveur n'expose que le modèle chargé) ;
//! - démarre `llama-server` au premier tour si le provider est `local`,
//!   en arrière-plan, avec un journal dans le dossier de données ;
//! - ne tue **que** le serveur qu'il a lui-même démarré (fichier PID), jamais
//!   celui d'un utilisateur ou d'ollama ;
//! - redémarre ce serveur quand on change de modèle local.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};

/// URL attendue par le provider `local` (voir `providers.rs`).
pub const BASE_URL: &str = "http://127.0.0.1:8080";

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .filter(|v| *v > 0)
        .unwrap_or(default)
}

/// Dossier des GGUF : `ZERO_MODELS_DIR`, sinon `<projet>/models`.
pub fn models_dir() -> PathBuf {
    std::env::var("ZERO_MODELS_DIR")
        .ok()
        .map(|v| PathBuf::from(v.trim()))
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| crate::mods::project_root().join("models"))
}

/// Liste récursive des `.gguf` : `(id, chemin)`, triés par id.
///
/// L'id est le nom de fichier sans `.gguf` — c'est ce qu'on écrit dans
/// `config.model` et ce que le sélecteur affiche.
pub fn scan_models_in(dir: &Path) -> Vec<(String, PathBuf)> {
    fn walk(dir: &Path, out: &mut Vec<(String, PathBuf)>) {
        let Ok(rd) = std::fs::read_dir(dir) else {
            return;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out);
            } else if p
                .extension()
                .and_then(|x| x.to_str())
                .map(|x| x.eq_ignore_ascii_case("gguf"))
                .unwrap_or(false)
            {
                if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                    // Un `mmproj` est le projecteur multimodal d'un modèle, pas
                    // un modèle de chat : il ne doit pas apparaître dans /model.
                    if stem.to_ascii_lowercase().contains("mmproj") {
                        continue;
                    }
                    out.push((stem.to_string(), p));
                }
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, &mut out);
    out.sort_by_key(|(id, _)| id.to_lowercase());
    out.dedup_by(|a, b| a.0 == b.0);
    out
}

pub fn scan_models() -> Vec<(String, PathBuf)> {
    scan_models_in(&models_dir())
}

/// Résout le chemin du GGUF pour un identifiant de modèle.
///
/// Accepte : un chemin absolu existant, l'id exact (`qwen2.5-…`), ou un id
/// partiel (`qwen2.5-coder-1.5b` matche `…-q5_k_m`). Repli : le premier GGUF.
pub fn resolve_model(models: &[(String, PathBuf)], wanted: Option<&str>) -> Option<PathBuf> {
    if let Some(w) = wanted.map(str::trim).filter(|w| !w.is_empty()) {
        let p = Path::new(w);
        if p.is_absolute() && p.is_file() {
            return Some(p.to_path_buf());
        }
        if let Some((_, path)) = models.iter().find(|(id, _)| id == w) {
            return Some(path.clone());
        }
        let lower = w.to_lowercase();
        if let Some((_, path)) = models.iter().find(|(id, _)| {
            id.to_lowercase().starts_with(&lower) || id.to_lowercase().contains(&lower)
        }) {
            return Some(path.clone());
        }
    }
    models.first().map(|(_, p)| p.clone())
}

/// Binaire `llama-server` : `ZER0_LLAMA_SERVER`, puis le PATH, puis l'ancien
/// build local (conservé pour les utilisateurs qui l'ont déjà).
pub fn server_bin() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("ZER0_LLAMA_SERVER") {
        let p = PathBuf::from(p.trim());
        if p.is_file() {
            return Some(p);
        }
    }
    if crate::platform::has("llama-server") {
        return Some(PathBuf::from(if cfg!(windows) {
            "llama-server.exe"
        } else {
            "llama-server"
        }));
    }
    let legacy = crate::platform::home() + "/.local/share/zer0/llama.cpp/build/bin/llama-server";
    let legacy = PathBuf::from(legacy);
    legacy.is_file().then_some(legacy)
}

pub fn pid_path() -> PathBuf {
    crate::config::data_dir().join("llama-server.pid")
}

pub fn log_path() -> PathBuf {
    crate::config::data_dir().join("llama-server.log")
}

/// Le serveur répond-il déjà ?
pub async fn is_up() -> bool {
    let url = format!("{BASE_URL}/v1/models");
    match crate::http_client::HTTP_CLIENT
        .get(&url)
        .timeout(Duration::from_millis(1500))
        .send()
        .await
    {
        Ok(r) => r.status().is_success(),
        Err(_) => false,
    }
}

/// Démarre `llama-server` si nécessaire. Retourne un message lisible.
///
/// `wanted` est le modèle demandé (config) ; les GGUF du dossier servent à le
/// résoudre. Échoue avec une consigne claire si aucun binaire/modèle.
pub async fn ensure(wanted: Option<&str>) -> Result<String> {
    // Pré-démarrage en tâche de fond + premier message peuvent arriver en
    // même temps : sans verrou, on lançait deux serveurs.
    static ENSURE_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    let _guard = ENSURE_LOCK.lock().await;
    if is_up().await {
        return Ok("serveur local déjà actif".to_string());
    }
    let bin = server_bin().ok_or_else(|| {
        anyhow!(
            "llama-server introuvable — installe llama.cpp (`scripts/serve.sh` en a besoin) \
             ou définis ZER0_LLAMA_SERVER=/chemin/llama-server"
        )
    })?;
    let models = scan_models();
    if models.is_empty() {
        return Err(anyhow!(
            "aucun GGUF dans {} — dépose un modèle .gguf ou définis ZERO_MODELS_DIR",
            models_dir().display()
        ));
    }
    let model =
        resolve_model(&models, wanted).ok_or_else(|| anyhow!("aucun modèle local résolu"))?;

    // Tous les cœurs disponibles par défaut (4 ici, 16 ailleurs). Le
    // garde-fou thermique (`thermal`) coupe avant la surchauffe ; `ZER0_THREADS`
    // permet de brider manuellement.
    let threads = env_u64("ZER0_THREADS", all_cores());
    // Préfill à la moitié des cœurs : sur un CPU à FPU partagés (FX-4100),
    // 4 threads de batch ont déjà crashé la machine ; 2 = 24 s au lieu de 33 s
    // pour « salut ». `ZER0_THREADS_BATCH` permet d'ajuster.
    let batch = env_u64("ZER0_THREADS_BATCH", (threads / 2).max(1));
    // Contexte volontairement petit : en chat, 2048 suffit et le préfill
    // coûte proportionnellement moins cher.
    let ctx = env_u64("ZER0_CTX", 2048);

    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path())
        .ok();
    let mut cmd = tokio::process::Command::new(&bin);
    cmd.arg("-m")
        .arg(&model)
        .arg("-c")
        .arg(ctx.to_string())
        .arg("--port")
        .arg("8080")
        .arg("--host")
        .arg("127.0.0.1")
        .arg("--jinja")
        // Les modèles « reasoning » (qwen3.5, DeepSeek distill…) brûlaient tout
        // `max_tokens` en `reasoning_content` : `content` vide, réponse perdue.
        // Budget 0 = raisonnement immédiatement clos (llama.cpp ≥ 0.6).
        .arg("--reasoning-budget")
        .arg("0")
        // Sans warmup, le serveur est prêt en ~4 s au lieu de ~38 s : le
        // warmup « à vide » préfillait tout le modèle pour rien.
        .arg("--no-warmup")
        .arg("-t")
        .arg(threads.to_string())
        .arg("-tb")
        .arg(batch.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::null());
    match log {
        Some(f) => {
            cmd.stderr(Stdio::from(f));
        }
        None => {
            cmd.stderr(Stdio::null());
        }
    }
    let child = cmd
        .spawn()
        .with_context(|| format!("démarrage de {}", bin.display()))?;
    let pid = child.id();
    if let Some(pid) = pid {
        let _ = std::fs::write(pid_path(), format!("{pid}\n"));
    }

    // Attente de disponibilité : un 4B Q4 met ~30 s à charger sur ce CPU.
    let deadline = std::time::Instant::now() + Duration::from_secs(120);
    while std::time::Instant::now() < deadline {
        if is_up().await {
            return Ok(format!(
                "llama-server démarré — {} ({} cœurs, ctx {ctx})",
                model.display(),
                threads
            ));
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    Err(anyhow!(
        "llama-server n'a pas répondu en 120 s — journal : {}",
        log_path().display()
    ))
}

/// Tue le serveur **uniquement s'il a été démarré par Zer0** (fichier PID).
pub fn stop_if_owned() -> bool {
    let Ok(raw) = std::fs::read_to_string(pid_path()) else {
        return false;
    };
    let Ok(pid) = raw.trim().parse::<u32>() else {
        let _ = std::fs::remove_file(pid_path());
        return false;
    };
    #[cfg(unix)]
    let killed = std::process::Command::new("kill")
        .args(["-9", &pid.to_string()])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    #[cfg(windows)]
    let killed = std::process::Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/F"])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    let _ = std::fs::remove_file(pid_path());
    killed
}

/// Redémarre le serveur local sur un autre modèle, seulement si c'est le nôtre.
/// Retourne un message quand une action a eu lieu.
pub async fn restart_if_owned(wanted: &str) -> Option<String> {
    if !pid_path().exists() {
        return None;
    }
    stop_if_owned();
    tokio::time::sleep(Duration::from_millis(400)).await;
    match ensure(Some(wanted)).await {
        Ok(msg) => Some(format!("serveur local redémarré — {msg}")),
        Err(e) => Some(format!(
            "serveur local arrêté mais non redémarré : {e}. Relance `scripts/serve.sh`."
        )),
    }
}

/// Fonctionnalités qui supposent plusieurs modèles : inutilisables en local.
/// Prompt système par défaut du mode local.
///
/// Volontairement **court** : sur CPU, chaque centaine de tokens de préfixe
/// coûte plusieurs secondes de préfill. Il dit l'essentiel (agir, vérifier,
/// ne pas inventer) et tait les fonctions cloud indisponibles.
pub const LOCAL_SYSTEM: &str = "Tu es ZER0|GH0ST, agent local (llama.cpp, CPU seul).\n\
Réponds en français, court et direct : pas de plan inutile, pas de répétition.\n\
Agis avec les outils (fichiers, commandes, recherche) au lieu d'expliquer ce qu'il faudrait faire.\n\
Après un outil, lis son résultat et continue ; si ça échoue, cite l'erreur exacte.\n\
N'annonce jamais une action non confirmée par un résultat d'outil.\n\
Indisponible en local : 0box, /role, /route, /select, embeddings — n'en parle pas.";

/// Choisit le prompt de base selon le provider : local dédié, cloud inchangé.
pub fn pick_system(provider_id: &str, cloud: &str, local: &str) -> String {
    if provider_id == "local" {
        local.to_string()
    } else {
        cloud.to_string()
    }
}

/// Nombre de cœurs logiques disponibles.
fn all_cores() -> u64 {
    std::thread::available_parallelism()
        .map(|n| n.get() as u64)
        .unwrap_or(4)
}

/// Prompt local effectif, avec les caractéristiques **réelles** de la machine.
pub fn system_local() -> String {
    format!(
        "{LOCAL_SYSTEM}\nMachine : {} cœurs logiques, CPU seul (pas de GPU utilisable).",
        all_cores()
    )
}

pub fn limitations() -> &'static [&'static str] {
    &[
        "rôles (`/role`) : un seul modèle chargé, pas de sélection par catégorie",
        "bascule (`/route`) : rien vers quoi basculer",
        "débat multi-agents (`0box`) : il faut plusieurs modèles",
        "recommandation de modèle (`/select`) : catalogue cloud",
        "embeddings (`/embeddings`) : requiert un provider d'embedding séparé",
        "outils : le local est un chat conversationnel ; l'agent outillé passe par le cloud",
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(dir: &Path, name: &str) -> PathBuf {
        let p = dir.join(name);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, b"x").unwrap();
        p
    }

    #[test]
    fn scan_finds_gguf_recursively_and_ignores_others() {
        let dir = std::env::temp_dir().join(format!("zer0-local-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        touch(&dir, "a/b/model-1.gguf");
        touch(&dir, "model-2.GGUF");
        touch(&dir, "notes.txt");
        touch(&dir, "Ternary-Bonsai-2-27B-mmproj-BF16.gguf");
        let list = scan_models_in(&dir);
        let ids: Vec<&str> = list.iter().map(|(id, _)| id.as_str()).collect();
        assert_eq!(ids, vec!["model-1", "model-2"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn resolve_model_accepts_exact_partial_and_path() {
        let models = vec![
            (
                "qwen2.5-coder-1.5b-instruct-q5_k_m".to_string(),
                PathBuf::from("/m/a.gguf"),
            ),
            (
                "qwen3.5-4B-super-coder.Q4_0".to_string(),
                PathBuf::from("/m/b.gguf"),
            ),
        ];
        assert_eq!(
            resolve_model(&models, Some("qwen3.5-4B-super-coder.Q4_0")),
            Some(PathBuf::from("/m/b.gguf"))
        );
        // id partiel
        assert_eq!(
            resolve_model(&models, Some("qwen2.5-coder-1.5b")),
            Some(PathBuf::from("/m/a.gguf"))
        );
        // inconnu → premier
        assert_eq!(
            resolve_model(&models, Some("inexistant")),
            Some(PathBuf::from("/m/a.gguf"))
        );
        assert_eq!(
            resolve_model(&models, None),
            Some(PathBuf::from("/m/a.gguf"))
        );
        // chemin absolu existant (le fichier de test courant)
        let me = std::env::current_exe().unwrap();
        assert_eq!(resolve_model(&models, me.to_str()), Some(me));
    }

    #[test]
    fn pick_system_routes_local_and_cloud() {
        assert_eq!(pick_system("local", "cloud!", "local!"), "local!");
        assert_eq!(pick_system("openrouter", "cloud!", "local!"), "cloud!");
    }

    #[test]
    fn system_local_reports_real_cores() {
        let s = system_local();
        assert!(s.contains(&format!("{} cœurs logiques", all_cores())));
        assert!(!s.contains("2 cœurs"), "compte de cœurs codé en dur");
    }

    #[test]
    fn default_local_system_is_short_and_action_oriented() {
        assert!(LOCAL_SYSTEM.chars().count() < 900, "trop long pour du CPU");
        for key in ["outil", "résultat", "0box"] {
            assert!(LOCAL_SYSTEM.contains(key), "manque {key}");
        }
    }

    #[test]
    fn limitations_mention_the_multi_model_features() {
        let text = limitations().join(" ");
        for key in ["role", "route", "0box"] {
            assert!(text.contains(key), "manque {key}");
        }
    }
}
