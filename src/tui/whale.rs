//! Animation « baleine deepseek » : un overlay joué quand le modèle actif
//! devient un `deepseek`. La baleine sort de l'eau, décrit un arc, puis plonge
//! dans le profond en laissant une gerbe puis des bulles.
//!
//! Choix assumés :
//! - **Tout en cellules** (demi-blocs `▀▄█`, `~≈-`, `░▒`) : aucun pixel hors
//!   d'une cellule, donc c'est portable sur n'importe quel terminal et ça ne
//!   dépend d'aucun protocole graphique.
//! - **Rien que du `f(t)`** : pas d'état par frame, pas d'aléatoire. La
//!   géométrie est une fonction du temps, ce qui rend chaque image reproductible
//!   et testable (le prototype est `tools/whale_preview.py`).
//! - **Aspect 2:1** : une cellule est deux fois plus haute que large ; la
//!   silhouette est donc échantillonnée en conséquence, sinon la baleine
//!   paraîtrait écrasée.
//! - **Pas de rotation de glyphes** : la baleine est une silhouette rasterisée,
//!   pivotée mathématiquement (`pitch`) avant rastérisation.

use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Borders, Clear};
use ratatui::Frame;
use std::time::{Duration, Instant};

/// Durée totale du plan (émergence → profondeur).
pub const DURATION: Duration = Duration::from_millis(3_200);
/// Largeur du sprite, en cellules.
pub const SPRITE_COLS: u16 = 22;
/// Hauteur du sprite, en cellules.
pub const SPRITE_ROWS: u16 = 6;
/// Sous-lignes par cellule (demi-blocs).
const SUB_ROWS: u16 = SPRITE_ROWS * 2;

// Cache de l'animation seulement si elle a un sens.
const MIN_WIDTH: u16 = 44;
const MIN_HEIGHT: u16 = 12;

const BODY: Color = Color::Rgb(231, 240, 250);
const BELLY: Color = Color::Rgb(168, 199, 226);
const FIN: Color = Color::Rgb(140, 176, 208);
const EYE: Color = Color::Rgb(14, 20, 30);
const SEA: Color = Color::Rgb(46, 134, 194);
const DEEP: Color = Color::Rgb(4, 13, 24);
const FOAM: Color = Color::Rgb(176, 240, 255);

// Types de matière renvoyés par `whale_shape` (index de couleur).
const M_BACK: u8 = 1;
const M_BELLY: u8 = 2;
const M_FLUKE: u8 = 3;
const M_FIN: u8 = 4;
const M_DORSAL: u8 = 5;
const M_EYE: u8 = 9;

/// Le modèle déclenche-t-il l'animation ? (comparaison insensible à la casse)
pub fn wanted(model: &str) -> bool {
    model.to_ascii_lowercase().contains("deepseek")
}

/// Une animation en cours : uniquement son instant de départ.
#[derive(Debug, Clone)]
pub struct Whale {
    t0: Instant,
    duration: Duration,
}

impl Whale {
    pub fn new() -> Self {
        Self {
            t0: Instant::now(),
            duration: DURATION,
        }
    }

    /// Départ antidaté (tests, rejeu).
    pub fn from_start(t0: Instant) -> Self {
        Self {
            t0,
            duration: DURATION,
        }
    }

    /// Progression dans `[0, 1]`.
    pub fn progress(&self) -> f32 {
        let total = self.duration.as_secs_f32().max(0.001);
        (self.t0.elapsed().as_secs_f32() / total).clamp(0.0, 1.0)
    }

    pub fn done(&self) -> bool {
        self.t0.elapsed() >= self.duration
    }

    /// Libellé de la phase courante (affiché en bas de l'overlay).
    pub fn label(&self) -> &'static str {
        label_for(self.progress() / 0.94)
    }
}

impl Default for Whale {
    fn default() -> Self {
        Self::new()
    }
}

fn label_for(phase: f32) -> &'static str {
    if phase < 0.14 {
        "émergence"
    } else if phase < 0.46 {
        "saut"
    } else if phase < 0.62 {
        "apex"
    } else if phase < 0.88 {
        "plongeon"
    } else {
        "profondeur"
    }
}

