//! Zero-chan — assistante vocale intégrée à la TUI de l'agent.
//!
//! Choix de conception : **pas de boîte chat ni de barre dédiées**. Zero-chan
//! réutilise la chat et l'input de l'agent. Il n'ajoute qu'un petit bloc d'état
//! (à droite du banner) et deux boutons (à droite de la barre de saisie).

use std::path::PathBuf;
use std::process::{Child, Command, Stdio};

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Off,
    Idle,
    Listening,
    Thinking,
    Speaking,
}

impl State {
    pub fn label(&self) -> &'static str {
        match self {
            State::Off => "off",
            State::Idle => "en veille",
            State::Listening => "écoute…",
            State::Thinking => "réfléchit…",
            State::Speaking => "parle…",
        }
    }
}

pub fn rec_path() -> PathBuf {
    std::env::temp_dir().join("zer0_chan_mic.wav")
}

pub struct ZeroChan {
    pub on: bool,
    pub state: State,
    pub frame: u32,
    pub last: String,
    rec: Option<Child>,
    rec_started: Option<std::time::Instant>,
}

/// Durée d'écoute maximale avant arrêt automatique (secondes).
pub const MAX_REC_SECS: f32 = 6.0;

impl Default for ZeroChan {
    fn default() -> Self {
        Self::new()
    }
}

impl ZeroChan {
    pub fn new() -> Self {
        ZeroChan {
            on: false,
            state: State::Off,
            frame: 0,
            last: String::new(),
            rec: None,
            rec_started: None,
        }
    }

    pub fn toggle_on(&mut self) {
        self.on = !self.on;
        if self.on {
            self.state = State::Idle;
            self.last = "zero-chan activé".into();
        } else {
            self.stop_rec();
            self.state = State::Off;
        }
    }

    /// Démarre l'écoute micro. Retourne true si l'enregistrement a commencé.
    pub fn start_rec(&mut self) -> bool {
        if !self.on || self.rec.is_some() {
            return false;
        }
        let path = rec_path();
        let child = Command::new("pw-record")
            .args([
                "--format", "s16", "--rate", "16000", "--channels", "1",
            ])
            .arg(&path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
        match child {
            Ok(c) => {
                self.rec = Some(c);
                self.rec_started = Some(std::time::Instant::now());
                self.state = State::Listening;
                self.last = "écoute du micro…".into();
                true
            }
            Err(e) => {
                self.last = format!("pw-record: {e}");
                false
            }
        }
    }

    /// Arrête l'enregistrement (SIGINT pour finaliser le WAV), attente bornée.
    pub fn stop_rec(&mut self) {
        self.rec_started = None;
        if let Some(mut c) = self.rec.take() {
            let pid = c.id();
            let _ = Command::new("kill").arg("-INT").arg(pid.to_string()).status();
            for _ in 0..20 {
                if let Ok(Some(_)) = c.try_wait() {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(25));
            }
            let _ = c.kill();
        }
    }

    /// Durée d'écoute écoulée (secondes), si enregistrement en cours.
    pub fn rec_elapsed(&self) -> Option<f32> {
        self.rec_started.map(|t| t.elapsed().as_secs_f32())
    }

    pub fn is_recording(&self) -> bool {
        self.rec.is_some()
    }

    pub fn tick(&mut self) {
        self.frame = self.frame.wrapping_add(1);
    }

    /// Visage animé style kawaii : `◕` + bouche variable + clins d'œil.
    fn face(&self) -> String {
        let f = self.frame;
        let mouth = match self.state {
            State::Speaking => ["o", "O", "‿", "_"][((f / 3) % 4) as usize],
            State::Listening => {
                if (f / 6) % 2 == 0 {
                    "o"
                } else {
                    "‿"
                }
            }
            State::Thinking => {
                if (f / 10) % 3 == 0 {
                    "~"
                } else {
                    "_"
                }
            }
            State::Idle => {
                if (f / 22) % 4 == 0 {
                    "_"
                } else {
                    "‿"
                }
            }
            State::Off => "_",
        };
        // clin d'œil / clignement occasionnel
        let left = if (f / 33) % 11 == 0 { "-" } else { "◕" };
        let right = if (f / 24) % 8 == 0 { "¬" } else { "◕" };
        format!("({left}{mouth}{right})")
    }

    /// Petit bloc d'état (à droite du banner).
    pub fn draw(&self, f: &mut Frame, area: Rect) {
        let color = match self.state {
            State::Off => Color::DarkGray,
            State::Idle => Color::Magenta,
            State::Listening => Color::LightRed,
            State::Thinking => Color::Yellow,
            State::Speaking => Color::LightGreen,
        };
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(color))
            .title(" ZERO-CHAN ");
        let inner = block.inner(area);
        f.render_widget(block, area);
        if inner.height < 2 {
            return;
        }

        // visualiseur animé (petites barres)
        let bars = inner.width.saturating_sub(2) as usize / 2;
        let mut vis: Vec<Span> = Vec::new();
        for i in 0..bars {
            let h = match self.state {
                State::Off => 0,
                State::Idle => ((self.frame as i64 + i as i64 * 3) % 8 == 0) as usize,
                _ => (((self.frame as f32 * 0.6 + i as f32 * 0.9).sin().abs()) * 3.0) as usize,
            };
            let ch = [" ", "▁", "▃", "▅", "▇"][h.min(4)];
            vis.push(Span::styled(ch, Style::default().fg(color)));
            vis.push(Span::raw(" "));
        }
        let mut lines = vec![
            Line::from(vec![
                Span::styled(format!("{}  ", self.face()), Style::default().fg(color)),
                Span::styled(self.state.label(), Style::default().fg(color)),
            ]),
            Line::from(vis),
        ];
        if !self.last.is_empty() {
            let t: String = self.last.chars().take(inner.width.saturating_sub(2) as usize).collect();
            lines.push(Line::from(Span::styled(t, Style::default().fg(Color::Gray))));
        }
        f.render_widget(Paragraph::new(lines), inner);
    }
}

