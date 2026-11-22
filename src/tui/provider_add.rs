//! Overlay `/provider add` — ajout d'un provider sans taper la ligne de commande.
//!
//! Deux chemins :
//!  • **préconfiguré** : on choisit dans le catalogue, il ne reste que la clé ;
//!  • **custom** : nom, base URL, puis clé.
//!
//! Dans les deux cas, une confirmation explicite précède le dépôt. La clé part
//! **uniquement** vers zer0-gw (`GwRoute::Gw`) : si la passerelle est
//! injoignable ou si l'utilisateur refuse, rien n'est écrit — jamais de repli
//! silencieux vers un stockage local.
//!
//! La clé n'est jamais journalisée ni ajoutée à l'historique d'entrée : la
//! commande est interceptée avant `send_message`.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;
use tokio::sync::mpsc::UnboundedSender;

use crate::event::UiEvent;
use crate::providers;
use crate::theme;

use super::App;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// Choix dans le catalogue, ou « Custom ».
    Choose,
    /// Clé API d'un provider préconfiguré.
    Key,
    /// Nom d'affichage d'un provider custom.
    CustomName,
    /// Base URL d'un provider custom.
    CustomUrl,
    /// Clé API d'un provider custom.
    CustomKey,
    /// Consentement : déposer la clé dans zer0-gw ?
    Confirm,
}

impl Step {
    fn title(self) -> &'static str {
        match self {
            Step::Choose => "Provider — lequel ?",
            Step::Key => "Provider — clé API",
            Step::CustomName => "Custom 1/3 — Nom",
            Step::CustomUrl => "Custom 2/3 — Base URL",
            Step::CustomKey => "Custom 3/3 — Clé API",
            Step::Confirm => "Provider — confirmation",
        }
    }

    pub fn needs_text(self) -> bool {
        matches!(
            self,
            Step::Key | Step::CustomName | Step::CustomUrl | Step::CustomKey
        )
    }
}

pub struct ProviderAdd {
    pub step: Step,
    pub cursor: usize,
    /// Entrées du catalogue (hors `local`), plus « Custom » en dernier.
    pub choices: Vec<Choice>,
    /// Provider préconfiguré retenu.
    pub picked: Option<providers::Provider>,
    pub name: String,
    pub base_url: String,
    pub key: String,
    pub error: Option<String>,
    /// Consentement : `None` = pas encore répondu.
    pub consent: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct Choice {
    pub label: String,
    pub note: String,
    /// `None` = entrée « Custom ».
    pub provider: Option<providers::Provider>,
}

impl ProviderAdd {
    fn new() -> Self {
        let mut choices: Vec<Choice> = providers::all()
            .into_iter()
            // `local` n'a pas de clé : il n'a rien à faire ici.
            .filter(|p| p.id != "local" && p.id != "local-embed")
            .map(|p| Choice {
                label: format!("{} — {}", p.id, p.name),
                note: if p.custom {
                    "custom".to_string()
                } else if p.signup.is_empty() {
                    String::new()
                } else {
                    p.signup.clone()
                },
                provider: Some(p),
            })
            .collect();
        choices.push(Choice {
            label: "Custom — autre provider OpenAI-compatible".to_string(),
            note: "nom + base URL + clé".to_string(),
            provider: None,
        });
        Self {
            step: Step::Choose,
            cursor: 0,
            choices,
            picked: None,
            name: String::new(),
            base_url: String::new(),
            key: String::new(),
            error: None,
            consent: None,
        }
    }

