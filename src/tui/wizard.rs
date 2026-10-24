//! Assistant de première connexion.
//!
//! Objectif : qu'un nouvel utilisateur puisse aller de « je viens d'installer »
//! à « l'agent répond » sans lire six documents. Huit étapes, chacune
//! expliquant **pourquoi** avant **comment**, avec reprise possible (`Échap`
//! enregistre l'étape, `/setup` reprend).
//!
//! Règles : `code_exec` reste désactivé par défaut (choix explicite), les
//! secrets sont saisis en mode masqué, et le coffre reste au CLI (la passphrase
//! ne transite pas par le buffer de la TUI).

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;
use tokio::sync::mpsc::UnboundedSender;

use crate::event::UiEvent;
use crate::models::ModelEntry;
use crate::{config, providers};

use super::{theme, App};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Inference,
    Provider,
    ApiKey,
    Model,
    Telegram,
    TelegramToken,
    Thinking,
    Exec,
    Commands,
    Done,
}

impl Step {
    /// Index persisté dans `config.toml` (`None` = terminé).
    pub fn idx(self) -> u8 {
        match self {
            Step::Inference => 0,
            Step::Provider => 1,
            Step::ApiKey => 2,
            Step::Model => 3,
            Step::Telegram => 4,
            // sous-étape : on reprend à l'écran Telegram, jamais au token
            Step::TelegramToken => 4,
            Step::Thinking => 5,
            Step::Exec => 6,
            Step::Commands => 7,
            Step::Done => 8,
        }
    }

    pub fn from_idx(i: u8) -> Step {
        match i {
            0 => Step::Inference,
            1 => Step::Provider,
            2 => Step::ApiKey,
            3 => Step::Model,
            4 => Step::Telegram,
            5 => Step::Thinking,
            6 => Step::Exec,
            7 => Step::Commands,
            _ => Step::Done,
        }
    }

    fn title(self) -> &'static str {
        match self {
            Step::Inference => "1/8 — Où tourne le modèle ?",
            Step::Provider => "2/8 — Quel provider ?",
            Step::ApiKey => "3/8 — Clé API",
            Step::Model => "4/8 — Quel modèle ?",
            Step::Telegram => "5/8 — Telegram",
            Step::TelegramToken => "5/8 — Jeton Telegram",
            Step::Thinking => "6/8 — Raisonnement (think)",
            Step::Exec => "7/8 — Exécution shell (code_exec)",
            Step::Commands => "8/8 — Les commandes",
            Step::Done => "Terminé",
        }
    }

    pub fn needs_text(self) -> bool {
        matches!(self, Step::ApiKey | Step::TelegramToken)
    }
}

pub struct Wizard {
    pub step: Step,
    pub cursor: usize,
    pub provider: Option<String>,
    pub loading: bool,
    pub models: Vec<ModelEntry>,
    pub telegram: Option<bool>,
    pub thinking: bool,
    pub exec: bool,
    pub error: Option<String>,
    pub page: usize,
    /// Le modèle courant a-t-il été choisi ? (pour l'étape Model en saisie libre)
    pub manual_model: bool,
}

impl Wizard {
    fn new(step: Step) -> Self {
        Self {
            step,
            cursor: 0,
            provider: None,
            loading: false,
            models: Vec::new(),
            telegram: None,
            thinking: false,
            exec: false,
            error: None,
            page: 0,
            manual_model: false,
        }
    }
}

/// Ouvre l'assistant (ou le reprend à `step`).
pub fn start(app: &mut App, ui_tx: &UnboundedSender<UiEvent>, step: Step) {
    app.wizard = Some(Wizard::new(step));
    app.input.clear();
    app.cursor = 0;
    app.secret_mode = matches!(step, Step::ApiKey | Step::TelegramToken);
    app.messages.push(super::ChatMessage::notice(
        "Assistant de démarrage — Échap pour reprendre plus tard (/setup).",
    ));
    if step == Step::Model {
        fetch_models(app, ui_tx);
    }
}

fn save_step(step: Step) {
    let mut g = config::load_global();
    g.setup_step = step.idx();
    let _ = config::save_global(&g);
}

