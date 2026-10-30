//! Menu latéral de commandes (cliquable et défilable), entre le bloc zero-chan
//! et le badge 0chan/mic. Les groupes se déplient (`▸`/`▾`) ; une commande sans
//! argument s'exécute au clic, une commande avec argument s'insère dans la saisie.

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::theme;

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

/// Liste plate des commandes (commande, « groupe · libellé ») pour la palette `/`.
pub fn flat_commands() -> Vec<(String, String)> {
    fn walk(nodes: &'static [Node], group: Option<&str>, out: &mut Vec<(String, String)>) {
        for node in nodes {
            match node.run {
                Some(run) => {
                    let label = match group {
                        Some(g) => format!("{g} · {}", node.label),
                        None => node.label.to_string(),
                    };
                    out.push((run.to_string(), label));
                }
                None => walk(node.children, Some(node.label), out),
            }
        }
    }
    let mut out = Vec::new();
    walk(TREE, None, &mut out);
    out
}

const fn n(label: &'static str, run: &'static str) -> Node {
    Node {
        label,
        run: Some(run),
        children: &[],
    }
}
const fn g(label: &'static str, children: &'static [Node]) -> Node {
    Node {
        label,
        run: None,
        children,
    }
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
    n("menu", "/menu"),
    n("setup", "/setup"),
    n("codeexec", "/codeexec"),
    n("readwrite", "/readwrite"),
    n("stream", "/stream"),
    n("system", "/system"),
    n("nudge", "/nudge"),
    n("mic", "/mic"),
    n("0chan", "/0chan"),
    n("audit", "/audit"),
    n("chats", "/chats"),
    n("login", "/login"),
    n("logout", "/logout"),
    // Visibles directement (sans déplier le groupe « model »).
    n("model use", "/model use "),
    n("free", "/free"),
    n("quota", "/quota"),
    n("vote", "/vote"),
    n("theme", "/theme"),
    n("update", "/update"),
    g(
        "provider",
        &[
            n("sélection", "/provider"),
            n("list", "/provider list"),
            n("add", "/provider add "),
            n("use", "/provider use "),
            n("remove", "/provider remove "),
        ],
    ),
    g(
        "model",
        &[
            n("sélection", "/model"),
            n("list", "/models"),
            n("image", "/img "),
        ],
    ),
    g(
        "route",
        &[
            n("list", "/route list"),
            n("add", "/route add "),
            n("auto", "/route auto"),
            n("clear", "/route clear"),
        ],
    ),
    g(
        "role",
        &[
            n("list", "/role list"),
            n("off", "/role off"),
            n("on", "/role on"),
            n("set", "/role set "),
            n("use", "/role use "),
            n("auto", "/role auto"),
            n("unset", "/role unset "),
        ],
    ),
    g(
        "ctx",
        &[
            n("status", "/ctx"),
            n("set", "/ctx "),
            n("auto", "/ctx auto"),
        ],
    ),
    g(
        "memory",
        &[
            n("status", "/memory"),
            n("remember", "/remember "),
            n("recall", "/recall "),
            n("forget", "/forget "),
            n("embedder", "/embeddings"),
            n("set embedder", "/embeddings set "),
        ],
    ),
    g(
        "task",
        &[
            n("list", "/task list"),
            n("add", "/task add "),
            n("done", "/task done "),
            n("claim", "/task claim"),
            n("update", "/task update "),
            n("rm", "/task rm "),
            n("clear", "/task clear"),
        ],
    ),
    g(
        "kb",
        &[
            n("stats", "/kb stats"),
            n("search", "/kb search "),
            n("ingest", "/kb ingest"),
            n("rebuild", "/kb rebuild"),
            n("auto", "/kb auto"),
            n("apis", "/apis "),
        ],
    ),
    g("project", &[n("status", "/project"), n("set", "/project ")]),
    g("whoami", &[n("status", "/whoami"), n("off", "/whoami off")]),
    g(
        "vault",
        &[n("status", "/vault status"), n("lock", "/vault lock")],
    ),
    g(
        "pdf",
        &[
            n("info", "/pdf info "),
            n("text", "/pdf text "),
            n("merge", "/pdf merge "),
            n("split", "/pdf split "),
        ],
    ),
    g(
        "agents",
        &[
            n("ask", "/ask "),
            n("review", "/review "),
            n("jev", "/jev "),
            n("0box", "/0box "),
            n("select", "/select "),
            n("search", "/search "),
            n("jimmy", "/jimmy"),
            n("provoke", "/provoke "),
        ],
    ),
    g(
        "telegram",
        &[
            n("on", "/telegram on"),
            n("off", "/telegram off"),
            n("status", "/telegram"),
        ],
    ),
    g(
        "mods",
        &[
            n("radio", "/radio"),
            n("bintime", "/bintime"),
            n("t0p", "/t0p"),
            n("pong", "/pong"),
            n("panel (fermer)", "/panel"),
        ],
    ),
];

