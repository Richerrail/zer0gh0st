use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::Paragraph;
use ratatui::Frame;
use std::sync::OnceLock;

use crate::theme;

const NEON: Color = Color::LightRed;
const NEON_DIM: Color = Color::Red;

fn truecolor() -> bool {
    static FLAG: OnceLock<bool> = OnceLock::new();
    *FLAG.get_or_init(|| {
        if std::env::var_os("WT_SESSION").is_some() {
            return true;
        }
        matches!(
            std::env::var("COLORTERM").as_deref(),
            Ok("truecolor") | Ok("24bit")
        )
    })
}

fn wave_color(k: f32, x: u16, y: u16) -> Option<Color> {
    if truecolor() {
        return (k >= 0.05).then(|| wave_color_rgb(k));
    }
    wave_color_ansi(k, x, y)
}

fn wave_color_rgb(k: f32) -> Color {
    let (br, bg, bb) = (255.0, 55.0, 62.0);
    let (dr, dg, db) = (55.0, 4.0, 8.0);
    let t = k * k;
    let f = |dark: f32, bright: f32| (dark + (bright - dark) * t).round().clamp(0.0, 255.0) as u8;
    Color::Rgb(f(dr, br), f(dg, bg), f(db, bb))
}

fn wave_color_ansi(k: f32, x: u16, y: u16) -> Option<Color> {
    if k < 0.05 {
        return None;
    }
    match k {
        k if k >= 0.55 => Some(NEON),
        k if k >= 0.22 => Some(NEON_DIM),
        _ if (x + y).is_multiple_of(2) => Some(NEON_DIM),
        _ => None,
    }
}

pub fn perimeter(area: Rect) -> Vec<(u16, u16)> {
    let mut cells = Vec::new();
    if area.width < 2 || area.height < 2 {
        return cells;
    }
    let right = area.x + area.width - 1;
    let bottom = area.y + area.height - 1;
    for x in area.x..=right {
        cells.push((x, area.y));
    }
    for y in (area.y + 1)..=bottom {
        cells.push((right, y));
    }
    for x in (area.x..right).rev() {
        cells.push((x, bottom));
    }
    for y in (area.y + 1..bottom).rev() {
        cells.push((area.x, y));
    }
    cells
}

fn closest_index(cells: &[(u16, u16)], (x, y): (u16, u16)) -> usize {
    let mut best = 0usize;
    let mut best_d = u32::MAX;
    for (i, &(cx, cy)) in cells.iter().enumerate() {
        let d = cx.abs_diff(x) as u32 + cy.abs_diff(y) as u32;
        if d < best_d {
            best_d = d;
            best = i;
        }
    }
    best
}

fn fade(distance: usize, trail: usize) -> f32 {
    if distance >= trail {
        return 0.0;
    }
    let t = 1.0 - distance as f32 / trail as f32;
    t * t
}

fn is_box_drawing(sym: &str) -> bool {
    !sym.is_empty()
        && sym.chars().all(|c| {
            matches!(
                c,
                '─' | '│'
                    | '┌'
                    | '┐'
                    | '└'
                    | '┘'
                    | '├'
                    | '┤'
                    | '┬'
                    | '┴'
                    | '┼'
                    | '━'
                    | '┃'
                    | '═'
                    | '║'
            )
        })
}

pub fn draw_handle(f: &mut Frame, column: Rect, open: bool) -> Rect {
    let inner = Rect {
        x: column.x + 1,
        y: column.y + 1,
        width: column.width.saturating_sub(2),
        height: column.height.saturating_sub(2),
    };
    if inner.width < 2 || inner.height < 3 {
        return Rect::default();
    }
    let arrow = if open { "<<" } else { theme::rail_arrow() };
    let top = inner.y + (inner.height - 3) / 2;
    let row = |y: u16| Rect {
        x: inner.x,
        y,
        width: inner.width,
        height: 1,
    };
    f.render_widget(
        Paragraph::new(Span::styled(
            arrow,
            Style::default().fg(NEON).add_modifier(Modifier::BOLD),
        )),
        row(top),
    );
    f.render_widget(
        Paragraph::new(Span::styled("══", Style::default().fg(NEON_DIM))),
        row(top + 1),
    );
    f.render_widget(
        Paragraph::new(Span::styled("F2", Style::default().fg(NEON_DIM))),
        row(top + 2),
    );
    Rect {
        x: inner.x,
        y: top,
        width: 2,
        height: 3,
    }
}

