use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

use crate::theme::{self, BannerStyle};

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
    let bevel = theme::active().banner == BannerStyle::Bevel;
    let mask: Vec<Vec<char>> = rows.iter().map(|r| r.chars().collect()).collect();
    let mut out: Vec<Line<'static>> = if bevel {
        (0..mask.len())
            .map(|y| bevel_line(&mask, y, animated, phase))
            .collect()
    } else {
        rows.into_iter()
            .map(|row| colorize(&row, width, animated, phase))
            .collect()
    };
    out.push(Line::from(Span::styled(
        format!("   {TAGLINE}"),
        Style::default().fg(theme::dim()),
    )));
    out
}

/// Teinte d'une cellule dans le style `Bevel` : 0 = arête claire,
/// 1 = face, 2 = ombre. Calculée sur le **masque** des lettres (haut/gauche
/// vides → arête ; bas/droite vides → ombre), donc valable pour n'importe quel
/// glyphe sans le redessiner.
fn bevel_tone(mask: &[Vec<char>], y: usize, x: usize) -> u8 {
    let filled = |yy: isize, xx: isize| -> bool {
        let (h, w) = (mask.len() as isize, mask[0].len() as isize);
        if yy < 0 || xx < 0 || yy >= h || xx >= w {
            return false;
        }
        mask[yy as usize][xx as usize] != ' '
    };
    if !filled(y as isize, x as isize) {
        return 1;
    }
    let up = filled(y as isize - 1, x as isize);
    let left = filled(y as isize, x as isize - 1);
    let down = filled(y as isize + 1, x as isize);
    let right = filled(y as isize, x as isize + 1);
    if !up || !left {
        0 // arête éclairée
    } else if !down || !right {
        2 // ombre portée
    } else {
        1
    }
}

/// Les quatre encres du style `Bevel` (bleu, rouge, vert, ocre) : lisibles sur
/// papier blanc comme sur fond sombre.
const INKS: [(u8, u8, u8); 4] = [
    (31, 78, 140), // bleu de plan
    (166, 27, 27), // rouge
    (30, 107, 58), // vert
    (138, 90, 0),  // ocre
];

/// Éclaircit (`>1`) ou assombrit (`<1`) une encre pour l'arête / l'ombre.
fn shade(c: (u8, u8, u8), f: f32) -> Color {
    let ch = |v: u8| ((v as f32 * f).clamp(0.0, 255.0)) as u8;
    Color::Rgb(ch(c.0), ch(c.1), ch(c.2))
}

/// Teinte « arc-en-ciel » pour l'état de repos, assombrie pour rester lisible
/// sur papier blanc : un vert saturé (`#00FF00`) sur blanc ne fait que 1,4:1.
fn rainbow_at(x: usize, width: usize) -> (u8, u8, u8) {
    let hue = (x as f32 * 360.0 / width.max(1) as f32) % 360.0;
    match hsv_to_rgb(hue, 0.82, 0.78) {
        Color::Rgb(r, g, b) => (r, g, b),
        _ => (31, 78, 140),
    }
}