    /// Identifiant technique dérivé du nom pour un provider custom.
    fn custom_id(&self) -> String {
        let mut id: String = self
            .name
            .trim()
            .to_ascii_lowercase()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect();
        while id.contains("--") {
            id = id.replace("--", "-");
        }
        id.trim_matches('-').to_string()
    }
}

/// Ouvre l'overlay.
pub fn start(app: &mut App) {
    app.provider_add = Some(ProviderAdd::new());
    app.input.clear();
    app.cursor = 0;
    app.secret_mode = false;
}

fn close(app: &mut App) {
    app.provider_add = None;
    app.secret_mode = false;
    app.input.clear();
    app.cursor = 0;
}

fn set_error(app: &mut App, msg: impl Into<String>) {
    if let Some(o) = app.provider_add.as_mut() {
        o.error = Some(msg.into());
    }
}

/// Gère une touche quand l'overlay est ouvert.
pub fn handle_key(key: KeyEvent, app: &mut App, ui_tx: &UnboundedSender<UiEvent>) {
    let Some(step) = app.provider_add.as_ref().map(|o| o.step) else {
        return;
    };

    match key.code {
        KeyCode::Esc => {
            close(app);
            app.messages.push(super::ChatMessage::notice(
                "Ajout de provider annulé — rien n'a été écrit.",
            ));
            return;
        }
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            close(app);
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
            if let Some(o) = app.provider_add.as_mut() {
                o.cursor = o.cursor.saturating_sub(1);
            }
        }
        KeyCode::Down => {
            if let Some(o) = app.provider_add.as_mut() {
                let max = o.choices.len().saturating_sub(1);
                o.cursor = (o.cursor + 1).min(max);
            }
        }
        KeyCode::Enter => activate(app, ui_tx),
        _ => {}
    }
}

fn activate(app: &mut App, ui_tx: &UnboundedSender<UiEvent>) {
    let Some(o) = app.provider_add.as_ref() else {
        return;
    };
    let step = o.step;
    let cursor = o.cursor;
    let input = app.input.trim().to_string();

    match step {
        Step::Choose => {
            let choice = match o.choices.get(cursor) {
                Some(c) => c.clone(),
                None => return,
            };
            match choice.provider {
                Some(p) => {
                    if let Some(o) = app.provider_add.as_mut() {
                        o.picked = Some(p);
                        o.step = Step::Key;
                        o.error = None;
                    }
                    app.input.clear();
                    app.cursor = 0;
                    app.secret_mode = true;
                }
                None => {
                    if let Some(o) = app.provider_add.as_mut() {
                        o.step = Step::CustomName;
                        o.error = None;
                    }
                    app.input.clear();
                    app.cursor = 0;
                    app.secret_mode = false;
                }
            }
        }
        Step::Key => {
            if input.is_empty() {
                set_error(app, "clé vide — colle-la, ou Échap pour annuler");
                return;
            }
            if let Some(o) = app.provider_add.as_mut() {
                o.key = input;
                o.step = Step::Confirm;
                o.error = None;
            }
            app.input.clear();
            app.cursor = 0;
            app.secret_mode = false;
        }
        Step::CustomName => {
            if input.is_empty() {
                set_error(app, "nom vide — ex. « Mon API »");
                return;
            }
            if let Some(o) = app.provider_add.as_mut() {
                o.name = input;
                o.step = Step::CustomUrl;
                o.error = None;
            }
            app.input.clear();
            app.cursor = 0;
        }
        Step::CustomUrl => {
            if !(input.starts_with("http://") || input.starts_with("https://")) {
                set_error(app, "URL invalide — http://… ou https://…");
                return;
            }
            if let Some(o) = app.provider_add.as_mut() {
                o.base_url = input;
                o.step = Step::CustomKey;
                o.error = None;
            }
            app.input.clear();
            app.cursor = 0;
            app.secret_mode = true;
        }
        Step::CustomKey => {
            if input.is_empty() {
                set_error(app, "clé vide — colle-la, ou Échap pour annuler");
                return;
            }
            if let Some(o) = app.provider_add.as_mut() {
                o.key = input;
                o.step = Step::Confirm;
                o.error = None;
            }
            app.input.clear();
            app.cursor = 0;
            app.secret_mode = false;
        }
        Step::Confirm => {
            // Entrée = consentir au dépôt dans la passerelle.
            submit(app, ui_tx);
        }
    }
}

