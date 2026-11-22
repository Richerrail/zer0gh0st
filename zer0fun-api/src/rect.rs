//! Rectangle pour le positionnement et le dimensionnement.

use serde::{Deserialize, Serialize};

/// Zone rectangulaire dans le terminal.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[repr(C)]
pub struct Rect {
    /// Position X (colonne, 0-indexed).
    pub x: u16,

    /// Position Y (ligne, 0-indexed).
    pub y: u16,

    /// Largeur en caractères.
    pub width: u16,

    /// Hauteur en caractères.
    pub height: u16,
}

impl Rect {
    /// Crée un nouveau rectangle.
    pub const fn new(x: u16, y: u16, width: u16, height: u16) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// Rectangle de taille nulle.
    pub const ZERO: Self = Self::new(0, 0, 0, 0);

    /// Vérifie si un point est dans le rectangle.
    pub fn contains(&self, x: u16, y: u16) -> bool {
        x >= self.x && x < self.x + self.width && y >= self.y && y < self.y + self.height
    }

    /// Retourne le centre du rectangle.
    pub fn center(&self) -> (u16, u16) {
        (self.x + self.width / 2, self.y + self.height / 2)
    }

    /// Retourne un rectangle centré de taille donnée.
    pub fn centered(&self, width: u16, height: u16) -> Self {
        let x = self.x + (self.width.saturating_sub(width)) / 2;
        let y = self.y + (self.height.saturating_sub(height)) / 2;
        Self::new(x, y, width.min(self.width), height.min(self.height))
    }

    /// Retourne un rectangle réduit par des marges.
    pub fn inner(&self, margin: u16) -> Self {
        Self::new(
            self.x + margin,
            self.y + margin,
            self.width.saturating_sub(margin * 2),
            self.height.saturating_sub(margin * 2),
        )
    }

    /// Retourne un rectangle réduit par des marges asymétriques.
    pub fn inner_margin(&self, horizontal: u16, vertical: u16) -> Self {
        Self::new(
            self.x + horizontal,
            self.y + vertical,
            self.width.saturating_sub(horizontal * 2),
            self.height.saturating_sub(vertical * 2),
        )
    }

    /// Divise horizontalement en deux rectangles.
    pub fn split_horizontal(&self, ratio: f32) -> (Self, Self) {
        let left_width = (self.width as f32 * ratio) as u16;
        let right_width = self.width - left_width;
        (
            Self::new(self.x, self.y, left_width, self.height),
            Self::new(self.x + left_width, self.y, right_width, self.height),
        )
    }

    /// Divise verticalement en deux rectangles.
    pub fn split_vertical(&self, ratio: f32) -> (Self, Self) {
        let top_height = (self.height as f32 * ratio) as u16;
        let bottom_height = self.height - top_height;
        (
            Self::new(self.x, self.y, self.width, top_height),
            Self::new(self.x, self.y + top_height, self.width, bottom_height),
        )
    }

    /// Vérifie si le rectangle est vide.
    pub fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0
    }

    /// Surface en caractères.
    pub fn area(&self) -> u32 {
        self.width as u32 * self.height as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_contains() {
        let r = Rect::new(10, 10, 20, 10);
        assert!(r.contains(10, 10));
        assert!(r.contains(29, 19));
        assert!(!r.contains(30, 20));
        assert!(!r.contains(9, 10));
    }

    #[test]
    fn test_centered() {
        let r = Rect::new(0, 0, 100, 50);
        let c = r.centered(20, 10);
        assert_eq!(c.x, 40);
        assert_eq!(c.y, 20);
        assert_eq!(c.width, 20);
        assert_eq!(c.height, 10);
    }

    #[test]
    fn test_split() {
        let r = Rect::new(0, 0, 100, 50);
        let (l, r_) = r.split_horizontal(0.3);
        assert_eq!(l.width, 30);
        assert_eq!(r_.width, 70);
        assert_eq!(r_.x, 30);
    }
}