fn finish(app: &mut App, ui_tx: &UnboundedSender<UiEvent>) {
    let mut g = config::load_global();
    g.setup_done = true;
    g.setup_step = Step::Done.idx();
    let _ = config::save_global(&g);

    let want_tg = app.wizard.as_ref().and_then(|w| w.telegram) == Some(true);
    let token = config::load_global().telegram_token.clone();
    app.wizard = None;
    app.secret_mode = false;
    app.input.clear();
    app.cursor = 0;

    if want_tg {
        if let Some(token) = token.filter(|t| !t.trim().is_empty()) {
            match crate::telegram::spawn(Some(token), Vec::new(), Some(ui_tx.clone())) {
                Ok(h) => {
                    app.tg = Some(h);
                    app.messages.push(super::ChatMessage::notice(
                        "🤖 Telegram enregistré et démarré. `/exec on` reste nécessaire pour le shell.",
                    ));
                }
                Err(e) => app.messages.push(super::ChatMessage::notice(format!(
                    "⚠ Telegram non démarré : {e}"
                ))),
            }
        }
    }

    app.messages.push(super::ChatMessage::notice(
        "✅ Configuration terminée.\n\
         Modifiables ensuite : /provider · /model · /role · /route · /telegram · /vault (CLI)\n\
         Aide : /help (ou /help all).",
    ));
}

fn fetch_models(app: &mut App, ui_tx: &UnboundedSender<UiEvent>) {
    if let Some(w) = app.wizard.as_mut() {
        w.loading = true;
        w.models.clear();
    }
    let settings = config::resolve();
    let ui = ui_tx.clone();
    tokio::spawn(async move {
        let list = match providers::find(&settings.provider_id) {
            Some(p) => crate::models::list(
                &crate::http_client::HTTP_CLIENT,
                &p,
                settings.api_key.as_deref(),
            )
            .await
            .unwrap_or_default(),
            None => Vec::new(),
        };
        let _ = ui.send(UiEvent::WizardModels(list));
    });
}

/// Applique la liste de modèles reçue (appelé depuis `handle_ui`).
pub fn on_models(app: &mut App, list: Vec<ModelEntry>) {
    if let Some(w) = app.wizard.as_mut() {
        w.models = list;
        w.loading = false;
        w.cursor = 0;
    }
}

fn provider_ids() -> Vec<String> {
    providers::all().into_iter().map(|p| p.id).collect()
}

fn choices(step: Step) -> &'static [&'static str] {
    match step {
        Step::Inference => &["Cloud (provider OpenAI-compatible)", "Local (llama.cpp)"],
        Step::Telegram => &["Non", "Plus tard", "Oui"],
        Step::Thinking => &["Désactivé (rapide)", "Activé (tâches difficiles)"],
        Step::Exec => &["Désactivé (recommandé)", "Activé — j'assume les risques"],
        _ => &[],
    }
}

