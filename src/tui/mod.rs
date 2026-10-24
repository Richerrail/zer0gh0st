mod banner;
mod menu;
mod picker;
mod wizard;

use anyhow::Result;
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;
use std::thread;
use std::time::Duration;
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};

use crate::agent::history::ChatMessage;
use crate::config::{self, Settings};
use crate::event::UiEvent;
use crate::http_client;
use crate::llm::llama::ChatOptions;
use crate::models;
use crate::providers;
use crate::route;
use crate::select;
use crate::tools::ToolSpec;

/// Aide par défaut : uniquement ce dont un nouvel utilisateur a besoin.
/// Le reste est derrière `/help all` (le pouvoir reste, le bruit part).
const HELP_SHORT: &str =
    "Commandes essentielles (tape `/` pour la palette, `/help all` pour tout) :

  /setup              assistant de démarrage (8 étapes)
  /login  /logout     connexion provider → clé → modèle, ou retrait
  /provider           choisir un provider (menu)
  /model              choisir un modèle (menu)
  /free               modèles gratuits (menu)
  /role               rôles ; /role set <rôle> ouvre un menu
  /route              chaîne de bascule si un modèle échoue
  /think              raisonnement on/off
  /codeexec           shell on/off (F4) — mêmes droits que toi, sans bac à sable
  /readwrite          écriture de fichiers on/off (F3)
  /img <chemin>       joindre une image
  /kb  /memory  /task outils (base locale, mémoire, tâches)
  /telegram           bot à distance on/off
  /help all           la liste complète (26 commandes de plus)
  /quit  /new  /chats session

  Touches : F2 menu · F3 écriture · F4 shell · F5 aide · F6 sessions
            F7 relance · F8 0chan · F10 micro · Ctrl+O déplier un outil";

/// Palette inspirée du thème Pi « omarchy-system ».
mod theme {
    use ratatui::style::Color;
    pub const ACCENT: Color = Color::Rgb(255, 113, 206);
    pub const PANEL: Color = Color::Rgb(34, 23, 51);
    pub const DIM: Color = Color::Rgb(122, 108, 142);
    pub const TEXT: Color = Color::Rgb(232, 213, 255);
    pub const CODE: Color = Color::Rgb(122, 232, 255);
    pub const BLUE: Color = Color::Rgb(138, 158, 255);
}

pub struct App {
    pub messages: Vec<ChatMessage>,
    pub input: String,
    /// Position du curseur dans `input` (index **en caractères**).
    pub cursor: usize,
    pub should_quit: bool,
    pub streaming: bool,
    pub streaming_text: String,
    pub reasoning_text: String,
    /// Animates the banner (rainbow phase).
    pub anim_phase: u16,
    /// Chat scroll offset from the top, and whether we follow new output.
    pub scroll: u16,
    pub auto_scroll: bool,
    pub max_scroll: u16,
    pub thinking: bool,
    pub max_tokens: u32,
    pub temperature: f32,
    /// Real conversation history (conversation roles only).
    pub conversation: Vec<ChatMessage>,
    /// Display index where transient turn output begins.
    pub display_mark: usize,
    /// System prompt (kept for commands like /compact).
    pub system: String,
    /// Base system prompt without any persona applied.
    pub base_system: String,
    /// Active persona name, if any.
    pub persona: Option<String>,
    pub all_tools: Vec<ToolSpec>,
    pub read_write: bool,
    pub code_exec: bool,
    pub provider_id: String,
    pub provider_name: String,
    pub base_url: String,
    pub api_key: Option<String>,
    pub model: String,
    /// Endpoint actually used last (after failover), for the status bar.
    pub active_endpoint: String,
    /// Forced role (None = auto-select per message).
    pub forced_role: Option<String>,
    /// Role chosen for the last message.
    pub last_role: String,
    /// Routage par rôle actif (miroir de la config, pour l'affichage).
    pub roles_enabled: bool,
    /// Sélecteur ouvert (`/model` ou `/provider` sans argument).
    pub picker: Option<picker::Picker>,
    /// Assistant de première connexion (écran plein).
    pub wizard: Option<wizard::Wizard>,
    /// Saisie masquée (clé API, jeton) : le buffer reste réel, l'affichage non.
    pub secret_mode: bool,
    /// Cible à appliquer quand la liste de modèles arrivera (ex. rôle à définir).
    pub pending_picker_target: Option<String>,
    /// Image en attente, jointe au prochain message envoyé (data URL).
    pub pending_image: Option<String>,
    pub pending_image_name: Option<String>,
    /// Ids de modèles acceptant une image, d'après le catalogue `/models`.
    pub vision_models: Vec<String>,
    /// Zone du fil de conversation (clic droit, molette).
    pub chat_rect: Rect,
    /// Défilement réellement appliqué au fil.
    pub chat_scroll: u16,
    /// Cache de rendu : (index du message, largeur, empreinte) → lignes.
    ///
    /// Sans lui, chaque frame re-découpait et re-parsait **toute** la
    /// conversation (O(conversation) par frame, donc par token). Le cache est
    /// invalidé quand le nombre de messages change.
    pub chat_cache: std::collections::HashMap<(usize, u16, u64), Vec<Line<'static>>>,
    /// Nombre de messages au moment où le cache a été vidé pour la dernière fois.
    pub chat_cache_len: usize,
    /// Blocs de sortie d'outil rendus : (1ʳᵉ ligne, dernière ligne, index message).
    pub block_ranges: Vec<(u16, u16, usize)>,
    /// Full-screen fun mode (chat by default).
    pub mode: crate::fun::Mode,
    pub radio: crate::fun::radio::Radio,
    pub bintime: crate::fun::bintime::BinTime,
    pub pong: crate::fun::pong::Pong,
    /// Side panel (right of the agent): bintime or radio.
    pub side_mode: Option<crate::fun::Mode>,
    /// Whether the side panel has keyboard focus.
    pub focus_panel: bool,
    /// Auto-inject relevant knowledge into the prompt.
    pub kb_auto: bool,
    /// Input history (recall with ↑/↓).
    pub history: Vec<String>,
    pub hist_pos: Option<usize>,
    pub hist_draft: String,
    /// Zero-chan (voice assistant).
    pub zc: crate::zerochan::ZeroChan,
    pub btn_0chan: Option<Rect>,
    pub btn_mic: Option<Rect>,
    /// Vote : compteur de stars GitHub observé (None = inconnu).
    pub vote_stars: Option<u64>,
    /// Vote : le compte courant a-t-il starré le dépôt ? (None = inconnu).
    pub vote_starred: Option<bool>,
    /// Zones cliquables des pouces (bas-droite de l'écran principal).
    pub btn_vote_up: Option<Rect>,
    pub btn_vote_down: Option<Rect>,
    /// Sélection souris (texte affiché à l'écran).
    pub sel_anchor: Option<(u16, u16)>,
    pub sel_head: Option<(u16, u16)>,
    pub screen: Vec<String>,
    /// Si Some, affiche le pop-up d'anciennes conversations avec (liste des fichiers, index sélectionné).
    pub chats_popup: Option<(Vec<std::path::PathBuf>, usize)>,
    /// Fenêtre de contexte du modèle actif (tokens).
    pub context_window: u64,
    /// L'utilisateur a forcé le contexte (/ctx) — la sonde ne l'écrase pas.
    pub ctx_manual: bool,
    /// Bot Telegram lancé depuis la TUI (None = arrêté).
    pub tg: Option<crate::telegram::Handle>,
    /// Menu latéral : groupe déplié.
    pub menu_open: Option<&'static str>,
    /// Zones cliquables du menu (recalculées au rendu).
    pub menu_hits: Vec<(Rect, menu::Hit)>,
    /// Défilement du menu + sa zone écran (pour la molette).
    pub menu_scroll: u16,
    pub menu_rect: Rect,
    /// Navigation clavier du menu (F2) : focus + index sélectionné.
    pub menu_focus: bool,
    pub menu_sel: usize,
    /// Dernier événement clavier traité (pour déduplication sur Windows).
    pub last_key_event: Option<(KeyCode, KeyModifiers, std::time::Instant)>,
}

impl App {
    fn settings(&self) -> Settings {
        Settings {
            provider_id: self.provider_id.clone(),
            provider_name: self.provider_name.clone(),
            base_url: self.base_url.clone(),
            model: self.model.clone(),
            api_key: self.api_key.clone(),
        }
    }

    /// Context d'exécution des outils (`0box`) : réinstallé à chaque changement
    /// de politique d'outils (F3/F4, `/readwrite`, `/codeexec`).
    ///
    /// Sans ça, l'outil `0box` gardait la politique du démarrage : désactiver
    /// `code_exec` après coup ne retirait pas le shell aux sous-agents.
    pub fn refresh_runtime(&self) {
        crate::tools::set_runtime(crate::tools::Runtime {
            settings: self.settings(),
            system: self.system.clone(),
            options: ChatOptions {
                max_tokens: self.max_tokens,
                thinking: self.thinking,
                temperature: self.temperature,
            },
            all_tools: self.all_tools.clone(),
            read_write: self.read_write,
            code_exec: self.code_exec,
        });
    }

    /// Recharge provider / modèle / clé depuis la config.
    ///
    /// Nécessaire après `/provider add|use|remove` : la TUI gardait sinon des
    /// valeurs périmées (ancien base_url, ancienne clé, ancien modèle) alors que
    /// la config avait changé — divergence classique avec le CLI.
    pub fn reload_settings(&mut self) {
        let s = config::resolve();
        self.provider_id = s.provider_id;
        self.provider_name = s.provider_name;
        self.base_url = s.base_url;
        self.api_key = s.api_key;
        self.model = s.model;
        self.roles_enabled = config::roles_enabled();
    }

    /// Recompute the effective system prompt from base + active persona.
    fn refresh_system(&mut self) {
        self.system = match &self.persona {
            Some(name) => match crate::personas::get(name) {
                Some(content) => format!("{}\n\n## Persona : {name}\n{content}", self.base_system),
                None => self.base_system.clone(),
            },
            None => self.base_system.clone(),
        };
    }
}

pub async fn run(
    settings: Settings,
    system: String,
    all_tools: Vec<ToolSpec>,
    options: ChatOptions,
    allow_exec: bool,
    resume: bool,
) -> Result<()> {
    let system = crate::enrich_system(system);
    let g_initial = config::load_global();
    let mut terminal = ratatui::init();
    let _ = crossterm::execute!(
        std::io::stdout(),
        crossterm::event::EnableMouseCapture,
        crossterm::event::EnableBracketedPaste
    );

    let (input_tx, mut input_rx) = mpsc::unbounded_channel::<Event>();
    thread::spawn(move || {
        while let Ok(ev) = crossterm::event::read() {
            if input_tx.send(ev).is_err() {
                break;
            }
        }
    });

    let (ui_tx, mut ui_rx) = mpsc::unbounded_channel::<UiEvent>();

    let ctx_override = config::load_global().context_window;
    let ctx_win = ctx_override
        .or_else(|| known_context(&settings.model))
        .unwrap_or(200_000);

    let mut app = App {
        messages: vec![ChatMessage::notice(
            "Bonjour 👋, je suis Zer0. Comment puis-je t'aider ?",
        )],
        input: String::new(),
        cursor: 0,
        should_quit: false,
        streaming: false,
        streaming_text: String::new(),
        reasoning_text: String::new(),
        anim_phase: 0,
        scroll: 0,
        auto_scroll: true,
        max_scroll: 0,
        conversation: Vec::new(),
        display_mark: 0,
        system: system.clone(),
        base_system: system.clone(),
        persona: None,
        all_tools,
        read_write: true,
        code_exec: allow_exec || g_initial.code_exec,
        provider_id: settings.provider_id,
        provider_name: settings.provider_name,
        base_url: settings.base_url,
        api_key: settings.api_key,
        model: settings.model,
        active_endpoint: String::new(),
        forced_role: None,
        last_role: String::new(),
        roles_enabled: config::roles_enabled(),
        picker: None,
        wizard: None,
        secret_mode: false,
        pending_picker_target: None,
        pending_image: None,
        pending_image_name: None,
        vision_models: Vec::new(),
        chat_rect: Rect::default(),
        chat_scroll: 0,
        block_ranges: Vec::new(),
        chat_cache: std::collections::HashMap::new(),
        chat_cache_len: 0,
        mode: crate::fun::Mode::Chat,
        radio: crate::fun::radio::Radio::new(),
        bintime: crate::fun::bintime::BinTime::new(),
        pong: crate::fun::pong::Pong::new(),
        side_mode: None,
        focus_panel: false,
        kb_auto: true,
        history: Vec::new(),
        hist_pos: None,
        hist_draft: String::new(),
        zc: crate::zerochan::ZeroChan::new(),
        btn_0chan: None,
        btn_mic: None,
        vote_stars: None,
        vote_starred: None,
        btn_vote_up: None,
        btn_vote_down: None,
        sel_anchor: None,
        sel_head: None,
        screen: Vec::new(),
        chats_popup: None,
        context_window: ctx_win,
        ctx_manual: ctx_override.is_some(),
        tg: None,
        menu_open: None,
        menu_hits: Vec::new(),
        menu_scroll: 0,
        menu_rect: Rect::default(),
        menu_focus: false,
        menu_sel: 0,
        last_key_event: None,
        thinking: options.thinking || g_initial.thinking,
        max_tokens: options.max_tokens,
        temperature: options.temperature,
    };

    // Contexte runtime pour l'outil `0box` (débat réel lancé par le modèle) :
    // mêmes réglages, mêmes options, mêmes outils que la TUI.
    app.refresh_runtime();

    // Config illisible = clés potentiellement perdues : on le montre une fois.
    if let Some(msg) = config::corruption() {
        app.messages
            .push(ChatMessage::notice(format!("⚠ configuration : {msg}")));
    }

    // Première connexion (ou assistant interrompu) : on prend l'utilisateur par
    // la main. `--resume`/`--once` ne sont pas concernés (headless).
    {
        let g = config::load_global();
        if !g.setup_done {
            let step = wizard::Step::from_idx(g.setup_step);
            wizard::start(&mut app, &ui_tx, step);
        }
    }

    // Démarrage = NOUVELLE conversation (l'ancienne est archivée → F6).
    // Avec `--resume`, on reprend la conversation active à la place.
    if resume {
        let restored = crate::convo::load_repaired(200);
        if !restored.is_empty() {
            let n = restored.len();
            app.messages = restored.clone();
            app.conversation = restored;
            app.display_mark = app.messages.len();
            app.messages.push(ChatMessage::notice(format!(
                "🗂️ session reprise ({n} messages) — F6 pour les autres"
            )));
        }
    } else {
        let _ = crate::convo::clear();
        app.messages.push(ChatMessage::notice(
            "🗂️ nouvelle conversation (précédente archivée — F6 pour la recharger)",
        ));
    }

    probe_context(&app, &ui_tx);

    // Vote : au tout premier lancement, on propose de connecter GitHub (une
    // fois) ; ensuite on se contente d'un rappel discret. Sans jeton, le 👍
    // ouvre le navigateur au lieu de poser une vraie star.
    if let Some(msg) = crate::vote::invitation_startup() {
        app.messages.push(ChatMessage::notice(msg));
        crate::vote::note_rappel();
    }

    // Vote : compteur de stars + état du compte (chargé une fois, en tâche de fond).
    {
        let ui = ui_tx.clone();
        tokio::spawn(async move {
            let (stars, starred) = crate::vote::state().await;
            let _ = ui.send(UiEvent::VoteState { stars, starred });
        });
    }

    let result = event_loop(&mut terminal, &mut app, &mut input_rx, &mut ui_rx, &ui_tx).await;

    let _ = crossterm::execute!(
        std::io::stdout(),
        crossterm::event::DisableMouseCapture,
        crossterm::event::DisableBracketedPaste
    );
    ratatui::restore();
    result
}

#[allow(clippy::too_many_arguments)]
async fn event_loop(
    terminal: &mut ratatui::DefaultTerminal,
    app: &mut App,
    input_rx: &mut UnboundedReceiver<Event>,
    ui_rx: &mut UnboundedReceiver<UiEvent>,
    ui_tx: &UnboundedSender<UiEvent>,
) -> Result<()> {
    let mut ticker = tokio::time::interval(Duration::from_millis(80));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        // Coalescence : on applique **tout** ce qui est déjà arrivé avant de
        // dessiner. Sans ça, chaque `UiEvent::Token` déclenchait un rendu complet
        // de la conversation (re-wrap + re-parse markdown de tous les messages),
        // soit O(conversation) par token.
        while let Ok(ev) = ui_rx.try_recv() {
            apply_ui(ev, app, ui_tx);
        }

        terminal.draw(|f| draw(f, app))?;

        tokio::select! {
            maybe = input_rx.recv() => {
                if let Some(ev) = maybe {
                    handle_input(ev, app, ui_tx);
                }
            }
            maybe = ui_rx.recv() => {
                if let Some(ev) = maybe {
                    apply_ui(ev, app, ui_tx);
                }
            }
            _ = ticker.tick(), if app.streaming
                || app.mode != crate::fun::Mode::Chat
                || app.side_mode.is_some()
                || app.zc.on =>
            {
                app.anim_phase = app.anim_phase.wrapping_add(6);
                if app.zc.on {
                    app.zc.tick();
                    // Arrêt automatique de l'écoute après MAX_REC_SECS.
                    if let Some(el) = app.zc.rec_elapsed() {
                        if el > crate::zerochan::MAX_REC_SECS {
                            toggle_mic(app, ui_tx);
                        }
                    }
                }
                if app.mode == crate::fun::Mode::Radio
                    || app.side_mode == Some(crate::fun::Mode::Radio)
                {
                    app.radio.tick();
                }
                if app.mode == crate::fun::Mode::BinTime
                    || app.side_mode == Some(crate::fun::Mode::BinTime)
                {
                    app.bintime.tick();
                }
                if app.mode == crate::fun::Mode::Pong {
                    app.pong.step();
                }
            }
        }

        if app.should_quit {
            break;
        }
    }
    Ok(())
}

/// Applique un `UiEvent`. `Voice` a un effet de bord (il envoie le message),
/// d'où ce point d'entrée unique partagé par la boucle et la coalescence.
fn apply_ui(ev: UiEvent, app: &mut App, ui_tx: &UnboundedSender<UiEvent>) {
    match ev {
        UiEvent::Voice(t) => {
            app.zc.state = crate::zerochan::State::Idle;
            input_set(app, t);
            send_message(app, ui_tx);
        }
        other => handle_ui(other, app),
    }
}

/// Base system prompt + liste explicite des outils (ce modèle ignore souvent le
/// paramètre `tools` de l'API ; il faut le lui dire dans le prompt). Utile pour
/// les sous-agents qui reçoivent des outils (sinon le modèle « hallucine »).
fn system_with_tools(base: &str, tools: &[ToolSpec]) -> String {
    let mut s = base.to_string();
    if !tools.is_empty() {
        s.push_str(
            "\n\n## Outils disponibles (utilise-les pour AGIR — ne dis jamais « je ne peux pas exécuter » quand un outil le permet)\n",
        );
        for t in tools {
            s.push_str(&format!("- {} : {}\n", t.name, t.description));
        }
    }
    s
}

fn start_turn(app: &mut App, text: String, ui_tx: &UnboundedSender<UiEvent>) {
    let role = match &app.forced_role {
        Some(r) => r.clone(),
        None => route::auto_role(&text).to_string(),
    };
    app.last_role = role.clone();
    let search_text = text.clone();
    let mut user_msg = ChatMessage::user(text.clone());
    if let Some(img) = app.pending_image.take() {
        user_msg.image = Some(img);
        if let Some(name) = app.pending_image_name.take() {
            app.messages
                .push(ChatMessage::notice(format!("🖼 envoyée : {name}")));
        }
    }
    app.conversation.push(user_msg);
    app.messages.push(ChatMessage::user(text));
    if let Some(m) = app.conversation.last() {
        let _ = crate::convo::append(m);
    }
    let endpoints = route::chain_for_role(&app.settings(), &role);
    let utility = route::chain_for_role(&app.settings(), "utility");
    let target = endpoints
        .first()
        .map(|e| e.label.clone())
        .unwrap_or_else(|| "—".to_string());
    app.messages.push(ChatMessage::notice(if app.roles_enabled {
        format!("🎯 rôle {role} → {target}")
    } else {
        format!("🎯 modèle unique → {target} (bascule désactivée)")
    }));
    app.display_mark = app.messages.len();
    app.streaming = true;
    app.streaming_text.clear();
    app.reasoning_text.clear();
    app.auto_scroll = true;

    let tools: Vec<ToolSpec> = app
        .all_tools
        .iter()
        .filter(|t| {
            (t.name != "text_editor" || app.read_write) && (t.name != "code_exec" || app.code_exec)
        })
        .cloned()
        .collect();

    let system = app.system.clone();
    let options = ChatOptions {
        max_tokens: app.max_tokens,
        thinking: app.thinking,
        temperature: app.temperature,
    };
    let messages = app.conversation.clone();
    let kb_auto = app.kb_auto;
    let ctx_window = app.context_window;
    let ui = ui_tx.clone();
    tokio::spawn(async move {
        let mut system = system;
        // Liste explicite des outils disponibles : ce modèle ignore souvent le
        // paramètre `tools` de l'API, il faut le lui dire dans le prompt.
        if !tools.is_empty() {
            system.push_str(
                "\n\n## Outils disponibles (utilise-les pour AGIR — ne dis jamais « je ne peux pas exécuter » quand un outil le permet)\n",
            );
            for t in &tools {
                system.push_str(&format!("- {} : {}\n", t.name, t.description));
            }
        }
        // Connaissance locale : bloc partagé avec `--once` (`crate::knowledge_block`).
        if kb_auto {
            if let Some((block, srcs)) = crate::knowledge_block(&search_text).await {
                system.push_str(&block);
                let _ = ui.send(UiEvent::Info(format!(
                    "📚 connaissance injectée : {}",
                    srcs.join(", ")
                )));
            }
        }
        let mut base = messages;
        // Seuil de compaction : 60 % de la fenêtre **réelle** du modèle (min
        // 4 000). Avant, 4 000 en dur : sur un modèle 200 k on résumait un
        // historique parfaitement utilisable ; sur un 8 k on compactait trop tard.
        let budget = ((ctx_window as f64 * 0.6) as usize).max(4_000);
        if crate::agent::compact::estimate_tokens(&base) >= budget {
            let (b, outcome) = crate::agent::compact::compact_now(utility, &system, base, 6).await;
            if matches!(outcome, crate::agent::compact::CompactOutcome::Compacted) {
                let _ = ui.send(UiEvent::Compacted(b.clone()));
            }
            base = b;
        }
        crate::agent::turn::run_turn(endpoints, system, base, tools, options, ui).await;
    });
}