/// Dépose le provider **via la passerelle uniquement**.
fn submit(app: &mut App, ui_tx: &UnboundedSender<UiEvent>) {
    let Some(o) = app.provider_add.as_ref() else {
        return;
    };
    let key = o.key.clone();
    let (id, base_url, name) = match &o.picked {
        Some(p) => (p.id.clone(), p.base_url.clone(), None),
        None => {
            let id = o.custom_id();
            if id.is_empty() {
                set_error(app, "nom invalide — utilise des lettres ou des chiffres");
                return;
            }
            if providers::builtins().iter().any(|p| p.id == id) {
                set_error(
                    app,
                    format!("« {id} » est déjà un provider intégré — choisis-le dans la liste"),
                );
                return;
            }
            (id, o.base_url.clone(), Some(o.name.clone()))
        }
    };

    if !crate::gwclient::probe() {
        set_error(
            app,
            "zer0-gw injoignable — la clé n'a PAS été enregistrée. \
             Démarre la passerelle (sudo systemctl start zer0-gw) puis réessaie.",
        );
        return;
    }

    let res = crate::commands::provider_add(crate::commands::ProviderAddArgs {
        id: &id,
        base_url: Some(&base_url),
        name: name.as_deref(),
        key: Some(&key),
        // Jamais de repli local : la clé doit rester dans la passerelle.
        route: crate::commands::GwRoute::Gw,
        force: false,
    });

    match res {
        Ok(lines) => {
            close(app);
            for line in lines {
                app.messages.push(super::ChatMessage::notice(line));
            }
            app.reload_settings();
            super::probe_context(app, ui_tx);
        }
        Err(e) => set_error(app, e.to_string()),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Rendu
// ─────────────────────────────────────────────────────────────────────────────

pub fn draw(f: &mut Frame, area: Rect, app: &App) {
    let Some(o) = app.provider_add.as_ref() else {
        return;
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::accent()))
        .title(format!(" ZER0|GH0ST — {} ", o.step.title()));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut lines: Vec<Line> = Vec::new();
    for l in explanatory(o.step) {
        lines.push(Line::from(Span::styled(
            l.to_string(),
            Style::default().fg(theme::text()),
        )));
    }
    lines.push(Line::from(""));

    match o.step {
        Step::Choose => {
            for (i, c) in o.choices.iter().enumerate() {
                let selected = i == o.cursor;
                let style = if selected {
                    Style::default()
                        .fg(theme::accent())
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(theme::text())
                };
                let mut spans = vec![Span::styled(
                    format!("{} {}", if selected { "▶" } else { " " }, c.label),
                    style,
                )];
                if !c.note.is_empty() {
                    spans.push(Span::styled(
                        format!("   {}", c.note),
                        Style::default().fg(theme::dim()),
                    ));
                }
                lines.push(Line::from(spans));
            }
        }
        Step::Key | Step::CustomName | Step::CustomUrl | Step::CustomKey => {
            let label = match o.step {
                Step::Key => "Clé API : ",
                Step::CustomName => "Nom : ",
                Step::CustomUrl => "Base URL : ",
                _ => "Clé API : ",
            };
            let placeholder = match o.step {
                Step::Key => " colle la clé, puis Entrée",
                Step::CustomName => " ex. Mon API, puis Entrée",
                Step::CustomUrl => " https://api.exemple.com/v1, puis Entrée",
                _ => " colle la clé, puis Entrée",
            };
            let mut spans = vec![Span::styled(
                format!("  {label}"),
                Style::default()
                    .fg(theme::accent())
                    .add_modifier(Modifier::BOLD),
            )];
            if app.input.is_empty() {
                spans.push(Span::styled(
                    "▏",
                    Style::default()
                        .fg(theme::accent())
                        .add_modifier(Modifier::SLOW_BLINK),
                ));
                spans.push(Span::styled(placeholder, Style::default().fg(theme::dim())));
            } else {
                let shown = if app.secret_mode {
                    "●".repeat(app.input.chars().count())
                } else {
                    app.input.clone()
                };
                spans.push(Span::styled(shown, Style::default().fg(theme::code())));
                spans.push(Span::styled("▏", Style::default().fg(theme::accent())));
            }
            lines.push(Line::from(spans));
        }
        Step::Confirm => {
            let kv = |k: &str, v: &str| {
                Line::from(vec![
                    Span::styled(format!("  {k:<12}"), Style::default().fg(theme::dim())),
                    Span::styled(v.to_string(), Style::default().fg(theme::code())),
                ])
            };
            match &o.picked {
                Some(p) => {
                    lines.push(kv("Provider", &format!("{} — {}", p.id, p.name)));
                    lines.push(kv("Base URL", &p.base_url));
                }
                None => {
                    lines.push(kv("Provider", &format!("{} (custom)", o.custom_id())));
                    lines.push(kv("Nom", &o.name));
                    lines.push(kv("Base URL", &o.base_url));
                }
            }
            lines.push(kv("Clé", &format!("{} caractères", o.key.chars().count())));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "  La clé sera déposée dans zer0-gw et n'entrera jamais dans la config de l'agent.",
                Style::default().fg(theme::dim()),
            )));
            lines.push(Line::from(Span::styled(
                "  Si la passerelle est injoignable, rien n'est écrit.",
                Style::default().fg(theme::dim()),
            )));
        }
    }

    if let Some(err) = &o.error {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!("⚠ {err}"),
            Style::default()
                .fg(theme::accent())
                .add_modifier(Modifier::BOLD),
        )));
    }

    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);

    let footer = match o.step {
        Step::Choose => " ↑↓ choisir · Entrée valider · Échap annuler ",
        Step::Key | Step::CustomKey => " Saisie masquée · Entrée valider · Échap annuler ",
        Step::CustomName | Step::CustomUrl => " Entrée valider · Échap annuler ",
        Step::Confirm => " Entrée : déposer dans zer0-gw · Échap : annuler ",
    };
    let farea = Rect {
        x: area.x,
        y: area.y + area.height.saturating_sub(1),
        width: area.width,
        height: 1,
    };
    f.render_widget(
        Paragraph::new(Span::styled(footer, Style::default().fg(theme::dim())))
            .alignment(Alignment::Center),
        farea,
    );
}

