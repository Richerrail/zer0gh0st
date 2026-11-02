//! Thèmes de couleurs de la TUI.
//!
//! Un thème est un jeu de rôles de couleurs (fond, accent, texte, code…),
//! plutôt qu'une liste de couleurs dispersées dans le code. Le thème actif est
//! global et thread-safe : le rendu de la TUI et ses overlays le consultent sans
//! avoir à transporter une palette dans chaque fonction.

use std::sync::atomic::{AtomicUsize, Ordering};

use anyhow::{anyhow, Result};
use ratatui::style::Color;

/// Style du banner principal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BannerStyle {
    /// Dégradé arc-en-ciel animé (thème historique).
    Rainbow,
    /// Dégradé gris, minimaliste.
    Mono,
    /// Dégradé rouge, du bordeaux au rouge vif.
    Red,
}

/// Palette complète d'un thème.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Theme {
    /// Identifiant stable, persisté dans la configuration.
    pub id: &'static str,
    /// Nom affiché dans le sélecteur.
    pub label: &'static str,
    /// Description courte affichée dans le sélecteur.
    pub description: &'static str,
    pub background: Color,
    pub accent: Color,
    pub panel: Color,
    pub dim: Color,
    pub text: Color,
    pub code: Color,
    /// Couleur des blocs d'outils (historiquement bleue).
    pub tool: Color,
    pub border: Color,
    pub highlight: Color,
    pub selection_bg: Color,
    pub selection_fg: Color,
    pub menu_selection_bg: Color,
    pub popup_bg: Color,
    pub success: Color,
    pub warning: Color,
    pub danger: Color,
    pub banner: BannerStyle,
}

/// Thèmes disponibles. Le premier est le thème par défaut.
pub const ALL: &[Theme] = &[
    Theme {
        id: "ghost",
        label: "Ghost",
        description: "rose, violet et cyan — thème d'origine",
        background: Color::Reset,
        accent: Color::Rgb(255, 113, 206),
        panel: Color::Rgb(34, 23, 51),
        dim: Color::Rgb(122, 108, 142),
        text: Color::Rgb(232, 213, 255),
        code: Color::Rgb(122, 232, 255),
        tool: Color::Rgb(138, 158, 255),
        border: Color::Rgb(110, 95, 140),
        highlight: Color::Cyan,
        selection_bg: Color::Rgb(40, 50, 110),
        selection_fg: Color::White,
        menu_selection_bg: Color::Rgb(72, 33, 75),
        popup_bg: Color::Rgb(15, 15, 25),
        success: Color::LightGreen,
        warning: Color::Yellow,
        danger: Color::LightRed,
        banner: BannerStyle::Rainbow,
    },
    Theme {
        id: "noir",
        label: "Noir",
        description: "noir, gris et blanc — minimaliste",
        background: Color::Rgb(4, 4, 5),
        accent: Color::Rgb(218, 218, 224),
        panel: Color::Rgb(18, 18, 21),
        dim: Color::Rgb(105, 105, 113),
        text: Color::Rgb(235, 235, 239),
        code: Color::Rgb(195, 195, 203),
        tool: Color::Rgb(150, 150, 158),
        border: Color::Rgb(75, 75, 83),
        highlight: Color::Rgb(220, 220, 226),
        selection_bg: Color::Rgb(55, 55, 61),
        selection_fg: Color::White,
        menu_selection_bg: Color::Rgb(40, 40, 46),
        popup_bg: Color::Rgb(8, 8, 10),
        success: Color::Rgb(170, 195, 175),
        warning: Color::Rgb(205, 185, 140),
        danger: Color::Rgb(220, 85, 85),
        banner: BannerStyle::Mono,
    },
    Theme {
        id: "noir-rouge",
        label: "Noir & Rouge",
        description: "noir profond, rouge vif et rose pâle",
        background: Color::Rgb(7, 2, 3),
        accent: Color::Rgb(255, 45, 55),
        panel: Color::Rgb(28, 5, 8),
        dim: Color::Rgb(145, 75, 82),
        text: Color::Rgb(245, 220, 222),
        code: Color::Rgb(255, 125, 130),
        tool: Color::Rgb(195, 45, 55),
        border: Color::Rgb(150, 25, 35),
        highlight: Color::Rgb(255, 75, 82),
        selection_bg: Color::Rgb(92, 10, 18),
        selection_fg: Color::Rgb(255, 235, 235),
        menu_selection_bg: Color::Rgb(68, 6, 12),
        popup_bg: Color::Rgb(12, 2, 4),
        success: Color::Rgb(175, 195, 175),
        warning: Color::Rgb(220, 165, 70),
        danger: Color::Rgb(255, 60, 68),
        banner: BannerStyle::Red,
    },
];