fn send_message(app: &mut App, ui_tx: &UnboundedSender<UiEvent>) {
    let text = app.input.trim().to_string();
    if text.is_empty() {
        return;
    }
    app.input.clear();
    app.cursor = 0;

    // Historique d'entrée (↑ le rappelle).
    if !app.history.last().map(|h| h == &text).unwrap_or(false) {
        app.history.push(text.clone());
    }
    app.hist_pos = None;
    app.hist_draft.clear();

    // Les commandes restent disponibles PENDANT une génération (/stop, /telegram…).
    if text.starts_with('/') {
        handle_command(app, &text, ui_tx);
        return;
    }
    if app.streaming {
        return;
    }

    start_turn(app, text, ui_tx);
}

fn nudge(app: &mut App, ui_tx: &UnboundedSender<UiEvent>) {
    if app.streaming {
        return;
    }
    app.messages
        .push(ChatMessage::notice("👉 Coup de coude (Nudge)"));
    let text = "Continue ton raisonnement ou ton exécution.".to_string();
    start_turn(app, text, ui_tx);
}

fn handle_command(app: &mut App, text: &str, ui_tx: &UnboundedSender<UiEvent>) {
    let mut it = text.split_whitespace();
    let cmd = it.next().unwrap_or("");
    let args: Vec<&str> = it.collect();
    match cmd {
        "/quit" | "/exit" => app.should_quit = true,
        "/clear" => {
            app.messages.clear();
            app.conversation.clear();
            app.display_mark = 0;
            app.streaming_text.clear();
        }
        "/new" => {
            app.conversation.clear();
            app.messages.clear();
            app.display_mark = 0;
            app.streaming_text.clear();
            let _ = crate::convo::clear();
            app.messages
                .push(ChatMessage::notice("🗂️ nouvelle conversation"));
        }
        "/history" => {
            let c = app.conversation.len();
            let mut s = format!("🗂️ conversation : {c} message(s)\n");
            for m in app.conversation.iter().rev().take(6).rev() {
                let t = m.content.clone().unwrap_or_default().replace('\n', " ");
                s.push_str(&format!(
                    "  [{}] {}\n",
                    m.role,
                    t.chars().take(70).collect::<String>()
                ));
            }
            s.push_str(&format!("\nfichier : {}", crate::convo::path().display()));
            app.messages.push(ChatMessage::notice(s));
        }
        "/think" => {
            app.thinking = !app.thinking;
            let state = if app.thinking {
                "ON (nettement plus lent)"
            } else {
                "OFF"
            };
            app.messages
                .push(ChatMessage::notice(format!("Raisonnement: {state}")));
        }
        "/tools" => {
            let mut s = String::from("Outils:\n");
            for t in &app.all_tools {
                let on = (t.name != "text_editor" || app.read_write)
                    && (t.name != "code_exec" || app.code_exec);
                s.push_str(&format!(
                    "  [{}] {} — {}\n",
                    if on { "on " } else { "off" },
                    t.name,
                    t.description
                ));
            }
            app.messages.push(ChatMessage::notice(s));
        }
        "/provider" => handle_provider(app, &args, ui_tx),
        "/model" => handle_model(app, &args, ui_tx),
        "/route" => handle_route(app, &args, ui_tx),
        "/role" => handle_role(app, &args, ui_tx),
        "/project" => handle_project(app),
        "/memory" => handle_memory(app, &args),
        "/embeddings" => handle_embeddings(app, &args),
        "/img" => handle_img(app, &args),
        "/update" => cmd_update(app, &args, ui_tx),
        "/remember" => cmd_remember(&args, ui_tx),
        "/recall" => cmd_recall(&args, ui_tx),
        "/forget" => handle_forget(app, &args),
        "/compact" => cmd_compact(app, ui_tx),
        "/task" => handle_task(app, &args),
        "/work" => cmd_work(app, ui_tx),
        "/ask" => cmd_ask(app, &args, ui_tx),
        "/review" => cmd_review(app, &args, ui_tx),
        "/search" => cmd_search(&args, ui_tx),
        "/stop" => {
            if app.streaming {
                crate::cancel::request();
            } else {
                app.messages
                    .push(ChatMessage::notice("Rien à interrompre."));
            }
        }
        "/context" => handle_context(app),
        "/whoami" => handle_whoami(app, &args),
        "/vault" => handle_vault(app, &args),
        "/pdf" => handle_pdf(app, &args),
        "/jev" => cmd_jev(app, &args, ui_tx),
        "/0box" => cmd_0box(app, &args, ui_tx),
        "/kb" => handle_kb(app, &args, ui_tx),
        "/apis" => handle_apis(app, &args),
        "/radio" => {
            let arg = args.join(" ");
            if !arg.trim().is_empty() {
                app.radio = crate::fun::radio::Radio::new();
                app.radio.open(&arg);
            } else if app.radio.tracks.is_empty() {
                app.radio = crate::fun::radio::Radio::new();
            }
            app.side_mode = Some(crate::fun::Mode::Radio);
            app.focus_panel = false;
        }
        "/bintime" => {
            app.bintime = crate::fun::bintime::BinTime::new();
            app.side_mode = Some(crate::fun::Mode::BinTime);
            app.focus_panel = false;
        }
        "/panel" => {
            app.radio.stop();
            app.side_mode = None;
            app.focus_panel = false;
        }
        "/pong" => {
            app.pong = crate::fun::pong::Pong::new();
            app.mode = crate::fun::Mode::Pong;
        }
        "/models" => cmd_list_models(app, ui_tx, false),
        "/ctx" => handle_ctx(app, &args),
        "/telegram" | "/tg" => handle_telegram(app, &args, ui_tx),
        "/menu" => {
            app.menu_focus = true;
            app.menu_sel = 0;
            app.messages.push(ChatMessage::notice(
                "Menu : ↑↓ naviguer · Entrée choisir (Esc pour sortir)",
            ));
        }
        "/readwrite" => {
            match args.first().copied() {
                Some("on") => app.read_write = true,
                Some("off") => app.read_write = false,
                _ => app.read_write = !app.read_write,
            }
            app.refresh_runtime();
            let s = if app.read_write { "ON" } else { "OFF" };
            app.messages
                .push(ChatMessage::notice(format!("Read&Write: {s}")));
        }
        "/codeexec" => {
            app.code_exec = !app.code_exec;
            app.refresh_runtime();
            let s = if app.code_exec { "ON" } else { "OFF" };
            app.messages
                .push(ChatMessage::notice(format!("Code-exec: {s}")));
            if app.code_exec {
                app.messages.push(ChatMessage::notice(
                    "⚠️ shell activé : les commandes s'exécutent SANS validation — tu es \
                     responsable des erreurs.",
                ));
            }
        }
        "/audit" => {
            let n = args
                .first()
                .and_then(|a| a.parse::<usize>().ok())
                .unwrap_or(30);
            let text = crate::commands::audit_tail(n).join("\n");
            app.messages.push(ChatMessage::notice(text));
        }
        "/chats" => app.chats_popup = Some((scan_chats(), 0)),
        "/nudge" => nudge(app, ui_tx),
        "/0chan" => app.zc.toggle_on(),
        "/mic" => toggle_mic(app, ui_tx),
        "/jimmy" => {
            let task = args.join(" ");
            if task.is_empty() {
                app.messages.push(ChatMessage::notice(format!(
                    "🐌 Jimmy : {}",
                    crate::jimmy::reply()
                )));
            } else {
                app.messages.push(ChatMessage::notice(format!(
                    "🐌 Jimmy se gratte la tête… (tâche : « {task} »)"
                )));
                let ui = ui_tx.clone();
                let delay = crate::jimmy::lazy_delay();
                tokio::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
                    let _ = ui.send(UiEvent::Info(format!(
                        "🐌 Jimmy (après {delay}s) : {}",
                        crate::jimmy::reply()
                    )));
                });
            }
        }
        "/provoke" => {
            // 1er mot = rôle SEULEMENT s'il est connu ; sinon tout est la tâche (rôle auto).
            let joined = args.join(" ");
            let (role, task) = match args.first().copied() {
                Some(r) if route::KNOWN_ROLES.contains(&r) && args.len() > 1 => {
                    (r.to_string(), args[1..].join(" "))
                }
                _ => (route::auto_role(&joined).to_string(), joined),
            };
            if task.trim().is_empty() {
                app.messages
                    .push(ChatMessage::notice("usage: /provoke [rôle] <tâche>"));
            } else {
                let jab = crate::jimmy::reply();
                app.messages.push(ChatMessage::notice(format!(
                    "🐌 Jimmy devait « {task} » → il a répondu : « {jab} »"
                )));
                app.messages
                    .push(ChatMessage::notice(format!("⚡ provocation → rôle {role}")));
                let prompt = format!(
                    "Le cancre Jimmy devait « {task} » et a seulement répondu : « {jab} ». \
                     Une IA compétente ferait mieux. Fais le travail correctement : {task}"
                );
                let endpoints = route::chain_for_role(&app.settings(), &role);
                // Respecte les toggles F3 (Read&Write) et F4 (Code-exec).
                let tools: Vec<ToolSpec> = app
                    .all_tools
                    .iter()
                    .filter(|t| {
                        (t.name != "text_editor" || app.read_write)
                            && (t.name != "code_exec" || app.code_exec)
                    })
                    .cloned()
                    .collect();
                let system = system_with_tools(&app.system, &tools);
                let options = ChatOptions {
                    max_tokens: app.max_tokens,
                    thinking: app.thinking,
                    temperature: app.temperature,
                };
                let ui = ui_tx.clone();
                tokio::spawn(async move {
                    let res = crate::agent::subagent::run_subagent(
                        endpoints, system, prompt, tools, options, None,
                    )
                    .await;
                    let _ = ui.send(UiEvent::Info(format!("⚡ Zer0 (piqué au vif) :\n{res}")));
                });
            }
        }
        "/quota" => cmd_quota(ui_tx, Some(app.provider_id.clone())),
        "/vote" => {
            let act = args.first().map(|s| s.to_ascii_lowercase());
            match act.as_deref() {
                Some("up") | Some("upvote") | Some("👍") => {
                    vote_click(ui_tx, crate::vote::Action::Up)
                }
                Some("down") | Some("downvote") | Some("👎") => {
                    vote_click(ui_tx, crate::vote::Action::Down)
                }
                Some("login") | Some("connect") => cmd_vote_login(ui_tx),
                Some("status") | Some("info") | None => cmd_vote(ui_tx),
                Some(other) => app.messages.push(ChatMessage::notice(format!(
                    "usage : /vote [up|down|status|login] (reçu : {other})"
                ))),
            }
        }
        "/free" => cmd_pick_free(ui_tx),
        "/select" => {
            let problem = args.join(" ");
            if problem.trim().is_empty() {
                app.messages
                    .push(ChatMessage::notice("usage: /select <problème à résoudre>"));
            } else {
                cmd_select(&problem, ui_tx);
            }
        }
        "/help" if args.first().copied() != Some("all") => {
            app.messages.push(ChatMessage::notice(HELP_SHORT))
        }
        "/setup" => wizard::start(app, ui_tx, wizard::Step::Inference),
        "/login" => wizard::start(app, ui_tx, wizard::Step::Provider),
        "/logout" => match crate::commands::provider_remove(&app.provider_id) {
            Ok(lines) => {
                for line in lines {
                    app.messages.push(ChatMessage::notice(line));
                }
                app.reload_settings();
            }
            Err(e) => app.messages.push(ChatMessage::notice(format!("{e}"))),
        },
        "/help" if args.first().copied() == Some("all") => {
            app.messages.push(ChatMessage::notice(
            "Commandes:\n\
             /help                      cette aide\n\
             /tools                     liste des outils\n\
             /menu /chats /nudge /codeexec /readwrite /0chan /mic   (= touches F2/F6/F7/F4/F3/F8/F10)\n\
             /                          palette de commandes (filtre en tapant)\n\
             /jimmy [tâche]             le cancre (répond « je sais pas »)\n\
             /provoke <rôle> <tâche>    Jimmy d'abord, puis provoque le vrai agent\n\
             /think                     active/desactive le raisonnement\n\
             /provider [list|keys|add|use|remove]  (sans arg : sélection)\n\
             /provider add <id> <clé>   dépose la clé sur zer0-gw (sinon local)\n\
             /provider keys             clés détenues par zer0-gw\n\
             /provider use <id>         change de provider\n\
             /model                     modèle actuel + sélection (liste)\n\
             /img <chemin>              joint une image au prochain message (vision)\n\
             /update [check]            met à jour le binaire (Releases GitHub)\n\
             Ctrl+O / clic droit        déplier CE bloc de sortie d'outil\n\
             /model use <id>            change de modèle\n\
             /models [free]             liste les modèles du provider\n\
             /ctx [N|1M|auto]           contexte du modèle (auto/override)\n\
             /telegram [on|off]         bot Telegram à distance (long polling)\n\
             /free                      menu des modèles gratuits\n\
             /quota                     usage des modèles (appels, 429, solde de clé)\n\
             /vote [up|down|status]     vote pour zer0gh0st (👍 = une star GitHub)\n\
             /route [list|add|clear]    chaîne de bascule (failover)\n\
             /route auto [n]            remplit la chaîne avec les modèles :free\n\
             /route add <provider>/<model>\n\
             /role [list]               rôles et modèles associés\n\
             /role off|on               off = un seul modèle, sans bascule ; on = routage\n\
             /role set <role> <p>/<m>   associe un modèle à un rôle\n\
             /role force <role>|unforce force un rôle ou repasse en auto\n\
             /project                   config du projet courant\n\
             /memory                    entrées + honnêteté du store\n\
             /embeddings set <p> <m>    embedder de la mémoire sémantique\n\
             /remember <texte>           sauvegarde en mémoire\n\
             /recall <requête>           recherche en mémoire\n\
             /forget <id>                supprime une entrée\n\
             /compact                    résume l'historique (rôle utility)\n\
             /task [list|add|done|claim|rm|clear] gère les tâches (rm <id>, clear = enlève les [x])\n\
             /work                       exécute la prochaine tâche (sous-agent)\n\
             /ask <role> <question>      sous-agent isolé (sans outils)\n\
             /review <fichier>           relecture par le rôle review + verdict\n\
             /search <requête>           recherche web (DuckDuckGo)\n\
             /stop                       interrompt la génération en cours\n\
             /context                    état de la conversation\n\
             /whoami [nom|off]           persona (prompt système alternatif)\n\
             /vault [status|lock]        coffre de clés (setup/unlock en CLI)\n\
             /pdf info|text|merge|split  outils PDF locaux\n\
             /jev <fichier>              score qualité code 0-3 (rôle review)\n\
             /kb [stats|search|ingest|auto|rebuild] base de connaissance (+ injection auto)\n\
             /apis [besoin]              catalogue d'APIs publiques (community, daté)\n\
             /radio [dossier|fichier|url] panneau audio à droite (Tab pour focus, s stop)\n\
             /bintime                    panneau horloge binaire (à droite)\n\
             /panel                      ferme le panneau latéral\n\
             /pong                       Pong contre l'agent (plein écran)\n\
             /select <problème>         recommande un modèle :free\n\
             /clear                     efface le chat (affichage + mémoire vive)\n\
             /new                       efface la conversation (fichier inclus)\n\
             /history                   montre la conversation persistée\n\
             /quit                      quitter",
            ));
        }
        other => app
            .messages
            .push(ChatMessage::notice(format!("Commande inconnue: {other}"))),
    }
}

fn handle_provider(app: &mut App, args: &[&str], ui_tx: &UnboundedSender<UiEvent>) {
    match args.first().copied() {
        // Sélecteur (spécifique TUI : nécessite un état d'interface).
        None => {
            let gw = config::gateway_enabled();
            let items: Vec<picker::Item> = providers::all()
                .into_iter()
                .map(|p| {
                    let note = if p.free == providers::FreeKind::Local {
                        "local".to_string()
                    } else if gw && !p.custom {
                        "passerelle (clé dans zer0-gw)".to_string()
                    } else if config::key_for(&p).is_some() {
                        "clé locale ✓".to_string()
                    } else {
                        "—".to_string()
                    };
                    picker::Item {
                        id: p.id.clone(),
                        label: format!("{} — {}", p.id, p.name),
                        note,
                    }
                })
                .collect();
            app.picker = Some(picker::Picker::new(
                "Providers",
                items,
                picker::Kind::Provider,
            ));
        }
        Some(sub) => {
            // Tout le reste est délégué au noyau : mêmes règles que le CLI.
            let res = match sub {
                "list" => Ok(crate::commands::provider_list()),
                "keys" => Ok(crate::commands::provider_keys()),
                "add" => {
                    let first = args.get(1).copied().unwrap_or("");
                    if first.eq_ignore_ascii_case("cline") {
                        // /provider add cline [url] [clé]
                        crate::commands::provider_add_cline(
                            args.get(2).copied(),
                            args.get(3).copied(),
                        )
                    } else {
                        crate::commands::provider_add(first, args.get(2).copied())
                    }
                }
                "add-custom" => crate::commands::provider_add_custom(
                    args.get(1).copied().unwrap_or(""),
                    args.get(2).copied().unwrap_or(""),
                    args.get(3).copied(),
                    args.get(4).copied(),
                ),
                "use" => crate::commands::provider_use(args.get(1).copied().unwrap_or("")),
                "remove" => crate::commands::provider_remove(args.get(1).copied().unwrap_or("")),
                _ => Err(anyhow::anyhow!(
                    "usage: /provider [list|keys|add|add-custom|use|remove]"
                )),
            };
            match res {
                Ok(lines) => {
                    for line in lines {
                        app.messages.push(ChatMessage::notice(line));
                    }
                    // La config a pu changer : on resynchronise l'état de session.
                    if matches!(sub, "add" | "add-custom" | "use" | "remove") {
                        app.reload_settings();
                        if sub == "add" || sub == "use" {
                            probe_context(app, ui_tx);
                        }
                    }
                }
                Err(e) => app.messages.push(ChatMessage::notice(format!("{e}"))),
            }
        }
    }
}
/// Touches quand un sélecteur est ouvert.
fn handle_picker_key(
    key: crossterm::event::KeyEvent,
    app: &mut App,
    ui_tx: &UnboundedSender<UiEvent>,
) {
    match key.code {
        KeyCode::Esc => app.picker = None,
        KeyCode::Up => {
            if let Some(p) = app.picker.as_mut() {
                p.move_sel(-1)
            }
        }
        KeyCode::Down => {
            if let Some(p) = app.picker.as_mut() {
                p.move_sel(1)
            }
        }
        KeyCode::PageUp => {
            if let Some(p) = app.picker.as_mut() {
                p.page(-1, 10)
            }
        }
        KeyCode::PageDown => {
            if let Some(p) = app.picker.as_mut() {
                p.page(1, 10)
            }
        }
        KeyCode::Backspace => {
            if let Some(p) = app.picker.as_mut() {
                p.pop_char()
            }
        }
        KeyCode::Char(c) => {
            // Palette de commandes : l'espace VALIDE la frappe et ferme la
            // palette. Sans ça, elle avalait la suite et `/whoami zero-vamp`
            // (ou `/task add …`) était impossible à écrire.
            let is_command = app
                .picker
                .as_ref()
                .map(|p| p.kind == picker::Kind::Command)
                .unwrap_or(false);
            if is_command && c == ' ' {
                if let Some(p) = app.picker.take() {
                    let text = palette_text(&p.filter);
                    input_set(app, format!("{text} "));
                }
                return;
            }
            if let Some(p) = app.picker.as_mut() {
                p.push_char(c)
            }
        }
        KeyCode::Enter => {
            let Some(p) = app.picker.take() else { return };
            let Some(item) = p.selected().cloned() else {
                // Aucun résultat : on rend la frappe à la saisie au lieu de
                // l'avaler (et de risquer d'exécuter le premier élément).
                input_set(app, palette_text(&p.filter));
                return;
            };
            match p.kind {
                picker::Kind::Model => {
                    if let Some(role) = p.target.clone() {
                        // `/role set <rôle>` sans modèle : le menu fournit le modèle.
                        let route = format!("{}/{}", app.provider_id, item.id);
                        match crate::commands::role_set(&role, &route) {
                            Ok(lines) => {
                                for line in lines {
                                    app.messages.push(ChatMessage::notice(line));
                                }
                            }
                            Err(e) => app
                                .messages
                                .push(ChatMessage::notice(format!("Erreur: {e}"))),
                        }
                    } else {
                        set_model(app, &item.id, ui_tx)
                    }
                }
                picker::Kind::Provider => set_provider(app, &item.id, ui_tx),
                picker::Kind::Command => {
                    // Une commande finissant par un espace attend un argument :
                    // on l'insère ; les autres s'exécutent.
                    if item.id.ends_with(' ') {
                        input_set(app, item.id.clone());
                    } else {
                        handle_command(app, &item.id, ui_tx);
                    }
                }
            }
        }
        _ => {}
    }
}

