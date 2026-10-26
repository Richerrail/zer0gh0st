//! Bot Telegram — converse avec Zer0 à distance.
//!
//! Choix de conception :
//! - **Long polling** (`getUpdates`) : **aucun port à ouvrir**, ça marche
//!   derrière NAT/pare-feu. Le bot appelle Telegram, l'inverse n'est pas vrai.
//! - **Allowlist** par identifiant Telegram ; le **premier chat** qui écrit est
//!   auto-lié (puis persisté dans la config).
//! - **Vocal** → Whisper (réutilise `zerochan`). **Fichiers** → dossier `drop/`.
//! - **Progression** : le message est édité au fil des appels d'outils.

use anyhow::{anyhow, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::agent::history::ChatMessage;
use crate::config::{self, Settings};
use crate::event::UiEvent;
use crate::llm::llama::ChatOptions;
use crate::route;
use crate::tools::ToolSpec;

const API: &str = "https://api.telegram.org";
const MAX_MSG: usize = 3800;

#[derive(Deserialize)]
struct Resp<T> {
    ok: bool,
    #[serde(default)]
    result: Option<T>,
    #[serde(default)]
    description: Option<String>,
}

#[derive(Deserialize)]
struct Update {
    update_id: i64,
    #[serde(default)]
    message: Option<Message>,
    #[serde(default)]
    edited_message: Option<Message>,
}

#[derive(Deserialize)]
struct Message {
    #[allow(dead_code)]
    message_id: i64,
    chat: Chat,
    #[serde(default)]
    from: Option<User>,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    caption: Option<String>,
    #[serde(default)]
    voice: Option<FileRef>,
    #[serde(default)]
    audio: Option<FileRef>,
    #[serde(default)]
    document: Option<DocRef>,
}

#[derive(Deserialize)]
struct Chat {
    id: i64,
}

#[derive(Deserialize)]
struct User {
    id: i64,
    #[serde(default)]
    username: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    first_name: Option<String>,
}

#[derive(Deserialize)]
struct FileRef {
    file_id: String,
}

#[derive(Deserialize)]
struct DocRef {
    file_id: String,
    #[serde(default)]
    file_name: Option<String>,
}

struct Bot {
    http: reqwest::Client,
    token: String,
    allow: Vec<i64>,
    settings: Settings,
    system: String,
    tools: Vec<ToolSpec>,
    options: ChatOptions,
    chats: HashMap<i64, Vec<ChatMessage>>,
    notify: Option<tokio::sync::mpsc::UnboundedSender<UiEvent>>,
    shutdown: bool,
    /// Autorise `code_exec` (shell) — désactivé par défaut.
    exec: bool,
    /// L'allowlist vient-elle de l'utilisateur (config/CLI) ou d'un auto-claim ?
    /// Un auto-claim seul n'ouvre pas le shell : sinon, quiconque trouve le bot
    /// avant son propriétaire obtient un `bash` sur la machine.
    exec_trusted: bool,
}
/// Shell distant explicitement autorisé malgré un auto-claim.
fn exec_optin() -> bool {
    matches!(
        std::env::var("ZER0_TELEGRAM_ALLOW_EXEC").as_deref(),
        Ok("1") | Ok("true") | Ok("yes")
    )
}

impl Bot {
    /// Outils exposés au bot.
    ///
    /// `code_exec` **et** `text_editor` sont derrière `/exec` : un chat lié
    /// automatiquement (auto-claim) ne doit pas pouvoir écrire ou supprimer un
    /// fichier avant que l'utilisateur ait autorisé les actions système depuis
    /// la machine locale. Avant, `text_editor` était toujours actif : un inconnu
    /// qui trouvait le bot pouvait déjà écraser des fichiers sans `/exec`.
    fn tools_for(&self) -> Vec<ToolSpec> {
        self.tools
            .iter()
            .filter(|t| match t.name {
                "code_exec" | "text_editor" => self.exec,
                _ => true,
            })
            .cloned()
            .collect()
    }

    fn info(&self, s: String) {
        if let Some(n) = &self.notify {
            let _ = n.send(UiEvent::Info(s));
        }
    }
}

impl Bot {
    async fn api(&self, method: &str, body: serde_json::Value) -> Result<serde_json::Value> {
        let url = format!("{API}/bot{}/{method}", self.token);
        let v: serde_json::Value = self
            .http
            .post(&url)
            .json(&body)
            .send()
            .await?
            .json()
            .await?;
        if v.get("ok").and_then(|b| b.as_bool()) == Some(true) {
            Ok(v.get("result").cloned().unwrap_or(serde_json::Value::Null))
        } else {
            Err(anyhow!(
                "{}",
                v.get("description")
                    .and_then(|d| d.as_str())
                    .unwrap_or("erreur Telegram")
            ))
        }
    }

    async fn get_updates(&self, offset: i64) -> Result<Vec<Update>> {
        let url = format!("{API}/bot{}/getUpdates", self.token);
        let v: Resp<Vec<Update>> = self
            .http
            .post(&url)
            .json(&serde_json::json!({
                "offset": offset,
                "timeout": 30,
                "allowed_updates": ["message", "edited_message"]
            }))
            .timeout(Duration::from_secs(70))
            .send()
            .await?
            .json()
            .await?;
        if v.ok {
            Ok(v.result.unwrap_or_default())
        } else {
            Err(anyhow!("{}", v.description.unwrap_or_default()))
        }
    }

    async fn send(&self, chat_id: i64, text: &str) -> Result<i64> {
        let r = self
            .api(
                "sendMessage",
                serde_json::json!({ "chat_id": chat_id, "text": text }),
            )
            .await?;
        Ok(r.get("message_id").and_then(|m| m.as_i64()).unwrap_or(0))
    }

    async fn edit(&self, chat_id: i64, message_id: i64, text: &str) {
        let _ = self
            .api(
                "editMessageText",
                serde_json::json!({ "chat_id": chat_id, "message_id": message_id, "text": text }),
            )
            .await;
    }

    async fn typing(&self, chat_id: i64) {
        let _ = self
            .api(
                "sendChatAction",
                serde_json::json!({ "chat_id": chat_id, "action": "typing" }),
            )
            .await;
    }

    async fn download(&self, file_id: &str) -> Result<PathBuf> {
        let r = self
            .api("getFile", serde_json::json!({ "file_id": file_id }))
            .await?;
        let path = r
            .get("file_path")
            .and_then(|p| p.as_str())
            .ok_or_else(|| anyhow!("file_path manquant"))?;
        let url = format!("{API}/file/bot{}/{path}", self.token);
        let bytes = self.http.get(&url).send().await?.bytes().await?;
        let name = path.split('/').next_back().unwrap_or("file");
        let dir = drop_dir();
        std::fs::create_dir_all(&dir)?;
        let dest = dir.join(format!("{}-{name}", now_secs()));
        std::fs::write(&dest, &bytes)?;
        Ok(dest)
    }

    async fn handle(&mut self, msg: Message) {
        let chat_id = msg.chat.id;
        let from_id = msg.from.as_ref().map(|u| u.id).unwrap_or(chat_id);

        // Allowlist + auto-claim du premier chat.
        if self.allow.is_empty() {
            self.allow = vec![from_id];
            self.exec_trusted = false; // provenance inconnue → shell verrouillé
            let mut g = config::load_global();
            g.telegram_allow = self.allow.clone();
            let _ = config::save_global(&g);
            let who = msg
                .from
                .as_ref()
                .and_then(|u| u.username.clone())
                .unwrap_or_else(|| from_id.to_string());
            let _ = self
                .send(chat_id, &format!("🔐 Chat lié à @{who}. Envoie /help."))
                .await;
        } else if !self.allow.contains(&from_id) {
            return;
        }

        let mut text = msg.text.clone().or_else(|| msg.caption.clone());

        // Vocal → transcription (Whisper).
        if let Some(v) = msg.voice.as_ref().or(msg.audio.as_ref()) {
            let _ = self.typing(chat_id).await;
            match self.download(&v.file_id).await {
                Ok(path) => match crate::zerochan::transcribe(&path).await {
                    Ok(t) if !t.trim().is_empty() => {
                        let _ = self.send(chat_id, &format!("🎤 « {t} »")).await;
                        text = Some(t);
                    }
                    Ok(_) => {
                        let _ = self.send(chat_id, "🎤 (rien entendu)").await;
                    }
                    Err(e) => {
                        let _ = self.send(chat_id, &format!("🎤 erreur STT : {e}")).await;
                    }
                },
                Err(e) => {
                    let _ = self.send(chat_id, &format!("🎤 erreur : {e}")).await;
                }
            }
        } else if let Some(d) = msg.document.as_ref() {
            match self.download(&d.file_id).await {
                Ok(path) => {
                    let name = d.file_name.clone().unwrap_or_default();
                    let _ = self
                        .send(chat_id, &format!("📥 reçu : {name} → {}", path.display()))
                        .await;
                    text = Some(format!(
                        "J'ai reçu le fichier « {name} » (chemin local : {}). Regarde-le et dis-moi ce que tu en penses.",
                        path.display()
                    ));
                }
                Err(e) => {
                    let _ = self.send(chat_id, &format!("📥 erreur : {e}")).await;
                }
            }
        }

        let Some(text) = text else { return };
        let text = text.trim().to_string();
        if text.is_empty() {
            return;
        }
        self.info(format!(
            "📨 Telegram : {}",
            text.chars().take(160).collect::<String>()
        ));

        let mut it = text.split_whitespace();
        let cmd = it.next().unwrap_or("");
        let args: Vec<&str> = it.collect();
        match cmd {
            "/start" | "/help" => {
                let _ = self
                    .send(
                        chat_id,
                        "ZER0│GH0ST — bot Telegram (agent complet).\n\
                         /new — nouvelle conversation\n\
                         /status — état\n\
                         /progress — où en est le projet\n\
                         /exec on|off — autoriser les actions système (shell + écriture de fichiers)\n\
                         /stop — interrompre\n\
                         /shutdown — éteindre le bot\n\
                         Sinon : écris, envoie un vocal 🎤 ou un fichier 📎.",
                    )
                    .await;
                return;
            }
            "/exec" => {
                match args.first().copied() {
                    Some("on") if self.exec_trusted || exec_optin() => self.exec = true,
                    Some("on") => {
                        let _ = self
                            .send(
                                chat_id,
                                "🚫 /exec refusé : ce chat a été lié automatiquement (premier \
                                 contact). Active le shell depuis la machine locale \
                                 (`/codeexec` dans la TUI) ou relance le bot avec \
                                 `--allow <ton_id>`. Force possible : ZER0_TELEGRAM_ALLOW_EXEC=1.",
                            )
                            .await;
                        return;
                    }
                    Some("off") => self.exec = false,
                    _ => {}
                }
                let msg = if self.exec {
                    "🐚 actions système (shell + écriture) : ON\n\
                     ⚠️  TOUTES les commandes s'exécutent SANS validation.\n\
                     Si l'agent fait un `rm -rf` sur ton disque de photos de famille \
                     par erreur, c'est TOI le responsable.\n\
                     (/exec off pour couper)"
                } else {
                    "🐚 actions système : OFF (lecture seule)\n/exec on | /exec off"
                };
                let _ = self.send(chat_id, msg).await;
                return;
            }
            "/new" => {
                self.chats.remove(&chat_id);
                let _ = self.send(chat_id, "🗂️ nouvelle conversation").await;
                return;
            }
            "/status" => {
                let n = self.chats.get(&chat_id).map(|c| c.len()).unwrap_or(0);
                let _ = self
                    .send(
                        chat_id,
                        &format!(
                            "📍 {} · modèle {} · shell {} · {} message(s)",
                            self.settings.provider_id,
                            self.settings.model,
                            if self.exec { "ON" } else { "off" },
                            n
                        ),
                    )
                    .await;
                return;
            }
            "/jimmy" => {
                let _ = self
                    .send(chat_id, &format!("🐌 Jimmy : {}", crate::jimmy::reply()))
                    .await;
                return;
            }
            "/stop" => {
                crate::cancel::request();
                let _ = self.send(chat_id, "⏹ stop demandé").await;
                return;
            }
            "/progress" => {
                let msg = progress_report(&self.settings, &self.chats, chat_id);
                let _ = self.send(chat_id, &msg).await;
                return;
            }
            "/shutdown" | "/off" => {
                let _ = self.send(chat_id, "🛑 extinction du bot…").await;
                self.info("🤖 Telegram: OFF (via /shutdown)".into());
                if let Some(n) = &self.notify {
                    let _ = n.send(UiEvent::TelegramShutdown);
                }
                self.shutdown = true;
                return;
            }
            _ => {}
        }

        self.ask(chat_id, text).await;
    }

    async fn ask(&mut self, chat_id: i64, prompt: String) {
        // Calculé AVANT d'emprunter `self.chats` (emprunt mutable plus bas).
        let tools = self.tools_for();
        let history = self.chats.entry(chat_id).or_default();
        history.push(ChatMessage::user(prompt.clone()));
        let role = route::auto_role(&prompt).to_string();
        let endpoints = route::chain_for_role(&self.settings, &role);
        let utility = route::chain_for_role(&self.settings, "utility");
        // Liste explicite des outils dans le prompt (ce modèle ignore le paramètre `tools`).
        let mut system = self.system.clone();
        if !tools.is_empty() {
            system.push_str(
                "\n\n## Outils disponibles (utilise-les pour AGIR — ne dis jamais « je ne peux pas exécuter » quand un outil le permet)\n",
            );
            for t in &tools {
                system.push_str(&format!("- {} : {}\n", t.name, t.description));
            }
        }
        let mut base = history.clone();
        let options = self.options.clone();

        if crate::agent::compact::estimate_tokens(&base) >= 4000 {
            let (b, _) = crate::agent::compact::compact_now(utility, &system, base, 6).await;
            base = b;
        }

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<UiEvent>();
        tokio::spawn(async move {
            crate::agent::turn::run_turn(endpoints, system, base, tools, options, tx).await;
        });

        let t0 = Instant::now();
        let _ = self.typing(chat_id).await;
        let mid = self.send(chat_id, "⏳ Zer0 travaille…").await.unwrap_or(0);
        let mut acc = String::new();
        let mut prog = String::new();
        let mut last = Instant::now();
        while let Some(ev) = rx.recv().await {
            match ev {
                UiEvent::Token(t) => {
                    acc.push_str(&t);
                    if last.elapsed() > Duration::from_millis(2000) {
                        self.edit(chat_id, mid, &progress_body(&prog, &acc, t0))
                            .await;
                        let _ = self.typing(chat_id).await;
                        last = Instant::now();
                    }
                }
                UiEvent::ToolCall { name, args } => {
                    let short: String = args.chars().take(80).collect();
                    prog.push_str(&format!("⚙ {name} {short}\n"));
                    self.edit(chat_id, mid, &progress_body(&prog, &acc, t0))
                        .await;
                }
                UiEvent::TurnComplete(_) => break,
                UiEvent::Error(e) => {
                    acc.push_str(&format!("\n⚠ {e}"));
                    break;
                }
                _ => {}
            }
        }
        let dur = t0.elapsed().as_secs();
        let ok = !acc.trim().is_empty();
        {
            let history = self.chats.entry(chat_id).or_default();
            history.push(ChatMessage::assistant(acc.clone(), vec![]));
        }
        // Message de progression → état final.
        let mut done = format!("{} terminé en {dur}s", if ok { "✅" } else { "⚠️" });
        if !prog.is_empty() {
            done.push('\n');
            done.push_str(prog.trim_end());
        }
        self.edit(chat_id, mid, &done).await;
        // Réponse finale dans un NOUVEAU message → déclenche une notification.
        if ok {
            self.finish(chat_id, 0, &acc).await;
        }
        self.info(format!(
            "✅ Telegram: réponse envoyée ({dur}s, {} caractères)",
            acc.chars().count()
        ));
    }

    async fn finish(&self, chat_id: i64, mid: i64, text: &str) {
        let chunks = chunk(text, MAX_MSG);
        if let Some(first) = chunks.first() {
            if mid != 0 {
                self.edit(chat_id, mid, first).await;
            } else {
                let _ = self.send(chat_id, first).await;
            }
        }
        for c in chunks.iter().skip(1) {
            let _ = self.send(chat_id, c).await;
        }
    }
}

fn progress_body(prog: &str, acc: &str, t0: Instant) -> String {
    let mut s = format!("⏳ Zer0 travaille… ({}s)\n", t0.elapsed().as_secs());
    if !prog.is_empty() {
        s.push_str(prog);
    }
    let v: Vec<char> = acc.chars().collect();
    let start = v.len().saturating_sub(200);
    let tail: String = v[start..].iter().collect();
    if !tail.trim().is_empty() {
        s.push('\n');
        s.push_str(tail.trim());
    }
    s.chars().take(MAX_MSG).collect()
}

fn chunk(text: &str, max: usize) -> Vec<String> {
    if text.chars().count() <= max {
        return vec![text.to_string()];
    }
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut n = 0usize;
    for line in text.split_inclusive('\n') {
        let l = line.chars().count();
        if n + l > max && !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
            n = 0;
        }
        cur.push_str(line);
        n += l;
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// Rapport d'avancement (tâches du projet + état du chat).
fn progress_report(
    settings: &Settings,
    chats: &HashMap<i64, Vec<ChatMessage>>,
    chat_id: i64,
) -> String {
    use crate::tasks::Status;
    let tasks = crate::tasks::load();
    let total = tasks.len();
    let done = tasks
        .iter()
        .filter(|t| matches!(t.status, Status::Done))
        .count();
    let doing = tasks
        .iter()
        .filter(|t| matches!(t.status, Status::Claimed))
        .count();
    let n = chats.get(&chat_id).map(|c| c.len()).unwrap_or(0);
    let mut s = String::from("📊 Progression\n");
    s.push_str(&format!(
        "• Modèle : {}/{}\n",
        settings.provider_id, settings.model
    ));
    s.push_str(&format!("• Chat Telegram : {n} message(s)\n"));
    if total == 0 {
        s.push_str("• Tâches : aucune (.zer0/tasks.md)\n");
    } else {
        s.push_str(&format!(
            "• Tâches : {done}/{total} faites, {doing} en cours\n"
        ));
        if let Some(t) = tasks.iter().find(|t| matches!(t.status, Status::Pending)) {
            s.push_str(&format!("• Suivante : {} — {}\n", t.id, t.text));
        } else if doing == 0 {
            s.push_str("• Tout est fait ✅\n");
        }
    }
    s
}

fn drop_dir() -> PathBuf {
    config::data_dir().join("drop").join("incoming")
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Lit `ZER0_TELEGRAM_TOKEN` depuis `~/.config/zer0/telegram.env` (chmod 600).
fn token_from_env_file() -> Option<String> {
    let s = std::fs::read_to_string(config::config_dir().join("telegram.env")).ok()?;
    s.lines()
        .find_map(|l| {
            l.strip_prefix("ZER0_TELEGRAM_TOKEN=")
                .map(|v| v.trim().to_string())
        })
        .filter(|t| !t.is_empty())
}

fn resolve_token(explicit: Option<String>) -> Option<String> {
    explicit
        .or_else(|| std::env::var("ZER0_TELEGRAM_TOKEN").ok())
        .or_else(token_from_env_file)
        .or_else(|| config::load_global().telegram_token.clone())
        .map(|t| t.trim().trim_matches(|c| c == '<' || c == '>').to_string())
        .filter(|t| !t.is_empty())
}

/// Poignée pour arrêter un bot lancé depuis la TUI.
pub struct Handle {
    stop: tokio::sync::watch::Sender<bool>,
    #[allow(dead_code)]
    pub join: tokio::task::JoinHandle<()>,
}

impl Handle {
    pub fn stop(&self) {
        let _ = self.stop.send(true);
    }
}

/// Lance le bot en tâche de fond. `notify` renvoie les infos à la TUI.
pub fn spawn(
    token: Option<String>,
    allow: Vec<i64>,
    notify: Option<tokio::sync::mpsc::UnboundedSender<UiEvent>>,
) -> Result<Handle> {
    let token = resolve_token(token).ok_or_else(|| {
        anyhow!("token Telegram manquant : `zer0 telegram --token <…>`, ZER0_TELEGRAM_TOKEN ou ~/.config/zer0/telegram.env")
    })?;
    let g = config::load_global();
    let allow = if allow.is_empty() {
        g.telegram_allow.clone()
    } else {
        allow
    };

    let (stop_tx, mut stop_rx) = tokio::sync::watch::channel(false);
    let join = tokio::spawn(async move {
        if let Err(e) = run_loop(token, allow, &mut stop_rx, notify).await {
            eprintln!("telegram: {e}");
        }
    });
    Ok(Handle {
        stop: stop_tx,
        join,
    })
}

/// Mode CLI : lance et bloque jusqu'à Ctrl+C.
pub async fn run(token: Option<String>, allow: Vec<i64>) -> Result<()> {
    let h = spawn(token, allow, None)?;
    let _ = h.join.await;
    Ok(())
}

async fn run_loop(
    token: String,
    allow: Vec<i64>,
    stop: &mut tokio::sync::watch::Receiver<bool>,
    notify: Option<tokio::sync::mpsc::UnboundedSender<UiEvent>>,
) -> Result<()> {
    let quiet = notify.is_some();
    let system =
        config::system_prompt_override().unwrap_or_else(|| crate::DEFAULT_SYSTEM.to_string());
    let system = crate::enrich_system(system);

    let allow_is_empty = allow.is_empty();
    let mut bot = Bot {
        http: reqwest::Client::new(),
        token,
        allow,
        settings: config::resolve(),
        system,
        tools: crate::tools::all(),
        options: ChatOptions {
            max_tokens: 2048,
            thinking: false,
            temperature: 0.3,
            tools_mode: crate::llm::llama::ToolsMode::Native,
            stream: true,
            local: false,
        },
        chats: HashMap::new(),
        notify,
        shutdown: false,
        exec: false,
        // Une allowlist fournie par l'utilisateur (--allow ou config) est de
        // confiance ; une liste vide = auto-claim au premier contact.
        exec_trusted: !allow_is_empty,
    };

    let me = bot.api("getMe", serde_json::json!({})).await?;
    let username = me
        .get("username")
        .and_then(|u| u.as_str())
        .unwrap_or("(bot)")
        .to_string();
    if quiet {
        bot.info(format!("🤖 Telegram: ON — @{username}"));
    } else {
        println!("🤖 @{username} démarré — long polling (Ctrl+C pour quitter)");
        if bot.allow.is_empty() {
            println!("→ envoie /start au bot pour lier ton compte (auto-claim).");
        } else {
            println!("→ autorisé : {:?}", bot.allow);
        }
    }

    let mut offset = 0i64;
    loop {
        if *stop.borrow() {
            break;
        }
        tokio::select! {
            _ = stop.changed() => break,
            res = bot.get_updates(offset) => match res {
                Ok(updates) => {
                    for u in updates {
                        offset = u.update_id + 1;
                        if let Some(m) = u.message.or(u.edited_message) {
                            bot.handle(m).await;
                            if bot.shutdown {
                                break;
                            }
                        }
                    }
                }
                Err(e) => {
                    eprintln!("⚠ getUpdates: {e}");
                    tokio::time::sleep(Duration::from_secs(3)).await;
                }
            },
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_preserves_and_bounds() {
        assert_eq!(chunk("hello", 10), vec!["hello"]);
        let text = "ligne\n".repeat(500);
        let chunks = chunk(&text, 300);
        assert!(chunks.iter().all(|c| c.chars().count() <= 300));
        assert_eq!(chunks.concat(), text);
    }
}
