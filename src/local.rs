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

/// Contenu du fichier PID : `pid` (ancien format) ou `pid starttime`
/// (nouveau format, avec le champ 22 de `/proc/<pid>/stat` au démarrage).
///
/// Le `starttime` permet de détecter la réutilisation du PID par un autre
/// processus après la mort du serveur : même PID, `starttime` différent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnedPid {
    pub pid: u32,
    pub starttime: Option<u64>,
}

/// Lit et parse le fichier PID. `None` si absent/illisible/invalide.
pub fn read_owned_pid() -> Option<OwnedPid> {
    read_pid_at(&pid_path())
}

/// Même chose pour un fichier arbitraire (le serveur d'embedding a le sien).
fn read_pid_at(path: &Path) -> Option<OwnedPid> {
    let raw = std::fs::read_to_string(path).ok()?;
    parse_owned_pid(&raw)
}

fn parse_owned_pid(raw: &str) -> Option<OwnedPid> {
    let mut it = raw.split_whitespace();
    let pid: u32 = it.next()?.parse().ok()?;
    let starttime = it.next().and_then(|s| s.parse::<u64>().ok());
    Some(OwnedPid { pid, starttime })
}

/// Écrit le fichier PID au format `pid starttime` (starttime lu dans
/// `/proc/<pid>/stat` ; absent si illisible — repli sur le seul PID).
fn write_owned_pid(pid: u32) {
    write_pid_at(&pid_path(), pid);
}

/// Écrit `pid starttime` à l'emplacement donné (chat ou embedder).
fn write_pid_at(path: &Path, pid: u32) {
    let start = proc_starttime(pid);
    let content = match start {
        Some(t) => format!("{pid} {t}\n"),
        None => format!("{pid}\n"),
    };
    let _ = std::fs::write(path, content);
}

/// Champ 22 de `/proc/<pid>/stat` (jiffies depuis le boot). `None` si le
/// processus n'existe pas ou si `/proc` est indisponible.
#[cfg(target_os = "linux")]
fn proc_starttime(pid: u32) -> Option<u64> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // Le champ 2 (comm) peut contenir des espaces/parenthèses : on part
    // après la dernière `)`.
    let after = stat.rsplit_once(')')?.1;
    // after = " <state> <ppid> …" ; starttime est le champ 22 global,
    // soit le 20e champ après comm (champ 3 = state).
    after.split_whitespace().nth(19)?.parse().ok()
}

#[cfg(not(target_os = "linux"))]
fn proc_starttime(_pid: u32) -> Option<u64> {
    None
}

/// Nom du processus (`/proc/<pid>/comm` sous Linux). `None` si mort.
#[cfg(target_os = "linux")]
fn proc_comm(pid: u32) -> Option<String> {
    std::fs::read_to_string(format!("/proc/{pid}/comm"))
        .ok()
        .map(|s| s.trim().to_string())
}

#[cfg(not(target_os = "linux"))]
#[allow(dead_code)] // pas de /proc : le repli n'appelle pas cette fonction
fn proc_comm(_pid: u32) -> Option<String> {
    None
}

/// Le PID désigne-t-il bien le `llama-server` démarré par Zer0 ?
///
/// Vérifie, dans l'ordre :
/// 1. le processus existe (`/proc/<pid>` sous Linux) ;
/// 2. son nom est `llama-server` (ou `llama-server.exe`) — protège contre
///    la réutilisation du PID par un processus quelconque ;
/// 3. si un `starttime` est stocké, il correspond à celui du processus
///    actuel — protège contre la réutilisation par un autre `llama-server`.
fn is_owned_llama(pid: &OwnedPid) -> bool {
    #[cfg(target_os = "linux")]
    {
        let Some(comm) = proc_comm(pid.pid) else {
            return false;
        };
        let base = comm.trim_end_matches(".exe");
        if base != "llama-server" {
            return false;
        }
        if let Some(expected) = pid.starttime {
            return proc_starttime(pid.pid) == Some(expected);
        }
        true
    }
    #[cfg(all(unix, not(target_os = "linux")))]
    {
        // Pas de `/proc`, donc pas de vérification du nom : mais `kill -0` dit
        // si le processus vit encore. Sans ça, un PID mort passait pour « le
        // nôtre » et on annonçait un redémarrage fantôme (B4, macOS).
        alive(pid.pid)
    }
    #[cfg(not(unix))]
    {
        // Windows : ni `/proc` ni `kill -0` sans dépendance supplémentaire.
        // Repli assumé sur le PID (comportement historique) — un PID réutilisé
        // par un autre processus n'est pas détecté ici.
        let _ = pid;
        true
    }
}