/// Applique un modèle (refuse les embedders pour le chat).
fn set_model(app: &mut App, m: &str, ui_tx: &UnboundedSender<UiEvent>) {
    if select::is_embedding_or_rerank(m) {
        app.messages.push(ChatMessage::notice(format!(
            "❌ {m} est un modèle d'embedding/rerank — inutilisable en chat \
             (endpoint /embeddings). Modèle inchangé : {}",
            app.model
        )));
        return;
    }
    let mut g = config::load_global();
    g.model = Some(m.to_string());
    let _ = config::save_global(&g);
    app.model = m.to_string();
    probe_context(app, ui_tx);
    app.messages
        .push(ChatMessage::notice(format!("Modèle actif: {m}")));
}

/// Applique un provider (le transport passerelle/direct est recalculé).
fn set_provider(app: &mut App, id: &str, ui_tx: &UnboundedSender<UiEvent>) {
    match providers::find(id) {
        Some(p) => {
            let mut g = config::load_global();
            g.provider = Some(p.id.to_string());
            if let Some(d) = providers::default_model(&p.id) {
                g.model = Some(d.to_string());
            }
            let _ = config::save_global(&g);
            let s = config::resolve();
            app.provider_id = s.provider_id.clone();
            app.provider_name = s.provider_name.clone();
            app.base_url = s.base_url.clone();
            app.api_key = s.api_key.clone();
            app.model = s.model.clone();
            app.messages
                .push(ChatMessage::notice(format!("Provider actif: {}", p.name)));
            probe_context(app, ui_tx);
        }
        None => app
            .messages
            .push(ChatMessage::notice(format!("Provider inconnu: {id}"))),
    }
}

/// `/update [check]` : met à jour le binaire depuis les Releases GitHub.
fn cmd_update(app: &mut App, args: &[&str], ui_tx: &UnboundedSender<UiEvent>) {
    let check = matches!(args.first().copied(), Some("check") | Some("--check"));
    app.messages
        .push(ChatMessage::notice("⏳ vérification des mises à jour…"));
    let ui = ui_tx.clone();
    tokio::spawn(async move {
        let msg = match crate::update::run(check, false).await {
            Ok(m) => m,
            Err(e) => format!("échec : {e}"),
        };
        let _ = ui.send(UiEvent::Info(format!("⬆ {msg}")));
    });
}

/// `/img <chemin>` : joint une image (PNG/JPEG/WebP/GIF) au **prochain** message.
/// Nécessite un modèle multimodal (vision) ; avertit honnêtement sinon.
fn handle_img(app: &mut App, args: &[&str]) {
    match args.first().copied() {
        None | Some("show") => match &app.pending_image_name {
            Some(n) => app.messages.push(ChatMessage::notice(format!(
                "🖼 en attente : {n} — jointe au prochain message · /img clear"
            ))),
            None => app.messages.push(ChatMessage::notice(
                "usage: /img <chemin>  (jointe au prochain message) · /img clear",
            )),
        },
        Some("clear") | Some("off") => {
            app.pending_image = None;
            app.pending_image_name = None;
            app.messages.push(ChatMessage::notice("Image retirée."));
        }
        Some(path) => {
            let (data_url, size) = match crate::platform::image_data_url(path) {
                Ok(v) => v,
                Err(e) => {
                    app.messages.push(ChatMessage::notice(e));
                    return;
                }
            };
            app.pending_image = Some(data_url);
            app.pending_image_name = Some(format!("{path} ({:.0} Kio)", size as f64 / 1024.0));

            // Garde-fou : le modèle actif accepte-t-il les images ?
            let note = if app.vision_models.iter().any(|id| id == &app.model) {
                format!("✓ {} accepte les images (catalogue)", app.model)
            } else if !app.vision_models.is_empty() {
                format!(
                    "⚠ {} n'accepte PAS les images d'après le catalogue — choisis un modèle 🖼 avec /model",
                    app.model
                )
            } else if select::supports_images_by_name(&app.model) {
                format!("~ {} : nom suggérant un modèle multimodal", app.model)
            } else {
                format!(
                    "⚠ {} : le nom ne suggère pas un modèle multimodal — l'envoi peut échouer (le provider le dira)",
                    app.model
                )
            };
            app.messages.push(ChatMessage::notice(format!(
                "🖼 image attachée (prochain message) : {path}\n{note}"
            )));
        }
    }
}

/// Ouvre la palette de commandes (filtrable) depuis l'arbre du menu cliquable.
/// Texte à réinjecter dans la saisie quand on quitte la palette de commandes.
/// (pur : testable sans interface)
fn palette_text(filter: &str) -> String {
    let f = filter.trim();
    if f.is_empty() {
        "/".to_string()
    } else {
        format!("/{f}")
    }
}

fn open_command_palette(app: &mut App) {
    let items: Vec<picker::Item> = menu::flat_commands()
        .into_iter()
        .map(|(code, label)| picker::Item {
            id: code.clone(),
            label: code,
            note: label,
        })
        .collect();
    app.picker = Some(picker::Picker::new(
        "Commandes",
        items,
        picker::Kind::Command,
    ));
}

/// Récupère la liste des modèles du provider actif puis ouvre le sélecteur.
/// `/free` : menu des modèles gratuits du provider (au lieu d'une liste texte).
fn cmd_pick_free(ui_tx: &UnboundedSender<UiEvent>) {
    let settings = config::resolve();
    let Some(provider) = providers::find(&settings.provider_id) else {
        return;
    };
    let key = settings.api_key.clone();
    let ui = ui_tx.clone();
    tokio::spawn(async move {
        match models::list(&http_client::HTTP_CLIENT, &provider, key.as_deref()).await {
            Ok(list) => {
                let free: Vec<_> = list.into_iter().filter(models::is_free).collect();
                let _ = ui.send(UiEvent::FreeModels(free));
            }
            Err(e) => {
                let _ = ui.send(UiEvent::Info(format!("Erreur modèles :free : {e}")));
            }
        }
    });
}

fn cmd_pick_model(app: &mut App, ui_tx: &UnboundedSender<UiEvent>, target: Option<String>) {
    app.pending_picker_target = target;
    let settings = config::resolve();
    let Some(provider) = providers::find(&settings.provider_id) else {
        return;
    };
    let key = settings.api_key.clone();
    let ui = ui_tx.clone();
    tokio::spawn(async move {
        match models::list(&http_client::HTTP_CLIENT, &provider, key.as_deref()).await {
            Ok(list) => {
                let _ = ui.send(UiEvent::ModelList(list));
            }
            Err(e) => {
                let _ = ui.send(UiEvent::Info(format!(
                    "Erreur modèles {}: {e}",
                    provider.name
                )));
            }
        }
    });
}

fn handle_model(app: &mut App, args: &[&str], ui_tx: &UnboundedSender<UiEvent>) {
    let model = match args.first().copied() {
        Some("use") | Some("set") => args.get(1).copied(),
        None => {
            app.messages.push(ChatMessage::notice(format!(
                "Modèle actuel: {} — sélection ci-dessous",
                app.model
            )));
            cmd_pick_model(app, ui_tx, None);
            return;
        }
        Some(m) => Some(m),
    };
    match model {
        Some(m) => {
            // Même contrôle et même persistance que `zer0 model use <id>`.
            match crate::commands::set_model(m) {
                Ok(lines) => {
                    app.model = m.to_string();
                    probe_context(app, ui_tx);
                    for line in lines {
                        app.messages.push(ChatMessage::notice(line));
                    }
                }
                Err(e) => app.messages.push(ChatMessage::notice(format!("❌ {e}"))),
            }
        }
        None => {
            // Sans identifiant : menu sélectionnable (plus de saisie à l'aveugle).
            cmd_pick_model(app, ui_tx, None);
            app.messages
                .push(ChatMessage::notice("Choisis un modèle dans la liste."));
        }
    }
}

fn handle_role(app: &mut App, args: &[&str], ui_tx: &UnboundedSender<UiEvent>) {
    match args.first().copied() {
        None | Some("list") => {
            let auto = match &app.forced_role {
                Some(r) => format!("forcé: {r}"),
                None if app.last_role.is_empty() => "auto".to_string(),
                None => format!("auto (dernier: {})", app.last_role),
            };
            let etat = if app.roles_enabled {
                "actifs"
            } else {
                "désactivés — /role on pour réactiver"
            };
            let mut s = format!("Rôles ({etat} · {auto}):\n");
            for (role, route) in crate::commands::roles_snapshot() {
                let used = if app.forced_role.as_deref() == Some(role) {
                    "*"
                } else {
                    " "
                };
                s.push_str(&format!(
                    " {used} {role:<10} {}\n",
                    route.unwrap_or_else(|| "—".to_string())
                ));
            }
            s.push_str(
                "\n/role on|off · /role set <role> <p>/<m> · /role force <role> · \
                 /role unforce · /role auto [provider] · /role unset <role>",
            );
            app.messages.push(ChatMessage::notice(s));
        }
        Some("set") => {
            let role = args.get(1).copied().unwrap_or("");
            let route_s = args.get(2).copied().unwrap_or("");
            if role.is_empty() {
                app.messages.push(ChatMessage::notice(
                    "usage: /role set <role>  (sans modèle : un menu s'ouvre)",
                ));
            } else if route_s.is_empty() {
                // Menu sélectionnable : plus besoin de connaître la syntaxe
                // `provider/model` pour associer un modèle à un rôle.
                cmd_pick_model(app, ui_tx, Some(role.to_string()));
                app.messages.push(ChatMessage::notice(format!(
                    "Choisis le modèle du rôle `{role}`."
                )));
            } else {
                match crate::commands::role_set(role, route_s) {
                    Ok(lines) => {
                        for line in lines {
                            app.messages.push(ChatMessage::notice(line));
                        }
                    }
                    Err(e) => app
                        .messages
                        .push(ChatMessage::notice(format!("Erreur: {e}"))),
                }
            }
        }
        Some("unset") => match crate::commands::role_unset(args.get(1).copied().unwrap_or("")) {
            Ok(lines) => {
                for line in lines {
                    app.messages.push(ChatMessage::notice(line));
                }
            }
            Err(e) => app
                .messages
                .push(ChatMessage::notice(format!("Erreur: {e}"))),
        },
        Some("off") | Some("on") => {
            let on = args[0] == "on";
            match crate::commands::set_roles(on) {
                Ok(lines) => {
                    app.roles_enabled = on;
                    if !on {
                        app.forced_role = None;
                    }
                    for line in lines {
                        app.messages.push(ChatMessage::notice(line));
                    }
                }
                Err(e) => app
                    .messages
                    .push(ChatMessage::notice(format!("Erreur: {e}"))),
            }
        }
        // Forcer un rôle est un état de session (pas de config) : TUI seulement.
        Some("use") | Some("force") => {
            let role = args.get(1).copied().unwrap_or("");
            if route::KNOWN_ROLES.contains(&role) {
                if !app.roles_enabled {
                    let _ = crate::commands::set_roles(true);
                    app.roles_enabled = true;
                }
                app.forced_role = Some(role.to_string());
                app.messages
                    .push(ChatMessage::notice(format!("Rôle forcé: {role}")));
            } else {
                app.messages.push(ChatMessage::notice(format!(
                    "Rôle inconnu: {role} — connus : {}",
                    route::KNOWN_ROLES.join(", ")
                )));
            }
        }
        Some("unforce") => {
            app.forced_role = None;
            app.messages
                .push(ChatMessage::notice("Rôle forcé retiré (routage auto)."));
        }
        Some("auto") => {
            let (provider, _) = crate::commands::parse_auto_args(&args[1.min(args.len())..]);
            let ui = ui_tx.clone();
            tokio::spawn(async move {
                let msg = match crate::commands::role_auto(&provider).await {
                    Ok(lines) => lines.join("\n"),
                    Err(e) => format!("Erreur auto: {e}"),
                };
                let _ = ui.send(UiEvent::Info(msg));
            });
        }
        _ => app.messages.push(ChatMessage::notice(
            "usage: /role [list|on|off|set <role> <p>/<m>|force <role>|unforce|auto [provider]|unset <role>]",
        )),
    }
}
fn cmd_compact(app: &mut App, ui_tx: &UnboundedSender<UiEvent>) {
    if app.streaming {
        app.messages.push(ChatMessage::notice(
            "Attends la fin de la réponse pour compacter.",
        ));
        return;
    }
    let utility = route::chain_for_role(&app.settings(), "utility");
    let system = app.system.clone();
    let messages = app.conversation.clone();
    let ui = ui_tx.clone();
    tokio::spawn(async move {
        let (base, outcome) =
            crate::agent::compact::compact_now(utility, &system, messages, 2).await;
        match outcome {
            crate::agent::compact::CompactOutcome::Compacted => {
                let _ = ui.send(UiEvent::Compacted(base));
            }
            other => {
                let _ = ui.send(UiEvent::Info(other.message()));
            }
        }
    });
}

fn handle_task(app: &mut App, args: &[&str]) {
    // Même parseur et même exécution que `zer0 task …`.
    let cmd = match crate::commands::parse_task(args) {
        Ok(c) => c,
        Err(e) => {
            app.messages.push(ChatMessage::notice(format!("{e}")));
            return;
        }
    };
    match crate::commands::task(cmd) {
        Ok(lines) => {
            let text = lines.join("\n");
            if !text.trim().is_empty() {
                app.messages.push(ChatMessage::notice(text));
            }
        }
        Err(e) => app
            .messages
            .push(ChatMessage::notice(format!("Erreur: {e}"))),
    }
}
fn cmd_ask(app: &mut App, args: &[&str], ui_tx: &UnboundedSender<UiEvent>) {
    let role = args.first().copied().unwrap_or("");
    let question = args.get(1..).map(|s| s.join(" ")).unwrap_or_default();
    if role.is_empty() || question.trim().is_empty() {
        app.messages
            .push(ChatMessage::notice("usage: /ask <role> <question>"));
        return;
    }
    let endpoints = route::chain_for_role(&app.settings(), role);
    let system = app.system.clone();
    let options = ChatOptions {
        max_tokens: app.max_tokens,
        thinking: app.thinking,
        temperature: app.temperature,
    };
    let ui = ui_tx.clone();
    let role_s = role.to_string();
    tokio::spawn(async move {
        let res = crate::agent::subagent::run_subagent(
            endpoints,
            system,
            question,
            Vec::new(),
            options,
            None,
        )
        .await;
        let _ = ui.send(UiEvent::Info(format!("🤖 [{role_s}] {res}")));
    });
}

fn cmd_review(app: &mut App, args: &[&str], ui_tx: &UnboundedSender<UiEvent>) {
    let path = args.first().copied().unwrap_or("");
    if path.is_empty() {
        app.messages
            .push(ChatMessage::notice("usage: /review <fichier>"));
        return;
    }
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            app.messages
                .push(ChatMessage::notice(format!("Erreur de lecture: {e}")));
            return;
        }
    };
    let excerpt = if content.len() > 20_000 {
        &content[..20_000]
    } else {
        &content
    };
    let task = format!(
        "Relis le fichier `{path}`. Donne un verdict clair. Termine par \
         'VERDICT: OK' ou 'VERDICT: PROBLEMES' puis la liste courte des problèmes. \
         Ne réécris pas le fichier.\n\n{excerpt}"
    );
    let endpoints = route::chain_for_role(&app.settings(), "review");
    let system = app.system.clone();
    let options = ChatOptions {
        max_tokens: app.max_tokens,
        thinking: app.thinking,
        temperature: app.temperature,
    };
    let ui = ui_tx.clone();
    let path_s = path.to_string();
    tokio::spawn(async move {
        let res = crate::agent::subagent::run_subagent(
            endpoints,
            system,
            task,
            Vec::new(),
            options,
            None,
        )
        .await;
        let _ = ui.send(UiEvent::Info(format!("🔍 [review {path_s}]\n{res}")));
    });
}

fn cmd_work(app: &mut App, ui_tx: &UnboundedSender<UiEvent>) {
    let Some(task) = crate::tasks::next_pending() else {
        app.messages.push(ChatMessage::notice(
            "Aucune tâche en attente (`/task add …`)",
        ));
        return;
    };
    let _ = crate::tasks::set_status(&task.id, crate::tasks::Status::Claimed);
    // Rôle de la tâche ; sinon on l'auto-détecte depuis le texte (comme un message).
    let role = task
        .role
        .clone()
        .unwrap_or_else(|| route::auto_role(&task.text).to_string());
    let endpoints = route::chain_for_role(&app.settings(), &role);
    let tools: Vec<ToolSpec> = app
        .all_tools
        .iter()
        .filter(|t| {
            (t.name != "text_editor" || app.read_write) && (t.name != "code_exec" || app.code_exec)
        })
        .cloned()
        .collect();
    let system = system_with_tools(&app.system, &tools);
    let options = ChatOptions {
        max_tokens: app.max_tokens,
        thinking: app.thinking,
        temperature: app.temperature,
    };
    let ui = ui_tx.clone();
    let id = task.id.clone();
    let text = task.text.clone();
    app.messages.push(ChatMessage::notice(format!(
        "🛠 tâche {id} [{role}] en cours…"
    )));
    tokio::spawn(async move {
        let res =
            crate::agent::subagent::run_subagent(endpoints, system, text, tools, options, None)
                .await;
        match crate::tasks::set_status(&id, crate::tasks::Status::Done) {
            Ok(crate::tasks::Update::Ok(_)) => {
                let _ = ui.send(UiEvent::Info(format!("🛠 [{id}] fait\n{res}")));
            }
            Ok(crate::tasks::Update::Refused(missing)) => {
                let mut m = format!(
                    "⚠ [{id}] NON marquée faite — livrable(s) en défaut (porte d'honnêteté) :\n"
                );
                for (p, why) in &missing {
                    m.push_str(&format!("  ✗ {p} ({why})\n"));
                }
                m.push_str(&format!("\nrésultat du sous-agent :\n{res}"));
                let _ = ui.send(UiEvent::Info(m));
            }
            _ => {
                let _ = ui.send(UiEvent::Info(format!("🛠 [{id}] (statut inchangé)\n{res}")));
            }
        }
    });
}

fn cmd_search(args: &[&str], ui_tx: &UnboundedSender<UiEvent>) {
    let q = args.join(" ");
    let ui = ui_tx.clone();
    tokio::spawn(async move {
        // Même implémentation que `zer0 search …`.
        let msg = match crate::commands::search(&q, 5).await {
            Ok(lines) => lines.join("\n"),
            Err(e) => format!("Erreur recherche : {e}"),
        };
        let _ = ui.send(UiEvent::Info(msg));
    });
}
fn handle_context(app: &mut App) {
    let n = app.conversation.len();
    let chars: usize = app
        .conversation
        .iter()
        .filter_map(|m| m.content.as_ref())
        .map(|c| c.chars().count())
        .sum();
    let tokens = chars / 4;
    let s = format!(
        "Contexte :\n  messages : {n}\n  ~tokens : {tokens}\n  compaction auto à : 6000 tokens\n\
         rôle : {}\n  endpoint : {}\n  think : {}\n\
         commandes : /context · /clear · /compact",
        app.forced_role
            .clone()
            .unwrap_or_else(|| format!("auto/{}", app.last_role)),
        if app.active_endpoint.is_empty() {
            format!("{} · {}", app.provider_name, app.model)
        } else {
            app.active_endpoint.clone()
        },
        if app.thinking { "on" } else { "off" }
    );
    app.messages.push(ChatMessage::notice(s));
}