fn explanatory(step: Step) -> &'static [&'static str] {
    match step {
        Step::Inference => &[
            "ZER0|GH0ST ne calcule rien lui-même : il parle à un modèle.",
            "",
            "• Cloud : aucun téléchargement, une clé API suffit, modèles puissants.",
            "  Les prompts partent chez le provider (voir SAFETY.md).",
            "• Local : rien ne sort de la machine, mais il faut llama-server + un GGUF.",
            "  Plus lent sur un vieux CPU, aucune clé, aucune limite de quota.",
        ],
        Step::Provider => &[
            "Le provider est le service qui héberge le modèle.",
            "",
            "• openrouter : une clé, beaucoup de modèles (dont des gratuits).",
            "• google / nvidia / moonshot / xai : chacun sa clé, quotas gratuits variables.",
            "• local : llama-server sur 127.0.0.1:8080, aucune clé.",
            "",
            "Si zer0-gw tourne, la clé y sera déposée (l'agent ne la stocke pas).",
        ],
        Step::ApiKey => &[
            "La clé est un secret : elle est saisie masquée et n'apparaît jamais",
            "dans l'historique ni dans le journal d'audit.",
            "",
            "Elle est déposée sur zer0-gw si la passerelle répond (écriture seule),",
            "sinon enregistrée localement (config 0600) avec un avertissement.",
            "",
            "Alternative : Échap, puis `export <PROVIDER>_API_KEY=…` et relance.",
        ],
        Step::Model => &[
            "Le modèle se choisit selon la tâche. Les modèles `:free` sont marqués.",
            "",
            "• un gros modèle raisonne mieux mais coûte/quota plus,",
            "• un petit modèle est rapide et suffit pour du texte simple.",
            "",
            "Tu pourras changer à tout moment : /model (menu), /free (gratuits).",
        ],
        Step::Telegram => &[
            "Telegram permet de piloter l'agent depuis ton téléphone (texte, vocal).",
            "",
            "• Ce n'est pas nécessaire pour utiliser la TUI.",
            "• Le bot ne reçoit que ce que tu lui écris ; le shell reste désactivé",
            "  tant que tu n'envoies pas `/exec on` (et il est refusé sur un chat",
            "  lié automatiquement).",
        ],
        Step::TelegramToken => &[
            "Crée un bot avec @BotFather, puis colle ici le jeton (masqué).",
            "",
            "Il sera enregistré et le bot démarrera à la fin de l'assistant.",
        ],
        Step::Thinking => &[
            "Le raisonnement (`think`) fait produire au modèle une réflexion interne",
            "avant la réponse.",
            "",
            "• Avantage : meilleur sur les tâches complexes (code, maths, plans).",
            "• Coût : plus lent, plus de tokens, plus de batterie/CPU en local.",
        ],
        Step::Exec => &[
            "`code_exec` exécute de vraies commandes shell, avec TES droits.",
            "",
            "• Activé : l'agent peut installer, compiler, tester, corriger — c'est là",
            "  qu'il devient vraiment utile. Chaque commande est journalisée (audit)",
            "  et les fichiers écrasés sont sauvegardés en `.zer0.bak`.",
            "• Mais : pas de bac à sable. Un `rm -rf`, un `git push --force`, un",
            "  `dd` sur le mauvais disque passent. C'est TOI le responsable.",
            "",
            "Désactivé : lecture/écriture de fichiers et réflexion, sans shell.",
            "Tu pourras l'activer plus tard avec F4 ou /codeexec.",
        ],
        Step::Commands => command_pages(),
        Step::Done => &["Configuration terminée."],
    }
}

fn command_pages() -> &'static [&'static str] {
    COMMAND_TOUR
}

const COMMAND_TOUR: &[&str] = &[
    "Les commandes essentielles — tape `/` pour la palette filtrante.",
    "",
    "/help              cette aide (les commandes essentielles)",
    "/help all          toutes les commandes, sans exception",
    "/setup             relancer cet assistant",
    "/provider          choisir un provider (menu)",
    "/login /logout     assistant connexion / retirer la clé",
    "/model             choisir un modèle (menu)",
    "/free              modèles gratuits du provider (menu)",
    "/role              rôles ; /role set <rôle> ouvre un menu",
    "/route             chaîne de bascule en cas d'échec",
    "---- page suivante ----",
    "/think             activer/désactiver le raisonnement",
    "/codeexec          activer/désactiver le shell (F4)",
    "/readwrite         autoriser l'écriture de fichiers (F3)",
    "/img <chemin>      joindre une image au prochain message",
    "/kb                base de connaissance locale",
    "/memory            mémoire persistante",
    "/task              liste de tâches",
    "/telegram          démarrer/arrêter le bot",
    "---- page suivante ----",
    "Session : /new /chats /compact /stop /quit",
    "Outils :  /tools (liste), Ctrl+O (déplier une sortie)",
    "Avancé : /quota /pdf /apis /embeddings /vault(CLI) /audit /0box /select",
    "",
    "Touches : F2 menu · F3 écriture · F4 shell · F5 aide · F6 sessions",
    "          F7 relance · F8 0chan · F10 micro · Ctrl+O déplier",
    "",
    "Entrée pour terminer l'assistant.",
];