/// Nettoie un fichier PID périmé (processus mort, nom différent, ou
/// `starttime` divergent). Retourne `true` si un fichier a été supprimé.
pub fn cleanup_stale_pid() -> bool {
    let Some(owned) = read_owned_pid() else {
        // Fichier absent ou illisible : rien à nettoyer (un fichier
        // illisible est supprimé par `stop_if_owned`).
        return false;
    };
    if is_owned_llama(&owned) {
        return false;
    }
    let _ = std::fs::remove_file(pid_path());
    true
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
    // PID périmé (serveur mort ou PID réutilisé) : on nettoie avant tout,
    // sinon `stop_if_owned`/`restart_if_owned` croiraient posséder un
    // serveur qui n'est plus le nôtre.
    cleanup_stale_pid();
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
    // GPU : les autres utilisateurs en ont. `auto` (défaut) demande au binaire
    // ses backends (CUDA/Vulkan/ROCm) ; macOS = Metal, toujours disponible.
    let gpu = gpu_layers(&bin);

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
        // Un seul slot (chat mono-utilisateur) et buffers dimensionnés pour
        // nos prompts courts : 3 s au lieu de 4,4 s au démarrage.
        .arg("-np")
        .arg("1")
        .arg("-b")
        .arg("512")
        .arg("-ub")
        .arg("256")
        .arg("-t")
        .arg(threads.to_string())
        .arg("-tb")
        .arg(batch.to_string());
    if gpu > 0 {
        // `-ngl 999` = offload maximal (llama.cpp plafonne aux couches réelles).
        cmd.arg("-ngl").arg(gpu.to_string());
    }
    cmd.stdin(Stdio::null()).stdout(Stdio::null());
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
        write_owned_pid(pid);
    }

    // Attente de disponibilité : un 4B Q4 met ~30 s à charger sur ce CPU.
    let deadline = std::time::Instant::now() + Duration::from_secs(120);
    while std::time::Instant::now() < deadline {
        if is_up().await {
            let gpu_txt = if gpu > 0 {
                format!(", GPU {gpu} couches")
            } else {
                ", CPU seul".to_string()
            };
            return Ok(format!(
                "llama-server démarré — {} ({} cœurs, ctx {ctx}{gpu_txt})",
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

/// Envoie un signal (`kill -TERM`/`-9`) ; `false` si le processus est déjà mort.
#[cfg(unix)]
fn signal(pid: u32, sig: &str) -> bool {
    std::process::Command::new("kill")
        .args([sig, &pid.to_string()])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Le processus existe-t-il encore ? (`kill -0`, sans envoyer de signal)
#[cfg(unix)]
fn alive(pid: u32) -> bool {
    std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Tue le serveur **uniquement s'il a été démarré par Zer0** (fichier PID).
///
/// Si le PID est périmé (mort, réutilisé par un autre processus, ou
/// `starttime` divergent), le fichier est supprimé et **aucun signal n'est
/// envoyé** — on ne tue jamais un processus qui n'est pas notre serveur.
pub fn stop_if_owned() -> bool {
    let Some(owned) = read_owned_pid() else {
        // Fichier absent ou illisible/invalide : on le supprime s'il existe.
        if pid_path().exists() {
            let _ = std::fs::remove_file(pid_path());
        }
        return false;
    };
    if !is_owned_llama(&owned) {
        let _ = std::fs::remove_file(pid_path());
        return false;
    }
    let pid = owned.pid;
    // Arrêt gracieux d'abord : SIGTERM, puis SIGKILL seulement si le
    // processus est encore vivant après ~2 s. `kill -9` d'emblée ne laissait
    // aucune chance au serveur de libérer VRAM/KV et de fermer son log.
    #[cfg(unix)]
    let killed = {
        let term = signal(pid, "-TERM");
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while std::time::Instant::now() < deadline && alive(pid) {
            std::thread::sleep(Duration::from_millis(100));
        }
        if alive(pid) {
            signal(pid, "-9")
        } else {
            // SIGTERM suffit : succès même si `kill` a renvoyé un code non nul
            // (course avec la sortie du processus).
            let _ = term;
            true
        }
    };
    #[cfg(windows)]
    let killed = std::process::Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    let _ = std::fs::remove_file(pid_path());
    killed
}

/// Faut-il redémarrer ? Uniquement si un fichier PID existe **et** désigne
/// encore notre `llama-server` (B4 : un fichier périmé ne compte pas).
///
/// Fonction pure : `pid_path()` dépend de `config::data_dir()`, figé par un
/// `OnceLock`, donc non injectable dans un test.
fn should_restart(pid_file_exists: bool, owned: Option<&OwnedPid>) -> bool {
    pid_file_exists && owned.is_some_and(is_owned_llama)
}

/// Redémarre le serveur local sur un autre modèle, seulement si c'est le nôtre.
/// Retourne un message quand une action a eu lieu.
pub async fn restart_if_owned(wanted: &str) -> Option<String> {
    // B4 : nettoyer AVANT le test `exists()`. Un fichier PID périmé (serveur
    // mort, PID réutilisé) faisait entrer dans la branche : `stop_if_owned`
    // supprimait le fichier sans rien tuer, puis `ensure` démarrait un serveur
    // et on annonçait « redémarré » alors qu'aucun serveur ne tournait.
    cleanup_stale_pid();
    if !should_restart(pid_path().exists(), read_owned_pid().as_ref()) {
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
/// Une seule phrase : chaque token de préfixe coûte du préfill sur CPU, et un
/// modèle de chat n'a pas besoin d'un mode d'emploi pour dire bonjour. Les
/// consignes d'action (outils, vérification) ne concernent que le cloud, qui
/// garde son prompt complet.
pub const LOCAL_SYSTEM: &str = "Je suis Zer0, ton AI conversationnel ultra rapide.";

/// Prompt local effectif (config `/system local` sinon le défaut).
pub fn system_local() -> String {
    LOCAL_SYSTEM.to_string()
}

/// Choisit le prompt de base selon le provider : local dédié, cloud inchangé.
pub fn pick_system(provider_id: &str, cloud: &str, local: &str) -> String {
    if provider_id == "local" {
        local.to_string()
    } else {
        cloud.to_string()
    }
}

/// Couches à décharger sur GPU. `ZER0_GPU_LAYERS` : `auto` (défaut), `0`, ou N.
fn gpu_layers(bin: &std::path::Path) -> u64 {
    match std::env::var("ZER0_GPU_LAYERS").ok().as_deref() {
        Some("0") => return 0,
        Some(v) if !v.eq_ignore_ascii_case("auto") => {
            return v.trim().parse().unwrap_or(0);
        }
        _ => {}
    }
    // macOS : Metal est le backend par défaut de llama.cpp.
    if cfg!(target_os = "macos") {
        return 999;
    }
    if detect_gpu(bin) {
        999
    } else {
        0
    }
}

/// Interroge le binaire : `--list-devices` (CUDA/Vulkan/ROCm/Metal/SYCL).
fn detect_gpu(bin: &std::path::Path) -> bool {
    let Ok(out) = std::process::Command::new(bin)
        .arg("--list-devices")
        .output()
    else {
        return false;
    };
    if !out.status.success() {
        return false;
    }
    let mut s = String::from_utf8_lossy(&out.stdout).to_string();
    s.push_str(&String::from_utf8_lossy(&out.stderr));
    s.lines().any(|l| {
        let l = l.trim_start();
        ["CUDA", "Vulkan", "ROCm", "Metal", "SYCL"]
            .iter()
            .any(|p| l.starts_with(p) && l.contains(':'))
    })
}

/// Nombre de cœurs logiques disponibles.
fn all_cores() -> u64 {
    std::thread::available_parallelism()
        .map(|n| n.get() as u64)
        .unwrap_or(4)
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
    fn default_local_system_is_a_single_short_phrase() {
        assert!(LOCAL_SYSTEM.chars().count() <= 60, "trop long pour du CPU");
        assert!(LOCAL_SYSTEM.contains("Zer0"));
        assert_eq!(system_local(), LOCAL_SYSTEM);
    }

    /// B4 : un fichier PID périmé ne doit pas déclencher un « redémarrage ».
    #[test]
    fn should_restart_ignores_a_stale_pid_file() {
        // Pas de fichier : rien à redémarrer.
        assert!(!should_restart(false, None));
        // Fichier présent mais illisible/invalide : rien à redémarrer.
        assert!(!should_restart(true, None));
        // Fichier présent, PID mort : `is_owned_llama` est faux -> pas de restart.
        // Vrai sous Linux (`/proc`) et macOS (`kill -0`) ; sous Windows il n'y a
        // ni l'un ni l'autre, donc le repli fait confiance au PID (assumé).
        let dead = OwnedPid {
            pid: u32::MAX,
            starttime: None,
        };
        #[cfg(not(windows))]
        assert!(!should_restart(true, Some(&dead)));
        // Fichier absent mais PID lisible (course) : pas de restart non plus.
        assert!(!should_restart(false, Some(&dead)));
    }

    #[test]
    fn parse_owned_pid_accepts_old_and_new_formats() {
        assert_eq!(
            parse_owned_pid("442035\n"),
            Some(OwnedPid {
                pid: 442035,
                starttime: None
            })
        );
        assert_eq!(
            parse_owned_pid("1305 1234567\n"),
            Some(OwnedPid {
                pid: 1305,
                starttime: Some(1234567)
            })
        );
        assert_eq!(parse_owned_pid(""), None);
        assert_eq!(parse_owned_pid("abc\n"), None);
        assert_eq!(
            parse_owned_pid("0\n"),
            Some(OwnedPid {
                pid: 0,
                starttime: None
            })
        );
    }

    #[test]
    fn dead_pid_is_not_owned() {
        #[cfg(target_os = "linux")]
        {
            // PID quasi certainement mort (au-delà de pid_max usuel).
            let dead = OwnedPid {
                pid: 4_194_303,
                starttime: None,
            };
            assert!(!is_owned_llama(&dead));
        }
    }

    #[test]
    fn current_process_is_not_a_llama_server() {
        // Notre propre processus de test ne s'appelle pas llama-server :
        // même avec son vrai starttime, il ne doit pas être considéré
        // comme notre serveur.
        #[cfg(target_os = "linux")]
        {
            let me = std::process::id();
            let owned = OwnedPid {
                pid: me,
                starttime: proc_starttime(me),
            };
            assert!(!is_owned_llama(&owned));
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

// ═══════════════════════════════════════════════════════════════════════════
// Serveur d'embedding local (`local-embed`)
// ═══════════════════════════════════════════════════════════════════════════
//
// Pourquoi un second serveur ? Le serveur de chat charge UN modèle et sert
// `/chat/completions`. L'embedding a besoin d'un modèle différent (bge-m3) et
// de `--embeddings --pooling mean`. Deux processus, deux ports, deux fichiers
// PID : le chat peut redémarrer ou changer de modèle sans invalider les
// vecteurs déjà calculés.
//
// Ce que ça supprime : la dépendance réseau. `memory::embed` ne change pas
// d'une ligne — il POSTe sur `{base}/embeddings`, ici en boucle locale.

/// Port du serveur d'embedding (le chat garde 8080).
pub const EMBED_PORT: u16 = 8082;

/// Un modèle d'embedding connu : URL, taille et empreinte.
///
/// L'empreinte est vérifiée après téléchargement : un fichier corrompu, tronqué
/// ou remplacé par autre chose est refusé au lieu de produire des vecteurs
/// silencieusement faux.
pub struct EmbedModel {
    pub name: &'static str,
    pub url: &'static str,
    pub bytes: u64,
    pub sha256: &'static str,
    pub note: &'static str,
}

pub const EMBED_MODELS: &[EmbedModel] = &[EmbedModel {
    name: "bge-m3-q8_0",
    url: "https://huggingface.co/vonjack/bge-m3-gguf/resolve/main/bge-m3-q8_0.gguf",
    // Mesurés sur l'artefact réel (téléchargé une fois, `sha256sum`). Un
    // fichier d'une *autre* provenance peut être valide et différent : il n'est
    // alors pas « vérifié », ce que `kb embed status` dit explicitement.
    bytes: 617_542_656,
    sha256: "25a1a713cc7e2c1b599a3c09dbf9a9a7774452312159434ceb335004ccc11255",
    note: "bge-m3 Q8_0 — multilingue (100+ langues), 1024 dims, mean pooling",
}];

/// Dossier d'accueil des modèles d'embedding : `<données>/models`.
pub fn embed_models_dir() -> PathBuf {
    crate::config::data_dir().join("models")
}

/// Où est (ou serait) le GGUF d'embedding `name`.
///
/// Ordre : `ZER0_EMBED_MODEL` (chemin explicite) → `<données>/models/<nom>.gguf`
/// → `<projet>/models/<nom>.gguf` (là où l'utilisateur a pu le déposer).
pub fn embed_model_path(name: &str) -> PathBuf {
    if let Ok(p) = std::env::var("ZER0_EMBED_MODEL") {
        let p = PathBuf::from(p.trim());
        if !p.as_os_str().is_empty() {
            return p;
        }
    }
    let data = embed_models_dir().join(format!("{name}.gguf"));
    if data.is_file() {
        return data;
    }
    let repo = models_dir().join(format!("{name}.gguf"));
    if repo.is_file() {
        return repo;
    }
    data
}

/// Le GGUF existe-t-il réellement ?
pub fn embed_model_present(name: &str) -> bool {
    embed_model_path(name).is_file()
}

pub fn embed_base_url() -> String {
    std::env::var("ZER0_EMBED_URL")
        .ok()
        .map(|v| v.trim().trim_end_matches('/').to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| format!("http://127.0.0.1:{EMBED_PORT}"))
}

pub fn embed_pid_path() -> PathBuf {
    crate::config::data_dir().join("llama-embed.pid")
}

pub fn embed_log_path() -> PathBuf {
    crate::config::data_dir().join("llama-embed.log")
}

/// Arguments de `llama-server` pour l'embedding. Fonction **pure** : les
/// drapeaux sont testables sans démarrer de processus.
///
/// `--pooling mean` : bge-m3 est entraîné avec un pooling moyen ; un `cls`
/// donnerait des vecteurs silencieusement dégradés (pas d'erreur, juste de
/// moins bons résultats — le pire des cas).
pub fn embed_args(model: &Path, threads: u64, ctx: u64) -> Vec<String> {
    vec![
        "-m".into(),
        model.display().to_string(),
        "--embeddings".into(),
        "--pooling".into(),
        "mean".into(),
        "--port".into(),
        EMBED_PORT.to_string(),
        "--host".into(),
        "127.0.0.1".into(),
        "-c".into(),
        ctx.to_string(),
        "-t".into(),
        threads.to_string(),
        // Pas de warmup : le chargement de bge-m3 prend ~2 s, inutile d'en
        // ajouter dix de plus.
        "--no-warmup".into(),
        "-np".into(),
        "1".into(),
    ]
}

/// Le serveur d'embedding répond-il ?
pub async fn embed_is_up() -> bool {
    let url = format!("{}/v1/models", embed_base_url());
    match crate::http_client::HTTP_CLIENT
        .get(&url)
        .timeout(Duration::from_millis(1200))
        .send()
        .await
    {
        Ok(r) => r.status().is_success(),
        Err(_) => false,
    }
}

fn cleanup_stale_embed_pid() -> bool {
    let path = embed_pid_path();
    let Some(owned) = read_pid_at(&path) else {
        return false;
    };
    if is_owned_llama(&owned) {
        return false;
    }
    let _ = std::fs::remove_file(&path);
    true
}

/// Nombre de threads de l'embedding : `ZER0_EMBED_THREADS`, sinon 2.
///
/// Deux, volontairement : un embedding ne gagne presque rien à saturer les
/// cœurs, et un serveur qui prend 4 cœurs sur 4 rend la machine inutilisable
/// pendant l'ingestion (vécu : plantage du poste).
pub fn embed_threads() -> u64 {
    env_u64("ZER0_EMBED_THREADS", 2)
}

/// Contexte : 1024 tokens suffisent pour un chunk de ~1000 caractères.
fn embed_ctx() -> u64 {
    env_u64("ZER0_EMBED_CTX", 1024)
}

/// Démarre le serveur d'embedding si nécessaire. Retourne un message lisible.
///
/// Un seul démarrage à la fois (verrou) : la recherche et l'ingestion peuvent
/// l'appeler en parallèle au premier tour.
pub async fn ensure_embed(wanted: Option<&str>) -> Result<String> {
    static LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    let _guard = LOCK.lock().await;
    cleanup_stale_embed_pid();
    if embed_is_up().await {
        return Ok("serveur d'embedding déjà actif".to_string());
    }
    let name = wanted
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| EMBED_MODELS.first().map(|m| m.name.to_string()))
        .ok_or_else(|| anyhow!("aucun modèle d'embedding configuré"))?;
    let model = embed_model_path(&name);
    if !model.is_file() {
        return Err(anyhow!(
            "GGUF d'embedding absent : {} — lance `zer0 kb embed fetch {name}` \
             (ou dépose le fichier à cet emplacement, ou définis ZER0_EMBED_MODEL)",
            model.display()
        ));
    }
    let bin = server_bin().ok_or_else(|| {
        anyhow!("llama-server introuvable — installe llama.cpp ou définis ZER0_LLAMA_SERVER")
    })?;
    let threads = embed_threads();
    let ctx = embed_ctx();
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(embed_log_path())
        .ok();
    let mut cmd = tokio::process::Command::new(&bin);
    cmd.args(embed_args(&model, threads, ctx));
    cmd.stdin(Stdio::null()).stdout(Stdio::null());
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
    if let Some(pid) = child.id() {
        write_pid_at(&embed_pid_path(), pid);
    }
    let deadline = std::time::Instant::now() + Duration::from_secs(90);
    while std::time::Instant::now() < deadline {
        if embed_is_up().await {
            return Ok(format!(
                "serveur d'embedding démarré — {} ({} threads, ctx {ctx}, port {EMBED_PORT})",
                model.display(),
                threads
            ));
        }
        tokio::time::sleep(Duration::from_millis(400)).await;
    }
    Err(anyhow!(
        "serveur d'embedding muet après 90 s — journal : {}",
        embed_log_path().display()
    ))
}

/// Arrête le serveur d'embedding **si c'est bien le nôtre**.
pub fn stop_embed_if_owned() -> bool {
    cleanup_stale_embed_pid();
    let Some(owned) = read_pid_at(&embed_pid_path()) else {
        return false;
    };
    if !is_owned_llama(&owned) {
        let _ = std::fs::remove_file(embed_pid_path());
        return false;
    }
    let pid = owned.pid;
    #[cfg(unix)]
    {
        signal(pid, "-TERM");
        for _ in 0..20 {
            if !alive(pid) {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        if alive(pid) {
            signal(pid, "-9");
        }
    }
    #[cfg(not(unix))]
    {
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/F"])
            .status();
    }
    let _ = std::fs::remove_file(embed_pid_path());
    true
}

/// État lisible du serveur d'embedding (CLI `/kb embed status`).
pub async fn embed_status() -> Vec<String> {
    let up = embed_is_up().await;
    let owned = read_pid_at(&embed_pid_path());
    let mut out = vec![format!(
        "Serveur d'embedding : {}{}",
        embed_base_url(),
        if up { "  ✅ actif" } else { "  ⛔ arrêté" },
    )];
    if let Some(o) = owned {
        out.push(format!("  PID détenu : {} (le nôtre)", o.pid));
    }
    out.push(format!(
        "  Threads : {} (ZER0_EMBED_THREADS)",
        embed_threads()
    ));
    for m in EMBED_MODELS {
        let path = embed_model_path(m.name);
        let state = if path.is_file() {
            match std::fs::metadata(&path) {
                Ok(md) if md.len() == m.bytes => {
                    "présent, conforme à l'artefact épinglé ✅".to_string()
                }
                Ok(md) => format!(
                    "présent ({} o ≠ {} o) — fichier local non vérifié, utilisable",
                    md.len(),
                    m.bytes
                ),
                Err(e) => format!("illisible ⚠ ({e})"),
            }
        } else {
            "absent → `zer0 kb embed fetch`".to_string()
        };
        out.push(format!("  {:<14} {} — {}", m.name, path.display(), state));
        out.push(format!("  {:<14} {}", "", m.note));
    }
    out
}

/// Télécharge un modèle d'embedding connu, vérifie son SHA256, puis le range
/// de façon atomique. `Ok(message)` décrit ce qui a été fait.
pub async fn fetch_embed_model(name: &str) -> Result<String> {
    use sha2::{Digest, Sha256};
    use std::io::Write;

    let Some(entry) = EMBED_MODELS.iter().find(|m| m.name == name) else {
        let known: Vec<&str> = EMBED_MODELS.iter().map(|m| m.name).collect();
        return Err(anyhow!(
            "modèle inconnu : {name} (connus : {})",
            known.join(", ")
        ));
    };
    let target = embed_models_dir().join(format!("{}.gguf", entry.name));
    if let Ok(md) = std::fs::metadata(&target) {
        if md.len() == entry.bytes {
            return Ok(format!("déjà présent : {}", target.display()));
        }
        return Err(anyhow!(
            "{} existe mais fait {} o au lieu de {} o — supprime-le pour retélécharger",
            target.display(),
            md.len(),
            entry.bytes
        ));
    }
    std::fs::create_dir_all(embed_models_dir())?;
    let part = target.with_extension("gguf.part");
    let resp = crate::http_client::HTTP_CLIENT
        .get(entry.url)
        .send()
        .await
        .with_context(|| format!("téléchargement de {}", entry.url))?;
    if !resp.status().is_success() {
        return Err(anyhow!("téléchargement {} : {}", entry.url, resp.status()));
    }
    let mut file = std::fs::File::create(&part)?;
    let mut hasher = Sha256::new();
    let mut got: u64 = 0;
    let mut stream = resp.bytes_stream();
    use futures_util::StreamExt;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("flux interrompu")?;
        hasher.update(&chunk);
        file.write_all(&chunk)?;
        got += chunk.len() as u64;
    }
    file.flush()?;
    drop(file);
    let digest = format!("{:x}", hasher.finalize());
    if got != entry.bytes || digest != entry.sha256 {
        let _ = std::fs::remove_file(&part);
        return Err(anyhow!(
            "fichier refusé : {got} o (attendu {}), sha256 {digest} (attendu {})",
            entry.bytes,
            entry.sha256
        ));
    }
    std::fs::rename(&part, &target)?;
    Ok(format!(
        "modèle prêt : {} ({} Mo, sha256 vérifié)",
        target.display(),
        got / 1_048_576
    ))
}

#[cfg(test)]
mod embed_tests {
    use super::*;

    #[test]
    fn known_models_are_well_formed() {
        assert!(!EMBED_MODELS.is_empty());
        for m in EMBED_MODELS {
            assert!(m.sha256.len() == 64, "sha256 de {} invalide", m.name);
            assert!(
                m.sha256.chars().all(|c| c.is_ascii_hexdigit()),
                "sha256 de {} non hexadécimal",
                m.name
            );
            assert!(m.bytes > 100_000_000, "taille suspecte pour {}", m.name);
            assert!(
                m.url.starts_with("https://"),
                "URL non https pour {}",
                m.name
            );
            assert_eq!(m.name, m.name.trim());
        }
    }

    #[test]
    fn server_args_use_mean_pooling_and_embeddings() {
        let args = embed_args(Path::new("/tmp/x.gguf"), 2, 1024);
        let joined = args.join(" ");
        assert!(args.contains(&"--embeddings".to_string()));
        assert!(
            joined.contains("--pooling mean"),
            "pooling absent : {joined}"
        );
        assert!(joined.contains(&format!("--port {EMBED_PORT}")), "{joined}");
        assert!(joined.contains("-t 2"), "threads absents : {joined}");
        assert!(joined.contains("-m /tmp/x.gguf"), "{joined}");
        // Deux serveurs distincts : l'embedding ne doit jamais prendre 8080.
        assert!(!joined.contains("--port 8080"));
    }

    #[test]
    fn stale_embed_pid_is_cleared() {
        // Un PID manifestement mort ne doit pas survivre au démarrage.
        // Le dossier de données peut ne pas exister sur une machine neuve
        // (runner CI) : on le crée, sinon l'écriture échouerait en silence et
        // le test mesurerait autre chose que ce qu'il annonce.
        let path = embed_pid_path();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let saved = std::fs::read(&path).ok();
        std::fs::write(&path, "999999 1\n").expect("écriture du PID de test");
        // La détection d'un PID mort dépend de la plateforme (`is_owned_llama`) :
        //   Linux  : `/proc` (nom + starttime) ;
        //   macOS  : `kill -0` (voir la PR d'audit) ;
        //   Windows: aucun des deux → le PID est cru, repli documenté.
        // On n'affirme donc le nettoyage que là où il est garanti aujourd'hui.
        #[cfg(target_os = "linux")]
        {
            assert!(cleanup_stale_embed_pid());
            assert!(!path.exists(), "fichier PID périmé non supprimé");
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = cleanup_stale_embed_pid();
            let _ = std::fs::remove_file(&path);
        }
        match saved {
            Some(bytes) => {
                let _ = std::fs::write(&path, bytes);
            }
            None => {
                let _ = std::fs::remove_file(&path);
            }
        }
    }

    #[test]
    fn unknown_model_is_refused_with_the_known_names() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let err = rt
            .block_on(fetch_embed_model("nope"))
            .unwrap_err()
            .to_string();
        assert!(err.contains("nope"), "{err}");
        assert!(
            err.contains("bge-m3"),
            "les noms connus doivent être listés : {err}"
        );
    }
}