fn handle_whoami(app: &mut App, args: &[&str]) {
    match args.first().copied() {
        // L'activation d'une persona est un état de session : TUI seulement.
        None | Some("list") => {
            let cur = app
                .persona
                .clone()
                .unwrap_or_else(|| "(aucune)".to_string());
            let mut lines = vec![format!("Persona actuelle : {cur}")];
            lines.extend(crate::commands::persona_list());
            lines.push("whoami <nom>  ·  whoami off  ·  whoami add <nom> <fichier>".to_string());
            app.messages.push(ChatMessage::notice(lines.join("\n")));
        }
        Some("off") | Some("none") => {
            app.persona = None;
            app.refresh_system();
            app.messages.push(ChatMessage::notice("Persona retirée."));
        }
        Some(name) => match crate::personas::get(name) {
            Some(_) => {
                app.persona = Some(name.to_string());
                app.refresh_system();
                app.messages
                    .push(ChatMessage::notice(format!("Persona '{name}' activée.")));
            }
            None => app.messages.push(ChatMessage::notice(format!(
                "Persona '{name}' introuvable — whoami list"
            ))),
        },
    }
}
fn handle_vault(app: &mut App, args: &[&str]) {
    // `setup`/`unlock`/`reset` demandent une passphrase : CLI uniquement.
    let lines = match args.first().copied() {
        None | Some("status") => {
            let mut l = crate::commands::vault_status();
            l.push(String::new());
            l.push("vault lock · setup/unlock/reset via `zer0 vault …` (CLI)".to_string());
            l
        }
        Some("lock") => crate::commands::vault_lock(),
        _ => vec!["usage: /vault [status|lock]".to_string()],
    };
    app.messages.push(ChatMessage::notice(lines.join("\n")));
}
fn handle_pdf(app: &mut App, args: &[&str]) {
    // Même parseur que `zer0 pdf …`.
    match crate::commands::parse_pdf_args(args).and_then(crate::commands::pdf) {
        Ok(lines) => app.messages.push(ChatMessage::notice(lines.join("\n"))),
        Err(e) => app.messages.push(ChatMessage::notice(format!("{e}"))),
    }
}
fn cmd_jev(app: &mut App, args: &[&str], ui_tx: &UnboundedSender<UiEvent>) {
    let path = args.first().copied().unwrap_or("");
    if path.is_empty() {
        app.messages
            .push(ChatMessage::notice("usage: /jev <fichier>"));
        return;
    }
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            app.messages
                .push(ChatMessage::notice(format!("Erreur de lecture: {e}")));
            return;
        }
    };
    let endpoints = route::chain_for_role(&app.settings(), "review");
    let system = app.system.clone();
    let options = ChatOptions {
        max_tokens: 512,
        thinking: false,
        temperature: 0.2,
    };
    let ui = ui_tx.clone();
    let path_s = path.to_string();
    let nchars = content.chars().count();
    app.messages.push(ChatMessage::notice(format!(
        "📊 jev : {path_s} ({nchars} caractères) → analyse du code complet…"
    )));
    tokio::spawn(async move {
        let res =
            crate::agent::subagent::run_jev(endpoints, system, &path_s, &content, options, None)
                .await;
        let _ = ui.send(UiEvent::Info(format!("📊 [jev {path_s}]\n{res}")));
    });
}

pub fn cmd_0box(app: &mut App, args: &[&str], ui_tx: &UnboundedSender<UiEvent>) {
    let mut args_iter = args.iter().peekable();

    // Handle subcommands
    if let Some(first) = args_iter.peek() {
        match **first {
            "fallback" => {
                handle_0box_fallback(app, &args_iter.skip(1).cloned().collect::<Vec<_>>(), ui_tx);
                return;
            }
            "help" => {
                show_0box_help(app);
                return;
            }
            _ => {}
        }
    }

    let topic = args.to_vec().join(" ");
    if topic.trim().is_empty() {
        app.messages.push(ChatMessage::notice("usage: /0box <sujet> [--perspectives main,code,reasoning]\n       /0box fallback <add|remove|list> <provider/model>\n       /0box help"));
        return;
    }

    // Parse optional --perspectives flag
    // Default order: code → reasoning → main (main always last for synthesis+execution)
    let mut perspectives: Vec<String> = vec![
        "code".to_string(),
        "reasoning".to_string(),
        "main".to_string(),
    ];
    let topic_parts: Vec<&str> = topic.split_whitespace().collect();
    let mut topic_words = Vec::new();
    let mut i = 0;
    while i < topic_parts.len() {
        if topic_parts[i] == "--perspectives" && i + 1 < topic_parts.len() {
            perspectives = topic_parts[i + 1]
                .split(',')
                .map(|s| s.trim().to_string())
                .collect();
            i += 2;
        } else {
            topic_words.push(topic_parts[i]);
            i += 1;
        }
    }
    let topic = topic_words.join(" ");

    // Launch sequential debate as a background task
    let app_settings = app.settings();
    let system = app.system.clone();
    let options = ChatOptions {
        max_tokens: app.max_tokens,
        thinking: app.thinking,
        temperature: app.temperature,
    };
    let read_write = app.read_write;
    let code_exec = app.code_exec;
    let all_tools = app.all_tools.clone();
    let ui = ui_tx.clone();
    let topic_owned = topic.clone();
    let perspectives_owned = perspectives.clone();

    app.messages.push(ChatMessage::notice(format!(
        "[0box] 🎭 DÉBAT SÉQUENTIEL sur '{}' — perspectives: {} — en cours…",
        topic,
        perspectives.join(", ")
    )));

    tokio::spawn(async move {
        run_0box_debate(
            topic_owned,
            perspectives_owned,
            DebateSetup {
                settings: app_settings,
                system,
                options,
                read_write,
                code_exec,
                all_tools,
                ui,
            },
        )
        .await;
    });
}

/// Contexte d'un débat 0box (regroupé : la fonction principale dépassait la
/// limite d'arguments de clippy, et 9 paramètres ne se lisent pas).
pub struct DebateSetup {
    pub settings: crate::config::Settings,
    pub system: String,
    pub options: ChatOptions,
    pub read_write: bool,
    pub code_exec: bool,
    pub all_tools: Vec<crate::tools::ToolSpec>,
    pub ui: UnboundedSender<UiEvent>,
}

/// Tour de débat : qui parle, à quoi, vers quels endpoints.
struct DebateTurn<'a> {
    role: &'a str,
    role_label: &'a str,
    prompt: &'a str,
    endpoints: &'a [crate::route::Endpoint],
}

/// Lance un vrai débat séquentiel : chaque modèle reçoit la conversation complète
/// et répond à son tour. Le dernier (MAIN) synthétise et peut exécuter.
/// Publique : également appelée par le CLI headless (`zer0 0box <sujet>`).
pub async fn run_0box_debate(topic: String, perspectives: Vec<String>, setup: DebateSetup) {
    let DebateSetup {
        settings: app_settings,
        system,
        options,
        read_write,
        code_exec,
        all_tools,
        ui,
    } = setup;
    // Build tools available (respect F3/F4)
    let tools: Vec<crate::tools::ToolSpec> = all_tools
        .iter()
        .filter(|t| (t.name != "text_editor" || read_write) && (t.name != "code_exec" || code_exec))
        .cloned()
        .collect();

    // Ensure MAIN is always last for synthesis + execution
    let mut debate_perspectives = perspectives.clone();
    if let Some(pos) = debate_perspectives.iter().position(|r| r == "main") {
        let main_role = debate_perspectives.remove(pos);
        debate_perspectives.push(main_role);
    } else {
        // If no main role specified, add it at the end
        debate_perspectives.push("main".to_string());
    }

    // Debug: log the actual perspectives order being used
    let _ = ui.send(UiEvent::Info(format!(
        "[0box] Perspectives: {} (F3={} F4={})",
        debate_perspectives.join(" -> "),
        if read_write { "ON" } else { "OFF" },
        if code_exec { "ON" } else { "OFF" }
    )));

    // Conversation partagée entre tous les participants
    let mut conversation: Vec<ChatMessage> = Vec::new();

    // Prompt initial système pour le débat
    let mut debate_system = format!(
        "{}\n\n## RÈGLES DU DÉBAT 0BOX\n\nTu participes à un débat structuré entre plusieurs perspectives IA.\nChaque participant parle À SON TOUR, reçoit l'historique complet, et doit :\n1. RÉPONDRE aux arguments précédents (accord/désaccord/nuance)\n2. APPORTER sa perspective unique\n3. UTILISER les outils si nécessaire (web_search, knowledge_search, etc.)\n4. RESTER CONSTRUCTIF — l'objectif est une solution concrète finale.\n\nLe DERNIER participant (MAIN) doit SYNTHÉTISER et PROPOSER UN PLAN D'ACTION EXÉCUTABLE.\nSi des outils sont nécessaires pour valider (code, fichiers, recherche), UTILISE-LES.",
        system
    );
    // Liste explicite des outils : ce modèle ignore souvent le paramètre `tools` de l'API,
    // il faut les lui nommer dans le prompt (même pattern que start_turn).
    if !tools.is_empty() {
        debate_system.push_str(
            "\n\n## Outils disponibles (utilise-les pour AGIR — ne dis jamais « je ne peux pas exécuter » quand un outil le permet)\n",
        );
        for t in &tools {
            debate_system.push_str(&format!("- {} : {}\n", t.name, t.description));
        }
    }

    // Tour 1 : Premier participant (pas d'historique de débat)
    for (turn_idx, role) in debate_perspectives.iter().enumerate() {
        let is_last = turn_idx == debate_perspectives.len() - 1;
        let role_label = if is_last {
            "MAIN (synthèse + action)"
        } else {
            role
        };

        // Build prompt for this turn
        let prompt = if turn_idx == 0 {
            format!(
                "=== DÉBAT 0BOX — TOUR 1/{} ===\nSUJET : {}\nTA PERSPECTIVE : {}\n\nTu OUVRES le débat. Donne ton analyse initiale, tes arguments clés, et tes questions aux autres perspectives.\nSois concret, structuré, et utilise les outils si tu as besoin de données.",
                debate_perspectives.len(),
                topic,
                role
            )
        } else if is_last {
            // MAIN : synthèse + attribution des tâches + EXÉCUTION IMMÉDIATE
            let history = format_conversation(&conversation);
            format!(
                "=== DÉBAT 0BOX — TOUR {}/{} (MAIN - SYNTHÈSE + TÂCHES + EXÉCUTION) ===\nSUJET : {}\nHISTORIQUE DU DÉBAT :\n{}\n\n⚠️⚠️⚠️ INSTRUCTION CRITIQUE POUR MAIN ⚠️⚠️⚠️\n\nTu es MAIN. Ton rôle : SYNTHÉTISER, ATTRIBUER LES TÂCHES, puis EXÉCUTER.\n\nRÈGLE ABSOLUE : Tu DOIS appeler des outils DANS CETTE RÉPONSE.\nNE RÉPONDS PAS AVEC DU TEXTE SEULEMENT.\n\nPROCÉDURE OBLIGATOIRE (dans cet ordre) :\n0. VÉRIFIE D'ABORD avec task_list ce qui existe déjà : ne recrée JAMAIS une tâche existante — reprends le travail là où il en est (task_update claim → travail → done).\n1. CRÉE LES TÂCHES manquantes : appelle task_add UNE FOIS PAR étape du plan d'action.\n   Exemple : task_add text=\"Créer TodoList.java\" role=\"code\"\n   → chaque tâche reçoit un id (t1, t2, …) que les outils te renvoient.\n2. EXÉCUTE : appelle text_editor (create) pour CHAQUE fichier à créer,\n   code_exec pour CHAQUE commande à lancer.\n3. Marque l'avancement : task_update id=t1 status=claim au démarrage,\n   task_update id=t1 status=done quand c'est fini.\n4. Enchaîne les appels SANS t'arrêter jusqu'à ce que le plan soit réalisé.\n\nLes tâches restent visibles avec /task : l'utilisateur pourra les reprendre\navec /work si le budget d'appels d'outils est épuisé avant la fin.\n\nSi tu ne fais qu'écrire du texte sans appeler task_add, TU ÉCHOUES.\n\nCommence MAINTENANT : task_list d'abord, puis task_add pour les étapes manquantes.",
                turn_idx + 1,
                debate_perspectives.len(),
                topic,
                history
            )
        } else {
            // Participants intermédiaires : réagissent au débat
            let history = format_conversation(&conversation);
            format!(
                "=== DÉBAT 0BOX — TOUR {}/{} ===\nSUJET : {}\nTA PERSPECTIVE : {}\nHISTORIQUE DU DÉBAT :\n{}\n\nRÉPONDS MAINTENANT :\n- Quels arguments précèdents VALIDES/INVALIDES ?\n- Quelle perspective UNIQUE apportes-tu ?\n- Quelles QUESTIONS aux autres ?\n- Utilise les outils si tu as besoin de preuves/données.",
                turn_idx + 1,
                debate_perspectives.len(),
                topic,
                role,
                history
            )
        };

        // Endpoint pour ce rôle
        let endpoints = crate::route::chain_for_0box_role(&app_settings, role);

        // Annonce du tour
        let _ = ui.send(UiEvent::Info(format!(
            "🎭 [0box:{}] Tour {}/{} — parole à {}",
            role,
            turn_idx + 1,
            debate_perspectives.len(),
            role_label
        )));

        // Exécute le tour avec validation et retry si échec
        let (speaker, response) = run_0box_turn_with_retry(
            DebateTurn {
                role,
                role_label,
                prompt: &prompt,
                endpoints: &endpoints,
            },
            &debate_system,
            &tools,
            &options,
            &ui,
            3, // max retries per turn
        )
        .await;

        // Ajoute à la conversation partagée
        conversation.push(ChatMessage::assistant(
            format!("[{}]: {}", speaker, response),
            vec![],
        ));

        // Affiche dans l'UI
        let _ = ui.send(UiEvent::Info(format!("📝 [{}]:\n{}", speaker, response)));

        // Petite pause entre les tours
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }

    // Fin du débat
    let _ = ui.send(UiEvent::Info(
        "✅ [0box] Débat terminé — synthèse et plan d'action ci-dessus.".to_string(),
    ));
}

/// Formate la conversation pour l'injection dans le prompt
/// Exécute un tour du débat 0box avec retry automatique et validation de la réponse.
/// Retourne (speaker, response) garanti valide (non vide, non erreur).
async fn run_0box_turn_with_retry(
    turn: DebateTurn<'_>,
    debate_system: &str,
    tools: &[crate::tools::ToolSpec],
    options: &ChatOptions,
    ui: &UnboundedSender<UiEvent>,
    max_retries: u32,
) -> (String, String) {
    let DebateTurn {
        role,
        role_label,
        prompt,
        endpoints,
    } = turn;
    let mut last_error = String::new();

    for attempt in 0..=max_retries {
        if attempt > 0 {
            let _ = ui.send(UiEvent::Info(format!(
                "🔄 [0box:{}] Retry {}/{} — nouvelle tentative…",
                role, attempt, max_retries
            )));
        }

        // Channel pour événements du sous-agent
        let (evt_tx, mut evt_rx) =
            tokio::sync::mpsc::unbounded_channel::<crate::agent::subagent::SubAgentEvent>();

        // Lance le sous-agent
        let system_clone = debate_system.to_string();
        let tools_clone = tools.to_vec();
        let options_clone = options.clone();
        let endpoints_clone = endpoints.to_vec();
        let prompt_clone = prompt.to_string();
        let role_owned = role.to_string();

        // Actions du tour (accumulées pour l'historique du débat : sans ça, les
        // perspectives suivantes ne voient NI les tâches créées NI les fichiers
        // écrits — MAIN recréait en double des tâches déjà posées par « code »).
        let actions: std::sync::Arc<std::sync::Mutex<Vec<String>>> =
            std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));

        let subagent_handle = tokio::spawn(async move {
            // Budget élargi pour les tours de débat : MAIN doit vérifier + créer +
            // marquer un plan multi-étapes — le budget par défaut (8/24) ne laisse
            // même pas place aux `task_update` après vérification (constat run 3).
            // Toujours borné : jamais illimité (principe d'endiguement).
            let res = crate::agent::subagent::run_subagent_budgeted(
                endpoints_clone,
                system_clone,
                prompt_clone,
                tools_clone,
                options_clone,
                Some(evt_tx),
                20,
                64,
            )
            .await;
            (role_owned, res)
        });

        // Forward événements en temps réel
        let ui_evt = ui.clone();
        let role_label_owned = role_label.to_string();
        let acts_fwd = actions.clone();
        let forwarder = tokio::spawn(async move {
            while let Some(evt) = evt_rx.recv().await {
                match evt {
                    crate::agent::subagent::SubAgentEvent::Fallback { from, to, reason } => {
                        let _ = ui_evt.send(UiEvent::Info(format!(
                            "{} ⚡ Bascule: {} → {} ({})",
                            role_label_owned, from, to, reason
                        )));
                    }
                    crate::agent::subagent::SubAgentEvent::ToolCall { name, args } => {
                        // Résumé compact (jamais le contenu brut tronqué à 100 chars,
                        // qui donnait l'impression d'un flux coupé en plein file_text).
                        let short = summarize_tool_call(&name, &args);
                        if let Ok(mut a) = acts_fwd.lock() {
                            a.push(format!("{} {}", name, short));
                        }
                        let _ = ui_evt.send(UiEvent::Info(format!(
                            "{} ⚙ {} {}",
                            role_label_owned, name, short
                        )));
                    }
                    crate::agent::subagent::SubAgentEvent::ToolResult { name, result } => {
                        let first: String = result
                            .lines()
                            .find(|l| !l.trim().is_empty())
                            .unwrap_or("")
                            .chars()
                            .take(120)
                            .collect();
                        let _ = ui_evt.send(UiEvent::Info(format!(
                            "{} ✓ {} → {}",
                            role_label_owned, name, first
                        )));
                    }
                    crate::agent::subagent::SubAgentEvent::Error(e) => {
                        let _ = ui_evt.send(UiEvent::Info(format!("{} ⚠ {}", role_label_owned, e)));
                    }
                    crate::agent::subagent::SubAgentEvent::Info(info) => {
                        let _ =
                            ui_evt.send(UiEvent::Info(format!("{} ℹ {}", role_label_owned, info)));
                    }
                }
            }
        });

        // Attend la fin du tour ET le drainage des événements
        let (speaker, response) = match subagent_handle.await {
            Ok((s, r)) => (s, r),
            Err(e) => {
                last_error = format!("Join error: {e}");
                continue; // retry
            }
        };
        let _ = forwarder.await;

        // Réponse enrichie des actions du tour — les perspectives suivantes (et MAIN
        // en particulier) doivent voir les tâches créées et fichiers écrits, sinon
        // elles recréent en double ce qui existe déjà (constat runs 1–2).
        let mut full = response.clone();
        let n_actions = actions.lock().map(|a| a.len()).unwrap_or(0);
        if n_actions > 0 {
            if let Ok(acts) = actions.lock() {
                let shown: Vec<String> = acts.iter().take(20).cloned().collect();
                let more = acts.len().saturating_sub(20);
                let suffix = if more > 0 {
                    format!(" (+{more} autres)")
                } else {
                    String::new()
                };
                full = format!(
                    "{full}\n\n[Actions de ce tour : {}{suffix}]",
                    shown.join(" ; ")
                );
            }
        }

        // VALIDATION — sur la réponse ORIGINALE (les actions comptent comme travail).
        let response_trimmed = response.trim();
        if response_trimmed.is_empty() && n_actions == 0 {
            last_error = "Réponse vide".to_string();
            let _ = ui.send(UiEvent::Info(format!(
                "⚠ [0box:{}] Réponse vide, retry…",
                role
            )));
            continue;
        }
        if response_trimmed.starts_with("⚠") || response_trimmed.starts_with("Erreur") {
            // Arrêt d'endiguement (boucle détectée, budget dépassé) : ce n'est PAS un
            // échec modèle — le travail accompli est réel. Le retry redémarrerait le
            // tour ENTIER pour rien (15 min perdues, run 4). On garde le tour tel quel.
            if response_trimmed.contains("boucle détectée")
                || response_trimmed.contains("budget d'appels d'outils dépassé")
            {
                let _ = ui.send(UiEvent::Info(format!(
                    "🛑 [0box:{}] Tour arrêté par l'endiguement — travail conservé (pas de retry)",
                    role
                )));
                return (speaker, full);
            }
            last_error = format!("Réponse d'erreur: {response_trimmed}");
            let _ = ui.send(UiEvent::Info(format!(
                "⚠ [0box:{}] Erreur détectée, retry…",
                role
            )));
            continue;
        }
        if response_trimmed.chars().count() < 10 && n_actions == 0 {
            last_error = "Réponse trop courte".to_string();
            let _ = ui.send(UiEvent::Info(format!(
                "⚠ [0box:{}] Réponse trop courte (<10 chars), retry…",
                role
            )));
            continue;
        }

        // Succès : réponse valide
        return (speaker, full);
    }

    // Tous les retries échoués
    let error_msg = format!(
        "[{}]: ÉCHEC après {} tentatives — {}. Le débat continue avec ce constat.",
        role,
        max_retries + 1,
        last_error
    );
    let _ = ui.send(UiEvent::Info(format!("❌ [0box:{}] {}", role, error_msg)));
    (role.to_string(), error_msg)
}

