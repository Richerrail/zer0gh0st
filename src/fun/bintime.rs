use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

const ON: Color = Color::Rgb(80, 255, 120);
const OFF: Color = Color::Rgb(12, 46, 24);
const DIM: Color = Color::Rgb(25, 90, 45);
const TEXT: Color = Color::Rgb(60, 220, 110);

/// CRT 8-bit binary clock: 3 groups (HEURE/MIN/SEC), 6 bits each (32..1).
pub struct BinTime {
    pub hms: (u8, u8, u8),
    pub date: String,
    ticks: u32,
}

impl Default for BinTime {
    fn default() -> Self {
        Self::new()
    }
}

fn shell(args: &[&str]) -> String {
    std::process::Command::new(args[0])
        .args(&args[1..])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_default()
        .trim()
        .to_string()
}

impl BinTime {
    pub fn new() -> Self {
        let mut b = BinTime {
            hms: (0, 0, 0),
            date: String::new(),
            ticks: 0,
        };
        b.refresh();
        b
    }

    fn refresh(&mut self) {
        let t = shell(&["date", "+%H %M %S"]);
        let mut it = t.split_whitespace();
        self.hms = (
            it.next().and_then(|s| s.parse().ok()).unwrap_or(0),
            it.next().and_then(|s| s.parse().ok()).unwrap_or(0),
            it.next().and_then(|s| s.parse().ok()).unwrap_or(0),
        );
        self.date = shell(&["date", "+%Y-%m-%d"]);
    }

    pub fn tick(&mut self) {
        self.ticks = self.ticks.wrapping_add(1);
        if self.ticks % 20 == 0 {
            self.refresh();
        }
    }

    /// (label, weights, leds, decimal) for one group, each padded to W.
    fn group(label: &str, value: u8) -> [String; 4] {
        let weights = [32u8, 16, 8, 4, 2, 1];
        let mut w_row = String::new();
        let mut leds = String::new();
        for w in weights {
            w_row.push_str(&format!("{w:<3}"));
            leds.push_str(if value & w != 0 { "██ " } else { "░░ " });
        }
        [
            format!("{label:^20}"),
            format!("{:<20}", w_row.trim_end()),
            format!("{:<20}", leds.trim_end()),
            format!("{value:^20}"),
        ]
    }

    pub fn draw(&self, f: &mut Frame, area: Rect) {
        let gs = [
            Self::group("HEURE", self.hms.0),
            Self::group("MIN", self.hms.1),
            Self::group("SEC", self.hms.2),
        ];
        let join = |idx: usize, color: Color| -> Line<'static> {
            Line::from(vec![
                Span::styled(gs[0][idx].clone(), Style::default().fg(color)),
                Span::styled(gs[1][idx].clone(), Style::default().fg(color)),
                Span::styled(gs[2][idx].clone(), Style::default().fg(color)),
            ])
        };

        let bits_line = format!(
            "{}   {}   {}",
            format!("{:06b}", self.hms.0),
            format!("{:06b}", self.hms.1),
            format!("{:06b}", self.hms.2)
        );

        let mut lines: Vec<Line> = Vec::new();
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "[ ZER0 // BINTIME ]",
            Style::default().fg(ON),
        )));
        lines.push(Line::from(Span::styled(
            self.date.clone(),
            Style::default().fg(DIM),
        )));
        lines.push(Line::from(""));
        lines.push(join(0, TEXT)); // labels
        lines.push(join(1, DIM)); // weights
        lines.push(led_line(&gs)); // LEDs
        lines.push(join(3, TEXT)); // decimals
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(bits_line, Style::default().fg(DIM))));
        lines.push(Line::from(Span::styled(
            format!("{:02}:{:02}:{:02}", self.hms.0, self.hms.1, self.hms.2),
            Style::default().fg(ON),
        )));
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "ESC / q pour quitter",
            Style::default().fg(DIM),
        )));

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Rgb(30, 120, 55)));
        let p = Paragraph::new(lines)
            .block(block)
            .alignment(Alignment::Center);
        f.render_widget(p, area);
    }
}

fn led_line(gs: &[[String; 4]]) -> Line<'static> {
    let mut spans: Vec<Span> = Vec::new();
    for row in gs.iter().map(|g| g[2].as_str()) {
        for ch in row.chars() {
            match ch {
                '█' => spans.push(Span::styled("█", Style::default().fg(ON))),
                '░' => spans.push(Span::styled("░", Style::default().fg(OFF))),
                _ => spans.push(Span::raw(" ")),
            }
        }
    }
    Line::from(spans)
}