/// Dessine le menu et remplit `hits` (coordonnées écran). `scroll` est ajusté.
/// `sel` = index sélectionné au clavier (surbrillance).
pub fn draw(
    f: &mut Frame,
    area: Rect,
    open: Option<&'static str>,
    scroll: &mut u16,
    hits: &mut Vec<(Rect, Hit)>,
    sel: Option<usize>,
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
                    Style::default().fg(theme::text()),
                )),
                node.run.map(Hit::Cmd),
            ));
        } else {
            let expanded = open == Some(node.label);
            let marker = if expanded { "▾" } else { "▸" };
            rows.push((
                Line::from(Span::styled(
                    format!(" {marker} {}", node.label),
                    Style::default()
                        .fg(theme::highlight())
                        .add_modifier(Modifier::BOLD),
                )),
                Some(Hit::Group(node.label)),
            ));
            if expanded {
                for ch in node.children {
                    rows.push((
                        Line::from(Span::styled(
                            format!("     {}", ch.label),
                            Style::default().fg(theme::dim()),
                        )),
                        ch.run.map(Hit::Cmd),
                    ));
                }
            }
        }
    }

    let title = if sel.is_some() {
        " Commandes ↑↓ ↵ Esc "
    } else {
        " Commandes "
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::border()))
        .title(Span::styled(
            title,
            Style::default()
                .fg(theme::highlight())
                .add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let max_scroll = rows.len().saturating_sub(inner.height as usize) as u16;
    *scroll = (*scroll).min(max_scroll);
    // Suivre la sélection clavier (la garder visible).
    if let Some(s) = sel {
        let h = inner.height as usize;
        if s < *scroll as usize {
            *scroll = s as u16;
        } else if s >= *scroll as usize + h {
            *scroll = (s + 1 - h).min(max_scroll as usize) as u16;
        }
    }
    let start = *scroll as usize;
    let visible = rows.iter().skip(start).take(inner.height as usize);

    let mut lines: Vec<Line> = Vec::new();
    let mut i = 0usize;
    for (y, (line, hit)) in (inner.y..).zip(visible) {
        let mut line = line.clone();
        if let Some(h) = hit {
            hits.push((
                Rect {
                    x: inner.x,
                    y,
                    width: inner.width,
                    height: 1,
                },
                *h,
            ));
            if sel == Some(i) {
                line = line.style(Style::default().bg(theme::menu_selection_bg()));
            }
            i += 1;
        }
        lines.push(line);
    }
    f.render_widget(Paragraph::new(lines), inner);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Anti-dérive : toute commande gérée dans `mod.rs` doit être dans le menu.
    /// C'est ce test qui aurait empêché `/codeexec` et `/readwrite` d'être
    /// invisibles alors qu'ils arment les outils dangereux.
    #[test]
    fn every_tui_command_appears_in_the_menu() {
        fn flatten(nodes: &[Node], out: &mut Vec<String>) {
            for n in nodes {
                if let Some(run) = n.run {
                    if let Some(cmd) = run.split_whitespace().next() {
                        if cmd.starts_with('/') {
                            out.push(cmd.to_string());
                        }
                    }
                }
                flatten(n.children, out);
            }
        }
        let mut listed = Vec::new();
        flatten(TREE, &mut listed);

        let src = include_str!("mod.rs");
        let mut missing = Vec::new();
        for line in src.lines() {
            let t = line.trim_start();
            if !t.starts_with('"') || !t.contains("\" =>") {
                continue;
            }
            let Some(end) = t[1..].find('"') else {
                continue;
            };
            let cmd = &t[1..1 + end];
            if cmd.starts_with('/') && !listed.iter().any(|c| c == cmd) {
                missing.push(cmd.to_string());
            }
        }
        assert!(
            missing.is_empty(),
            "commandes absentes du menu : {missing:?}"
        );
    }

    #[test]
    fn flat_commands_is_non_empty_and_covers_model_provider_role() {
        let cmds = flat_commands();
        assert!(cmds.len() > 10, "palette trop pauvre : {}", cmds.len());
        assert!(cmds.iter().any(|(c, _)| c == "/model"));
        assert!(cmds.iter().any(|(c, _)| c == "/provider"));
        // présentes en direct dans le menu défilable
        assert!(cmds.iter().any(|(c, _)| c == "/model use "));
        assert!(cmds.iter().any(|(c, _)| c == "/free"));
        // les sous-commandes portent le groupe dans le libellé
        assert!(cmds
            .iter()
            .any(|(c, l)| c == "/role off" && l.contains('·')));
        // pas de doublon de commande
        let mut codes: Vec<&String> = cmds.iter().map(|(c, _)| c).collect();
        codes.sort();
        let before = codes.len();
        codes.dedup();
        assert_eq!(before, codes.len(), "commandes en double dans la palette");
    }
}