fn format_conversation(conversation: &[ChatMessage]) -> String {
    if conversation.is_empty() {
        return "(début du débat)".to_string();
    }
    conversation
        .iter()
        .map(|m| {
            if let Some(content) = &m.content {
                // Extrait le speaker si format [speaker]: content
                if let Some(rest) = content.strip_prefix('[') {
                    if let Some(end) = rest.find(']') {
                        let speaker = &rest[..end];
                        let text = &rest[end + 2..];
                        return format!(
                            "  🎭 {}: {}",
                            speaker,
                            text.chars().take(800).collect::<String>()
                        );
                    }
                }
                format!("  💬 {}", content.chars().take(800).collect::<String>())
            } else {
                String::new()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn handle_0box_fallback(app: &mut App, args: &[&str], _ui_tx: &UnboundedSender<UiEvent>) {
    match args.first().copied() {
        Some("list") => {
            let g = crate::config::load_global();
            let fallbacks = g.zero_box_fallbacks.unwrap_or_default();
            if fallbacks.is_empty() {
                app.messages.push(ChatMessage::notice("[0box] Aucun modèle de fallback configuré. Utilise la chaîne de bascule standard (rôles + routes)."));
            } else {
                let mut s = String::from("[0box] Modèles de fallback configurés:\n");
                for fb in &fallbacks {
                    s.push_str(&format!("  - {}\n", fb));
                }
                s.push_str(
                    "\nCes modèles sont ajoutés EN PREMIER dans la chaîne pour les agents 0box.",
                );
                app.messages.push(ChatMessage::notice(s));
            }
        }
        Some("add") => {
            if args.len() < 2 {
                app.messages.push(ChatMessage::notice(
                    "usage: /0box fallback add <provider/model>",
                ));
                return;
            }
            let route = args[1];
            if crate::route::parse_route(route).is_none() {
                app.messages.push(ChatMessage::notice(format!(
                    "Route invalide: {}. Format: provider/model",
                    route
                )));
                return;
            }
            let mut g = crate::config::load_global();
            let fallbacks = g.zero_box_fallbacks.get_or_insert_with(Vec::new);
            if !fallbacks.contains(&route.to_string()) {
                fallbacks.push(route.to_string());
                if let Err(e) = crate::config::save_global(&g) {
                    app.messages
                        .push(ChatMessage::notice(format!("Erreur: {e}")));
                } else {
                    app.messages.push(ChatMessage::notice(format!(
                        "[0box] Fallback ajouté: {route}"
                    )));
                }
            } else {
                app.messages
                    .push(ChatMessage::notice(format!("[0box] Déjà présent: {route}")));
            }
        }
        Some("remove") => {
            if args.len() < 2 {
                app.messages.push(ChatMessage::notice(
                    "usage: /0box fallback remove <provider/model>",
                ));
                return;
            }
            let route = args[1];
            let mut g = crate::config::load_global();
            if let Some(fallbacks) = &mut g.zero_box_fallbacks {
                fallbacks.retain(|r| r != route);
                if let Err(e) = crate::config::save_global(&g) {
                    app.messages
                        .push(ChatMessage::notice(format!("Erreur: {e}")));
                } else {
                    app.messages.push(ChatMessage::notice(format!(
                        "[0box] Fallback supprimé: {route}"
                    )));
                }
            }
        }
        _ => {
            app.messages.push(ChatMessage::notice(
                "usage: /0box fallback <add|remove|list> <provider/model>",
            ));
        }
    }
}

fn show_0box_help(app: &mut App) {
    app.messages.push(ChatMessage::notice(
        String::from("[0box] Brainstorm multi-perspectives\n\n")
            + "Usage:\n"
            + "  /0box <sujet> [--perspectives main,code,reasoning]  Lance un brainstorm\n"
            + "  /0box fallback list                                  Liste les fallbacks 0box\n"
            + "  /0box fallback add <provider/model>                  Ajoute un fallback 0box\n"
            + "  /0box fallback remove <provider/model>               Retire un fallback 0box\n"
            + "  /0box help                                           Cette aide\n\n"
            + "Exemples:\n"
            + "  /0box comment optimiser mon build Rust --perspectives code,reasoning,fast\n"
            + "  /0box fallback add openrouter/qwen/qwen-2.5-72b:free\n"
            + "  /0box fallback add google/gemini-1.5-flash\n"
            + "\n"
            + "Le système utilise la chaîne de bascule standard (rôle → routes globales).\n"
            + "Les fallbacks 0box s'ajoutent EN TÊTE de chaîne pour les agents 0box uniquement.",
    ));
}

/// `/apis [besoin]` : catalogue d'APIs publiques (source community, datée).
fn handle_apis(app: &mut App, args: &[&str]) {
    // `/apis <besoin> [keyless]` ou `/apis` (stats).
    let mut words: Vec<&str> = args.to_vec();
    let keyless = words.iter().any(|w| w.eq_ignore_ascii_case("keyless"));
    words.retain(|w| !w.eq_ignore_ascii_case("keyless"));
    let q = words.join(" ");
    let res = if q.trim().is_empty() {
        crate::commands::apis_stats()
    } else {
        crate::commands::apis_search(&q, crate::commands::APIS_DEFAULT_K, keyless)
    };
    match res {
        Ok(lines) => app.messages.push(ChatMessage::notice(lines.join("\n"))),
        Err(e) => app.messages.push(ChatMessage::notice(format!("{e}"))),
    }
}
fn handle_kb(app: &mut App, args: &[&str], ui_tx: &UnboundedSender<UiEvent>) {
    match args.first().copied() {
        None | Some("stats") => match crate::commands::kb_stats() {
            Ok(lines) => app.messages.push(ChatMessage::notice(lines.join("\n"))),
            Err(e) => app
                .messages
                .push(ChatMessage::notice(format!("Erreur: {e}"))),
        },
        Some("search") => {
            let q = args.get(1..).map(|s| s.join(" ")).unwrap_or_default();
            let ui = ui_tx.clone();
            tokio::spawn(async move {
                let msg = match crate::commands::kb_search(&q, 5).await {
                    Ok(lines) => lines.join("\n"),
                    Err(e) => format!("{e}"),
                };
                let _ = ui.send(UiEvent::Info(msg));
            });
        }
        Some("ingest") => {
            // `--embed` est reconnu ici aussi (le CLI l'avait, la TUI l'ignorait).
            let embed = args.contains(&"--embed");
            let dir = args
                .get(1)
                .filter(|a| **a != "--embed")
                .map(std::path::PathBuf::from);
            let ui = ui_tx.clone();
            tokio::spawn(async move {
                let msg = match crate::commands::kb_ingest(dir, embed).await {
                    Ok(lines) => lines.join("\n"),
                    Err(e) => format!("Erreur ingest : {e}"),
                };
                let _ = ui.send(UiEvent::Info(msg));
            });
        }
        Some("log") => {
            let n = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(15);
            app.messages
                .push(ChatMessage::notice(crate::commands::kb_log(n).join("\n")));
        }
        // État de session (pas de config) : TUI uniquement.
        Some("auto") => {
            app.kb_auto = !app.kb_auto;
            let state = if app.kb_auto { "ON" } else { "OFF" };
            app.messages.push(ChatMessage::notice(format!(
                "Injection connaissance : {state}"
            )));
        }
        Some("rebuild") => match crate::commands::kb_rebuild() {
            Ok(lines) => app.messages.push(ChatMessage::notice(lines.join("\n"))),
            Err(e) => app
                .messages
                .push(ChatMessage::notice(format!("Erreur: {e}"))),
        },
        _ => app.messages.push(ChatMessage::notice(
            "usage: /kb [stats|search <q>|ingest [dir] [--embed]|auto|log [n]|rebuild]",
        )),
    }
}
/// `/embeddings [show|set <provider> <model>|unset]` — embedder de la mémoire.
/// Un modèle d'embedding ne sert QUE pour `/embeddings` (pas de chat).
fn handle_embeddings(app: &mut App, args: &[&str]) {
    let res = match args.first().copied() {
        None | Some("show") => Ok(crate::commands::embeddings_show()),
        Some("set") => crate::commands::embeddings_set(
            args.get(1).copied().unwrap_or(""),
            args.get(2).copied().unwrap_or(""),
        ),
        Some("unset") | Some("clear") | Some("off") => crate::commands::embeddings_unset(),
        _ => Err(anyhow::anyhow!(
            "usage: /embeddings [show|set <provider> <model>|unset]"
        )),
    };
    match res {
        Ok(lines) => app.messages.push(ChatMessage::notice(lines.join("\n"))),
        Err(e) => app.messages.push(ChatMessage::notice(format!("{e}"))),
    }
}
fn handle_memory(app: &mut App, args: &[&str]) {
    // Même source que `zer0 memory list|stats`.
    let lines = match args.first().copied() {
        Some("stats") => crate::commands::memory_stats(),
        Some("list") | None => crate::commands::memory_list(20),
        Some(other) => vec![format!("usage: /memory [list|stats] (reçu : `{other}`)")],
    };
    app.messages.push(ChatMessage::notice(lines.join("\n")));
}
fn handle_forget(app: &mut App, args: &[&str]) {
    match crate::commands::memory_forget(args.first().copied().unwrap_or("")) {
        Ok(lines) => {
            for line in lines {
                app.messages.push(ChatMessage::notice(line));
            }
        }
        Err(e) => app.messages.push(ChatMessage::notice(format!("{e}"))),
    }
}
fn cmd_remember(args: &[&str], ui_tx: &UnboundedSender<UiEvent>) {
    let text = args.join(" ");
    let ui = ui_tx.clone();
    tokio::spawn(async move {
        // Même chemin que `zer0 memory save`.
        let msg = match crate::commands::memory_save(&text, None).await {
            Ok(lines) => lines.join("\n"),
            Err(e) => format!("Erreur mémoire: {e}"),
        };
        let _ = ui.send(UiEvent::Info(msg));
    });
}
fn cmd_recall(args: &[&str], ui_tx: &UnboundedSender<UiEvent>) {
    let q = args.join(" ");
    let ui = ui_tx.clone();
    tokio::spawn(async move {
        // Même chemin que `zer0 memory search`.
        let msg = match crate::commands::memory_search(&q, 5).await {
            Ok(lines) => lines.join("\n"),
            Err(e) => format!("Erreur mémoire: {e}"),
        };
        let _ = ui.send(UiEvent::Info(msg));
    });
}
fn handle_project(app: &mut App) {
    let p = config::load_project();
    let mut s = format!("Projet: {}\n", config::project_path().display());
    s.push_str(&format!(
        "  provider : {}\n",
        p.provider.unwrap_or_else(|| "(global)".into())
    ));
    s.push_str(&format!(
        "  model    : {}\n",
        p.model.unwrap_or_else(|| "(global)".into())
    ));
    s.push_str(&format!(
        "  routes   : {}\n",
        if p.routes.is_empty() {
            "(aucune)".to_string()
        } else {
            p.routes.join(", ")
        }
    ));
    if !p.roles.is_empty() {
        let roles = p
            .roles
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join(", ");
        s.push_str(&format!("  roles    : {roles}\n"));
    }
    if p.system_prompt.is_some() || p.prompt_file.is_some() {
        s.push_str("  prompt   : override projet\n");
    }
    app.messages.push(ChatMessage::notice(s));
}

fn handle_route(app: &mut App, args: &[&str], ui_tx: &UnboundedSender<UiEvent>) {
    match args.first().copied() {
        None | Some("list") => {
            let mut lines = crate::commands::route_lines(&app.settings());
            lines.push(String::new());
            lines.push(
                "route add <provider>/<model>  ·  route auto [provider] [n]  ·  route clear"
                    .to_string(),
            );
            app.messages.push(ChatMessage::notice(lines.join("\n")));
        }
        Some("add") => {
            let r = args.get(1).copied().unwrap_or("");
            match crate::commands::route_add(r) {
                Ok(lines) => {
                    for line in lines {
                        app.messages.push(ChatMessage::notice(line));
                    }
                }
                Err(e) => app
                    .messages
                    .push(ChatMessage::notice(format!("Erreur: {e}"))),
            }
        }
        Some("clear") => match crate::commands::route_clear() {
            Ok(lines) => {
                for line in lines {
                    app.messages.push(ChatMessage::notice(line));
                }
            }
            Err(e) => app
                .messages
                .push(ChatMessage::notice(format!("Erreur: {e}"))),
        },
        Some("auto") => {
            let (provider, n) = crate::commands::parse_auto_args(&args[1.min(args.len())..]);
            let ui = ui_tx.clone();
            tokio::spawn(async move {
                let msg = match crate::commands::route_auto(&provider, n).await {
                    Ok(lines) => lines.join("\n"),
                    Err(e) => format!("Erreur auto: {e}"),
                };
                let _ = ui.send(UiEvent::Info(msg));
            });
        }
        _ => app.messages.push(ChatMessage::notice(
            "usage: /route [list|add <provider>/<model>|auto [provider] [n]|clear]",
        )),
    }
}
/// `/quota` : usage observé + solde de la clé quand le provider le publie
/// (OpenRouter `/auth/key` ; Cline ne publie rien). Le fetch live est async.
fn cmd_quota(ui_tx: &UnboundedSender<UiEvent>, provider: Option<String>) {
    let ui = ui_tx.clone();
    tokio::spawn(async move {
        // Même rapport que `zer0 quota` (y compris l'absence de solde publié).
        let msg = match crate::commands::quota_report(provider.as_deref()).await {
            Ok(lines) => lines.join("\n"),
            Err(e) => format!("Erreur quota : {e}"),
        };
        let _ = ui.send(UiEvent::Info(msg));
    });
}
/// Clic sur 👍 / 👎 : un upvote = une star GitHub (via l'API si un jeton est
/// présent, sinon la page du dépôt s'ouvre dans le navigateur).
fn vote_click(ui_tx: &UnboundedSender<UiEvent>, action: crate::vote::Action) {
    let ui = ui_tx.clone();
    tokio::spawn(async move {
        let msg = crate::vote::act(action).await;
        let _ = ui.send(UiEvent::Info(msg));
        // Le navigateur a pu rendre le mode « souris » à la console : on le
        // réarme, sinon plus aucun clic n'atteindrait le TUI (menu compris).
        crate::platform::restore_mouse_capture();
        // Rafraîchit le compteur et l'état du compte après le vote.
        let (stars, starred) = crate::vote::state().await;
        let _ = ui.send(UiEvent::VoteState { stars, starred });
    });
}

/// `/vote` : état des lieux (compteur, star du compte, URL).
fn cmd_vote(ui_tx: &UnboundedSender<UiEvent>) {
    let ui = ui_tx.clone();
    tokio::spawn(async move {
        let mut s = crate::vote::status().await;
        s.push_str("\\n\\n👍 = star · 👎 = retrait de la star (clic ou /vote up|down)");
        let _ = ui.send(UiEvent::Info(s));
    });
}

/// `/vote login` : connexion GitHub par device flow (une seule fois).
///
/// Le jeton est rangé **chiffré** dans le coffre. Deux précautions :
/// - la passphrase du coffre doit être connue (`ZER0_VAULT_PASSPHRASE`) : on ne
///   peut pas lire le clavier depuis la TUI (mode raw) ;
/// - si l'écriture échoue, le jeton **n'est pas perdu** : on le propose dans le
///   presse-papier et on dit comment le stocker, plutôt que de demander à
///   l'utilisateur de recommencer la connexion.
fn cmd_vote_login(ui_tx: &UnboundedSender<UiEvent>) {
    let ui = ui_tx.clone();
    tokio::spawn(async move {
        // Sans passphrase connue, on ne peut pas ecrire le coffre chiffre
        // (et on ne peut pas lire le clavier depuis la TUI : mode raw).
        let pass_env = std::env::var("ZER0_VAULT_PASSPHRASE").unwrap_or_default();
        if pass_env.is_empty() && !crate::vault::is_unlocked() {
            let _ = ui.send(UiEvent::Info(format!(
                "Pas de coffre disponible : la connexion GitHub ne peut pas etre stockee.\n\nLance `{cmd} vault setup` puis `{cmd} vote login` dans un terminal normal :\nle jeton sera range chiffre, et les votes deviendront automatiques.",
                cmd = crate::vote::self_cmd(),
            )));
            return;
        }

        let res = crate::vote::device_login(|code, url| {
            let _ = ui.send(UiEvent::Info(format!(
                "Connexion GitHub : ouvre {url} et saisis le code {code} - puis valide."
            )));
        })
        .await;

        match res {
            Ok(creds) => match crate::vault::put("github", &creds.access) {
                Ok(()) => {
                    // App à jetons expirables : on conserve aussi le refresh
                    // token (chiffré) → le vote reste automatique après 8 h.
                    // **Opt-in** : sans refresh token, rien ne change.
                    let mut msg = String::from(
                        "Jeton GitHub enregistre (chiffre dans le coffre) : le vote est desormais automatique (pouce haut = une star).",
                    );
                    if let Some(rt) = &creds.refresh {
                        if crate::vault::put("github_refresh", rt).is_ok() {
                            msg.push_str("\nRenouvellement automatique activé (jeton expirable).");
                        }
                    }
                    let _ = ui.send(UiEvent::Info(msg));
                }
                Err(e) => {
                    // Le jeton est bon mais non stocke : on ne le perd pas.
                    crate::clipboard::copy(&creds.access);
                    let _ = ui.send(UiEvent::Info(format!(
                        "Jeton obtenu mais non stocke dans le coffre : {e}\n\nIl est copie dans le presse-papier : colle-le dans\n\
~/.config/zer0/config.toml  →  [keys]  github = \"…\"\n\
(ou relance `{cmd} vote login` dans un terminal normal).",
                        cmd = crate::vote::self_cmd(),
                    )));
                }
            },
            Err(e) => {
                let _ = ui.send(UiEvent::Info(format!("Connexion GitHub : {e}")));
            }
        }
    });
}

fn cmd_list_models(app: &App, ui_tx: &UnboundedSender<UiEvent>, free_only: bool) {
    let Some(provider) = providers::find(&app.provider_id) else {
        return;
    };
    let key = app.api_key.clone();
    let ui = ui_tx.clone();
    tokio::spawn(async move {
        match models::list(&http_client::HTTP_CLIENT, &provider, key.as_deref()).await {
            Ok(mut list) => {
                if free_only {
                    list.retain(models::is_free);
                }
                let mut s = String::new();
                s.push_str(&format!(
                    "{} modèle(s){} chez {}\n",
                    list.len(),
                    if free_only { " :free" } else { "" },
                    provider.name
                ));
                for m in list.iter().take(80) {
                    // Usage observé cette session (429, appels ok) — rien d'observé ⇒ rien d'affiché.
                    let badge = crate::quota::badge(&format!("{}/{}", provider.id, m.id))
                        .unwrap_or_default();
                    s.push_str(&format!(
                        "  {}{}{}\n",
                        m.id,
                        badge,
                        m.context_length
                            .map(|c| format!("  (ctx {c})"))
                            .unwrap_or_default()
                    ));
                }
                if list.is_empty() {
                    s = format!(
                        "Aucun modèle pour {}. Clé API requise ? (/provider add {})",
                        provider.name, provider.id
                    );
                }
                let _ = ui.send(UiEvent::Info(s));
            }
            Err(e) => {
                let _ = ui.send(UiEvent::Info(format!(
                    "Erreur modèles {}: {e}",
                    provider.name
                )));
            }
        }
    });
}

fn cmd_select(problem: &str, ui_tx: &UnboundedSender<UiEvent>) {
    let problem = problem.to_string();
    let ui = ui_tx.clone();
    tokio::spawn(async move {
        // Même implémentation que `zer0 select …` (top 8, même avertissement).
        let msg = match crate::commands::select_models(&problem, 8).await {
            Ok(lines) => lines.join("\n"),
            Err(e) => format!("Erreur: {e}"),
        };
        let _ = ui.send(UiEvent::Info(msg));
    });
}
/// Liste les fichiers de conversation (active + archives), plus récents d'abord.
fn scan_chats() -> Vec<std::path::PathBuf> {
    let dir = config::data_dir();
    let mut files = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.is_file() {
                let name = path.file_name().unwrap_or_default().to_string_lossy();
                if name.starts_with("conversation") && name.ends_with(".jsonl") {
                    files.push(path);
                }
            }
        }
    }
    files.sort_by(|a, b| {
        let ma = a.metadata().and_then(|m| m.modified()).ok();
        let mb = b.metadata().and_then(|m| m.modified()).ok();
        mb.cmp(&ma)
    });
    files
}

/// Touches du pop-up Chats. Retourne `true` s'il faut le fermer.
fn handle_chats_key(app: &mut App, code: KeyCode) -> bool {
    let len = app.chats_popup.as_ref().map(|(f, _)| f.len()).unwrap_or(0);
    match code {
        KeyCode::Up | KeyCode::Char('k') => {
            if let Some((_, s)) = app.chats_popup.as_mut() {
                *s = s.saturating_sub(1);
            }
            false
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if let Some((_, s)) = app.chats_popup.as_mut() {
                *s = (*s + 1).min(len.saturating_sub(1));
            }
            false
        }
        KeyCode::Esc | KeyCode::Char('q') | KeyCode::F(6) => true,
        KeyCode::Enter => {
            let pick = app
                .chats_popup
                .as_ref()
                .and_then(|(f, s)| f.get(*s).cloned());
            if let Some(path) = pick {
                load_chat_session(app, &path);
            }
            true
        }
        _ => false,
    }
}

/// Charge une session depuis un fichier et la rend active (archive l'actuelle).
fn load_chat_session(app: &mut App, path: &std::path::Path) {
    let active = crate::convo::path();
    if path != active.as_path() {
        if active.exists() {
            let current = crate::convo::load(0);
            if !current.is_empty() {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                let archive = active.with_file_name(format!("conversation_{now}.jsonl"));
                let _ = std::fs::rename(&active, &archive);
            }
        }
        let _ = std::fs::copy(path, &active);
    }
    // Réparation : une session archivée peut avoir été coupée au milieu d'un
    // échange d'outils ; la renvoyer telle quelle produit un 400 chez le provider.
    let loaded = crate::convo::repair(crate::convo::load_from_path(&active, 0));
    let conv: Vec<ChatMessage> = loaded
        .iter()
        .filter(|m| m.role != "notice")
        .cloned()
        .collect();
    app.conversation = conv;
    app.messages = loaded;
    app.display_mark = app.messages.len();
    app.scroll = 0;
    app.auto_scroll = true;
    let name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    app.messages
        .push(ChatMessage::notice(format!("📝 session chargée : {name}")));
}

fn handle_input(ev: Event, app: &mut App, ui_tx: &UnboundedSender<UiEvent>) {
    // ========== FILTRE RELEASE (Windows/ConPTY : chaque touche = Press + Release) ==========
    // Sans ce filtre, le sélecteur/palette traite le Release comme une 2e frappe
    // (le filtre Press du dispatch principal ne couvre pas ce chemin) → « 0 » devient « 00 ».
    if let Event::Key(k) = &ev {
        if k.kind == KeyEventKind::Release {
            return;
        }
    }
    // ========================================================================================

    // ========== DÉDUPLICATION GLOBALE (doublons de Press, ex. ConPTY) ==========
    if let Event::Key(k) = &ev {
        // On ne déduplique que les Press (Repeat = auto-repeat légitime)
        if k.kind == KeyEventKind::Press {
            let now = std::time::Instant::now();
            let code = k.code;
            let mods = k.modifiers;

            // Vérifier si c'est un doublon (même touche + mêmes modifs < 50ms)
            if let Some((last_code, last_mods, last_time)) = app.last_key_event {
                if last_code == code
                    && last_mods == mods
                    && now.duration_since(last_time).as_millis() < 50
                {
                    return; // Ignorer le doublon
                }
            }
            app.last_key_event = Some((code, mods, now));
        }
    }
    // ================================================================================

    // Assistant ouvert : il capte tout (clavier + collage), y compris le collage
    // des clés API (le presse-papier est indispensable pour une clé).
    if app.wizard.is_some() {
        match ev {
            Event::Key(k) => wizard::handle_key(k, app, ui_tx),
            Event::Paste(text)
                if app
                    .wizard
                    .as_ref()
                    .map(|w| w.step.needs_text())
                    .unwrap_or(false) =>
            {
                input_insert_str(app, &text);
            }
            _ => {}
        }
        return;
    }

    // Sélecteur ouvert : il capte tout le clavier.
    if let Event::Key(key) = ev {
        if app.picker.is_some() {
            handle_picker_key(key, app, ui_tx);
            return;
        }
    }
    // Collage (bracketed paste) : insérer tel quel, sans déclencher d'envoi.
    if let Event::Paste(s) = &ev {
        input_insert_str(app, s);
        return;
    }
    // Clics souris sur les boutons 0chan / micro.
    if let Event::Mouse(me) = ev {
        use crossterm::event::{MouseButton, MouseEventKind};
        let inside = |r: Option<Rect>| {
            r.map(|r| {
                me.column >= r.x
                    && me.column < r.x + r.width
                    && me.row >= r.y
                    && me.row < r.y + r.height
            })
            .unwrap_or(false)
        };
        match me.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                // Le menu passe EN PREMIER : ses zones sont des lignes entières
                // et ne croisent pas les pouces (bas-droite). Le vote est
                // ensuite vérifié, avant toute sélection de texte.
                if let Some((idx, code, is_group)) = app
                    .menu_hits
                    .iter()
                    .enumerate()
                    .find(|(_, (r, _))| {
                        me.column >= r.x
                            && me.column < r.x + r.width
                            && me.row >= r.y
                            && me.row < r.y + r.height
                    })
                    .map(|(i, (_, h))| match h {
                        menu::Hit::Group(l) => (i, *l, true),
                        menu::Hit::Cmd(c) => (i, *c, false),
                    })
                {
                    app.menu_focus = true;
                    app.menu_sel = idx;
                    if is_group {
                        app.menu_open = if app.menu_open == Some(code) {
                            None
                        } else {
                            Some(code)
                        };
                    } else if code.ends_with(' ') {
                        input_set(app, code.to_string());
                        app.menu_focus = false;
                    } else {
                        handle_command(app, code, ui_tx);
                        // Après une commande, on rend la main à la saisie.
                        app.menu_focus = false;
                    }
                } else if inside(app.btn_vote_up) {
                    vote_click(ui_tx, crate::vote::Action::Up);
                } else if inside(app.btn_vote_down) {
                    vote_click(ui_tx, crate::vote::Action::Down);
                } else if inside(app.btn_0chan) {
                    app.zc.toggle_on();
                } else if inside(app.btn_mic) {
                    toggle_mic(app, ui_tx);
                } else {
                    // Clic hors menu : on retire le focus du menu.
                    app.menu_focus = false;
                    app.sel_anchor = Some((me.column, me.row));
                    app.sel_head = Some((me.column, me.row));
                }
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                if app.sel_anchor.is_some() {
                    app.sel_head = Some((me.column, me.row));
                }
            }
            MouseEventKind::Up(MouseButton::Left) => {
                if app.sel_anchor.is_some() {
                    copy_selection(app);
                }
            }
            MouseEventKind::ScrollDown => {
                if let Some(p) = app.picker.as_mut() {
                    p.move_sel(1);
                } else if inside(Some(app.menu_rect)) {
                    app.menu_scroll = app.menu_scroll.saturating_add(3);
                } else {
                    let (s, auto) = wheel_scroll(app.scroll, app.max_scroll, false);
                    app.scroll = s;
                    app.auto_scroll = auto;
                }
            }
            MouseEventKind::ScrollUp => {
                if let Some(p) = app.picker.as_mut() {
                    p.move_sel(-1);
                } else if inside(Some(app.menu_rect)) {
                    app.menu_scroll = app.menu_scroll.saturating_sub(3);
                } else {
                    let (s, auto) = wheel_scroll(app.scroll, app.max_scroll, true);
                    app.scroll = s;
                    app.auto_scroll = auto;
                }
            }
            // Clic droit sur une sortie d'outil : déplier/replier CE bloc.
            MouseEventKind::Down(MouseButton::Right) => {
                let r = app.chat_rect;
                if me.column >= r.x
                    && me.column < r.x + r.width
                    && me.row >= r.y
                    && me.row < r.y + r.height
                {
                    let line = app.chat_scroll.saturating_add(me.row.saturating_sub(r.y));
                    if let Some(idx) = block_at(&app.block_ranges, line) {
                        app.messages[idx].expanded = !app.messages[idx].expanded;
                    }
                }
            }
            _ => {}
        }
        return;
    }
    if let Event::Key(k) = ev {
        if k.kind != KeyEventKind::Press {
            return;
        }
        // Ctrl+C quitte TOUJOURS (même menu/panneau/pop-up ouverts).
        if k.code == KeyCode::Char('c') && k.modifiers.contains(KeyModifiers::CONTROL) {
            app.should_quit = true;
            return;
        }
        // Fun modes take over input.
        if app.mode != crate::fun::Mode::Chat {
            match app.mode {
                crate::fun::Mode::Radio => match k.code {
                    KeyCode::Up | KeyCode::Char('k') => app.radio.prev(),
                    KeyCode::Down | KeyCode::Char('j') => app.radio.next(),
                    KeyCode::Enter => app.radio.play_selected(),
                    KeyCode::Char('n') => app.radio.next_play(),
                    KeyCode::Char('p') => app.radio.prev_play(),
                    KeyCode::Char('s') => app.radio.stop(),
                    KeyCode::Esc | KeyCode::Char('q') => {
                        app.radio.stop();
                        app.mode = crate::fun::Mode::Chat;
                    }
                    _ => {}
                },
                crate::fun::Mode::BinTime => {
                    if matches!(k.code, KeyCode::Esc | KeyCode::Char('q')) {
                        app.mode = crate::fun::Mode::Chat;
                    }
                }
                crate::fun::Mode::Pong => match k.code {
                    KeyCode::Esc | KeyCode::Char('q') => app.mode = crate::fun::Mode::Chat,
                    KeyCode::Char('r') => app.pong.restart(),
                    KeyCode::Up | KeyCode::Char('w') => app.pong.set_dir(-1),
                    KeyCode::Down | KeyCode::Char('s') => app.pong.set_dir(1),
                    _ => {}
                },
                crate::fun::Mode::Chat => {}
            }
            return;
        }
        // Pop-up « Chats » (F6) : capture les touches tant qu'il est ouvert.
        if app.chats_popup.is_some() {
            if handle_chats_key(app, k.code) {
                app.chats_popup = None;
            }
            return;
        }
        // Menu « Commandes » au clavier (focus via F2).
        // Les touches gérées (↑↓ Entrée Esc) restent dans le menu ; toute autre
        // touche RELÂCHE le focus et retombe dans la saisie normale (pas de blocage).
        if app.menu_focus {
            match k.code {
                KeyCode::Up => {
                    app.menu_sel = app.menu_sel.saturating_sub(1);
                    return;
                }
                KeyCode::Down => {
                    if !app.menu_hits.is_empty() {
                        app.menu_sel = (app.menu_sel + 1).min(app.menu_hits.len() - 1);
                    }
                    return;
                }
                KeyCode::Enter => {
                    let hit = app.menu_hits.get(app.menu_sel).map(|(_, h)| *h);
                    if let Some(hit) = hit {
                        match hit {
                            menu::Hit::Group(label) => {
                                app.menu_open = if app.menu_open == Some(label) {
                                    None
                                } else {
                                    Some(label)
                                };
                            }
                            menu::Hit::Cmd(code) => {
                                if code.ends_with(' ') {
                                    input_set(app, code.to_string());
                                } else {
                                    handle_command(app, code, ui_tx);
                                }
                            }
                        }
                    }
                    app.menu_focus = false;
                    return;
                }
                KeyCode::Esc | KeyCode::F(2) => {
                    app.menu_focus = false;
                    return;
                }
                // Toute autre touche : on sort du menu et on continue dessous.
                _ => app.menu_focus = false,
            }
        }
        // Side panel focus (right pane). On ne consomme QUE ses touches : toute
        // autre touche (F3–F10, etc.) relâche le focus et retombe dans la saisie.
        if app.side_mode.is_some() && app.focus_panel {
            let is_radio = app.side_mode == Some(crate::fun::Mode::Radio);
            match k.code {
                KeyCode::Tab => {
                    app.focus_panel = false;
                    return;
                }
                KeyCode::Esc | KeyCode::Char('q') => {
                    app.radio.stop();
                    app.side_mode = None;
                    app.focus_panel = false;
                    return;
                }
                KeyCode::Up | KeyCode::Char('k') if is_radio => {
                    app.radio.prev();
                    return;
                }
                KeyCode::Down | KeyCode::Char('j') if is_radio => {
                    app.radio.next();
                    return;
                }
                KeyCode::Enter if is_radio => {
                    app.radio.play_selected();
                    return;
                }
                KeyCode::Char('n') if is_radio => {
                    app.radio.next_play();
                    return;
                }
                KeyCode::Char('p') if is_radio => {
                    app.radio.prev_play();
                    return;
                }
                KeyCode::Char('s') if is_radio => {
                    app.radio.stop();
                    return;
                }
                // Touche non gérée : on quitte le focus et on continue plus bas.
                _ => app.focus_panel = false,
            }
        }
        // Tab moves focus to the side panel (when one is open).
        if app.side_mode.is_some() && k.code == KeyCode::Tab {
            app.focus_panel = true;
            return;
        }
        // Toute frappe efface la sélection souris.
        app.sel_anchor = None;
        app.sel_head = None;
        match k.code {
            // Copier le dernier message assistant (Ctrl+Shift+C)
            KeyCode::Char('c')
                if k.modifiers
                    .contains(KeyModifiers::CONTROL | KeyModifiers::SHIFT) =>
            {
                copy_last_assistant(app);
            }
            // Coller depuis le presse-papier (Ctrl+V)
            KeyCode::Char('v')
                if k.modifiers.contains(KeyModifiers::CONTROL)
                    && !k.modifiers.contains(KeyModifiers::SHIFT) =>
            {
                match crate::clipboard::paste() {
                    Some(text) if !text.trim().is_empty() => {
                        input_insert_str(app, &text);
                    }
                    Some(_) => {
                        app.messages
                            .push(ChatMessage::notice("⚠ presse-papier vide"));
                    }
                    None => {
                        app.messages.push(ChatMessage::notice(
                            "⚠ presse-papier inaccessible (PowerShell requis sur Windows)",
                        ));
                    }
                }
            }
            // Coller depuis le presse-papier (Ctrl+Shift+V) - alias
            KeyCode::Char('v')
                if k.modifiers
                    .contains(KeyModifiers::CONTROL | KeyModifiers::SHIFT) =>
            {
                match crate::clipboard::paste() {
                    Some(text) if !text.trim().is_empty() => {
                        input_insert_str(app, &text);
                    }
                    Some(_) => {
                        app.messages
                            .push(ChatMessage::notice("⚠ presse-papier vide"));
                    }
                    None => {
                        app.messages.push(ChatMessage::notice(
                            "⚠ presse-papier inaccessible (PowerShell requis sur Windows — essayez Ctrl+V du terminal)"
                        ));
                    }
                }
            }
            // Ctrl+C simple = quitter (comportement actuel)
            KeyCode::Char('c') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                app.should_quit = true
            }
            KeyCode::Esc => app.should_quit = true,
            KeyCode::Enter => send_message(app, ui_tx),
            KeyCode::Left => app.cursor = app.cursor.saturating_sub(1),
            KeyCode::Right => {
                app.cursor = (app.cursor + 1).min(app.input.chars().count());
            }
            KeyCode::Delete => input_delete(app),
            KeyCode::Backspace => input_backspace(app),
            KeyCode::Char('o') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                toggle_last_tool_block(app);
            }
            KeyCode::Char('a') if k.modifiers.contains(KeyModifiers::CONTROL) => app.cursor = 0,
            KeyCode::Char('e') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                app.cursor = app.input.chars().count();
            }
            KeyCode::Char(ch) => {
                // `/` en début de saisie ouvre la palette de commandes filtrante.
                if ch == '/' && app.picker.is_none() && app.input.trim().is_empty() {
                    open_command_palette(app);
                } else {
                    input_insert(app, ch);
                }
            }
            KeyCode::F(2) => {
                app.menu_focus = true;
                app.menu_sel = app.menu_sel.min(app.menu_hits.len().saturating_sub(1));
            }
            KeyCode::F(3) => {
                app.read_write = !app.read_write;
                app.refresh_runtime();
                let state = if app.read_write { "ON" } else { "OFF" };
                app.messages
                    .push(ChatMessage::notice(format!("Read&Write: {state}")));
            }
            KeyCode::F(4) => {
                app.code_exec = !app.code_exec;
                app.refresh_runtime();
                let state = if app.code_exec { "ON" } else { "OFF" };
                app.messages
                    .push(ChatMessage::notice(format!("Code-exec: {state}")));
                if app.code_exec {
                    app.messages.push(ChatMessage::notice(
                        "⚠️ shell activé : les commandes s'exécutent SANS validation — tu es \
                         responsable des erreurs (ex. `rm -rf` sur tes photos).",
                    ));
                }
            }
            KeyCode::F(5) => handle_command(app, "/help", ui_tx),
            KeyCode::F(8) => app.zc.toggle_on(),
            KeyCode::F(10) => toggle_mic(app, ui_tx),
            KeyCode::F(6) => app.chats_popup = Some((scan_chats(), 0)),
            KeyCode::F(7) => nudge(app, ui_tx),
            KeyCode::Up => history_up(app),
            KeyCode::Down => history_down(app),
            KeyCode::PageUp => {
                app.auto_scroll = false;
                app.scroll = app.scroll.saturating_sub(10);
            }
            KeyCode::PageDown => {
                app.scroll = (app.scroll + 10).min(app.max_scroll);
                if app.scroll >= app.max_scroll {
                    app.auto_scroll = true;
                }
            }
            KeyCode::Home => {
                app.auto_scroll = false;
                app.scroll = 0;
            }
            KeyCode::End => {
                app.auto_scroll = true;
            }
            _ => {}
        }
    }
}