/// Gère une touche quand l'assistant est ouvert.
pub fn handle_key(key: KeyEvent, app: &mut App, ui_tx: &UnboundedSender<UiEvent>) {
    let Some(w) = app.wizard.as_ref() else {
        return;
    };
    let step = w.step;

    match key.code {
        KeyCode::Esc => {
            save_step(step);
            app.wizard = None;
            app.secret_mode = false;
            app.input.clear();
            app.cursor = 0;
            app.messages.push(super::ChatMessage::notice(
                "Assistant suspendu. `/setup` pour reprendre.",
            ));
            return;
        }
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            save_step(step);
            app.wizard = None;
            app.secret_mode = false;
            return;
        }
        _ => {}
    }

    if step.needs_text() {
        match key.code {
            KeyCode::Char(c) => super::input_insert(app, c),
            KeyCode::Backspace => super::input_backspace(app),
            KeyCode::Delete => super::input_delete(app),
            KeyCode::Left => app.cursor = app.cursor.saturating_sub(1),
            KeyCode::Right => {
                app.cursor = (app.cursor + 1).min(app.input.chars().count());
            }
            KeyCode::Enter => activate(app, ui_tx),
            _ => {}
        }
        return;
    }

    match key.code {
        KeyCode::Up => {
            if let Some(w) = app.wizard.as_mut() {
                w.cursor = w.cursor.saturating_sub(1);
            }
        }
        KeyCode::Down => {
            if let Some(w) = app.wizard.as_mut() {
                w.cursor = w.cursor.saturating_add(1);
            }
        }
        KeyCode::Enter => activate(app, ui_tx),
        _ => {}
    }
}

fn activate(app: &mut App, ui_tx: &UnboundedSender<UiEvent>) {
    let Some(w) = app.wizard.as_ref() else { return };
    let step = w.step;
    let cursor = w.cursor;
    let provider = w.provider.clone();
    let input = app.input.trim().to_string();

    let mut next: Option<Step> = None;
    let mut clear_input = false;
    let mut secret = false;
    let mut fetch = false;

    match step {
        Step::Inference => {
            if cursor == 0 {
                next = Some(Step::Provider);
            } else {
                // local : provider "local", pas de clé
                if let Some(w) = app.wizard.as_mut() {
                    w.provider = Some("local".to_string());
                }
                let _ = crate::commands::provider_use("local");
                next = Some(Step::Model);
                fetch = true;
            }
        }
        Step::Provider => {
            let ids = provider_ids();
            let id = ids.get(cursor).cloned().unwrap_or_default();
            if id.is_empty() {
                return;
            }
            match crate::commands::provider_use(&id) {
                Ok(_) => {
                    app.reload_settings();
                    if let Some(w) = app.wizard.as_mut() {
                        w.provider = Some(id.clone());
                        w.cursor = 0;
                    }
                    if id == "local" {
                        next = Some(Step::Model);
                        fetch = true;
                    } else {
                        next = Some(Step::ApiKey);
                        clear_input = true;
                        secret = true;
                    }
                }
                Err(e) => set_error(app, e.to_string()),
            }
        }
        Step::ApiKey => {
            if input.is_empty() {
                set_error(
                    app,
                    "clé vide — colle la clé ou Échap pour plus tard".into(),
                );
                return;
            }
            let Some(id) = provider else {
                set_error(app, "provider inconnu".into());
                return;
            };
            match crate::commands::provider_add(&id, Some(&input)) {
                Ok(_) => {
                    app.reload_settings();
                    next = Some(Step::Model);
                    clear_input = true;
                    secret = false;
                    fetch = true;
                }
                Err(e) => set_error(app, e.to_string()),
            }
        }
        Step::Model => {
            let w = app.wizard.as_ref().unwrap();
            if w.loading {
                return;
            }
            let selected = w
                .models
                .get(cursor)
                .map(|m| m.id.clone())
                .filter(|id| !id.is_empty());
            let chosen = if let Some(m) = selected {
                // modèle déjà préfixé par le provider dans la liste ? non : id nu
                m
            } else if !input.is_empty() {
                input.clone()
            } else {
                set_error(app, "choisis un modèle (↑↓) ou tape son identifiant".into());
                return;
            };
            match crate::commands::set_model(&chosen) {
                Ok(_) => {
                    app.model = chosen;
                    next = Some(Step::Telegram);
                    clear_input = true;
                    if let Some(w) = app.wizard.as_mut() {
                        w.cursor = 0;
                    }
                }
                Err(e) => set_error(app, e.to_string()),
            }
        }
        Step::Telegram => match cursor {
            0 => {
                if let Some(w) = app.wizard.as_mut() {
                    w.telegram = Some(false);
                }
                next = Some(Step::Thinking);
            }
            1 => {
                if let Some(w) = app.wizard.as_mut() {
                    w.telegram = Some(false);
                }
                next = Some(Step::Thinking);
            }
            _ => {
                if let Some(w) = app.wizard.as_mut() {
                    w.telegram = Some(true);
                    w.cursor = 0;
                }
                next = Some(Step::TelegramToken);
                clear_input = true;
                secret = true;
            }
        },
        Step::TelegramToken => {
            if input.is_empty() {
                set_error(app, "jeton vide — colle-le ou choisis « Non »".into());
                return;
            }
            let mut g = config::load_global();
            g.telegram_token = Some(input.clone());
            if let Err(e) = config::save_global(&g) {
                set_error(app, e.to_string());
                return;
            }
            next = Some(Step::Thinking);
            clear_input = true;
            secret = false;
        }
        Step::Thinking => {
            app.thinking = cursor == 1;
            let mut g = config::load_global();
            g.thinking = app.thinking;
            let _ = config::save_global(&g);
            if let Some(w) = app.wizard.as_mut() {
                w.thinking = app.thinking;
                w.cursor = 0;
            }
            next = Some(Step::Exec);
        }
        Step::Exec => {
            app.code_exec = cursor == 1;
            let mut g = config::load_global();
            g.code_exec = app.code_exec;
            let _ = config::save_global(&g);
            app.refresh_runtime();
            if let Some(w) = app.wizard.as_mut() {
                w.exec = app.code_exec;
                w.cursor = 0;
            }
            next = Some(Step::Commands);
        }
        Step::Commands => {
            let pages = COMMAND_TOUR
                .iter()
                .filter(|l| l.starts_with("----"))
                .count()
                + 1;
            let w = app.wizard.as_mut().unwrap();
            if w.page + 1 >= pages {
                // Dernière page : on termine directement (pas d'écran vide).
                finish(app, ui_tx);
                return;
            } else {
                w.page += 1;
            }
        }
        Step::Done => {
            finish(app, ui_tx);
            return;
        }
    }

    if clear_input {
        app.input.clear();
        app.cursor = 0;
    }
    app.secret_mode = secret;
    if let Some(n) = next {
        save_step(n);
        if let Some(w) = app.wizard.as_mut() {
            w.step = n;
            w.error = None;
            if n == Step::Provider {
                w.cursor = 0;
            }
        }
        if fetch {
            fetch_models(app, ui_tx);
        }
    }
}