static ACTIVE_INDEX: AtomicUsize = AtomicUsize::new(0);

/// Thème actuellement actif.
pub fn active() -> Theme {
    let index = ACTIVE_INDEX.load(Ordering::Relaxed).min(ALL.len() - 1);
    ALL[index]
}

/// Normalise un nom et ses alias vers l'identifiant canonique.
fn canonical_name(name: &str) -> String {
    let lowered = name.trim().to_lowercase().replace('&', "et");
    let normalized: String = lowered
        .chars()
        .map(|c| if c == '_' || c == ' ' { '-' } else { c })
        .collect();
    match normalized.as_str() {
        "default" | "omarchy" | "rose" => "ghost".to_string(),
        "black" | "monochrome" | "mono" => "noir".to_string(),
        "red" | "rouge" | "black-red" | "black-et-red" | "noir-et-rouge" => {
            "noir-rouge".to_string()
        }
        other => other.to_string(),
    }
}

/// Résout un nom de thème sans modifier le thème actif.
pub fn resolve(name: &str) -> Result<&'static Theme> {
    let canonical = canonical_name(name);
    ALL.iter()
        .find(|theme| theme.id == canonical)
        .ok_or_else(|| {
            anyhow!(
                "thème inconnu « {name} » — disponibles : {}",
                ALL.iter()
                    .map(|theme| theme.id)
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })
}

/// Active un thème par son identifiant ou l'un de ses alias.
pub fn activate(name: &str) -> Result<&'static Theme> {
    let selected = resolve(name)?;
    let index = ALL
        .iter()
        .position(|theme| theme.id == selected.id)
        .unwrap_or_default();
    ACTIVE_INDEX.store(index, Ordering::Relaxed);
    Ok(selected)
}

// Raccourcis de lecture : le rendu ne doit pas répéter `active().champ`.
pub fn background() -> Color {
    active().background
}
pub fn accent() -> Color {
    active().accent
}
pub fn panel() -> Color {
    active().panel
}
pub fn dim() -> Color {
    active().dim
}
pub fn text() -> Color {
    active().text
}
pub fn code() -> Color {
    active().code
}
pub fn tool() -> Color {
    active().tool
}
pub fn border() -> Color {
    active().border
}
pub fn highlight() -> Color {
    active().highlight
}
pub fn selection_bg() -> Color {
    active().selection_bg
}
pub fn selection_fg() -> Color {
    active().selection_fg
}
pub fn menu_selection_bg() -> Color {
    active().menu_selection_bg
}
pub fn popup_bg() -> Color {
    active().popup_bg
}
pub fn success() -> Color {
    active().success
}
pub fn warning() -> Color {
    active().warning
}
pub fn danger() -> Color {
    active().danger
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ghost_is_the_default_theme() {
        assert_eq!(ALL[0].id, "ghost");
    }

    #[test]
    fn activation_changes_the_active_palette() {
        activate("noir-rouge").unwrap();
        assert_eq!(active().id, "noir-rouge");
        activate("ghost").unwrap();
    }

    #[test]
    fn theme_aliases_resolve_to_canonical_ids() {
        assert_eq!(resolve("default").unwrap().id, "ghost");
        assert_eq!(resolve("BLACK").unwrap().id, "noir");
        assert_eq!(resolve("noir_rouge").unwrap().id, "noir-rouge");
        assert_eq!(resolve("rouge").unwrap().id, "noir-rouge");
        assert_eq!(resolve("Noir & Rouge").unwrap().id, "noir-rouge");
    }

    #[test]
    fn unknown_theme_lists_the_available_choices() {
        let err = resolve("inconnu").unwrap_err().to_string();
        assert!(err.contains("noir-rouge"), "{err}");
        assert!(err.contains("ghost"), "{err}");
    }
}