fn handle_ui(ev: UiEvent, app: &mut App) {
    match ev {
        UiEvent::Token(t) => {
            if app.zc.on {
                app.zc.state = crate::zerochan::State::Speaking;
            }
            // Defensive cap; the client already bounds the stream.
            if app.streaming_text.len() < 1_200_000 {
                app.streaming_text.push_str(&t);
            }
        }
        UiEvent::Reasoning(t) => {
            if app.reasoning_text.len() < 400_000 {
                app.reasoning_text.push_str(&t);
            }
        }
        UiEvent::WizardModels(list) => {
            wizard::on_models(app, list);
        }
        UiEvent::FreeModels(list) => {
            let items: Vec<picker::Item> = list
                .into_iter()
                .map(|m| picker::Item {
                    id: m.id.clone(),
                    label: m.id,
                    note: m
                        .context_length
                        .map(|c| format!("ctx {c}"))
                        .unwrap_or_default(),
                })
                .collect();
            if items.is_empty() {
                app.messages
                    .push(ChatMessage::notice("Aucun modèle :free pour ce provider."));
            } else {
                let target = app.pending_picker_target.take();
                let mut p = picker::Picker::new("Modèles :free", items, picker::Kind::Model);
                p.target = target;
                app.picker = Some(p);
            }
        }
        UiEvent::ModelList(list) => {
            // Retient qui accepte les images (garde-fou de /img)…
            app.vision_models = list
                .iter()
                .filter(|m| {
                    m.architecture
                        .as_ref()
                        .and_then(|a| a.input_modalities.as_ref())
                        .map(|v| v.iter().any(|x| x.contains("image")))
                        .unwrap_or(false)
                })
                .map(|m| m.id.clone())
                .collect();
            // … puis ouvre le sélecteur.
            let items: Vec<picker::Item> = list
                .into_iter()
                .map(|m| {
                    let vis = app.vision_models.iter().any(|v| v == &m.id);
                    let mut note = String::new();
                    if vis {
                        note.push_str("🖼 images");
                    }
                    if let Some(c) = m.context_length {
                        if !note.is_empty() {
                            note.push_str(" · ");
                        }
                        note.push_str(&format!("ctx {c}"));
                    }
                    picker::Item {
                        label: m.id.clone(),
                        note,
                        id: m.id,
                    }
                })
                .collect();
            app.picker = Some(picker::Picker::new("Modèles", items, picker::Kind::Model));
        }
        UiEvent::Active(label) => app.active_endpoint = label,
        UiEvent::ToolCall { name, args } => app.messages.push(ChatMessage::notice(format!(
            "⚙ {name} · {}",
            summarize_tool_call(&name, &args)
        ))),
        UiEvent::ToolResult(r) => {
            let first = r.lines().next().unwrap_or("").to_string();
            app.messages
                .push(ChatMessage::notice(format!("⚙ → {first}")));
        }
        UiEvent::TurnComplete(full) => {
            let base = app.conversation.len();
            app.messages.truncate(app.display_mark);
            for m in full.iter().skip(base) {
                app.messages.push(m.clone());
                let _ = crate::convo::append(m);
            }
            app.conversation = full;
            app.streaming = false;
            app.streaming_text.clear();
            app.reasoning_text.clear();
            if app.zc.on {
                app.zc.state = crate::zerochan::State::Idle;
            }
        }
        UiEvent::Compacted(msgs) => {
            let n = msgs.len();
            // La version compactée doit survivre au redémarrage : sinon elle
            // n'existait qu'en mémoire et `--resume` rechargeait tout l'historique.
            if let Err(e) = crate::convo::rewrite(&msgs) {
                app.messages.push(ChatMessage::notice(format!(
                    "⚠ compaction non persistée : {e}"
                )));
            }
            app.conversation = msgs.clone();
            app.messages = msgs;
            app.messages.insert(
                0,
                ChatMessage::notice(format!("📝 historique compacté → {n} message(s)")),
            );
            app.display_mark = app.messages.len();
            app.auto_scroll = true;
        }
        UiEvent::Error(e) => {
            app.messages.truncate(app.display_mark);
            app.messages
                .push(ChatMessage::notice(format!("⚠ erreur: {e}")));
            app.streaming = false;
            app.streaming_text.clear();
            app.reasoning_text.clear();
        }
        UiEvent::Info(t) => app.messages.push(ChatMessage::notice(t)),
        UiEvent::VoteState { stars, starred } => {
            app.vote_stars = stars;
            app.vote_starred = starred;
        }
        UiEvent::Voice(_) => {}
        UiEvent::ContextWindow(c) => {
            if !app.ctx_manual {
                app.context_window = c;
            }
        }
        UiEvent::TelegramShutdown => {
            if let Some(h) = app.tg.take() {
                h.stop();
            }
            app.messages.push(ChatMessage::notice(
                "🤖 Telegram: OFF (demandé depuis le bot)",
            ));
        }
    }
}

// ---------------------------------------------------------------- rendering

fn draw(f: &mut Frame, app: &mut App) {
    let area = f.area();

    // Assistant de première connexion : plein écran, avant tout le reste.
    if app.wizard.is_some() {
        wizard::draw(f, area, app);
        return;
    }

    // Full-screen modes.
    match app.mode {
        crate::fun::Mode::Pong => {
            app.pong.draw(f, area);
            return;
        }
        crate::fun::Mode::Radio | crate::fun::Mode::BinTime => {
            draw_panel(f, area, app, app.mode);
            return;
        }
        crate::fun::Mode::Chat => {}
    }

    // Panneau latéral : le mod s'ouvre à droite du menu de commandes.
    if let Some(sm) = app.side_mode {
        if area.width >= 130 {
            let cols = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Min(90), Constraint::Length(40)])
                .split(area);
            draw_agent(f, cols[0], app, true);
            draw_panel(f, cols[1], app, sm);
        } else if area.width >= 110 {
            let cols = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Min(70), Constraint::Length(40)])
                .split(area);
            draw_agent(f, cols[0], app, false);
            draw_panel(f, cols[1], app, sm);
        } else {
            draw_panel(f, area, app, sm);
        }
    } else {
        draw_agent(f, area, app, true);
    }
    draw_chats_popup(f, area, app);
    finalize_buffer(f, app);

    // Sélecteur (/model, /provider) par-dessus tout le reste.
    if let Some(p) = &app.picker {
        picker::draw(f, area, p);
    }
}