/// Transcrit un WAV via `faster_whisper` (Python, déjà installé).
pub async fn transcribe(path: &PathBuf) -> anyhow::Result<String> {
    let code = "import sys\n\
        from faster_whisper import WhisperModel\n\
        m = WhisperModel('base', device='cpu', compute_type='int8')\n\
        segs, _ = m.transcribe(sys.argv[1], language='fr')\n\
        print(''.join(s.text for s in segs).strip())\n";
    let out = tokio::process::Command::new("python3")
        .arg("-c")
        .arg(code)
        .arg(path)
        .output()
        .await?;
    if !out.status.success() {
        return Err(anyhow::anyhow!(
            "{}",
            String::from_utf8_lossy(&out.stderr).chars().take(200).collect::<String>()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Deux boutons à droite de la barre de saisie : 0chan et micro.
/// Retourne leurs rectangles (pour la détection de clic).
pub fn draw_buttons(f: &mut Frame, area: Rect, zc: &ZeroChan) -> (Option<Rect>, Option<Rect>) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Magenta))
        .title(" 0chan/mic ");
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.height < 1 || inner.width == 0 {
        return (None, None);
    }

    let on_color = if zc.on { Color::LightGreen } else { Color::Gray };
    let mic_color = if zc.is_recording() {
        Color::LightRed
    } else if zc.on {
        Color::Cyan
    } else {
        Color::Gray
    };

    let r1 = Rect { x: inner.x, y: inner.y, width: inner.width, height: 1 };
    let r2 = Rect {
        x: inner.x,
        y: inner.y + 1,
        width: inner.width,
        height: 1,
    };
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            if zc.on { "[0chan ON ]" } else { "[0chan off]" },
            Style::default().fg(on_color),
        ))),
        r1,
    );
    if inner.height >= 2 {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                if zc.is_recording() { "[MIC  REC]" } else if zc.on { "[MIC  on ]" } else { "[MIC  off]" },
                Style::default().fg(mic_color),
            ))),
            r2,
        );
    }
    (Some(r1), if inner.height >= 2 { Some(r2) } else { None })
}

/// Zone du bloc d'état à droite du banner (largeur fixe).
pub fn block_layout(banner_area: Rect, on: bool) -> (Rect, Rect) {
    if on && banner_area.width >= 96 {
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(70), Constraint::Length(26)])
            .split(banner_area);
        (cols[0], cols[1])
    } else {
        (banner_area, Rect { x: 0, y: 0, width: 0, height: 0 })
    }
}
