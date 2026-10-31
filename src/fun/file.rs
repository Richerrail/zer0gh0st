//! Mod « 0file » : explorateur de dossiers rétro (lecture + navigation +
//! copie du chemin). Aucune écriture, aucune exécution.
//!
//! Style : phosphore vert / ambre, bordure double, comme un terminal des
//! années 80. Les dossiers sont `▸`, les fichiers `·`, la sélection est un
//! bloc inversé.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Wrap};
use ratatui::Frame;

const PHOSPHOR: Color = Color::Rgb(0, 255, 154);
const AMBER: Color = Color::Rgb(255, 176, 0);
const DIM: Color = Color::Rgb(86, 140, 110);
const DARK: Color = Color::Rgb(6, 18, 10);

/// Une ligne de la liste (`..` inclus).
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub size: u64,
    pub up: bool,
}

pub struct FileExplorer {
    pub cwd: PathBuf,
    entries: Vec<Entry>,
    sel: usize,
    scroll: usize,
    rows: usize,
    show_hidden: bool,
    status: Option<(String, Instant, bool)>,
}

impl FileExplorer {
    /// Ouvre un dossier, ou le dossier parent d'un fichier (et le sélectionne).
    pub fn open(arg: &str) -> Self {
        let arg = expand_tilde(arg.trim());
        let path = if arg.is_empty() {
            std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
        } else {
            PathBuf::from(&arg)
        };
        let (cwd, wanted) = if path.is_file() {
            (
                path.parent().map(Path::to_path_buf).unwrap_or(path.clone()),
                path.file_name().map(|n| n.to_string_lossy().to_string()),
            )
        } else if path.is_dir() {
            (path, None)
        } else {
            (
                std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
                None,
            )
        };
        let mut fe = Self {
            cwd,
            entries: Vec::new(),
            sel: 0,
            scroll: 0,
            rows: 10,
            show_hidden: false,
            status: None,
        };
        fe.load();
        if let Some(name) = wanted {
            if let Some(i) = fe.entries.iter().position(|e| e.name == name) {
                fe.sel = i;
            }
        }
        fe
    }

