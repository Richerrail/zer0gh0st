//! Menu latéral de commandes (cliquable et défilable), entre le bloc zero-chan
//! et le badge 0chan/mic. Les groupes se déplient (`▸`/`▾`) ; une commande sans
//! argument s'exécute au clic, une commande avec argument s'insère dans la saisie.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

pub struct Node {
    pub label: &'static str,
    /// Commande exécutée/insérée. `None` = groupe (a des enfants).
    pub run: Option<&'static str>,
    pub children: &'static [Node],
}

#[derive(Clone, Copy)]
pub enum Hit {
    /// Déplier/replier un groupe.
    Group(&'static str),
    /// Exécuter (ou insérer) une commande.
    Cmd(&'static str),
}

const fn n(label: &'static str, run: &'static str) -> Node {
    Node { label, run: Some(run), children: &[] }
}
const fn g(label: &'static str, children: &'static [Node]) -> Node {
    Node { label, run: None, children }
}

/// Arbre complet des commandes. Un `run` finissant par un espace attend un argument.
pub const TREE: &[Node] = &[
    n("help", "/help"),
    n("tools", "/tools"),
    n("new", "/new"),
    n("clear", "/clear"),
    n("history", "/history"),
    n("compact", "/compact"),
    n("context", "/context"),
    n("think", "/think"),
    n("work", "/work"),
    n("stop", "/stop"),
    n("quit", "/quit"),
    g("provider", &[
        n("list", "/provider list"),
        n("add", "/provider add "),
        n("use", "/provider use "),
        n("remove", "/provider remove "),
    ]),
    g("model", &[
        n("status", "/model"),
        n("use", "/model use "),
        n("list", "/models"),
        n("free", "/free"),
    ]),
    g("route", &[
        n("list", "/route list"),
        n("add", "/route add "),
        n("auto", "/route auto"),
        n("clear", "/route clear"),
    ]),
    g("role", &[
        n("list", "/role list"),
        n("set", "/role set "),
        n("use", "/role use "),
        n("auto", "/role auto"),
        n("unset", "/role unset "),
    ]),
    g("ctx", &[
        n("status", "/ctx"),
        n("set", "/ctx "),
        n("auto", "/ctx auto"),
    ]),
    g("memory", &[
        n("status", "/memory"),
        n("remember", "/remember "),
        n("recall", "/recall "),
        n("forget", "/forget "),
    ]),
    g("task", &[
        n("list", "/task list"),
        n("add", "/task add "),
        n("done", "/task done "),
        n("claim", "/task claim"),
        n("update", "/task update "),
    ]),
    g("kb", &[
        n("stats", "/kb stats"),
        n("search", "/kb search "),
        n("ingest", "/kb ingest"),
        n("rebuild", "/kb rebuild"),
        n("auto", "/kb auto"),
    ]),
    g("project", &[
        n("status", "/project"),
        n("set", "/project "),
    ]),
    g("whoami", &[
        n("status", "/whoami"),
        n("off", "/whoami off"),
    ]),
    g("vault", &[
        n("status", "/vault status"),
        n("lock", "/vault lock"),
    ]),
    g("pdf", &[
        n("info", "/pdf info "),
        n("text", "/pdf text "),
        n("merge", "/pdf merge "),
        n("split", "/pdf split "),
    ]),
    g("agents", &[
        n("ask", "/ask "),
        n("review", "/review "),
        n("jev", "/jev "),
        n("select", "/select "),
        n("search", "/search "),
    ]),
    g("telegram", &[
        n("on", "/telegram on"),
        n("off", "/telegram off"),
        n("status", "/telegram"),
    ]),
    g("mods", &[
        n("radio", "/radio"),
        n("bintime", "/bintime"),
        n("pong", "/pong"),
        n("kristal", "/kristal"),
        n("streaming", "/streaming"),
        n("retro", "/retro"),
        n("panel (fermer)", "/panel"),
    ]),
];

/// Dessine le menu et remplit `hits` (coordonnées écran). `scroll` est ajusté.
pub fn draw(
    f: &mut Frame,
    area: Rect,
    open: Option<&'static str>,
    scroll: &mut u16,
    hits: &mut Vec<(Rect, Hit)>,
) {
    hits.clear();
    if area.width < 10 || area.height < 3 {
        return;
    }

    // Construit toutes les lignes + l'action associée.
    let mut rows: Vec<(Line<'static>, Option<Hit>)> = Vec::new();
    for node in TREE {
        if node.children.is_empty() {
            rows.push((
                Line::from(Span::styled(
                    format!(" {}", node.label),
                    Style::default().fg(Color::Gray),
                )),
                node.run.map(Hit::Cmd),
            ));
        } else {
            let expanded = open == Some(node.label);
            let marker = if expanded { "▾" } else { "▸" };
            rows.push((
                Line::from(Span::styled(
                    format!(" {marker} {}", node.label),
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                )),
                Some(Hit::Group(node.label)),
            ));
            if expanded {
                for ch in node.children {
                    rows.push((
                        Line::from(Span::styled(
                            format!("     {}", ch.label),
                            Style::default().fg(Color::Gray),
                        )),
                        ch.run.map(Hit::Cmd),
                    ));
                }
            }
        }
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Rgb(110, 95, 140)))
        .title(Span::styled(
            " Commandes ",
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let max_scroll = rows.len().saturating_sub(inner.height as usize) as u16;
    *scroll = (*scroll).min(max_scroll);
    let start = *scroll as usize;
    let visible = rows.iter().skip(start).take(inner.height as usize);

    let mut lines: Vec<Line> = Vec::new();
    let mut y = inner.y;
    for (line, hit) in visible {
        if let Some(h) = hit {
            hits.push((Rect { x: inner.x, y, width: inner.width, height: 1 }, *h));
        }
        lines.push(line.clone());
        y += 1;
    }
    f.render_widget(Paragraph::new(lines), inner);
}