fn set_error(app: &mut App, msg: String) {
    if let Some(w) = app.wizard.as_mut() {
        w.error = Some(msg);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Rendu
// ─────────────────────────────────────────────────────────────────────────────

pub fn draw(f: &mut Frame, area: Rect, app: &App) {
    let Some(w) = app.wizard.as_ref() else {
        draw_error(f, area, "assistant indisponible");
        return;
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::ACCENT))
        .title(format!(" ZER0|GH0ST — {} ", w.step.title()));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut lines: Vec<Line> = Vec::new();
    for l in explanatory(w.step) {
        lines.push(Line::from(Span::styled(
            l.to_string(),
            Style::default().fg(theme::TEXT),
        )));
    }
    lines.push(Line::from(""));

    match w.step {
        Step::Provider => {
            let list = providers::all();
            for (i, p) in list.iter().enumerate() {
                lines.push(item_line(i == w.cursor, &format!("{} — {}", p.id, p.name)));
            }
        }
        Step::Model => {
            if w.loading {
                lines.push(Line::from(Span::styled(
                    "⏳ récupération de la liste des modèles…",
                    Style::default().fg(theme::DIM),
                )));
            } else if w.models.is_empty() {
                lines.push(Line::from(Span::styled(
                    "Liste indisponible — tape l'identifiant du modèle :",
                    Style::default().fg(theme::DIM),
                )));
                lines.push(Line::from(format!(
                    "  {}",
                    mask(&app.input, app.secret_mode)
                )));
            } else {
                for (i, m) in w.models.iter().take(14).enumerate() {
                    let free = if crate::models::is_free(m) {
                        " :free"
                    } else {
                        ""
                    };
                    let ctx = m
                        .context_length
                        .map(|c| format!("  ctx {c}"))
                        .unwrap_or_default();
                    lines.push(item_line(
                        i == w.cursor,
                        &format!("{}{}{}", m.id, free, ctx),
                    ));
                }
                if w.models.len() > 14 {
                    lines.push(Line::from(Span::styled(
                        format!(
                            "  … {} autres (tape l'id pour filtrer)",
                            w.models.len() - 14
                        ),
                        Style::default().fg(theme::DIM),
                    )));
                }
            }
        }
        Step::Telegram | Step::Thinking | Step::Exec | Step::Inference => {
            for (i, c) in choices(w.step).iter().enumerate() {
                lines.push(item_line(i == w.cursor, c));
            }
        }
        Step::ApiKey | Step::TelegramToken => {
            lines.push(Line::from(Span::styled(
                format!("  {}", mask(&app.input, true)),
                Style::default().fg(theme::CODE),
            )));
        }
        Step::Commands => lines.extend(command_page(w.page, inner.width as usize)),
        Step::Done => {}
    }

    if let Some(err) = &w.error {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!("⚠ {err}"),
            Style::default()
                .fg(theme::ACCENT)
                .add_modifier(Modifier::BOLD),
        )));
    }

    let body = Paragraph::new(lines).wrap(Wrap { trim: false });
    f.render_widget(body, inner);

    let footer = " ↑↓ choisir · Entrée valider · Échap reprendre plus tard ";
    let farea = Rect {
        x: area.x,
        y: area.y + area.height.saturating_sub(1),
        width: area.width,
        height: 1,
    };
    f.render_widget(
        Paragraph::new(Span::styled(footer, Style::default().fg(theme::DIM)))
            .alignment(Alignment::Center),
        farea,
    );
}