// ── Silhouette ───────────────────────────────────────────────────────────────
// Coordonnées locales : `x` de 0 (pointe de la caudale) à 1 (bout du museau),
// `y` de 0 (dos) à 1 (ventre). Deux profils interpolés = un cétacé lisible.
const TOP: [(f32, f32); 10] = [
    (0.00, 0.42),
    (0.10, 0.39),
    (0.20, 0.32),
    (0.35, 0.25),
    (0.52, 0.21),
    (0.68, 0.22),
    (0.80, 0.26),
    (0.90, 0.34),
    (0.96, 0.39),
    (1.00, 0.47),
];
const BOT: [(f32, f32); 9] = [
    (0.00, 0.46),
    (0.12, 0.50),
    (0.28, 0.56),
    (0.46, 0.60),
    (0.62, 0.61),
    (0.76, 0.59),
    (0.88, 0.53),
    (0.96, 0.48),
    (1.00, 0.47),
];

fn interp(pts: &[(f32, f32)], x: f32) -> f32 {
    if x <= pts[0].0 {
        return pts[0].1;
    }
    for w in pts.windows(2) {
        let ((x0, y0), (x1, y1)) = (w[0], w[1]);
        if x <= x1 {
            let t = if x1 > x0 { (x - x0) / (x1 - x0) } else { 0.0 };
            return y0 + (y1 - y0) * t;
        }
    }
    pts[pts.len() - 1].1
}

/// Matière d'un point local (0 = vide). Fonction pure : testable sans terminal.
pub fn whale_shape(x: f32, y: f32) -> u8 {
    // corps : dos, ventre, et museau arrondi
    if (0.06..=1.0).contains(&x) {
        let (top, bot) = (interp(&TOP, x), interp(&BOT, x));
        if (top..=bot).contains(&y) {
            return if y > bot - 0.13 { M_BELLY } else { M_BACK };
        }
    }
    // caudale : éventail vertical fourchu à gauche
    if x <= 0.17 {
        let spread = 0.26 * (1.0 - x / 0.17) + 0.03;
        let d = (y - 0.44).abs();
        if d <= spread && !(x < 0.09 && d < 0.07) {
            return M_FLUKE;
        }
    }
    // dorsale : petit aileron sur le dos
    if (0.42..=0.54).contains(&x) {
        let t = (x - 0.42) / 0.12;
        let top = interp(&TOP, x);
        if top - (0.14 - 0.10 * (t - 0.5).abs() * 2.0) <= y && y <= top {
            return M_DORSAL;
        }
    }
    // pectorale : triangle sous le ventre
    if (0.58..=0.74).contains(&x) {
        let t = (0.74 - x) / 0.16;
        let bot = interp(&BOT, x);
        if bot <= y && y <= bot + 0.18 * t {
            return M_FIN;
        }
    }
    0
}

/// Une cellule du sprite : (glyphe, matière).
pub type SpriteCell = (char, u8);

/// Rasterise la baleine pivotée de `pitch` (radians) en demi-blocs.
pub fn rasterize(pitch: f32) -> Vec<Vec<SpriteCell>> {
    let mut out = vec![vec![(' ', 0u8); SPRITE_COLS as usize]; SPRITE_ROWS as usize];
    let (sin_p, cos_p) = (-pitch).sin_cos();
    for (row, line) in out.iter_mut().enumerate() {
        for (col, cell) in line.iter_mut().enumerate() {
            let mut mat = [0u8; 2];
            for (half, slot) in mat.iter_mut().enumerate() {
                let lx = (col as f32 + 0.5) / SPRITE_COLS as f32;
                let ly = (row as f32 * 2.0 + half as f32 + 0.5) / SUB_ROWS as f32;
                // rotation inverse autour du centre de la silhouette
                let (dx, dy) = (lx - 0.5, ly - 0.5);
                let rx = dx * cos_p - dy * sin_p + 0.5;
                let ry = dx * sin_p + dy * cos_p + 0.5;
                *slot = whale_shape(rx, ry);
            }
            let glyph = match (mat[0] != 0, mat[1] != 0) {
                (false, false) => continue,
                (true, true) => '█',
                (true, false) => '▀',
                (false, true) => '▄',
            };
            let kind = if mat[0] != 0 { mat[0] } else { mat[1] };
            *cell = (glyph, kind);
        }
    }
    // œil : une demi-cellule sombre près du museau
    let (ec, er) = (
        (SPRITE_COLS as f32 * 0.88) as usize,
        (SPRITE_ROWS as f32 * 0.55) as usize,
    );
    if let Some(cell) = out.get_mut(er).and_then(|l| l.get_mut(ec)) {
        if cell.0 != ' ' {
            *cell = ('▄', M_EYE);
        }
    }
    out
}

