use std::path::{Path, PathBuf};
use std::process::{Child, Command};

use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

const EXTS: &[&str] = &["mp3", "wav", "ogg", "flac", "m4a", "opus", "aac", "wma"];

/// A terminal "8-bit radio": track list + external audio player + ASCII bars.
/// Can open a folder, an audio file, or a stream URL.
pub struct Radio {
    pub tracks: Vec<PathBuf>,
    pub selected: usize,
    playing: Option<Child>,
    pub now: Option<String>,
    pub frame: u32,
    pub error: Option<String>,
}

/// Default music folder: `$ZER0_MUSIC_DIR`, else `<projet>/music` (à côté du binaire).
pub fn default_music_dir() -> PathBuf {
    if let Ok(d) = std::env::var("ZER0_MUSIC_DIR") {
        return PathBuf::from(d);
    }
    if let Ok(exe) = std::env::current_exe() {
        // target/release/zer0 → target/release → target → <projet>
        if let Some(root) = exe.parent().and_then(|p| p.parent()).and_then(|p| p.parent()) {
            return root.join("music");
        }
    }
    PathBuf::from("music")
}

fn command_exists(name: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|p| p.join(name).exists()))
        .unwrap_or(false)
}

fn is_audio(p: &Path) -> bool {
    p.extension()
        .and_then(|s| s.to_str())
        .map(|e| EXTS.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

fn is_url(s: &str) -> bool {
    let l = s.to_ascii_lowercase();
    l.starts_with("http://")
        || l.starts_with("https://")
        || l.starts_with("rtsp://")
        || l.starts_with("rtmp://")
        || l.starts_with("magnet:")
        || l.starts_with("udp://")
        || l.starts_with("icy://")
}

impl Default for Radio {
    fn default() -> Self {
        Self::new()
    }
}

impl Radio {
    pub fn new() -> Self {
        let mut r = Radio {
            tracks: Vec::new(),
            selected: 0,
            playing: None,
            now: None,
            frame: 0,
            error: None,
        };
        let dir = default_music_dir();
        let _ = std::fs::create_dir_all(&dir);
        r.load_dir(&dir);
        r
    }

    /// Replace the track list with the audio files of `dir` (non-recursive).
    pub fn load_dir(&mut self, dir: &Path) {
        let mut tracks = Vec::new();
        if let Ok(rd) = std::fs::read_dir(dir) {
            for e in rd.flatten() {
                let p = e.path();
                if p.is_file() && is_audio(&p) {
                    tracks.push(p);
                }
            }
        }
        tracks.sort();
        if tracks.is_empty() {
            self.error = Some(format!("aucun audio dans {}", dir.display()));
        } else {
            self.error = None;
        }
        self.tracks = tracks;
        self.selected = 0;
    }

    /// Append a single audio file to the list.
    pub fn add_file(&mut self, path: PathBuf) {
        if !self.tracks.contains(&path) {
            self.tracks.push(path.clone());
            self.tracks.sort();
        }
        if let Some(i) = self.tracks.iter().position(|p| *p == path) {
            self.selected = i;
        }
    }

    pub fn next(&mut self) {
        if !self.tracks.is_empty() {
            self.selected = (self.selected + 1) % self.tracks.len();
        }
    }
    pub fn prev(&mut self) {
        if !self.tracks.is_empty() {
            self.selected = (self.selected + self.tracks.len() - 1) % self.tracks.len();
        }
    }

    /// Next track and play it (n).
    pub fn next_play(&mut self) {
        if !self.tracks.is_empty() {
            self.next();
            self.play_selected();
        }
    }
    /// Previous track and play it (p).
    pub fn prev_play(&mut self) {
        if !self.tracks.is_empty() {
            self.prev();
            self.play_selected();
        }
    }

    pub fn stop(&mut self) {
        if let Some(mut c) = self.playing.take() {
            let _ = c.kill();
        }
        self.now = None;
    }

    pub fn play_selected(&mut self) {
        if let Some(p) = self.tracks.get(self.selected).cloned() {
            let label = p
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            self.play_target(&p.to_string_lossy(), label);
        }
    }

    fn play_target(&mut self, target: &str, label: String) {
        self.stop();
        let spawn = if command_exists("ffplay") {
            Command::new("ffplay")
                .args(["-nodisp", "-autoexit", "-loglevel", "quiet"])
                .arg(target)
                .spawn()
        } else if command_exists("mpv") {
            Command::new("mpv")
                .args(["--no-video", "--really-quiet"])
                .arg(target)
                .spawn()
        } else if !is_url(target) && command_exists("paplay") {
            Command::new("paplay").arg(target).spawn()
        } else {
            self.error = Some("aucun lecteur (ffplay/mpv/paplay) trouvé".into());
            return;
        };
        match spawn {
            Ok(child) => {
                self.playing = Some(child);
                self.now = Some(label);
                self.error = None;
            }
            Err(e) => self.error = Some(format!("lecture: {e}")),
        }
    }

    /// Open a folder, a file, or a stream URL.
    pub fn open(&mut self, arg: &str) {
        if is_url(arg) {
            self.play_target(arg, arg.to_string());
            return;
        }
        let p = PathBuf::from(shellexpand(arg));
        if p.is_dir() {
            self.load_dir(&p);
        } else if p.is_file() {
            self.add_file(p.clone());
            self.play_selected();
        } else {
            self.error = Some(format!("chemin introuvable: {}", p.display()));
        }
    }

    pub fn tick(&mut self) {
        self.frame = self.frame.wrapping_add(1);
        let ended = match self.playing.as_mut() {
            Some(c) => matches!(c.try_wait(), Ok(Some(_))),
            None => false,
        };
        if ended {
            self.playing = None;
            // Playlist: enchaîne automatiquement sur la piste suivante.
            if !self.tracks.is_empty() {
                self.next();
                self.play_selected();
            } else {
                self.now = None;
            }
        }
    }

    fn bar(&self, i: usize) -> f32 {
        let t = self.frame as f32 * 0.15 + i as f32 * 0.7;
        (t.sin() * 0.5 + 0.5).abs() * (0.4 + 0.6 * ((i * 13 % 7) as f32 / 7.0))
    }

    pub fn draw(&self, f: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Magenta))
            .title(" ZER0 RADIO — ↑↓ choisir · Entrée jouer · n/p suiv./préc. · s stop · q quitter ");

        let inner = block.inner(area);
        f.render_widget(block, area);
        if inner.height < 4 {
            return;
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(4), Constraint::Length(8)])
            .split(inner);

        let items: Vec<ListItem> = self
            .tracks
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let name = p
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default();
                let marker = if self.now.as_deref() == Some(name.as_str()) {
                    "▶ "
                } else if i == self.selected {
                    "> "
                } else {
                    "  "
                };
                ListItem::new(format!("{marker}{name}"))
            })
            .collect();
        let list = List::new(items)
            .highlight_style(Style::default().fg(Color::Black).bg(Color::Magenta));
        let mut state = ListState::default();
        state.select(Some(self.selected));
        f.render_stateful_widget(list, chunks[0], &mut state);

        let width = chunks[1].width.saturating_sub(2) as usize;
        let height = chunks[1].height.saturating_sub(1) as usize;
        let bars = width / 2;
        let mut rows: Vec<Line> = Vec::with_capacity(height);
        for row in (0..height).rev() {
            let mut spans: Vec<Span> = Vec::new();
            for i in 0..bars {
                let h = (self.bar(i) * height as f32) as usize;
                let cell = if h > row { "█" } else { " " };
                spans.push(Span::styled(cell, Style::default().fg(Color::Green)));
                spans.push(Span::raw(" "));
            }
            rows.push(Line::from(spans));
        }
        let status = match (&self.now, &self.error) {
            (Some(n), _) => format!("♪ {n}"),
            (None, Some(e)) => e.clone(),
            _ => "(arrêté)".to_string(),
        };
        rows.push(Line::from(Span::styled(status, Style::default().fg(Color::Cyan))));
        f.render_widget(Paragraph::new(rows).alignment(Alignment::Left), chunks[1]);
    }
}

/// Minimal `~` expansion (avoids a dependency).
fn shellexpand(s: &str) -> String {
    if let Some(rest) = s.strip_prefix("~/") {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/home".into());
        return format!("{home}/{rest}");
    }
    s.to_string()
}
