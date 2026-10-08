use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

/// 5-row block glyphs for the big banner.
fn glyph(c: char) -> [&'static str; 5] {
    match c {
        'A' => [" ███ ", "█   █", "█████", "█   █", "█   █"],
        'G' => [" ████", "█    ", "█ ███", "█   █", " ███ "],
        'E' => ["█████", "█    ", "████ ", "█    ", "█████"],
        'N' => ["█   █", "██  █", "█ █ █", "█  ██", "█   █"],
        'H' => ["█   █", "█   █", "█████", "█   █", "█   █"],
        'S' => [" ████", "█    ", " ███ ", "    █", "████ "],
        'T' => ["█████", "  █  ", "  █  ", "  █  ", "  █  "],
        'Z' => ["█████", "   █ ", "  █  ", " █   ", "█████"],
        'R' => ["████ ", "█   █", "████ ", "█  █ ", "█   █"],
        // 'O' sert aussi de '0' (ZER0, GH0ST)
        'O' | '0' => [" ███ ", "█   █", "█   █", "█   █", " ███ "],
        // séparateur
        '|' | '│' | '¦' => ["│", "│", "│", "│", "│"],
        ' ' => ["  ", "  ", "  ", "  ", "  "],
        _ => ["     ", "     ", "     ", "     ", "     "],
    }
}

const TEXT: &str = "ZER0│GH0ST";
const TAGLINE: &str = "ghost in the shell";

/// Idle banner: static cyan → blue gradient.
pub fn lines() -> Vec<Line<'static>> {
    render(false, 0)
}

/// Animated banner: rainbow gradient that scrolls with `phase`.
pub fn lines_animated(phase: u16) -> Vec<Line<'static>> {
    render(true, phase)
}

fn render(animated: bool, phase: u16) -> Vec<Line<'static>> {
    let mut rows: Vec<String> = vec![String::new(); 5];
    for (i, ch) in TEXT.chars().enumerate() {
        let g = glyph(ch);
        for (r, row) in rows.iter_mut().enumerate() {
            if i > 0 {
                row.push(' ');
            }
            row.push_str(g[r]);
        }
    }
    let width = rows[0].chars().count();
    let mut out: Vec<Line<'static>> = rows
        .into_iter()
        .map(|row| colorize(&row, width, animated, phase))
        .collect();
    out.push(Line::from(Span::styled(
        format!("   {TAGLINE}"),
        Style::default().fg(Color::DarkGray),
    )));
    out
}

fn colorize(row: &str, width: usize, animated: bool, phase: u16) -> Line<'static> {
    let spans: Vec<Span> = row
        .chars()
        .enumerate()
        .map(|(x, ch)| {
            if ch == '█' || ch == '│' {
                Span::styled(
                    ch.to_string(),
                    Style::default().fg(color_at(x, width, animated, phase)),
                )
            } else {
                Span::raw(" ")
            }
        })
        .collect();
    Line::from(spans)
}

fn color_at(x: usize, w: usize, animated: bool, phase: u16) -> Color {
    if animated {
        let hue = ((x as u32 * 360 / w.max(1) as u32) + phase as u32) % 360;
        hsv_to_rgb(hue as f32, 1.0, 1.0)
    } else {
        let t = if w <= 1 { 0.0 } else { x as f32 / (w - 1) as f32 };
        let r = (70.0 * t) as u8;
        let g = (200.0 * (1.0 - t) + 90.0 * t) as u8;
        Color::Rgb(r, g, 255)
    }
}

/// h in [0,360), s and v in [0,1].
fn hsv_to_rgb(h: f32, s: f32, v: f32) -> Color {
    let c = v * s;
    let hp = h / 60.0;
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let (r1, g1, b1) = match hp as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    Color::Rgb(
        ((r1 + m) * 255.0) as u8,
        ((g1 + m) * 255.0) as u8,
        ((b1 + m) * 255.0) as u8,
    )
}
