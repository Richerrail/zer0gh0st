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
    /// Dégradé acier bleuté (thème castle / DOS).
    Steel,
    /// Bloc 2D en relief : lettres pleines avec arête claire (haut/gauche) et
    /// ombre (bas/droite), coloriées par bandes d'encres. Sur fond clair, la
    /// couleur vient de la teinte, pas de la luminosité.
    Bevel,
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
    /// Texte posé **sur** `highlight` (pastilles de raccourcis, pied de page).
    /// `Black` convient aux thèmes dont la pastille est claire ; un thème à
    /// pastille foncée doit mettre ici sa couleur de fond.
    pub chip_fg: Color,
    pub selection_bg: Color,
    pub selection_fg: Color,
    pub menu_selection_bg: Color,
    pub popup_bg: Color,
    pub success: Color,
    pub warning: Color,
    pub danger: Color,
    pub banner: BannerStyle,
    /// Pack de sons du thème (`""` = aucun son de thème).
    pub sound_pack: &'static str,
    /// Glyphe du bouton « ouvrir le menu déroulant » (rail replié, à droite).
    pub rail_arrow: &'static str,
    /// Le rail est-il une « poignée de tiroir » (ASCII, verticale) au lieu du
    /// simple `◀☰` ?
    pub rail_drawer: bool,
    /// Halo lumineux autour du glyphe du rail (effet néon).
    pub rail_glow: bool,
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
        chip_fg: Color::Black,
        selection_bg: Color::Rgb(40, 50, 110),
        selection_fg: Color::White,
        menu_selection_bg: Color::Rgb(72, 33, 75),
        popup_bg: Color::Rgb(15, 15, 25),
        success: Color::LightGreen,
        warning: Color::Yellow,
        danger: Color::LightRed,
        banner: BannerStyle::Rainbow,
        sound_pack: "",
        rail_arrow: "◀☰",
        rail_drawer: false,
        rail_glow: false,
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
        border: Color::Rgb(91, 91, 99),
        highlight: Color::Rgb(220, 220, 226),
        chip_fg: Color::Black,
        selection_bg: Color::Rgb(55, 55, 61),
        selection_fg: Color::White,
        menu_selection_bg: Color::Rgb(40, 40, 46),
        popup_bg: Color::Rgb(8, 8, 10),
        success: Color::Rgb(170, 195, 175),
        warning: Color::Rgb(205, 185, 140),
        danger: Color::Rgb(220, 85, 85),
        banner: BannerStyle::Mono,
        sound_pack: "",
        rail_arrow: "◀☰",
        rail_drawer: false,
        rail_glow: false,
    },
    Theme {
        id: "noir-rouge",
        label: "Noir & Rouge",
        description: "noir profond, rouge vif et rose pâle",
        background: Color::Rgb(7, 2, 3),
        accent: Color::Rgb(255, 45, 55),
        panel: Color::Rgb(28, 5, 8),
        dim: Color::Rgb(178, 104, 110),
        text: Color::Rgb(245, 220, 222),
        code: Color::Rgb(255, 125, 130),
        tool: Color::Rgb(232, 78, 86),
        border: Color::Rgb(190, 62, 70),
        highlight: Color::Rgb(255, 75, 82),
        chip_fg: Color::Black,
        selection_bg: Color::Rgb(92, 10, 18),
        selection_fg: Color::Rgb(255, 235, 235),
        menu_selection_bg: Color::Rgb(68, 6, 12),
        popup_bg: Color::Rgb(12, 2, 4),
        success: Color::Rgb(175, 195, 175),
        warning: Color::Rgb(220, 165, 70),
        danger: Color::Rgb(255, 60, 68),
        banner: BannerStyle::Red,
        sound_pack: "",
        // Poignée de tiroir (ASCII : aucun glyphe exotique) pour tirer le menu.
        // `>>` = le tiroir est rentré, on tire pour l'ouvrir.
        rail_arrow: ">>",
        rail_drawer: true,
        rail_glow: true,
    },
    Theme {
        id: "castle",
        label: "Castle",
        description: "acier bleu, rouge sang — Wolfenstein 3D (1992)",
        background: Color::Rgb(10, 10, 18),
        accent: Color::Rgb(222, 72, 72),
        panel: Color::Rgb(20, 24, 36),
        dim: Color::Rgb(128, 138, 156),
        text: Color::Rgb(216, 216, 224),
        code: Color::Rgb(112, 184, 216),
        tool: Color::Rgb(128, 144, 176),
        border: Color::Rgb(84, 96, 118),
        highlight: Color::Rgb(224, 224, 240),
        chip_fg: Color::Black,
        selection_bg: Color::Rgb(176, 32, 32),
        selection_fg: Color::White,
        menu_selection_bg: Color::Rgb(140, 26, 26),
        popup_bg: Color::Rgb(14, 16, 24),
        success: Color::Rgb(80, 160, 80),
        warning: Color::Rgb(208, 160, 48),
        danger: Color::Rgb(232, 74, 74),
        banner: BannerStyle::Steel,
        sound_pack: "castle",
        rail_arrow: "◀☰",
        rail_drawer: false,
        rail_glow: false,
    },
    Theme {
        id: "paper",
        label: "Paper",
        description: "feuille blanche, encres bleue/rouge/verte/noire — dessin industriel",
        // Fond **forcé** : un thème clair sur le fond noir d'un autre terminal
        // serait illisible. On peint le papier, on ne l'espère pas.
        background: Color::Rgb(250, 249, 245),
        accent: Color::Rgb(31, 78, 140), // encre bleue (bleu de plan)
        panel: Color::Rgb(240, 239, 233),
        dim: Color::Rgb(116, 114, 105), // crayon gris
        text: Color::Rgb(27, 27, 31),   // encre noire
        code: Color::Rgb(56, 56, 62),
        tool: Color::Rgb(15, 106, 106), // bleu-vert de tire-ligne
        border: Color::Rgb(74, 74, 69), // trait de construction
        highlight: Color::Rgb(31, 78, 140),
        chip_fg: Color::Rgb(250, 249, 245), // papier sur pastille encre
        selection_bg: Color::Rgb(214, 228, 240), // lavis bleu clair
        selection_fg: Color::Rgb(16, 20, 24),
        menu_selection_bg: Color::Rgb(220, 231, 243),
        popup_bg: Color::Rgb(243, 242, 236),
        success: Color::Rgb(30, 107, 58), // encre verte
        warning: Color::Rgb(138, 90, 0),  // ocre (le jaune vif est illisible)
        danger: Color::Rgb(166, 27, 27),  // encre rouge
        banner: BannerStyle::Bevel,
        sound_pack: "",
        rail_arrow: "◀☰",
        rail_drawer: false,
        rail_glow: false,
    },
];