// ── Trajectoire ──────────────────────────────────────────────────────────────

/// Position (coin haut-gauche, en cellules flottantes) et inclinaison.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pose {
    pub x: f32,
    pub y: f32,
    pub pitch: f32,
}

/// Ligne d'eau dans la zone intérieure de l'overlay.
pub fn sea_line(inner: Rect) -> u16 {
    inner.y + (inner.height.saturating_sub(1) * 7 / 10)
}

/// Trajectoire paramétrique. `phase` ∈ `[0, 1]`.
///
/// 0 → 0,14 poussée sous l'eau · 0,14 → 0,46 montée · 0,46 → 0,62 apex ·
/// 0,62 → 0,88 plongée · 0,88 → 1 disparition dans le profond.
pub fn pose(phase: f32, inner: Rect) -> Pose {
    let phase = phase.clamp(0.0, 1.0);
    let sea = sea_line(inner) as f32;
    let x0 = inner.x as f32 + 2.0;
    let x1 = (inner.x as f32 + inner.width as f32) - SPRITE_COLS as f32 - 3.0;
    let span = (x1 - x0).max(1.0);
    let up_y = sea - SPRITE_ROWS as f32 + 2.0;
    let apex_y = inner.y as f32 + 1.0;

    if phase < 0.14 {
        let u = phase / 0.14;
        return Pose {
            x: x0,
            y: up_y + 5.0 * (1.0 - u),
            pitch: -0.15,
        };
    }
    if phase < 0.46 {
        let u = (phase - 0.14) / 0.32;
        return Pose {
            x: x0 + span * 0.55 * u,
            y: up_y + (apex_y - up_y) * (u * std::f32::consts::FRAC_PI_2).sin(),
            pitch: -0.55 + 0.45 * u,
        };
    }
    if phase < 0.62 {
        let u = (phase - 0.46) / 0.16;
        return Pose {
            x: x0 + span * (0.55 + 0.15 * u),
            y: apex_y,
            pitch: 0.0,
        };
    }
    if phase < 0.88 {
        let u = (phase - 0.62) / 0.26;
        return Pose {
            x: x0 + span * (0.70 + 0.30 * u),
            y: apex_y + (sea - apex_y + 2.0) * u.powf(1.4),
            pitch: 0.15 + 0.55 * u,
        };
    }
    let u = (phase - 0.88) / 0.12;
    Pose {
        x: x0 + span,
        y: sea + 1.0 + 4.0 * u,
        pitch: 0.75,
    }
}

// ── Rendu ────────────────────────────────────────────────────────────────────

fn lerp(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    match (a, b) {
        (Color::Rgb(r1, g1, b1), Color::Rgb(r2, g2, b2)) => Color::Rgb(
            (r1 as f32 + (r2 as f32 - r1 as f32) * t).round() as u8,
            (g1 as f32 + (g2 as f32 - g1 as f32) * t).round() as u8,
            (b1 as f32 + (b2 as f32 - b1 as f32) * t).round() as u8,
        ),
        _ => b,
    }
}

fn put(buf: &mut Buffer, x: f32, y: f32, ch: char, color: Color) {
    let (x, y) = (x.round(), y.round());
    if x < 0.0 || y < 0.0 {
        return;
    }
    let (x, y) = (x as u16, y as u16);
    if x >= buf.area.width || y >= buf.area.height {
        return;
    }
    let cell = &mut buf[(x, y)];
    let mut s = [0u8; 4];
    cell.set_symbol(ch.encode_utf8(&mut s));
    cell.set_fg(color);
}