pub fn draw_wave(f: &mut Frame, area: Rect, sources: [(u16, u16); 2], phase: u16) {
    let cells = perimeter(area);
    if cells.len() < 16 {
        return;
    }
    let p = cells.len();
    let src_top = closest_index(&cells, sources[0]);
    let src_bottom = closest_index(&cells, sources[1]);

    let step = phase as usize % p;
    let front_top = (src_top + p - step) % p;
    let front_bottom = (src_bottom + step) % p;
    let trail = (p / 20).clamp(8, 20);

    let buf = f.buffer_mut();
    for (i, &(x, y)) in cells.iter().enumerate() {
        let d_top = (i + p - front_top) % p;
        let d_bottom = (front_bottom + p - i) % p;
        let k = fade(d_top, trail).max(fade(d_bottom, trail));
        let Some(color) = wave_color(k, x, y) else {
            continue;
        };

        let cell = &mut buf[(x, y)];
        let sym = cell.symbol().to_string();
        if sym.trim().is_empty() {
            let ch = if y == area.y || y == area.y + area.height - 1 {
                '─'
            } else {
                '│'
            };
            cell.set_symbol(ch.to_string().as_str());
        } else if !is_box_drawing(&sym) {
            continue;
        }
        let style = cell.style().fg(color);
        cell.set_style(style);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn perimeter_walks_the_border_once() {
        let area = Rect {
            x: 0,
            y: 0,
            width: 10,
            height: 5,
        };
        let cells = perimeter(area);
        assert_eq!(cells.len(), 26);
        assert_eq!(cells[0], (0, 0));
        assert_eq!(cells[9], (9, 0));
        assert_eq!(cells[10], (9, 1));
        assert_eq!(cells[13], (9, 4));
        assert_eq!(cells[14], (8, 4));
        assert_eq!(cells[22], (0, 4));
        assert_eq!(cells[25], (0, 1));
        let mut seen = cells.clone();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), cells.len());
    }

    #[test]
    fn perimeter_is_empty_on_tiny_areas() {
        assert!(perimeter(Rect::default()).is_empty());
        assert!(perimeter(Rect {
            x: 0,
            y: 0,
            width: 1,
            height: 9
        })
        .is_empty());
    }

    #[test]
    fn closest_index_finds_the_nearest_border_cell() {
        let cells = perimeter(Rect {
            x: 0,
            y: 0,
            width: 10,
            height: 5,
        });
        assert_eq!(closest_index(&cells, (9, 0)), 9);
        assert_eq!(closest_index(&cells, (0, 0)), 0);
        assert!(closest_index(&cells, (5, 0)) <= 9);
    }

    #[test]
    fn fade_decreases_with_distance() {
        assert!((fade(0, 20) - 1.0).abs() < f32::EPSILON);
        assert!(fade(10, 20) < fade(1, 20));
        assert_eq!(fade(20, 20), 0.0);
        assert_eq!(fade(99, 20), 0.0);
    }

    #[test]
    fn neon_glyphs_are_ascii_or_box_drawing() {
        let red = theme::resolve("noir-rouge").unwrap();
        assert!(red.rail_arrow.is_ascii());
        assert!(is_box_drawing("═"));
        assert!(is_box_drawing("│"));
        assert!(!is_box_drawing("a"));
    }

    #[test]
    fn wave_color_fades_from_bright_to_nothing() {
        assert_eq!(wave_color_ansi(1.0, 0, 0), Some(NEON));
        assert_eq!(wave_color_ansi(0.0, 0, 0), None);
        assert_eq!(wave_color_ansi(0.01, 0, 0), None);
        assert_eq!(wave_color_ansi(0.3, 0, 0), Some(NEON_DIM));
        assert_eq!(wave_color_ansi(0.1, 0, 0), Some(NEON_DIM));
        assert_eq!(wave_color_ansi(0.1, 1, 0), None);
        match (wave_color_rgb(1.0), wave_color_rgb(0.2)) {
            (Color::Rgb(br, _, _), Color::Rgb(dr, _, _)) => assert!(br > dr),
            _ => panic!("le dégradé RGB est attendu"),
        }
    }
}