static ACTIVE_INDEX: AtomicUsize = AtomicUsize::new(0);

/// Verrou des tests qui changent le thème actif (état global partagé).
#[cfg(test)]
pub(crate) static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

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
        "wolf" | "wolf3d" | "wolfenstein" | "bleu" | "acier" | "steel" => "castle".to_string(),
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
/// Couleur du texte posé sur une pastille `highlight` (pied de page).
pub fn chip_fg() -> Color {
    active().chip_fg
}

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
/// Glyphe du bouton d'ouverture du menu (rail replié, à droite).
pub fn rail_arrow() -> &'static str {
    active().rail_arrow
}
/// Le rail se dessine-t-il en « poignée de tiroir » ?
pub fn rail_drawer() -> bool {
    active().rail_drawer
}
/// Le rail affiche-t-il un halo néon autour de son glyphe ?
pub fn rail_glow() -> bool {
    active().rail_glow
}

#[cfg(test)]
mod contrast_tests {
    use super::*;

    /// Couleur ANSI → RGB conventionnel (xterm). `Reset` reste non mesurable.
    fn to_rgb(c: Color) -> Option<(u8, u8, u8)> {
        Some(match c {
            Color::Rgb(r, g, b) => (r, g, b),
            Color::Black => (0, 0, 0),
            Color::Red => (205, 49, 49),
            Color::Green => (13, 188, 121),
            Color::Yellow => (229, 229, 16),
            Color::Blue => (36, 114, 200),
            Color::Magenta => (188, 63, 188),
            Color::Cyan => (17, 168, 205),
            Color::Gray => (229, 229, 229),
            Color::DarkGray => (102, 102, 102),
            Color::LightRed => (241, 76, 76),
            Color::LightGreen => (35, 209, 139),
            Color::LightYellow => (245, 245, 67),
            Color::LightBlue => (59, 142, 234),
            Color::LightMagenta => (214, 112, 214),
            Color::LightCyan => (41, 184, 219),
            Color::White => (255, 255, 255),
            _ => return None,
        })
    }