fn put_str(buf: &mut Buffer, x: u16, y: u16, s: &str, color: Color) {
    for (i, ch) in s.chars().enumerate() {
        put(buf, (x as usize + i) as f32, y as f32, ch, color);
    }
}

/// Rectangle de l'overlay, centré : 64×16 au plus, avec une marge.
pub fn overlay_rect(area: Rect) -> Option<Rect> {
    if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
        return None;
    }
    // Marge de 2 colonnes / 1 ligne pour que l'overlay reste un *overlay*.
    let w = 64.min(area.width.saturating_sub(4));
    let h = 16.min(area.height.saturating_sub(2));
    let x = area.x + (area.width - w) / 2;
    let y = area.y + (area.height - h) / 2;
    Some(Rect::new(x, y, w, h))
}

/// Dessine l'animation. Ne fait rien si l'écran est trop petit.
pub fn draw(f: &mut Frame, area: Rect, whale: &Whale) {
    let Some(rect) = overlay_rect(area) else {
        return;
    };
    f.render_widget(Clear, rect);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(crate::theme::dim()))
        .title(" deepseek ")
        .title_alignment(Alignment::Center);
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    if inner.width < SPRITE_COLS + 6 || inner.height < SPRITE_ROWS + 2 {
        return;
    }

    let t = whale.progress();
    let secs = t * DURATION.as_secs_f32();
    let phase = (t / 0.94).min(1.0);
    let sea = sea_line(inner);
    let buf = f.buffer_mut();

    draw_sea(buf, inner, sea, secs);
    let head = draw_sprite(buf, inner, sea, phase);
    draw_splash(buf, inner, sea, t);
    draw_bubbles(buf, inner, sea, t, head);
    // Libellé en haut à gauche : le bas de l'overlay est occupé par la plongée.
    put_str(
        buf,
        inner.x + 2,
        inner.y,
        &format!("◈ {}", label_for(phase)),
        crate::theme::dim(),
    );
}

fn draw_sea(buf: &mut Buffer, inner: Rect, sea: u16, secs: f32) {
    let bottom = inner.y + inner.height.saturating_sub(1);
    for y in sea..=bottom {
        let depth = (y - sea) as f32 / (bottom.saturating_sub(sea)).max(1) as f32;
        for x in inner.x..inner.x + inner.width {
            let xf = x as f32;
            let h = (xf * 0.30 + secs * 3.4).sin() + 0.55 * (xf * 0.09 - secs * 1.9).sin();
            if y == sea {
                let (ch, color) = if h > 0.85 {
                    ('≈', FOAM)
                } else if h > 0.10 {
                    ('~', SEA)
                } else {
                    ('-', lerp(SEA, DEEP, 0.30))
                };
                put(buf, xf, y as f32, ch, color);
                continue;
            }
            let k = depth * 2.6 + (xf * 0.8 + y as f32 * 1.7 + secs * 2.0).sin() * 0.30;
            if k < 0.85 {
                continue;
            }
            let ch = if k < 1.9 { '░' } else { '▒' };
            put(buf, xf, y as f32, ch, lerp(SEA, DEEP, 0.22 + depth * 1.15));
        }
    }
}

