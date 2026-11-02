//! Mod « zer0CL » : un shell dans le panneau latéral.
//!
//! Aucune dépendance : on réutilise le shell de la plateforme
//! (`platform::shell_command`), celui-là même que `code_exec` utilise. Sous
//! Windows, le shell hérité du lancement est préféré (PowerShell si zer0 a été
//! lancé depuis PowerShell, bash si lancé depuis Git Bash).
//!
//! Sécurité : les commandes sont **tapées par l'utilisateur**, jamais générées
//! par le modèle. Elles tournent avec ses droits, comme `code_exec`.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Wrap};
use ratatui::Frame;

const MAX_LINES: usize = 600;
const MAX_LINE_CHARS: usize = 2_000;

const PHOSPHOR: Color = Color::Rgb(0, 255, 154);
const AMBER: Color = Color::Rgb(255, 176, 0);
const DIM: Color = Color::Rgb(86, 140, 110);
const DARK: Color = Color::Rgb(6, 18, 10);

pub struct Term {
    pub cwd: PathBuf,
    pub input: String,
    history: Vec<String>,
    hist_pos: Option<usize>,
    lines: Arc<Mutex<Vec<String>>>,
    running: Arc<AtomicBool>,
    scroll: usize,
}

impl Term {
    pub fn new() -> Self {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let t = Self {
            cwd,
            input: String::new(),
            history: Vec::new(),
            hist_pos: None,
            lines: Arc::new(Mutex::new(Vec::new())),
            running: Arc::new(AtomicBool::new(false)),
            scroll: 0,
        };
        let shell = crate::platform::shell_label();
        t.push_line(format!("zer0CL — shell : {shell}"));
        t.push_line("Tape une commande, Entrée pour l'exécuter. Esc ferme.".to_string());
        t
    }

    fn push_line(&self, line: String) {
        let line = if line.chars().count() > MAX_LINE_CHARS {
            line.chars().take(MAX_LINE_CHARS).collect::<String>() + "…"
        } else {
            line
        };
        if let Ok(mut v) = self.lines.lock() {
            v.push(line);
            if v.len() > MAX_LINES {
                let drop = v.len() - MAX_LINES;
                v.drain(0..drop);
            }
        }
    }

    pub fn prompt(&self) -> String {
        format!("{} $ ", crate::fun::file::shorten_home(&self.cwd))
    }

    pub fn push_char(&mut self, c: char) {
        self.input.push(c);
    }

    pub fn backspace(&mut self) {
        self.input.pop();
    }

    pub fn history_prev(&mut self) {
        if self.history.is_empty() {
            return;
        }
        let pos = match self.hist_pos {
            None => self.history.len() - 1,
            Some(0) => 0,
            Some(p) => p - 1,
        };
        self.hist_pos = Some(pos);
        self.input = self.history[pos].clone();
    }

    pub fn history_next(&mut self) {
        match self.hist_pos {
            None => {}
            Some(p) if p + 1 < self.history.len() => {
                self.hist_pos = Some(p + 1);
                self.input = self.history[p + 1].clone();
            }
            Some(_) => {
                self.hist_pos = None;
                self.input.clear();
            }
        }
    }

    pub fn scroll_up(&mut self, n: usize) {
        let max = self.lines.lock().map(|v| v.len()).unwrap_or(0);
        self.scroll = (self.scroll + n).min(max);
    }

    pub fn scroll_down(&mut self, n: usize) {
        self.scroll = self.scroll.saturating_sub(n);
    }