    /// Luminance relative (WCAG 2.1).
    fn lum(c: Color) -> Option<f64> {
        let (r, g, b) = to_rgb(c)?;
        let f = |v: u8| {
            let s = v as f64 / 255.0;
            if s <= 0.039_28 {
                s / 12.92
            } else {
                ((s + 0.055) / 1.055).powf(2.4)
            }
        };
        Some(0.2126 * f(r) + 0.7152 * f(g) + 0.0722 * f(b))
    }

    fn contrast(a: Color, b: Color) -> Option<f64> {
        let (la, lb) = (lum(a)?, lum(b)?);
        let (hi, lo) = if la > lb { (la, lb) } else { (lb, la) };
        Some((hi + 0.05) / (lo + 0.05))
    }

    /// Textes lisibles : au moins 4,5:1 (AA) sur le fond du thème.
    #[test]
    fn text_colors_are_readable_on_their_background() {
        for theme in ALL {
            // `ghost` hérite du fond du terminal : on teste contre un fond
            // sombre, ce qu'il suppose.
            let bg = match theme.background {
                Color::Reset => Color::Rgb(16, 16, 20),
                c => c,
            };
            for (role, color) in [
                ("text", theme.text),
                ("accent", theme.accent),
                ("tool", theme.tool),
                ("danger", theme.danger),
                ("success", theme.success),
            ] {
                let c = contrast(color, bg).expect("couleurs Rgb attendues");
                assert!(
                    c >= 4.5,
                    "{} / {role} : contraste {c:.2} < 4.5 (illisible)",
                    theme.id
                );
            }
            // Les traits et le texte atténué peuvent être plus discrets,
            // mais pas invisibles.
            for (role, color) in [
                ("dim", theme.dim),
                ("border", theme.border),
                ("code", theme.code),
            ] {
                let c = contrast(color, bg).expect("couleurs Rgb attendues");
                assert!(c >= 3.0, "{} / {role} : contraste {c:.2} < 3.0", theme.id);
            }
        }
    }

    /// Le texte des pastilles doit trancher avec la pastille elle-même.
    #[test]
    fn chip_text_stands_out_on_its_chip() {
        for theme in ALL {
            let c = contrast(theme.chip_fg, theme.highlight);
            if let Some(c) = c {
                assert!(c >= 3.0, "{} : pastilles illisibles ({c:.2})", theme.id);
            }
        }
    }

    #[test]
    fn paper_is_a_light_theme() {
        let paper = ALL.iter().find(|t| t.id == "paper").expect("thème paper");
        let bg = lum(paper.background).expect("fond Rgb");
        let text = lum(paper.text).expect("texte Rgb");
        assert!(bg > 0.7, "le fond papier doit être clair, pas {bg:.2}");
        assert!(text < 0.2, "l'encre doit être sombre, pas {text:.2}");
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn castle_theme_is_selectable_and_has_its_pack() {
        let t = resolve("castle").expect("castle");
        assert_eq!(t.id, "castle");
        assert_eq!(t.sound_pack, "castle");
        assert_eq!(resolve("wolf3d").expect("alias").id, "castle");
        assert_eq!(t.banner, BannerStyle::Steel);
    }

    use super::*;

    #[test]
    fn ghost_is_the_default_theme() {
        assert_eq!(ALL[0].id, "ghost");
    }

    #[test]
    fn activation_changes_the_active_palette() {
        let _lock = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        activate("noir-rouge").unwrap();
        assert_eq!(active().id, "noir-rouge");
        activate("ghost").unwrap();
    }

    #[test]
    fn noir_rouge_gets_a_neon_drawer_handle_on_the_rail() {
        let red = resolve("noir-rouge").unwrap();
        // ASCII uniquement : les chevrons `▶` ne s'affichent pas partout.
        assert_eq!(red.rail_arrow, ">>");
        assert!(
            red.rail_drawer,
            "le thème noir-rouge doit avoir une poignée"
        );
        assert!(red.rail_glow, "le thème noir-rouge doit avoir un halo néon");
        for other in ALL.iter().filter(|t| t.id != "noir-rouge") {
            assert_eq!(other.rail_arrow, "◀☰");
            assert!(!other.rail_drawer);
            assert!(!other.rail_glow);
        }
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
