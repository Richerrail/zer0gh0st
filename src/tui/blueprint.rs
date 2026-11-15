//! Fond « plan » du thème `paper` : cadre gradué, cartouche et trame de points.
//!
//! Pourquoi ce fichier séparé : c'est **de la décoration**, pas de la mise en
//! page. Le chat garde sa géométrie ; on peint seulement des cellules que le
//! texte ne recouvre pas (la trame est dessinée *avant* les messages, qui
//! réécrivent leurs propres cellules).
//!
//! Contrainte de lisibilité : la trame doit rester **≤ 1,3:1** de contraste avec
//! le papier, sinon elle détruit la lecture. Sur `paper` (250,249,245), cela
//! donne `#E4E2D9` — une texture, pas une image.
//!
//! Le cadre coûte **une cellule** de marge : l'appelant insère le chat d'un cran
//! (`rect.inner(Margin::new(1, 1))`) quand ce fond est actif.

use ratatui::layout::{Margin, Rect};
use ratatui::style::{Color, Style};
use ratatui::Frame;

use crate::theme;

/// Couleur de la trame (très claire : ~1,2:1 sur le papier).
const GRID: Color = Color::Rgb(228, 226, 217);
/// Pas de la trame, en colonnes et en lignes.
const GRID_X: u16 = 4;
const GRID_Y: u16 = 2;

/// Le fond « plan » est-il actif ? (thème `paper` uniquement)
pub fn active() -> bool {
    theme::active().id == "paper"
}

/// Zone de texte à l'intérieur du cadre.
pub fn inner(area: Rect) -> Rect {
    area.inner(Margin::new(1, 1))
}

/// Peint le plan : cadre gradué, cartouche, trame. À appeler **avant** le chat.
pub fn draw(f: &mut Frame, area: Rect, title: &str) {
    if area.width < 12 || area.height < 4 {
        return;
    }
    let buf = f.buffer_mut();
    let (x0, y0) = (area.x, area.y);
    let (x1, y1) = (area.x + area.width - 1, area.y + area.height - 1);
    let border = Style::default().fg(theme::border());

    let put = |buf: &mut ratatui::buffer::Buffer, x: u16, y: u16, ch: &str, st: Style| {
        if x < buf.area.width && y < buf.area.height {
            let cell = &mut buf[(x, y)];
            cell.set_symbol(ch);
            cell.set_style(st);
        }
    };

    // Cadre : traits fins + croix d'angle.
    for x in x0..=x1 {
        put(buf, x, y0, "─", border);
        put(buf, x, y1, "─", border);
    }
    for y in y0..=y1 {
        put(buf, x0, y, "│", border);
        put(buf, x1, y, "│", border);
    }
    for (x, y) in [(x0, y0), (x1, y0), (x0, y1), (x1, y1)] {
        put(buf, x, y, "┼", border);
    }
    // Graduations : tous les 8 colonnes en haut/bas, tous les 4 rangs sur les
    // côtés — comme les repères d'un plan.
    let mut x = x0 + 8;
    while x < x1 {
        put(buf, x, y0, "┬", border);
        put(buf, x, y1, "┴", border);
        x += 8;
    }
    let mut y = y0 + 3;
    while y < y1 {
        put(buf, x0, y, "├", border);
        put(buf, x1, y, "┤", border);
        y += 4;
    }

    // Cartouche de plan dans le bord supérieur.
    for (tx, ch) in (x0 + 3..x1.saturating_sub(1)).zip(title.chars()) {
        put(
            buf,
            tx,
            y0,
            &ch.to_string(),
            Style::default().fg(theme::dim()),
        );
    }

    // Trame de points, uniquement dans les cellules vides.
    let inner = inner(area);
    for y in (inner.y..inner.y + inner.height).step_by(GRID_Y as usize) {
        for x in (inner.x..inner.x + inner.width).step_by(GRID_X as usize) {
            if x < buf.area.width && y < buf.area.height && buf[(x, y)].symbol() == " " {
                put(buf, x, y, "·", Style::default().fg(GRID));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_paper_theme_has_the_blueprint() {
        crate::theme::activate("paper").unwrap();
        assert!(active());
        crate::theme::activate("noir").unwrap();
        assert!(!active());
        // Le thème est global : on remet l'état initial pour les autres tests.
        crate::theme::activate("ghost").unwrap();
        assert!(!active());
    }

    #[test]
    fn inner_rect_is_inset_by_one_cell() {
        let r = Rect::new(0, 10, 80, 20);
        let i = inner(r);
        assert_eq!((i.x, i.y), (1, 11));
        assert_eq!((i.width, i.height), (78, 18));
    }
}
