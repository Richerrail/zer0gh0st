use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

const W: i16 = 60;
const H: i16 = 18;
const PADDLE: i16 = 3; // half-height

/// Grid-based Pong against the agent (right paddle). Integer positions, one
/// step per fixed tick — no sub-cell rounding, so no tearing/ghosting.
pub struct Pong {
    bx: i16,
    by: i16,
    dx: i16,    // -1 / +1
    ystep: i16, // -1 / +1, applied every other tick
    tick: u32,
    player: i16,
    agent: i16,
    player_dir: i16,
    dir_ticks: u8,
    pub pscore: u32,
    pub ascore: u32,
    pub over: bool,
}

impl Default for Pong {
    fn default() -> Self {
        Self::new()
    }
}

impl Pong {
    pub fn new() -> Self {
        Pong {
            bx: W / 2,
            by: H / 2,
            dx: 1,
            ystep: 1,
            tick: 0,
            player: H / 2,
            agent: H / 2,
            player_dir: 0,
            dir_ticks: 0,
            pscore: 0,
            ascore: 0,
            over: false,
        }
    }

    pub fn restart(&mut self) {
        *self = Pong::new();
    }

    /// Start moving the player paddle; the direction persists a few ticks.
    pub fn set_dir(&mut self, d: i16) {
        self.player_dir = d.signum();
        self.dir_ticks = 4;
    }

    fn reset_ball(&mut self, to_right: bool) {
        self.bx = W / 2;
        self.by = H / 2;
        self.dx = if to_right { 1 } else { -1 };
        self.ystep = 1;
    }

    pub fn step(&mut self) {
        if self.over {
            return;
        }
        self.tick = self.tick.wrapping_add(1);

        // player
        if self.dir_ticks > 0 {
            self.dir_ticks -= 1;
            self.player = (self.player + self.player_dir).clamp(PADDLE, H - 1 - PADDLE);
        } else {
            self.player_dir = 0;
        }

        // agent AI: follow the ball, one cell per tick
        let diff = self.by - self.agent;
        if diff.abs() > 1 {
            self.agent += diff.signum();
            self.agent = self.agent.clamp(PADDLE, H - 1 - PADDLE);
        }

        // ball
        self.bx += self.dx;
        if self.tick % 2 == 0 {
            self.by += self.ystep;
        }

        if self.by <= 0 {
            self.by = 0;
            self.ystep = self.ystep.abs();
        }
        if self.by >= H - 1 {
            self.by = H - 1;
            self.ystep = -self.ystep.abs();
        }

        // player paddle
        if self.bx <= 1 && self.dx < 0 && (self.by - self.player).abs() <= PADDLE {
            self.bx = 1;
            self.dx = self.dx.abs();
            let s = (self.by - self.player).signum();
            self.ystep = if s == 0 { 1 } else { s };
        }
        // agent paddle
        if self.bx >= W - 2 && self.dx > 0 && (self.by - self.agent).abs() <= PADDLE {
            self.bx = W - 2;
            self.dx = -self.dx.abs();
        }

        // scoring
        if self.bx < 0 {
            self.ascore += 1;
            self.reset_ball(true);
        } else if self.bx > W - 1 {
            self.pscore += 1;
            self.reset_ball(false);
        }
        if self.pscore >= 7 || self.ascore >= 7 {
            self.over = true;
        }
    }

    pub fn draw(&self, f: &mut Frame, area: Rect) {
        let mut lines: Vec<Line> = Vec::with_capacity(H as usize);
        for y in 0..H {
            let mut spans: Vec<Span> = Vec::with_capacity(W as usize);
            for x in 0..W {
                let (ch, color) = if x == 1 && (y - self.player).abs() <= PADDLE {
                    ("█", Color::White)
                } else if x == W - 2 && (y - self.agent).abs() <= PADDLE {
                    ("█", Color::Red)
                } else if x == self.bx && y == self.by {
                    ("O", Color::Yellow)
                } else if x == W / 2 && y % 2 == 0 {
                    (".", Color::DarkGray)
                } else {
                    (" ", Color::Reset)
                };
                spans.push(Span::styled(ch, Style::default().fg(color)));
            }
            lines.push(Line::from(spans));
        }

        let title = if self.over {
            let win = if self.pscore > self.ascore {
                "VICTOIRE"
            } else {
                "L'AGENT GAGNE"
            };
            format!(
                " ZER0 PONG — {win} {}:{} — r relancer · q quitter ",
                self.pscore, self.ascore
            )
        } else {
            format!(
                " ZER0 PONG — TOI {} : {} AGENT — W/S ou ↑↓ · premier à 7 · q quitter ",
                self.pscore, self.ascore
            )
        };
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Green))
            .title(title);
        let p = Paragraph::new(lines)
            .block(block)
            .alignment(Alignment::Left);

        let vertical = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(H as u16 + 2), Constraint::Min(0)])
            .split(area);
        let horizontal = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(0), Constraint::Length(W as u16 + 2), Constraint::Min(0)])
            .split(vertical[1]);
        f.render_widget(p, horizontal[1]);
    }
}