/// Capture le texte affiché (pour la sélection) et surligne la sélection.
fn finalize_buffer(f: &mut Frame, app: &mut App) {
    let buf = f.buffer_mut();
    // capture le texte affiché (pour la sélection)
    let mut rows = Vec::with_capacity(buf.area.height as usize);
    for y in 0..buf.area.height {
        let mut s = String::new();
        for x in 0..buf.area.width {
            s.push_str(buf[(x, y)].symbol());
        }
        rows.push(s);
    }
    app.screen = rows;
    // surligne la sélection
    if let (Some(a), Some(h)) = (app.sel_anchor, app.sel_head) {
        let x0 = a.0.min(h.0);
        let x1 = a.0.max(h.0);
        let y0 = a.1.min(h.1);
        let y1 = a.1.max(h.1);
        let sel = Color::Rgb(40, 50, 110);
        for y in y0..=y1 {
            for x in x0..=x1 {
                if x < buf.area.width && y < buf.area.height {
                    let st = buf[(x, y)].style();
                    buf[(x, y)].set_style(st.bg(sel));
                }
            }
        }
    }
}

fn draw_panel(f: &mut Frame, area: Rect, app: &App, mode: crate::fun::Mode) {
    match mode {
        crate::fun::Mode::BinTime => app.bintime.draw(f, area),
        crate::fun::Mode::Radio => app.radio.draw(f, area),
        _ => {}
    }
}

fn centered_rect(pct_x: u16, pct_y: u16, r: Rect) -> Rect {
    let vert = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - pct_y) / 2),
            Constraint::Percentage(pct_y),
            Constraint::Percentage((100 - pct_y) / 2),
        ])
        .split(r);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - pct_x) / 2),
            Constraint::Percentage(pct_x),
            Constraint::Percentage((100 - pct_x) / 2),
        ])
        .split(vert[1])[1]
}

/// Convertit un timestamp Unix (UTC) en `YYYY-MM-DD HH:MM`.
fn fmt_time(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let (h, mi) = (rem / 3600, (rem % 3600) / 60);
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02} {h:02}:{mi:02}")
}

/// Pop-up de gestion des sessions (F6).
fn draw_chats_popup(f: &mut Frame, area: Rect, app: &App) {
    let Some((files, selected)) = &app.chats_popup else {
        return;
    };
    let rect = centered_rect(66, 60, area);
    f.render_widget(ratatui::widgets::Clear, rect);
    let mut lines: Vec<Line> = Vec::new();
    if files.is_empty() {
        lines.push(Line::from(Span::styled(
            "Aucune session enregistrée",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        for (i, p) in files.iter().enumerate() {
            let name = p
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let label = if name == "conversation.jsonl" {
                "● Session active".to_string()
            } else {
                let ts = name
                    .trim_start_matches("conversation_")
                    .trim_end_matches(".jsonl");
                match ts.parse::<u64>() {
                    Ok(t) => format!("🕒 {}", fmt_time(t)),
                    Err(_) => format!("📁 {ts}"),
                }
            };
            let style = if i == *selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };
            let marker = if i == *selected { "▶ " } else { "  " };
            lines.push(Line::from(Span::styled(format!("{marker}{label}"), style)));
        }
    }
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Chats — ↑↓ naviguer · Entrée charger · Esc fermer ")
        .style(Style::default().bg(Color::Rgb(15, 15, 25)).fg(Color::Cyan));
    f.render_widget(Paragraph::new(lines).block(block), rect);
}

fn draw_agent(f: &mut Frame, area: Rect, app: &mut App, show_menu: bool) {
    let input_h = input_height(app, area.width);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),       // status
            Constraint::Length(6),       // banner
            Constraint::Length(1),       // location
            Constraint::Min(3),          // chat
            Constraint::Length(input_h), // input (règles haut/bas)
            Constraint::Length(1),       // footer
        ])
        .split(area);

    draw_status(f, chunks[0], app);

    // Banner + petit bloc Zero-chan à droite.
    let (banner_area, zc_area) = crate::zerochan::block_layout(chunks[1], app.zc.on);
    let banner = if app.streaming {
        banner::lines_animated(app.anim_phase)
    } else {
        banner::lines()
    };
    f.render_widget(
        Paragraph::new(banner).alignment(Alignment::Left),
        banner_area,
    );
    if app.zc.on {
        app.zc.draw(f, zc_area);
    }

    draw_location(f, chunks[2]);
    let body = chunks[3];
    if show_menu && body.width >= 80 {
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(40), Constraint::Length(30)])
            .split(body);
        draw_chat(f, cols[0], app);
        app.menu_rect = cols[1];
        let open = app.menu_open;
        let sel = if app.menu_focus {
            Some(app.menu_sel)
        } else {
            None
        };
        menu::draw(
            f,
            cols[1],
            open,
            &mut app.menu_scroll,
            &mut app.menu_hits,
            sel,
        );
    } else {
        app.menu_hits.clear();
        app.menu_rect = Rect::default();
        draw_chat(f, body, app);
    }

    // Barre de saisie + boutons 0chan / micro à droite (visibles dès 60 colonnes).
    if chunks[4].width >= 60 {
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(30), Constraint::Length(13)])
            .split(chunks[4]);
        draw_input(f, cols[0], app);
        let (r1, r2) = crate::zerochan::draw_buttons(f, cols[1], &app.zc);
        app.btn_0chan = r1;
        app.btn_mic = r2;
    } else {
        draw_input(f, chunks[4], app);
        app.btn_0chan = None;
        app.btn_mic = None;
    }
    draw_footer(f, chunks[5], app);
}

fn draw_status(f: &mut Frame, area: Rect, app: &App) {
    let tokens = estimate_tokens(app);
    let line = Line::from(vec![
        Span::styled(
            format!("ctx {} · {tokens} tok", ctx_label(app)),
            Style::default().fg(Color::Gray),
        ),
        Span::raw("  ·······  "),
        Span::styled("O No project", Style::default().fg(Color::DarkGray)),
        Span::raw("   "),
        Span::styled(
            format!("think:{}", if app.thinking { "on" } else { "off" }),
            Style::default().fg(if app.thinking {
                Color::Magenta
            } else {
                Color::DarkGray
            }),
        ),
        Span::raw("   "),
        Span::styled(
            format!(
                "role:{}",
                if app.roles_enabled {
                    app.forced_role.clone().unwrap_or_else(|| {
                        if app.last_role.is_empty() {
                            "auto".to_string()
                        } else {
                            format!("auto/{}", app.last_role)
                        }
                    })
                } else {
                    "off".to_string()
                }
            ),
            Style::default().fg(if app.roles_enabled {
                Color::Yellow
            } else {
                Color::DarkGray
            }),
        ),
        Span::raw("   "),
        Span::styled(
            if app.active_endpoint.is_empty() {
                format!("{} · {}", app.provider_name, app.model)
            } else {
                app.active_endpoint.clone()
            },
            Style::default().fg(Color::Cyan),
        ),
        Span::raw("  "),
        Span::styled("●", Style::default().fg(Color::Green)),
    ]);
    f.render_widget(Paragraph::new(line), area);
}

fn draw_location(f: &mut Frame, area: Rect) {
    let local = std::env::current_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    let project = crate::mods::project_root().display().to_string();
    let line = Line::from(vec![
        Span::styled("Local ", Style::default().fg(Color::DarkGray)),
        Span::styled(local, Style::default().fg(Color::White)),
        Span::raw("   |   "),
        Span::styled("Projet ", Style::default().fg(Color::DarkGray)),
        Span::styled(project, Style::default().fg(Color::White)),
    ]);
    f.render_widget(Paragraph::new(line), area);
}

fn draw_chat(f: &mut Frame, area: Rect, app: &mut App) {
    let w = area.width as usize;
    let width = area.width;
    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(""));

    // Invalidation simple et sûre : tout changement de nombre de messages
    // (nouveau tour, compaction, chargement de session, /clear) vide le cache.
    if app.chat_cache_len != app.messages.len() {
        app.chat_cache.clear();
        app.chat_cache_len = app.messages.len();
    }
    // Garde-fou mémoire : un historique édité peut créer des clés orphelines.
    if app.chat_cache.len() > 4 * app.messages.len() + 64 {
        app.chat_cache.clear();
        app.chat_cache_len = app.messages.len();
    }

    app.block_ranges.clear();
    {
        // `messages` et `chat_cache` sont deux champs distincts : on les emprunte
        // séparément (le compilateur ne sait pas le faire via `app`).
        let App {
            messages,
            chat_cache,
            block_ranges,
            ..
        } = app;
        for (mi, m) in messages.iter().enumerate() {
            let text = m.content.clone().unwrap_or_default();
            let key = (mi, width, fingerprint(m));
            let start = lines.len();

            // Sorties d'outils : repliées par défaut, dépliées bloc par bloc.
            if m.role == "tool" {
                if let Some(cached) = chat_cache.get(&key) {
                    lines.extend(cached.iter().cloned());
                } else {
                    let before = lines.len();
                    let total = text.lines().count();
                    if m.expanded {
                        let capped: String = text.lines().take(200).collect::<Vec<_>>().join("\n");
                        push_message(&mut lines, &m.role, &capped, w);
                        if total > 200 {
                            push_message(&mut lines, "notice", "   … (tronqué à 200 lignes)", w);
                        }
                    } else {
                        let preview: String = text.lines().take(3).collect::<Vec<_>>().join("\n");
                        push_message(&mut lines, &m.role, &preview, w);
                        if total > 3 {
                            push_message(
                                &mut lines,
                                "notice",
                                &format!(
                                    "   … {} ligne(s) de plus — clic droit ou Ctrl+O",
                                    total - 3
                                ),
                                w,
                            );
                        }
                    }
                    let rendered: Vec<Line<'static>> = lines[before..].to_vec();
                    chat_cache.insert(key, rendered);
                }
                block_ranges.push((start as u16, (lines.len() as u16).saturating_sub(1), mi));
            } else if let Some(cached) = chat_cache.get(&key) {
                lines.extend(cached.iter().cloned());
            } else {
                let before = lines.len();
                push_message(&mut lines, &m.role, &text, w);
                let rendered: Vec<Line<'static>> = lines[before..].to_vec();
                chat_cache.insert(key, rendered);
            }
        }
    }

    if app.streaming {
        if !app.reasoning_text.is_empty() {
            push_block(
                &mut lines,
                " ··· ",
                theme::DIM,
                &app.reasoning_text,
                w,
                None,
                true,
            );
        }
        let text = if app.streaming_text.is_empty() {
            "…".to_string()
        } else {
            app.streaming_text.clone()
        };
        push_block(&mut lines, " ◕ ", theme::ACCENT, &text, w, None, false);
    }

    let inner_h = area.height as usize;
    let max_scroll = lines.len().saturating_sub(inner_h).min(u16::MAX as usize) as u16;
    app.max_scroll = max_scroll;
    let scroll = if app.auto_scroll {
        max_scroll
    } else {
        app.scroll.min(max_scroll)
    };
    app.scroll = scroll;
    app.chat_rect = area;
    app.chat_scroll = scroll;

    let paragraph = Paragraph::new(lines).scroll((scroll, 0));
    f.render_widget(paragraph, area);
}

/// Empreinte courte d'un message : invalide le cache si le contenu ou l'état
/// déplié change. FNV-1a, O(n) mais bien moins cher que re-découper le texte.
fn fingerprint(m: &ChatMessage) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut mix = |b: u8| {
        h ^= b as u64;
        h = h.wrapping_mul(0x100_0000_01b3);
    };
    for b in m.role.as_bytes() {
        mix(*b);
    }
    mix(m.expanded as u8);
    if let Some(c) = &m.content {
        for b in c.as_bytes() {
            mix(*b);
        }
    }
    h
}

/// Index du message dépliable dont le bloc occupe `line` dans le fil.
fn block_at(ranges: &[(u16, u16, usize)], line: u16) -> Option<usize> {
    ranges
        .iter()
        .find(|(a, b, _)| line >= *a && line <= *b)
        .map(|(_, _, i)| *i)
}

/// Ctrl+O : déplie/replie le **dernier** bloc de sortie d'outil.
fn toggle_last_tool_block(app: &mut App) {
    if let Some(i) = app.messages.iter().rposition(|m| m.role == "tool") {
        app.messages[i].expanded = !app.messages[i].expanded;
    }
}

/// Résumé compact (une ligne) d'un appel d'outil : on n'affiche **jamais** tout
/// le contenu envoyé (ex. un fichier Python entier dans `text_editor`).
fn summarize_tool_call(name: &str, args: &str) -> String {
    let v: serde_json::Value = serde_json::from_str(args).unwrap_or(serde_json::Value::Null);
    let get = |k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
    let one = |s: &str| -> String {
        let l = s.lines().next().unwrap_or("").trim();
        if l.chars().count() > 100 {
            l.chars().take(99).collect::<String>() + "…"
        } else {
            l.to_string()
        }
    };
    match name {
        "text_editor" => {
            let cmd = one(&get("command"));
            let path = one(&get("path"));
            let lines = v
                .get("file_text")
                .and_then(|x| x.as_str())
                .map(|t| t.lines().count())
                .unwrap_or(0);
            let extra = if lines > 0 {
                format!(" ({lines} lignes)")
            } else {
                String::new()
            };
            format!("{cmd} {path}{extra}").trim_end().to_string()
        }
        "code_exec" => one(&get("command")),
        "web_search" | "knowledge_search" => one(&get("query")),
        _ => {
            if let Some(o) = v.as_object() {
                for k in o.keys() {
                    let s = get(k);
                    if !s.is_empty() {
                        return format!("{k}={}", one(&s));
                    }
                }
            }
            one(args)
        }
    }
}

fn push_message(lines: &mut Vec<Line>, role: &str, text: &str, w: usize) {
    match role {
        "user" => push_block(
            lines,
            " ❯ ",
            theme::ACCENT,
            text,
            w,
            Some(theme::PANEL),
            false,
        ),
        "assistant" => push_block(lines, " ◕ ", theme::ACCENT, text, w, None, false),
        "tool" => push_block(lines, " ⚙ ", theme::BLUE, text, w, None, true),
        "system" => push_block(lines, " ⚙ ", theme::DIM, text, w, None, true),
        _ => push_block(lines, " · ", theme::DIM, text, w, None, true),
    }
}

/// Rend un message : marqueur coloré + texte, avec fond optionnel (bloc) et
/// coloration légère (`code` et **gras**). Les blocs ``` sont affichés en surbrillance.
fn push_block(
    lines: &mut Vec<Line>,
    prefix: &str,
    pcolor: Color,
    text: &str,
    w: usize,
    bg: Option<Color>,
    dim_text: bool,
) {
    let indent = prefix.chars().count();
    let mut first = true;
    let mut in_code = false;
    let fill = |n: usize| {
        let s = Style::default();
        Span::styled(
            " ".repeat(n),
            match bg {
                Some(b) => s.bg(b),
                None => s,
            },
        )
    };
    for seg in wrap_text(text, w.saturating_sub(indent).max(1)) {
        if seg.trim_start().starts_with("```") {
            in_code = !in_code;
            continue;
        }
        let mut spans: Vec<Span> = Vec::new();
        let pstyle = {
            let s = Style::default().fg(pcolor).add_modifier(Modifier::BOLD);
            match bg {
                Some(b) => s.bg(b),
                None => s,
            }
        };
        if first {
            spans.push(Span::styled(prefix.to_string(), pstyle));
            first = false;
        } else {
            spans.push(fill(indent));
        }
        let base = {
            let s = Style::default().fg(if in_code {
                theme::CODE
            } else if dim_text {
                theme::DIM
            } else {
                theme::TEXT
            });
            match bg {
                Some(b) => s.bg(b),
                None => s,
            }
        };
        let mut used = indent;
        if in_code {
            used += seg.chars().count();
            spans.push(Span::styled(seg, base));
        } else {
            let sp = inline_spans(&seg, base);
            for s in &sp {
                used += s.content.chars().count();
            }
            spans.extend(sp);
        }
        if bg.is_some() && used < w {
            spans.push(fill(w - used));
        }
        lines.push(Line::from(spans));
    }
    lines.push(Line::from(""));
}

/// Découpe `\`code\`` et `**gras**` en spans stylés.
fn inline_spans(text: &str, base: Style) -> Vec<Span<'static>> {
    let mut out: Vec<Span<'static>> = Vec::new();
    let mut buf = String::new();
    let mut code = false;
    let mut bold = false;
    let mut it = text.chars().peekable();
    let styled = |buf: &str, code: bool, bold: bool| {
        let mut s = base;
        if code {
            s = s.fg(theme::CODE);
        }
        if bold {
            s = s.add_modifier(Modifier::BOLD);
        }
        Span::styled(buf.to_string(), s)
    };
    while let Some(c) = it.next() {
        if c == '`' {
            if !buf.is_empty() {
                out.push(styled(&std::mem::take(&mut buf), code, bold));
            }
            code = !code;
        } else if c == '*' && it.peek() == Some(&'*') {
            it.next();
            if !buf.is_empty() {
                out.push(styled(&std::mem::take(&mut buf), code, bold));
            }
            bold = !bold;
        } else {
            buf.push(c);
        }
    }
    if !buf.is_empty() {
        out.push(styled(&buf, code, bold));
    }
    out
}

/// Découpe `s` en lignes de largeur `w` (par caractères) et renvoie la position
/// `(ligne, colonne)` du curseur (index en caractères).
fn wrap_with_cursor(s: &str, w: usize, cursor: usize) -> (Vec<String>, usize, usize) {
    let w = w.max(1);
    let mut lines: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut col = 0usize;
    let mut idx = 0usize;
    let (mut crow, mut ccol) = (0usize, 0usize);
    for ch in s.chars() {
        if idx == cursor {
            crow = lines.len();
            ccol = col;
        }
        if ch == '\n' {
            lines.push(std::mem::take(&mut cur));
            col = 0;
        } else {
            if col >= w {
                lines.push(std::mem::take(&mut cur));
                col = 0;
            }
            cur.push(ch);
            col += 1;
        }
        idx += 1;
    }
    if idx == cursor {
        crow = lines.len();
        ccol = col;
    }
    lines.push(cur);
    (lines, crow, ccol)
}

fn draw_input(f: &mut Frame, area: Rect, app: &App) {
    // Saisie simple : juste `❯ texte` (aucun cadre ni règle).
    let prompt = "❯ ";
    let pad = prompt.chars().count();
    let inner_w = (area.width as usize).saturating_sub(pad).max(1);
    let shown: String = if app.secret_mode {
        "●".repeat(app.input.chars().count())
    } else {
        app.input.clone()
    };
    let (text_lines, crow, ccol) = wrap_with_cursor(&shown, inner_w, app.cursor);
    let max_lines = area.height.max(1) as usize;
    // Défilement : garder le curseur visible.
    let mut start = text_lines.len().saturating_sub(max_lines);
    if crow < start {
        start = crow;
    }
    if crow >= start + max_lines {
        start = crow + 1 - max_lines;
    }
    let base = Style::default()
        .fg(theme::ACCENT)
        .add_modifier(Modifier::BOLD);

    let mut lines: Vec<Line> = Vec::new();
    for (i, seg) in text_lines.iter().enumerate().skip(start).take(max_lines) {
        if start == 0 && i == 0 {
            lines.push(Line::from(vec![
                Span::styled(prompt, base),
                Span::styled(seg.clone(), Style::default().fg(theme::TEXT)),
            ]));
        } else {
            lines.push(Line::from(Span::styled(
                format!("{}{}", " ".repeat(pad), seg),
                Style::default().fg(theme::TEXT),
            )));
        }
    }
    f.render_widget(Paragraph::new(lines), area);

    if !app.streaming && area.height > 0 {
        let row = crow.saturating_sub(start) as u16;
        let x = (area.x + pad as u16 + ccol as u16).min(area.x + area.width.saturating_sub(1));
        let y = area.y + row;
        f.set_cursor_position(Position::new(x, y));
    }
}

/// Index d'octet correspondant à un index de caractères.
fn byte_at(s: &str, char_idx: usize) -> usize {
    s.char_indices()
        .nth(char_idx)
        .map(|(b, _)| b)
        .unwrap_or(s.len())
}

/// Défilement à la molette : renvoie `(offset, auto_scroll)`.
/// 3 lignes par cran ; revenir tout en bas réactive le suivi automatique.
fn wheel_scroll(scroll: u16, max: u16, up: bool) -> (u16, bool) {
    if up {
        (scroll.saturating_sub(3), false)
    } else {
        let s = (scroll + 3).min(max);
        (s, s >= max)
    }
}

fn input_set(app: &mut App, s: String) {
    app.cursor = s.chars().count();
    app.input = s;
}