/// Ligne du banner en relief.
///
/// Deux états, comme demandé :
/// - **au repos** : arc-en-ciel (assombri pour le papier), statique ;
/// - **quand le modèle travaille** (`animated`) : **4 encres** fixes par bande,
///   qui défilent — le relief reste, la couleur signale l'activité.
fn bevel_line(mask: &[Vec<char>], y: usize, animated: bool, phase: u16) -> Line<'static> {
    let width = mask[0].len();
    let band = 7usize;
    let shift = if animated { (phase / 6) as usize } else { 0 };
    let spans: Vec<Span> = (0..width)
        .map(|x| {
            let glyph = mask[y][x];
            if glyph == ' ' {
                return Span::raw(" ");
            }
            let base = if animated {
                INKS[((x + shift) / band) % 4]
            } else {
                rainbow_at(x, width)
            };
            let color = match bevel_tone(mask, y, x) {
                0 => shade(base, 1.45), // arête éclairée
                2 => shade(base, 0.62), // ombre
                _ => shade(base, 1.0),  // face
            };
            // Le séparateur `│` garde sa finesse même en relief.
            let ch = if glyph == '│' { "│" } else { "█" };
            Span::styled(ch, Style::default().fg(color))
        })
        .collect();
    Line::from(spans)
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
    match theme::active().banner {
        BannerStyle::Rainbow => {
            if animated {
                let hue = ((x as u32 * 360 / w.max(1) as u32) + phase as u32) % 360;
                hsv_to_rgb(hue as f32, 1.0, 1.0)
            } else {
                let t = if w <= 1 {
                    0.0
                } else {
                    x as f32 / (w - 1) as f32
                };
                let r = (70.0 * t) as u8;
                let g = (200.0 * (1.0 - t) + 90.0 * t) as u8;
                Color::Rgb(r, g, 255)
            }
        }
        BannerStyle::Mono => {
            let t = if w <= 1 {
                0.0
            } else {
                x as f32 / (w - 1) as f32
            };
            let shift = if animated { phase as f32 / 90.0 } else { 0.0 };
            let wave = (t + shift).sin() * 0.5 + 0.5;
            let gray = (70.0 + wave * 150.0) as u8;
            Color::Rgb(gray, gray, gray)
        }
        BannerStyle::Steel => {
            let t = if w <= 1 {
                0.0
            } else {
                x as f32 / (w - 1) as f32
            };
            let shift = if animated { phase as f32 / 120.0 } else { 0.0 };
            let wave = (t * 6.0 + shift).sin() * 0.22 + 0.78;
            Color::Rgb(
                (70.0 + wave * 90.0) as u8,
                (95.0 + wave * 105.0) as u8,
                (135.0 + wave * 105.0) as u8,
            )
        }
        // Le style `Bevel` ne passe pas par ce dégradé : il colore ses arêtes
        // et ses ombres (`bevel_line`). Repli : encre bleue.
        BannerStyle::Bevel => Color::Rgb(31, 78, 140),
        BannerStyle::Red => {
            let t = if w <= 1 {
                0.0
            } else {
                x as f32 / (w - 1) as f32
            };
            let shift = if animated { phase as f32 / 90.0 } else { 0.0 };
            let wave = (t + shift).sin() * 0.5 + 0.5;
            Color::Rgb(
                (95.0 + wave * 160.0) as u8,
                (8.0 + wave * 45.0) as u8,
                (14.0 + wave * 50.0) as u8,
            )
        }
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

#[cfg(test)]
mod bevel_tests {
    use super::*;

    fn mask_of(lines: &[&str]) -> Vec<Vec<char>> {
        lines.iter().map(|l| l.chars().collect()).collect()
    }

    #[test]
    fn bevel_marks_the_lit_edge_and_the_shadow() {
        // Un carré 3×3 : coin haut-gauche = arête, coin bas-droit = ombre,
        // centre = face.
        let m = mask_of(&["███", "███", "███"]);
        assert_eq!(bevel_tone(&m, 0, 0), 0, "haut-gauche : arête");
        assert_eq!(bevel_tone(&m, 1, 1), 1, "centre : face");
        assert_eq!(bevel_tone(&m, 2, 2), 2, "bas-droite : ombre");
        assert_eq!(bevel_tone(&m, 0, 2), 0, "bord haut : arête");
        assert_eq!(bevel_tone(&m, 1, 2), 2, "bord droit : ombre");
        // Bord gauche : éclairé sur toute la hauteur (lumière en haut-gauche).
        assert_eq!(bevel_tone(&m, 2, 0), 0, "bord gauche : arête");
    }

    #[test]
    fn the_four_inks_are_dark_enough_for_white_paper() {
        // Sur papier (250,249,245), chaque encre doit rester lisible même
        // assombrie pour l'arête. On vérifie surtout que rien n'est blanc.
        for (r, g, b) in INKS {
            let lum = 0.2126 * r as f32 + 0.7152 * g as f32 + 0.0722 * b as f32;
            assert!(lum < 140.0, "encre trop claire sur papier : {r},{g},{b}");
        }
    }

    #[test]
    fn banner_rows_have_the_same_width() {
        let lines = lines();
        let widths: Vec<usize> = lines.iter().map(|l| l.width()).collect();
        assert!(widths.len() >= 6, "banner + tagline attendus");
        assert!(widths.iter().take(5).all(|w| *w == widths[0]));
    }
}
