//! Buffer de dessin pour les modules fun.
//!
//! Abstraction au-dessus de ratatui pour simplifier le rendu.

use serde::{Deserialize, Serialize};

/// Buffer de caractères pour le dessin.
///
/// Représente une zone du terminal où le module peut dessiner.
/// Chaque cellule contient un caractère et un style.
///
/// # Safety FFI
///
/// Cette structure est passée à travers la frontière FFI. Elle utilise
/// un layout C compatible avec des pointeurs bruts pour les données.
#[derive(Debug, Clone)]
#[repr(C)]
pub struct Buffer {
    /// Largeur du buffer.
    pub width: u16,

    /// Hauteur du buffer.
    pub height: u16,

    /// Cellules du buffer (row-major).
    ///
    /// Note: Ce Vec est géré par Rust des deux côtés de la frontière FFI.
    /// Le module ne doit pas le réallouer.
    cells: Vec<Cell>,
}

/// Une cellule du buffer.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[repr(C)]
pub struct Cell {
    /// Caractère affiché.
    pub symbol: String,

    /// Style de la cellule.
    pub style: Style,
}

/// Style d'une cellule.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[repr(C)]
pub struct Style {
    /// Couleur de premier plan.
    pub fg: Option<Color>,

    /// Couleur d'arrière-plan.
    pub bg: Option<Color>,

    /// Gras.
    pub bold: bool,

    /// Italique.
    pub italic: bool,

    /// Souligné.
    pub underline: bool,

    /// Inversé (vidéo inverse).
    pub reversed: bool,
}

/// Couleur terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Color {
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
    Gray,
    DarkGray,
    LightRed,
    LightGreen,
    LightYellow,
    LightBlue,
    LightMagenta,
    LightCyan,
    Rgb(u8, u8, u8),
    Indexed(u8),
}

impl Buffer {
    /// Crée un nouveau buffer vide.
    pub fn new(width: u16, height: u16) -> Self {
        let size = (width as usize) * (height as usize);
        Self {
            width,
            height,
            cells: vec![Cell::default(); size],
        }
    }

    /// Retourne une cellule mutable à la position donnée.
    pub fn get_mut(&mut self, x: u16, y: u16) -> Option<&mut Cell> {
        if x < self.width && y < self.height {
            let idx = (y as usize) * (self.width as usize) + (x as usize);
            self.cells.get_mut(idx)
        } else {
            None
        }
    }

    /// Retourne une cellule à la position donnée.
    pub fn get(&self, x: u16, y: u16) -> Option<&Cell> {
        if x < self.width && y < self.height {
            let idx = (y as usize) * (self.width as usize) + (x as usize);
            self.cells.get(idx)
        } else {
            None
        }
    }

    /// Définit un caractère à la position donnée.
    pub fn set_char(&mut self, x: u16, y: u16, c: char) {
        if let Some(cell) = self.get_mut(x, y) {
            cell.symbol = c.to_string();
        }
    }

    /// Définit une chaîne à partir de la position donnée.
    pub fn set_string(&mut self, x: u16, y: u16, s: &str) {
        for (i, c) in s.chars().enumerate() {
            let x = x.saturating_add(i as u16);
            if x >= self.width {
                break;
            }
            self.set_char(x, y, c);
        }
    }

    /// Définit une chaîne avec style.
    pub fn set_string_styled(&mut self, x: u16, y: u16, s: &str, style: Style) {
        for (i, c) in s.chars().enumerate() {
            let x = x.saturating_add(i as u16);
            if x >= self.width {
                break;
            }
            if let Some(cell) = self.get_mut(x, y) {
                cell.symbol = c.to_string();
                cell.style = style.clone();
            }
        }
    }

    /// Remplit une zone avec un caractère.
    pub fn fill(&mut self, x: u16, y: u16, width: u16, height: u16, c: char) {
        for dy in 0..height {
            for dx in 0..width {
                self.set_char(x + dx, y + dy, c);
            }
        }
    }

    /// Remplit tout le buffer avec un caractère.
    pub fn clear(&mut self, c: char) {
        self.fill(0, 0, self.width, self.height, c);
    }

    /// Dessine une bordure autour du buffer.
    pub fn draw_border(&mut self, style: BorderStyle) {
        let (tl, tr, bl, br, h, v) = style.chars();

        // Coins
        self.set_char(0, 0, tl);
        self.set_char(self.width - 1, 0, tr);
        self.set_char(0, self.height - 1, bl);
        self.set_char(self.width - 1, self.height - 1, br);

        // Bordures horizontales
        for x in 1..self.width - 1 {
            self.set_char(x, 0, h);
            self.set_char(x, self.height - 1, h);
        }

        // Bordures verticales
        for y in 1..self.height - 1 {
            self.set_char(0, y, v);
            self.set_char(self.width - 1, y, v);
        }
    }

    /// Dessine une ligne horizontale.
    pub fn draw_hline(&mut self, x: u16, y: u16, width: u16, c: char) {
        for dx in 0..width {
            self.set_char(x + dx, y, c);
        }
    }

    /// Dessine une ligne verticale.
    pub fn draw_vline(&mut self, x: u16, y: u16, height: u16, c: char) {
        for dy in 0..height {
            self.set_char(x, y + dy, c);
        }
    }

    /// Convertit le buffer en string (pour debug).
    pub fn to_string(&self) -> String {
        let mut s = String::new();
        for y in 0..self.height {
            for x in 0..self.width {
                if let Some(cell) = self.get(x, y) {
                    s.push_str(&cell.symbol);
                }
            }
            s.push('\n');
        }
        s
    }
}

/// Style de bordure.
#[derive(Debug, Clone, Copy)]
pub enum BorderStyle {
    /// Simple: ┌─┐│└─┘
    Single,

    /// Double: ╔═╗║╚═╝
    Double,

    /// Arrondi: ╭─╮│╰─╯
    Rounded,

    /// Épais: ┏━┓┃┗━┛
    Thick,
}

impl BorderStyle {
    fn chars(&self) -> (char, char, char, char, char, char) {
        match self {
            Self::Single => ('┌', '┐', '└', '┘', '─', '│'),
            Self::Double => ('╔', '╗', '╚', '╝', '═', '║'),
            Self::Rounded => ('╭', '╮', '╰', '╯', '─', '│'),
            Self::Thick => ('┏', '┓', '┗', '┛', '━', '┃'),
        }
    }
}

impl Default for BorderStyle {
    fn default() -> Self {
        Self::Single
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_buffer() {
        let mut buf = Buffer::new(10, 5);
        buf.set_string(0, 0, "Hello");
        assert_eq!(buf.get(0, 0).unwrap().symbol, "H");
        assert_eq!(buf.get(4, 0).unwrap().symbol, "o");
    }

    #[test]
    fn test_border() {
        let mut buf = Buffer::new(5, 3);
        buf.draw_border(BorderStyle::Single);
        assert_eq!(buf.get(0, 0).unwrap().symbol, "┌");
        assert_eq!(buf.get(4, 0).unwrap().symbol, "┐");
        assert_eq!(buf.get(0, 2).unwrap().symbol, "└");
        assert_eq!(buf.get(4, 2).unwrap().symbol, "┘");
    }
}
