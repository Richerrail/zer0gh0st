//! Événements normalisés pour les modules fun.

use serde::{Deserialize, Serialize};

/// Événement utilisateur ou système.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Event {
    /// Touche clavier pressée.
    Key(KeyEvent),

    /// Touche clavier relâchée.
    KeyRelease(KeyEvent),

    /// Souris déplacée.
    MouseMove(MouseEvent),

    /// Clic souris.
    MouseClick(MouseEvent, MouseButton),

    /// Molette souris.
    MouseScroll(MouseEvent, ScrollDirection),

    /// Redimensionnement du terminal.
    Resize { width: u16, height: u16 },

    /// Tick régulier (pour animations).
    Tick,

    /// Événement personnalisé inter-modules.
    Custom {
        name: String,
        data: serde_json::Value,
    },

    /// Focus gagné/perdu.
    Focus(bool),

    /// Coller (paste) de texte.
    Paste(String),
}

/// Événement clavier.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum KeyEvent {
    /// Caractère Unicode.
    Char(char),

    /// Touche fonction (F1-F12).
    F(u8),

    /// Flèches.
    Up,
    Down,
    Left,
    Right,

    /// Navigation.
    Home,
    End,
    PageUp,
    PageDown,

    /// Édition.
    Backspace,
    Delete,
    Insert,

    /// Contrôle.
    Esc,
    Enter,
    Tab,
    BackTab,

    /// Modificateurs avec caractère.
    Ctrl(char),
    Alt(char),
    Shift(char),

    /// Modificateurs avec touche spéciale.
    CtrlUp,
    CtrlDown,
    CtrlLeft,
    CtrlRight,

    /// Inconnu/non supporté.
    Unknown,
}

impl KeyEvent {
    /// Vérifie si c'est une touche de sortie (Esc ou 'q').
    pub fn is_exit(&self) -> bool {
        matches!(
            self,
            KeyEvent::Esc | KeyEvent::Char('q') | KeyEvent::Ctrl('c')
        )
    }

    /// Vérifie si c'est une touche de navigation.
    pub fn is_navigation(&self) -> bool {
        matches!(
            self,
            KeyEvent::Up
                | KeyEvent::Down
                | KeyEvent::Left
                | KeyEvent::Right
                | KeyEvent::Home
                | KeyEvent::End
                | KeyEvent::PageUp
                | KeyEvent::PageDown
        )
    }
}

/// Événement souris.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MouseEvent {
    /// Colonne (0-indexed).
    pub x: u16,

    /// Ligne (0-indexed).
    pub y: u16,

    /// Modificateurs actifs.
    pub modifiers: Modifiers,
}

/// Bouton de souris.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

/// Direction de scroll.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum ScrollDirection {
    Up,
    Down,
    Left,
    Right,
}

/// Modificateurs clavier/souris.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct Modifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub meta: bool,
}

impl Modifiers {
    pub const NONE: Self = Self {
        shift: false,
        ctrl: false,
        alt: false,
        meta: false,
    };

    pub const CTRL: Self = Self {
        shift: false,
        ctrl: true,
        alt: false,
        meta: false,
    };

    pub const SHIFT: Self = Self {
        shift: true,
        ctrl: false,
        alt: false,
        meta: false,
    };

    pub const ALT: Self = Self {
        shift: false,
        ctrl: false,
        alt: true,
        meta: false,
    };
}