fn draw_error(f: &mut Frame, area: Rect, msg: &str) {
    f.render_widget(Paragraph::new(msg), area);
}

fn item_line(selected: bool, text: &str) -> Line<'static> {
    let style = if selected {
        Style::default()
            .fg(theme::ACCENT)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT)
    };
    Line::from(Span::styled(
        format!("{} {}", if selected { "▶" } else { " " }, text),
        style,
    ))
}

fn mask(text: &str, secret: bool) -> String {
    if secret {
        "●".repeat(text.chars().count())
    } else {
        text.to_string()
    }
}

fn command_page(page: usize, _width: usize) -> Vec<Line<'static>> {
    // Découpe le tour des commandes en pages séparées par « ---- page suivante ---- ».
    let mut pages: Vec<Vec<&str>> = vec![Vec::new()];
    for l in COMMAND_TOUR {
        if l.starts_with("----") {
            pages.push(Vec::new());
        } else {
            pages.last_mut().unwrap().push(l);
        }
    }
    pages
        .get(page)
        .map(|p| {
            p.iter()
                .map(|l| {
                    Line::from(Span::styled(
                        (*l).to_string(),
                        Style::default().fg(theme::TEXT),
                    ))
                })
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn step_indices_roundtrip() {
        // `TelegramToken` est une sous-étape : elle ne doit jamais être persistée.
        for i in 0..=8u8 {
            let step = Step::from_idx(i);
            assert_ne!(step, Step::TelegramToken);
            assert_eq!(step.idx(), i, "étape {i} mal mappée");
        }
        assert_eq!(Step::TelegramToken.idx(), Step::Telegram.idx());
    }

    #[test]
    fn only_secret_steps_accept_text() {
        assert!(Step::ApiKey.needs_text());
        assert!(Step::TelegramToken.needs_text());
        for s in [
            Step::Inference,
            Step::Provider,
            Step::Model,
            Step::Telegram,
            Step::Thinking,
            Step::Exec,
            Step::Commands,
            Step::Done,
        ] {
            assert!(
                !s.needs_text(),
                "{s:?} ne doit pas être une saisie de texte"
            );
        }
    }

    #[test]
    fn command_tour_has_three_pages() {
        let pages = COMMAND_TOUR
            .iter()
            .filter(|l| l.starts_with("----"))
            .count()
            + 1;
        assert_eq!(pages, 3);
        for p in 0..pages {
            assert!(!command_page(p, 80).is_empty(), "page {p} vide");
        }
        assert!(command_page(99, 80).is_empty());
    }
}