fn input_insert(app: &mut App, ch: char) {
    let b = byte_at(&app.input, app.cursor);
    app.input.insert(b, ch);
    app.cursor += 1;
}

fn input_insert_str(app: &mut App, s: &str) {
    let b = byte_at(&app.input, app.cursor);
    app.input.insert_str(b, s);
    app.cursor += s.chars().count();
}

fn input_backspace(app: &mut App) {
    if app.cursor == 0 {
        return;
    }
    let b0 = byte_at(&app.input, app.cursor - 1);
    let b1 = byte_at(&app.input, app.cursor);
    app.input.replace_range(b0..b1, "");
    app.cursor -= 1;
}

fn input_delete(app: &mut App) {
    if app.cursor >= app.input.chars().count() {
        return;
    }
    let b0 = byte_at(&app.input, app.cursor);
    let b1 = byte_at(&app.input, app.cursor + 1);
    app.input.replace_range(b0..b1, "");
}

fn ctx_label(app: &App) -> String {
    let t = estimate_tokens(app);
    let pct = (t as u64 * 100 / app.context_window.max(1)).min(100);
    format!("{pct}%/{} ", ctx_window_label(app.context_window))
        .trim_end()
        .to_string()
}

fn ctx_window_label(c: u64) -> String {
    if c >= 1_000_000 {
        format!("{:.1}M", c as f64 / 1_000_000.0)
    } else if c >= 1_000 {
        format!("{}k", c / 1000)
    } else {
        c.to_string()
    }
}

/// Contexte connu pour les modèles dont le provider ne le rapporte pas.
fn known_context(model: &str) -> Option<u64> {
    let m = model.to_ascii_lowercase();
    let has = |s: &str| m.contains(s);
    if has("kimi-k3") || has("kimi-k2") || has("kimi-latest") {
        return Some(1_000_000);
    }
    if has("gemini") {
        return Some(1_000_000);
    }
    if has("claude") {
        return Some(200_000);
    }
    if has("gpt-4.1") || has("gpt-4o") || has("o3") || has("o4") {
        return Some(128_000);
    }
    if has("qwen3") || has("deepseek") || has("mistral-large") {
        return Some(128_000);
    }
    None
}

fn parse_ctx(s: &str) -> Option<u64> {
    let s = s.trim().to_ascii_lowercase();
    if let Some(x) = s.strip_suffix('m') {
        return x.parse::<f64>().ok().map(|v| (v * 1_000_000.0) as u64);
    }
    if let Some(x) = s.strip_suffix('k') {
        return x.parse::<f64>().ok().map(|v| (v * 1000.0) as u64);
    }
    s.parse::<u64>().ok()
}

fn handle_telegram(app: &mut App, args: &[&str], ui_tx: &UnboundedSender<UiEvent>) {
    match args.first().copied() {
        Some("on") | Some("start") => {
            if app.tg.is_some() {
                app.messages
                    .push(ChatMessage::notice("🤖 Telegram: déjà actif"));
            } else {
                match crate::telegram::spawn(None, Vec::new(), Some(ui_tx.clone())) {
                    Ok(h) => {
                        app.tg = Some(h);
                        app.messages.push(ChatMessage::notice(
                            "🤖 Telegram: ON — envoie /start au bot",
                        ));
                    }
                    Err(e) => app
                        .messages
                        .push(ChatMessage::notice(format!("🤖 Telegram: {e}"))),
                }
            }
        }
        Some("off") | Some("stop") => {
            if let Some(h) = app.tg.take() {
                h.stop();
                app.messages.push(ChatMessage::notice("🤖 Telegram: OFF"));
            } else {
                app.messages
                    .push(ChatMessage::notice("🤖 Telegram: déjà inactif"));
            }
        }
        _ => {
            let state = if app.tg.is_some() { "ON" } else { "off" };
            app.messages.push(ChatMessage::notice(format!(
                "🤖 Telegram: {state} — /telegram on|off"
            )));
        }
    }
}

fn handle_ctx(app: &mut App, args: &[&str]) {
    match args.first().copied() {
        None => app.messages.push(ChatMessage::notice(format!(
            "ctx: {} ({} tokens) — /ctx 1M | 200k | 128000 | auto",
            ctx_window_label(app.context_window),
            app.context_window
        ))),
        Some("auto") => {
            let mut g = config::load_global();
            g.context_window = None;
            let _ = config::save_global(&g);
            app.ctx_manual = false;
            app.context_window = known_context(&app.model).unwrap_or(200_000);
            app.messages.push(ChatMessage::notice(format!(
                "ctx: auto → {}",
                ctx_window_label(app.context_window)
            )));
        }
        Some(s) => match parse_ctx(s) {
            Some(n) => {
                let mut g = config::load_global();
                g.context_window = Some(n);
                let _ = config::save_global(&g);
                app.ctx_manual = true;
                app.context_window = n;
                app.messages
                    .push(ChatMessage::notice(format!("ctx: {n} tokens")));
            }
            None => app
                .messages
                .push(ChatMessage::notice("usage: /ctx <tokens|200k|1M|auto>")),
        },
    }
}

/// Interroge le provider pour la fenêtre de contexte du modèle actif.
fn probe_context(app: &App, ui_tx: &UnboundedSender<UiEvent>) {
    let Some(provider) = providers::find(&app.provider_id) else {
        return;
    };
    let model = app.model.clone();
    let key = app.api_key.clone();
    let ui = ui_tx.clone();
    tokio::spawn(async move {
        if let Ok(list) = models::list(&http_client::HTTP_CLIENT, &provider, key.as_deref()).await {
            if let Some(c) = list
                .iter()
                .find(|m| m.id == model)
                .and_then(|m| m.context_length)
            {
                let _ = ui.send(UiEvent::ContextWindow(c));
            }
        }
    });
}

/// Hauteur nécessaire pour la zone de saisie (1..=8 lignes + règles haut/bas).
fn input_height(app: &App, width: u16) -> u16 {
    // Saisie simple : `❯ ` puis le texte, 1..=8 lignes (pas de cadre).
    let inner_w = width.saturating_sub(2).max(1) as usize;
    wrap_text(&app.input, inner_w).len().clamp(1, 8) as u16
}

fn copy_selection(app: &mut App) {
    if let (Some(a), Some(h)) = (app.sel_anchor, app.sel_head) {
        let x0 = a.0.min(h.0);
        let x1 = a.0.max(h.0);
        let y0 = a.1.min(h.1);
        let y1 = a.1.max(h.1);

        let mut text = String::new();
        for y in y0..=y1 {
            if let Some(row) = app.screen.get(y as usize) {
                let chars: Vec<char> = row.chars().collect();
                let mut line = String::new();
                for x in x0..=x1 {
                    if let Some(c) = chars.get(x as usize) {
                        line.push(*c);
                    }
                }
                text.push_str(line.trim_end());
                if y < y1 {
                    text.push('\n');
                }
            }
        }
        let text = text.trim().to_string();
        if !text.is_empty() {
            crate::clipboard::copy(&text);
            app.messages.push(ChatMessage::notice(format!(
                "📋 sélection copiée ({} caractères)",
                text.chars().count()
            )));
        }
    }
    app.sel_anchor = None;
    app.sel_head = None;
}

/// Copie le contenu propre d'un message par index (sans UI chrome)
fn copy_message_content(app: &mut App, msg_idx: usize) {
    if let Some(msg) = app.messages.get(msg_idx) {
        if let Some(content) = &msg.content {
            if !content.trim().is_empty() {
                crate::clipboard::copy(content);
                let role_label = match msg.role.as_str() {
                    "assistant" => "assistant",
                    "user" => "utilisateur",
                    "tool" => "outil",
                    _ => &msg.role,
                };
                app.messages.push(ChatMessage::notice(format!(
                    "📋 message {role_label} copié ({} caractères)",
                    content.chars().count()
                )));
            }
        }
    }
}

/// Copie le dernier message assistant (raccourci Ctrl+Shift+C)
fn copy_last_assistant(app: &mut App) {
    if let Some(idx) = app.messages.iter().rposition(|m| m.role == "assistant") {
        copy_message_content(app, idx);
    }
}

fn toggle_mic(app: &mut App, ui_tx: &UnboundedSender<UiEvent>) {
    if !app.zc.on {
        app.zc.toggle_on();
    }
    if app.zc.is_recording() {
        app.zc.stop_rec();
        app.zc.state = crate::zerochan::State::Thinking;
        app.messages.push(ChatMessage::notice("🎤 transcription…"));
        let path = crate::zerochan::rec_path();
        let ui = ui_tx.clone();
        tokio::spawn(async move {
            match crate::zerochan::transcribe(&path).await {
                Ok(t) if !t.trim().is_empty() => {
                    let _ = ui.send(UiEvent::Voice(t));
                }
                Ok(_) => {
                    let _ = ui.send(UiEvent::Info("zero-chan : (rien entendu)".into()));
                }
                Err(e) => {
                    let _ = ui.send(UiEvent::Info(format!("zero-chan STT : {e}")));
                }
            }
        });
    } else if app.zc.start_rec() {
        app.messages.push(ChatMessage::notice(format!(
            "🎤 écoute… (re-clic ou {} s pour arrêter)",
            crate::zerochan::MAX_REC_SECS as u32
        )));
    } else {
        let detail = app.zc.last.clone();
        let message = if detail.trim().is_empty() {
            "zero-chan : micro indisponible".to_string()
        } else {
            format!("zero-chan : micro indisponible — {detail}")
        };
        app.messages.push(ChatMessage::notice(message));
    }
}

fn history_up(app: &mut App) {
    if app.history.is_empty() {
        return;
    }
    match app.hist_pos {
        None => {
            app.hist_draft = app.input.clone();
            app.hist_pos = Some(app.history.len() - 1);
        }
        Some(0) => {}
        Some(i) => app.hist_pos = Some(i - 1),
    }
    if let Some(i) = app.hist_pos {
        input_set(app, app.history[i].clone());
    }
}

fn history_down(app: &mut App) {
    match app.hist_pos {
        None => {}
        Some(i) if i + 1 < app.history.len() => {
            app.hist_pos = Some(i + 1);
            input_set(app, app.history[i + 1].clone());
        }
        Some(_) => {
            app.hist_pos = None;
            input_set(app, app.hist_draft.clone());
        }
    }
}

fn draw_footer(f: &mut Frame, area: Rect, app: &mut App) {
    let key = Style::default().fg(Color::Black).bg(Color::Cyan);
    let txt = Style::default().fg(Color::Gray);
    let spans = vec![
        Span::styled(" F6 ", key),
        Span::styled(" Chats  ", txt),
        Span::styled(" F7 ", key),
        Span::styled(" Nudge  ", txt),
        Span::styled(" Ctrl+C ", key),
        Span::styled(" Exit  ", txt),
        Span::styled(" F3 ", key),
        Span::styled(" Read&Write  ", txt),
        Span::styled(" F4 ", key),
        Span::styled(" Code-exec  ", txt),
        Span::styled(" F5 ", key),
        Span::styled(" Aide  ", txt),
        Span::styled(" F8 ", key),
        Span::styled(" 0chan  ", txt),
        Span::styled(" F10 ", key),
        Span::styled(" Mic  ", txt),
        Span::styled(" Ctrl+O ", key),
        Span::styled(" détail ", txt),
    ];
    // Le vote (👍/👎) occupe la droite du pied de page : c'est le bas-droite
    // de l'écran principal. Les raccourcis gardent la place restante.
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(0), Constraint::Length(crate::vote::WIDTH)])
        .split(area);
    f.render_widget(Paragraph::new(Line::from(spans)), cols[0]);
    let (up, down) = crate::vote::draw(f, cols[1], app.vote_stars, app.vote_starred);
    app.btn_vote_up = up;
    app.btn_vote_down = down;
}

fn estimate_tokens(app: &App) -> usize {
    let chars: usize = app
        .messages
        .iter()
        .filter_map(|m| m.content.as_ref())
        .map(|c| c.chars().count())
        .sum();
    chars / 4
}

fn wrap_text(s: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut out = Vec::new();
    for raw in s.split('\n') {
        if raw.is_empty() {
            out.push(String::new());
            continue;
        }
        let mut cur = String::new();
        let mut count = 0;
        for ch in raw.chars() {
            if count >= width {
                out.push(std::mem::take(&mut cur));
                count = 0;
            }
            cur.push(ch);
            count += 1;
        }
        out.push(cur);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// App minimal pour les tests headless (rendu + clics).
    fn test_app() -> App {
        App {
            messages: vec![],
            input: String::new(),
            cursor: 0,
            should_quit: false,
            streaming: false,
            streaming_text: String::new(),
            reasoning_text: String::new(),
            anim_phase: 0,
            scroll: 0,
            auto_scroll: false,
            max_scroll: 0,
            thinking: false,
            max_tokens: 0,
            temperature: 0.0,
            conversation: vec![],
            display_mark: 0,
            system: String::new(),
            base_system: String::new(),
            persona: None,
            all_tools: vec![],
            read_write: false,
            code_exec: false,
            provider_id: "local".into(),
            provider_name: "local".into(),
            base_url: String::new(),
            api_key: None,
            model: String::new(),
            active_endpoint: String::new(),
            forced_role: None,
            last_role: String::new(),
            roles_enabled: true,
            picker: None,
            wizard: None,
            secret_mode: false,
            pending_picker_target: None,
            pending_image: None,
            pending_image_name: None,
            vision_models: vec![],
            chat_rect: Rect::default(),
            chat_scroll: 0,
            block_ranges: vec![],
            chat_cache: std::collections::HashMap::new(),
            chat_cache_len: 0,
            mode: crate::fun::Mode::Chat,
            radio: crate::fun::radio::Radio::new(),
            bintime: crate::fun::bintime::BinTime::new(),
            pong: crate::fun::pong::Pong::new(),
            side_mode: None,
            focus_panel: false,
            kb_auto: false,
            history: vec![],
            hist_pos: None,
            hist_draft: String::new(),
            zc: crate::zerochan::ZeroChan::new(),
            btn_0chan: None,
            btn_mic: None,
            vote_stars: None,
            vote_starred: None,
            btn_vote_up: None,
            btn_vote_down: None,
            sel_anchor: None,
            sel_head: None,
            screen: vec![],
            chats_popup: None,
            context_window: 0,
            ctx_manual: false,
            tg: None,
            menu_open: None,
            menu_hits: vec![],
            menu_scroll: 0,
            menu_rect: Rect::default(),
            menu_focus: false,
            menu_sel: 0,
            last_key_event: None,
        }
    }

    /// Régression : le menu ET les boutons de vote doivent rester cliquables.
    ///
    /// Le clic sur le menu ne doit pas être traité comme une sélection de texte,
    /// et le clic sur un pouce non plus. Avant, un clic upvote ouvrait un
    /// navigateur via un process enfant qui héritait de la console : la capture
    /// souris était perdue et AUCUN clic n'atteignait plus le TUI (menu
    /// compris). D'où `platform::open()` sans héritage de console et
    /// `platform::restore_mouse_capture()`.
    #[tokio::test]
    async fn clicks_reach_menu_and_vote_buttons() {
        use crossterm::event::{Event, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;

        let mut app = test_app();
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let mut term = Terminal::new(TestBackend::new(120, 40)).unwrap();

        // Rendu initial : remplit menu_hits + btn_vote_*.
        term.draw(|f| draw(f, &mut app)).unwrap();

        // 1) Le menu doit exposer des zones cliquables.
        assert!(
            !app.menu_hits.is_empty(),
            "le menu n'expose aucune zone cliquable"
        );

        // 2) Un clic sur une entrée du menu est routé vers le menu.
        let (r, _hit) = app.menu_hits[0];
        let before = app.menu_hits.len();
        handle_input(
            Event::Mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: r.x + 1,
                row: r.y,
                modifiers: KeyModifiers::NONE,
            }),
            &mut app,
            &tx,
        );
        assert!(
            app.sel_anchor.is_none(),
            "le clic sur le menu a été traité comme une sélection de texte"
        );
        assert_eq!(app.menu_hits.len(), before);

        // 3) Un clic sur un pouce n'est pas une sélection de texte non plus.
        let up = app.btn_vote_up.expect("pas de zone upvote");
        let down = app.btn_vote_down.expect("pas de zone downvote");
        assert!(up.x > 100, "upvote pas à droite : {up:?}");
        assert!(down.x > up.x, "downvote pas après upvote");
        handle_input(
            Event::Mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: up.x,
                row: up.y,
                modifiers: KeyModifiers::NONE,
            }),
            &mut app,
            &tx,
        );
        assert!(
            app.sel_anchor.is_none(),
            "le clic sur l'upvote a été traité comme une sélection de texte"
        );
    }

    #[test]
    fn parse_ctx_units() {
        assert_eq!(parse_ctx("1M"), Some(1_000_000));
        assert_eq!(parse_ctx("1.5M"), Some(1_500_000));
        assert_eq!(parse_ctx("200k"), Some(200_000));
        assert_eq!(parse_ctx(" 128000 "), Some(128_000));
        assert_eq!(parse_ctx("abc"), None);
    }

    #[test]
    fn known_context_models() {
        assert_eq!(known_context("moonshotai/kimi-k3"), Some(1_000_000));
        assert_eq!(known_context("gemini-3.5-flash-lite"), Some(1_000_000));
        assert_eq!(known_context("claude-3-opus"), Some(200_000));
        assert_eq!(known_context("gpt-4o-mini"), Some(128_000));
        assert_eq!(known_context("qwen3-coder"), Some(128_000));
        assert_eq!(known_context("some-unknown-model"), None);
    }

    #[test]
    fn ctx_window_format() {
        assert_eq!(ctx_window_label(1_000_000), "1.0M");
        assert_eq!(ctx_window_label(2_000_000), "2.0M");
        assert_eq!(ctx_window_label(200_000), "200k");
        assert_eq!(ctx_window_label(128_000), "128k");
        assert_eq!(ctx_window_label(500), "500");
    }

    #[test]
    fn palette_commit_keeps_the_typed_text() {
        assert_eq!(palette_text("whoami"), "/whoami");
        assert_eq!(palette_text("  whoami  "), "/whoami");
        assert_eq!(palette_text(""), "/");
    }

    #[test]
    fn wrap_text_behavior() {
        assert_eq!(wrap_text("abcdef", 3), vec!["abc", "def"]);
        assert_eq!(wrap_text("ab\ncd", 10), vec!["ab", "cd"]);
        assert_eq!(wrap_text("héllo", 2).len(), 3); // comptage par caractères
    }

    #[test]
    fn fmt_time_epochs() {
        assert_eq!(fmt_time(0), "1970-01-01 00:00");
        assert_eq!(fmt_time(1_000_000_000), "2001-09-09 01:46");
    }

    #[test]
    fn wrap_cursor_positions() {
        let (l, r, c) = wrap_with_cursor("abcdef", 3, 4);
        assert_eq!(l, vec!["abc", "def"]);
        assert_eq!((r, c), (1, 1));
        let (_, r, c) = wrap_with_cursor("abc", 3, 3);
        assert_eq!((r, c), (0, 3));
        let (_, r, c) = wrap_with_cursor("a\nb", 10, 0);
        assert_eq!((r, c), (0, 0));
    }

    #[test]
    fn wheel_up_stops_auto_scroll_and_clamps() {
        assert_eq!(super::wheel_scroll(10, 50, true), (7, false));
        assert_eq!(super::wheel_scroll(2, 50, true), (0, false));
    }

    #[test]
    fn wheel_down_clamps_and_reactivates_follow_at_bottom() {
        assert_eq!(super::wheel_scroll(10, 50, false), (13, false));
        assert_eq!(super::wheel_scroll(49, 50, false), (50, true));
        assert_eq!(super::wheel_scroll(50, 50, false), (50, true));
    }

    #[test]
    fn tool_call_summary_hides_full_file_content() {
        let args = serde_json::json!({
            "command": "create",
            "path": "/tmp/x.py",
            "file_text": "ligne1\nligne2\nligne3\nligne4"
        })
        .to_string();
        let s = super::summarize_tool_call("text_editor", &args);
        assert!(s.contains("create"));
        assert!(s.contains("/tmp/x.py"));
        assert!(s.contains("4 lignes"));
        assert!(
            !s.contains("ligne1"),
            "le contenu ne doit pas apparaître : {s}"
        );
    }

    #[test]
    fn tool_call_summary_keeps_only_the_first_line() {
        let args = serde_json::json!({ "command": "python3 - <<'EOF'\nprint(1)\nEOF" }).to_string();
        assert_eq!(
            super::summarize_tool_call("code_exec", &args),
            "python3 - <<'EOF'"
        );
    }

    #[test]
    fn tool_call_summary_truncates_long_lines() {
        let args = serde_json::json!({ "query": "x".repeat(300) }).to_string();
        let s = super::summarize_tool_call("web_search", &args);
        assert!(
            s.chars().count() <= 101,
            "trop long : {}",
            s.chars().count()
        );
    }

    #[test]
    fn block_at_maps_a_line_to_its_message() {
        let ranges = [(10u16, 14u16, 3usize), (20, 25, 7)];
        assert_eq!(super::block_at(&ranges, 10), Some(3));
        assert_eq!(super::block_at(&ranges, 12), Some(3));
        assert_eq!(super::block_at(&ranges, 15), None);
        assert_eq!(super::block_at(&ranges, 22), Some(7));
        assert_eq!(super::block_at(&[], 0), None);
    }
}