    pub fn running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }

    /// Exécute la ligne courante (ou `cd`/`clear`).
    pub fn run(&mut self) {
        let line = self.input.trim().to_string();
        if line.is_empty() {
            return;
        }
        self.input.clear();
        self.hist_pos = None;
        if self.history.last() != Some(&line) {
            self.history.push(line.clone());
        }

        if line == "clear" {
            if let Ok(mut v) = self.lines.lock() {
                v.clear();
            }
            self.scroll = 0;
            return;
        }
        if let Some(arg) =
            line.strip_prefix("cd ")
                .or_else(|| if line == "cd" { Some("") } else { None })
        {
            let target = expand_tilde(arg.trim());
            let path = if target.is_empty() {
                PathBuf::from(crate::platform::home())
            } else if Path::new(&target).is_absolute() {
                PathBuf::from(&target)
            } else {
                self.cwd.join(&target)
            };
            if path.is_dir() {
                self.cwd = path.canonicalize().unwrap_or(path);
                self.push_line(format!("→ {}", self.cwd.display()));
            } else {
                self.push_line(format!("cd: dossier introuvable : {}", path.display()));
            }
            return;
        }

        self.push_line(format!("{}{}", self.prompt(), line));
        self.scroll = 0;
        self.running.store(true, Ordering::Relaxed);

        let mut cmd = tokio::process::Command::from(crate::platform::shell_command(&line));
        cmd.current_dir(&self.cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let lines = self.lines.clone();
        let running = self.running.clone();
        tokio::spawn(async move {
            let mut child = match cmd.spawn() {
                Ok(c) => c,
                Err(e) => {
                    push_to(&lines, format!("échec du lancement : {e}"));
                    running.store(false, Ordering::Relaxed);
                    return;
                }
            };
            // stdout et stderr sont deux types distincts : on les efface
            // derrière `dyn AsyncRead` pour les traiter dans la même boucle.
            let mut pipes: Vec<Box<dyn tokio::io::AsyncRead + Unpin + Send>> = Vec::new();
            if let Some(o) = child.stdout.take() {
                pipes.push(Box::new(o));
            }
            if let Some(e) = child.stderr.take() {
                pipes.push(Box::new(e));
            }
            let mut readers = Vec::new();
            for pipe in pipes {
                let lines = lines.clone();
                readers.push(tokio::spawn(async move {
                    use tokio::io::{AsyncBufReadExt, BufReader};
                    let mut it = BufReader::new(pipe).lines();
                    while let Ok(Some(l)) = it.next_line().await {
                        push_to(&lines, l);
                    }
                }));
            }
            let status = child.wait().await;
            for r in readers {
                let _ = r.await;
            }
            let code = status
                .ok()
                .and_then(|s| s.code())
                .map(|c| format!("[exit {c}]"))
                .unwrap_or_else(|| "[terminé]".to_string());
            push_to(&lines, code);
            running.store(false, Ordering::Relaxed);
        });
    }

    pub fn draw(&mut self, f: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Double)
            .border_style(Style::default().fg(PHOSPHOR))
            .style(Style::default().bg(DARK))
            .title(Span::styled(
                " ▓ zer0CL ",
                Style::default().fg(AMBER).add_modifier(Modifier::BOLD),
            ));
        let inner = block.inner(area);
        f.render_widget(block, area);
        if inner.height < 3 || inner.width < 8 {
            return;
        }

        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(1),
                Constraint::Length(1),
                Constraint::Length(1),
            ])
            .split(inner);

        // Sortie : on garde les dernières lignes, moins le décalage de molette.
        let all: Vec<String> = self.lines.lock().map(|v| v.clone()).unwrap_or_default();
        let h = rows[0].height as usize;
        let total = all.len();
        let end = total.saturating_sub(self.scroll.min(total));
        let start = end.saturating_sub(h);
        let lines: Vec<Line> = all[start..end]
            .iter()
            .map(|l| {
                let color = if l.starts_with("[exit") || l.starts_with("[terminé") {
                    DIM
                } else if l.starts_with("échec") || l.starts_with("cd:") {
                    AMBER
                } else {
                    PHOSPHOR
                };
                Line::from(Span::styled(l.clone(), Style::default().fg(color)))
            })
            .collect();
        f.render_widget(
            Paragraph::new(lines)
                .style(Style::default().bg(DARK))
                .wrap(Wrap { trim: false }),
            rows[0],
        );

        // Saisie.
        let cursor = Span::styled(
            "▏",
            Style::default()
                .fg(PHOSPHOR)
                .add_modifier(Modifier::SLOW_BLINK),
        );
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(self.prompt(), Style::default().fg(AMBER)),
                Span::styled(self.input.clone(), Style::default().fg(PHOSPHOR)),
                cursor,
            ])),
            rows[1],
        );

        let state = if self.running() { "⏳ " } else { "" };
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!(" {state}↵ exécuter · ↑↓ historique · ⇥ chat · Esc fermer "),
                Style::default().fg(DIM),
            ))),
            rows[2],
        );
    }
}

impl Default for Term {
    fn default() -> Self {
        Self::new()
    }
}

fn push_to(lines: &Arc<Mutex<Vec<String>>>, line: String) {
    let line = if line.chars().count() > MAX_LINE_CHARS {
        line.chars().take(MAX_LINE_CHARS).collect::<String>() + "…"
    } else {
        line
    };
    if let Ok(mut v) = lines.lock() {
        v.push(line);
        if v.len() > MAX_LINES {
            let drop = v.len() - MAX_LINES;
            v.drain(0..drop);
        }
    }
}

fn expand_tilde(arg: &str) -> String {
    if arg == "~" {
        return crate::platform::home();
    }
    if let Some(rest) = arg.strip_prefix("~/") {
        return format!("{}/{}", crate::platform::home(), rest);
    }
    arg.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn history_walks_back_and_forth() {
        let mut t = Term::new();
        for cmd in ["a", "b", "c"] {
            t.input = cmd.to_string();
            t.run();
        }
        t.history_prev();
        assert_eq!(t.input, "c");
        t.history_prev();
        assert_eq!(t.input, "b");
        t.history_next();
        assert_eq!(t.input, "c");
        t.history_next();
        assert!(t.input.is_empty());
    }

    #[test]
    fn cd_changes_the_working_directory() {
        let mut t = Term::new();
        t.input = format!("cd {}", std::env::temp_dir().display());
        t.run();
        assert_eq!(t.cwd, std::env::temp_dir().canonicalize().unwrap());
        assert!(t.input.is_empty());
    }

    #[test]
    fn clear_empties_the_output() {
        let mut t = Term::new();
        t.push_line("bruit".into());
        t.input = "clear".into();
        t.run();
        assert!(t.lines.lock().unwrap().is_empty());
    }
}
