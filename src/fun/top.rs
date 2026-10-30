//! Mod « t0p » : moniteur système dans le panneau latéral (comme radio/bintime).
//!
//! Le relevé est rafraîchi **une fois par seconde** : inutile de redessiner à
//! 45 Hz pour des compteurs qui ne bougent pas, et ça évite de chauffer.

use std::time::{Duration, Instant};

use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

use crate::monitor::{render, Monitor, Snapshot};

pub struct Top {
    mon: Monitor,
    snap: Snapshot,
    last: Instant,
}

impl Top {
    pub fn new() -> Self {
        let mut mon = Monitor::new();
        mon.warmup();
        let snap = mon.snapshot();
        Self {
            mon,
            snap,
            last: Instant::now(),
        }
    }

    /// Rafraîchit au plus une fois par seconde.
    pub fn tick(&mut self) {
        if self.last.elapsed() >= Duration::from_secs(1) {
            self.mon.refresh();
            self.snap = self.mon.snapshot();
            self.last = Instant::now();
        }
    }

    pub fn snapshot(&self) -> &Snapshot {
        &self.snap
    }

    pub fn draw(&self, f: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(" t0p ")
            .border_style(Style::default().fg(Color::Cyan));
        let text = render(&self.snap);
        f.render_widget(
            Paragraph::new(text).block(block).wrap(Wrap { trim: false }),
            area,
        );
    }
}

impl Default for Top {
    fn default() -> Self {
        Self::new()
    }
}