    fn load(&mut self) {
        let mut out = Vec::new();
        if let Some(parent) = self.cwd.parent() {
            out.push(Entry {
                name: "..".into(),
                path: parent.to_path_buf(),
                is_dir: true,
                size: 0,
                up: true,
            });
        }
        if let Ok(rd) = std::fs::read_dir(&self.cwd) {
            for e in rd.flatten() {
                let name = e.file_name().to_string_lossy().to_string();
                if !self.show_hidden && name.starts_with('.') {
                    continue;
                }
                let path = e.path();
                let md = e.metadata().ok();
                out.push(Entry {
                    name,
                    path,
                    is_dir: md.as_ref().map(|m| m.is_dir()).unwrap_or(false),
                    size: md.map(|m| m.len()).unwrap_or(0),
                    up: false,
                });
            }
        }
        out.sort_by(|a, b| {
            b.up.cmp(&a.up)
                .then_with(|| b.is_dir.cmp(&a.is_dir))
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        self.entries = out;
        self.sel = self.sel.min(self.entries.len().saturating_sub(1));
        self.clamp_scroll();
    }

    fn clamp_scroll(&mut self) {
        if self.sel < self.scroll {
            self.scroll = self.sel;
        } else if self.rows > 0 && self.sel >= self.scroll + self.rows {
            self.scroll = self.sel + 1 - self.rows;
        }
    }

    pub fn selected(&self) -> Option<&Entry> {
        self.entries.get(self.sel)
    }

    pub fn move_up(&mut self) {
        self.sel = self.sel.saturating_sub(1);
        self.clamp_scroll();
    }

    pub fn move_down(&mut self) {
        if self.sel + 1 < self.entries.len() {
            self.sel += 1;
        }
        self.clamp_scroll();
    }

    /// Avance/recule d'une page (hauteur visible).
    pub fn page(&mut self, forward: bool) {
        let step = self.rows.max(1);
        if forward {
            self.sel = (self.sel + step).min(self.entries.len().saturating_sub(1));
        } else {
            self.sel = self.sel.saturating_sub(step);
        }
        self.clamp_scroll();
    }

    pub fn go_top(&mut self) {
        self.sel = 0;
        self.clamp_scroll();
    }

    pub fn go_bottom(&mut self) {
        self.sel = self.entries.len().saturating_sub(1);
        self.clamp_scroll();
    }

    /// Entrée : descendre dans un dossier, copier le chemin d'un fichier.
    pub fn enter(&mut self) {
        let Some(e) = self.selected().cloned() else {
            return;
        };
        if e.is_dir {
            self.cwd = e.path;
            self.sel = 0;
            self.scroll = 0;
            self.load();
        } else {
            self.copy_path();
        }
    }

    pub fn parent(&mut self) {
        if let Some(p) = self.cwd.parent() {
            let previous = self.cwd.clone();
            self.cwd = p.to_path_buf();
            self.sel = 0;
            self.scroll = 0;
            self.load();
            if let Some(i) = self
                .entries
                .iter()
                .position(|e| e.path == previous && e.is_dir)
            {
                self.sel = i;
                self.clamp_scroll();
            }
        }
    }

    pub fn toggle_hidden(&mut self) {
        self.show_hidden = !self.show_hidden;
        self.load();
    }

    pub fn refresh(&mut self) {
        self.load();
        self.flash("liste relue", true);
    }

    /// Copie le chemin absolu de l'élément sélectionné.
    pub fn copy_path(&mut self) {
        let Some(e) = self.selected().cloned() else {
            return;
        };
        let text = e.path.to_string_lossy().to_string();
        let ok = crate::clipboard::copy(&text);
        self.flash(
            if ok {
                format!("copié : {text}")
            } else {
                "aucun outil presse-papier (wl-copy/xclip/pbcopy)".to_string()
            },
            ok,
        );
    }

    fn flash(&mut self, msg: impl Into<String>, ok: bool) {
        self.status = Some((msg.into(), Instant::now(), ok));
    }

    pub fn draw(&mut self, f: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Double)
            .border_style(Style::default().fg(PHOSPHOR))
            .style(Style::default().bg(DARK))
            .title(Span::styled(
                " ▓ 0file ",
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
                Constraint::Length(1),
                Constraint::Min(1),
                Constraint::Length(1),
            ])
            .split(inner);
        self.rows = rows[1].height as usize;
        self.clamp_scroll();

        // Chemin courant, raccourci avec `~`.
        let shown = shorten_home(&self.cwd);
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(" ⌂ ", Style::default().fg(DIM)),
                Span::styled(shown, Style::default().fg(AMBER)),
            ])),
            rows[0],
        );

        // Liste : fenêtre glissante sur `self.scroll`.
        let mut lines: Vec<Line> = Vec::new();
        for (i, e) in self
            .entries
            .iter()
            .enumerate()
            .skip(self.scroll)
            .take(self.rows)
        {
            let selected = i == self.sel;
            let (mark, color) = if e.up {
                ("↰", DIM)
            } else if e.is_dir {
                ("▸", PHOSPHOR)
            } else {
                ("·", DIM)
            };
            let size = if e.is_dir {
                "<DIR>".to_string()
            } else {
                crate::monitor::bytes(e.size)
            };
            let name_w = rows[1].width.saturating_sub(6) as usize;
            let mut name = e.name.clone();
            if unicode_width::UnicodeWidthStr::width(name.as_str()) > name_w {
                name = truncate_width(&name, name_w);
            }
            let pad = name_w.saturating_sub(unicode_width::UnicodeWidthStr::width(name.as_str()));
            let line = Line::from(vec![
                Span::styled(
                    if selected { "▶ " } else { "  " },
                    Style::default().fg(AMBER).bg(DARK),
                ),
                Span::styled(format!("{mark} "), Style::default().fg(color).bg(DARK)),
                Span::styled(
                    name,
                    Style::default()
                        .fg(if selected { Color::Black } else { PHOSPHOR })
                        .bg(if selected { PHOSPHOR } else { DARK })
                        .add_modifier(if e.is_dir {
                            Modifier::BOLD
                        } else {
                            Modifier::empty()
                        }),
                ),
                Span::styled(
                    format!("{}{size} ", " ".repeat(pad)),
                    Style::default()
                        .fg(if selected { Color::Black } else { DIM })
                        .bg(if selected { PHOSPHOR } else { DARK }),
                ),
            ]);
            lines.push(line);
        }
        f.render_widget(
            Paragraph::new(lines)
                .style(Style::default().bg(DARK))
                .wrap(Wrap { trim: false }),
            rows[1],
        );

        // Pied : statut éphémère, sinon rappel des touches.
        let status = self
            .status
            .as_ref()
            .filter(|(_, at, _)| at.elapsed() < Duration::from_secs(3));
        let footer = if let Some((msg, _, ok)) = status {
            Span::styled(
                format!(" {msg}"),
                Style::default().fg(if *ok { PHOSPHOR } else { AMBER }),
            )
        } else {
            let total = self.entries.len();
            let pos = if total == 0 { 0 } else { self.sel + 1 };
            Span::styled(
                format!(" ↑↓ ↵ c:copier .:cachés  {pos}/{total} "),
                Style::default().fg(DIM),
            )
        };
        f.render_widget(Paragraph::new(Line::from(footer)), rows[2]);
    }
}