/// Dessine la baleine ; renvoie la tête (pour les bulles).
fn draw_sprite(buf: &mut Buffer, inner: Rect, sea: u16, phase: f32) -> (f32, f32) {
    let pose = pose(phase, inner);
    let sprite = rasterize(pose.pitch);
    let sink = if phase > 0.88 {
        ((phase - 0.88) / 0.12).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let floor = inner.y + inner.height;
    for (r, line) in sprite.iter().enumerate() {
        for (c, (ch, kind)) in line.iter().enumerate() {
            if *ch == ' ' {
                continue;
            }
            let y = pose.y + r as f32;
            if y < inner.y as f32 || y >= floor as f32 {
                continue;
            }
            // immergée : la silhouette prend la teinte de l'eau
            let under = ((y - sea as f32 + 1.0) / 4.0 + sink).clamp(0.0, 1.0);
            let base = match *kind {
                M_BELLY => BELLY,
                M_FLUKE | M_FIN | M_DORSAL => FIN,
                M_EYE => EYE,
                _ => BODY,
            };
            put(
                buf,
                pose.x + c as f32,
                y,
                *ch,
                lerp(base, lerp(SEA, DEEP, 0.55), under * 0.85),
            );
        }
    }
    (
        pose.x + SPRITE_COLS as f32,
        pose.y + SPRITE_ROWS as f32 / 2.0,
    )
}

fn draw_splash(buf: &mut Buffer, inner: Rect, sea: u16, t: f32) {
    let right = inner.x + inner.width.saturating_sub(1);
    let floor = (inner.y + inner.height).saturating_sub(1);
    for (tb, xb, power) in [
        (0.12, inner.x + 4, 1.0),
        (0.88, right.saturating_sub(12), 0.85),
    ] {
        let dt = t - tb;
        if !(0.0..=0.38).contains(&dt) {
            continue;
        }
        let u = dt / 0.38;
        for i in 0..17 {
            let vx = (i as f32 - 8.0) * 1.0 * power;
            let vy = -8.5 + (i as f32 - 8.0).abs() * 0.62;
            let px = xb as f32 + vx * u * 2.4;
            let py = sea as f32 + vy * u + 12.0 * u * u;
            if py > floor as f32 || py < inner.y as f32 {
                continue;
            }
            let ch = match i % 3 {
                0 => '·',
                1 => '\'',
                _ => '°',
            };
            put(buf, px, py, ch, lerp(FOAM, SEA, u * 0.85));
        }
        if u <= 0.55 {
            for i in -9i32..=9 {
                let px = xb as f32 + i as f32 * 2.0 * power;
                let ch = if i.abs() < 3 { '*' } else { '·' };
                put(buf, px, sea as f32, ch, lerp(FOAM, SEA, 0.35));
            }
        }
    }
}

fn draw_bubbles(buf: &mut Buffer, inner: Rect, sea: u16, t: f32, head: (f32, f32)) {
    if t < 0.88 {
        return;
    }
    let floor = (inner.y + inner.height).saturating_sub(1);
    for i in 0..9 {
        let px = head.0 - 10.0 + i as f32 * 4.0 + (i as f32 * 2.3 + t * 5.0).sin() * 2.0;
        let py = sea as f32 + 1.0 + i as f32 * 0.5 + (t - 0.88) * 10.0;
        if py >= floor as f32 {
            continue;
        }
        put(
            buf,
            px,
            py,
            if i % 2 == 0 { '·' } else { 'o' },
            lerp(FOAM, SEA, 0.45),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn inner_rect() -> Rect {
        Rect::new(10, 4, 62, 14)
    }

    #[test]
    fn only_deepseek_triggers_the_animation() {
        assert!(wanted("deepseek/deepseek-chat"));
        assert!(wanted("DeepSeek-R1"));
        assert!(!wanted("gpt-4o-mini"));
        assert!(!wanted("qwen3-coder"));
    }

    #[test]
    fn shape_is_empty_far_away_and_filled_inside() {
        assert_eq!(whale_shape(0.5, -3.0), 0);
        assert_eq!(whale_shape(1.9, 0.5), 0);
        assert_ne!(whale_shape(0.5, 0.5), 0); // corps
        assert_ne!(whale_shape(0.05, 0.30), 0); // lobe de la caudale
        assert_eq!(whale_shape(0.05, 0.44), 0); // échancrure de la caudale
    }

    #[test]
    fn sprite_is_non_empty_and_has_demi_blocks() {
        let s = rasterize(0.0);
        assert_eq!(s.len(), SPRITE_ROWS as usize);
        assert_eq!(s[0].len(), SPRITE_COLS as usize);
        let filled = s.iter().flatten().filter(|(c, _)| *c != ' ').count();
        // Assez de cellules pour lire une baleine, pas un point.
        assert!(filled > 25, "seulement {filled} cellules pleines");
        assert!(
            s.iter()
                .flatten()
                .any(|(c, _)| *c == '▀' || *c == '▄' || *c == '█'),
            "aucun demi-bloc : le sprite n'est pas rasterisé"
        );
    }

    #[test]
    fn trajectory_goes_up_then_dives() {
        let inner = inner_rect();
        let sea = sea_line(inner) as f32;
        let first = pose(0.05, inner);
        let apex = pose(0.54, inner);
        let end = pose(1.0, inner);
        assert!(apex.y < first.y, "l'apex doit être plus haut que le départ");
        assert!(apex.y < sea, "l'apex doit être hors de l'eau");
        assert!(end.y > sea, "la fin doit être sous la ligne d'eau");
        // Le saut avance vers la droite.
        assert!(end.x > apex.x && apex.x >= first.x);
        // L'inclinaison change de signe entre montée et plongée.
        assert!(pose(0.2, inner).pitch < 0.0);
        assert!(pose(0.8, inner).pitch > 0.0);
    }

    #[test]
    fn progress_and_done_are_driven_by_time() {
        let fresh = Whale::from_start(Instant::now());
        assert!(fresh.progress() < 0.1);
        assert!(!fresh.done());
        let finished = Whale::from_start(Instant::now() - DURATION - Duration::from_millis(10));
        assert!(finished.done());
        assert!((finished.progress() - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn overlay_is_skipped_on_a_tiny_screen() {
        assert!(overlay_rect(Rect::new(0, 0, 30, 10)).is_none());
        let r = overlay_rect(Rect::new(0, 0, 120, 40)).expect("overlay attendu");
        assert!(r.width <= 64 && r.height <= 16);
        assert!(r.x >= 2 && r.y >= 1);
    }

    /// Outil de vérification visuelle (ignoré en CI) : exporte chaque frame en
    /// texte + une carte de palette, pour la comparer au prototype Python.
    #[test]
    #[ignore]
    fn dump_frames_for_preview() {
        use ratatui::style::Color as C;
        let pal = |c: C| -> char {
            match c {
                C::Rgb(231, 240, 250) => 'B',
                C::Rgb(168, 199, 226) => 'b',
                C::Rgb(140, 176, 208) => 'f',
                C::Rgb(14, 20, 30) => 'e',
                C::Rgb(46, 134, 194) => 's',
                C::Rgb(4, 13, 24) => 'd',
                C::Rgb(176, 240, 255) => 'o',
                C::Rgb(0, 0, 0) => 'd',
                _ => 't',
            }
        };
        let mut term = Terminal::new(TestBackend::new(78, 26)).unwrap();
        let mut out = String::new();
        for i in 0..54 {
            let w = Whale::from_start(Instant::now() - DURATION.mul_f32(i as f32 / 54.0));
            term.draw(|f| draw(f, f.area(), &w)).unwrap();
            let buf = term.backend().buffer().clone();
            for y in 0..buf.area.height {
                for x in 0..buf.area.width {
                    out.push_str(buf[(x, y)].symbol());
                }
                out.push('\n');
            }
            out.push('\u{1e}');
            for y in 0..buf.area.height {
                for x in 0..buf.area.width {
                    let cell = &buf[(x, y)];
                    out.push(pal(cell.fg));
                }
                out.push('\n');
            }
            out.push('\u{1d}');
        }
        std::fs::write("/tmp/whale-rust.txt", out).unwrap();
    }

    #[test]
    fn drawing_puts_the_whale_and_the_sea_on_screen() {
        let mut term = Terminal::new(TestBackend::new(100, 30)).unwrap();
        // Départ antidaté : on force la phase « apex » pour un rendu déterministe.
        let w = Whale::from_start(Instant::now() - DURATION.mul_f32(0.55));
        term.draw(|f| draw(f, f.area(), &w)).unwrap();
        let buf = term.backend().buffer().clone();
        let mut text = String::new();
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                text.push_str(buf[(x, y)].symbol());
            }
            text.push('\n');
        }
        assert!(text.contains("deepseek"), "titre de l'overlay absent");
        assert!(text.contains('~') || text.contains('≈'), "pas de mer");
        assert!(
            text.contains('▀') || text.contains('▄') || text.contains('█'),
            "pas de baleine"
        );
    }
}