fn explanatory(step: Step) -> &'static [&'static str] {
    match step {
        Step::Choose => &[
            "Choisis un provider du catalogue, ou « Custom » s'il n'y figure pas.",
            "",
            "La clé ne sera jamais écrite dans la config de l'agent : elle part",
            "directement dans zer0-gw, qui la garde et signe les appels.",
        ],
        Step::Key => &[
            "Colle la clé API du provider sélectionné.",
            "",
            "Elle est masquée à la saisie et n'apparaît ni dans l'historique",
            "d'entrée, ni dans les journaux.",
        ],
        Step::CustomName => &[
            "Un nom lisible pour toi — il devient l'identifiant technique",
            "(minuscules, tirets).",
            "",
            "Exemple : « Mon API » → mon-api",
        ],
        Step::CustomUrl => &[
            "L'URL de base de l'API, préfixe de version compris.",
            "",
            "Exemple : https://api.exemple.com/v1",
            "Le chemin /chat/completions est ajouté par l'agent.",
        ],
        Step::CustomKey => &[
            "Colle la clé API de ce provider.",
            "",
            "Saisie masquée, jamais journalisée.",
        ],
        Step::Confirm => &[
            "Vérifie avant de déposer :",
            "",
            "Entrée = enregistrer le provider dans zer0-gw et tester la clé.",
            "Échap = annuler, rien n'est écrit.",
        ],
    }
}