/// `~` pour le dossier personnel, sinon chemin tel quel.
pub fn shorten_home(p: &Path) -> String {
    let home = crate::platform::home();
    let s = p.to_string_lossy();
    if !home.is_empty() {
        if let Some(rest) = s.strip_prefix(&home) {
            return format!("~{rest}");
        }
    }
    s.to_string()
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

/// Tronque à `width` colonnes, avec `…`.
fn truncate_width(s: &str, width: usize) -> String {
    use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};
    if width == 0 {
        return String::new();
    }
    if UnicodeWidthStr::width(s) <= width {
        return s.to_string();
    }
    let mut out = String::new();
    let mut w = 0;
    for ch in s.chars() {
        let cw = UnicodeWidthChar::width(ch).unwrap_or(0);
        if w + cw > width.saturating_sub(1) {
            break;
        }
        out.push(ch);
        w += cw;
    }
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("zer0-file-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(p.join("zdir")).unwrap();
        std::fs::create_dir_all(p.join("adir")).unwrap();
        std::fs::write(p.join("b.txt"), b"hello").unwrap();
        std::fs::write(p.join(".cache"), b"x").unwrap();
        p
    }

    #[test]
    fn listing_puts_dirs_first_then_files() {
        let d = tmpdir("order");
        let fe = FileExplorer::open(d.to_str().unwrap());
        let names: Vec<&str> = fe
            .entries
            .iter()
            .filter(|e| !e.up)
            .map(|e| e.name.as_str())
            .collect();
        assert_eq!(names, vec!["adir", "zdir", "b.txt"], "{names:?}");
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn hidden_entries_are_filtered_then_shown() {
        let d = tmpdir("hidden");
        let mut fe = FileExplorer::open(d.to_str().unwrap());
        assert!(!fe.entries.iter().any(|e| e.name == ".cache"));
        fe.toggle_hidden();
        assert!(fe.entries.iter().any(|e| e.name == ".cache"));
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn up_entry_points_to_parent() {
        let d = tmpdir("up");
        let fe = FileExplorer::open(d.join("adir").to_str().unwrap());
        let up = fe.entries.first().expect("..");
        assert!(up.up);
        assert_eq!(up.path, d);
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn page_moves_by_visible_height() {
        let d = tmpdir("page");
        for i in 0..30 {
            std::fs::write(d.join(format!("f{i:02}.txt")), b"x").unwrap();
        }
        let mut fe = FileExplorer::open(d.to_str().unwrap());
        fe.rows = 5;
        fe.page(true);
        assert_eq!(fe.sel, 5);
        fe.page(false);
        assert_eq!(fe.sel, 0);
        fe.go_bottom();
        assert_eq!(fe.sel, fe.entries.len() - 1);
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn home_is_shortened() {
        let home = crate::platform::home();
        assert_eq!(shorten_home(Path::new(&format!("{home}/src"))), "~/src");
    }

    #[test]
    fn truncate_never_exceeds_width() {
        let s = truncate_width("un nom de fichier très long", 10);
        assert!(unicode_width::UnicodeWidthStr::width(s.as_str()) <= 10);
        assert!(s.ends_with('…'));
    }
}
