use std::sync::atomic::{AtomicUsize, Ordering};

use anyhow::{anyhow, Result};
use ratatui::style::Color;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BannerStyle {
    Rainbow,
    Mono,
    Red,
    Steel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Theme {
    pub id: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    pub background: Color,
    pub accent: Color,
    pub panel: Color,
    pub dim: Color,
    pub text: Color,
    pub code: Color,
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
    pub sound_pack: &'static str,
    pub rail_arrow: &'static str,
    pub rail_drawer: bool,
    pub rail_glow: bool,
}

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
        sound_pack: "",
        rail_arrow: ">>",
        rail_drawer: true,
        rail_glow: true,
    },
    Theme {
        id: "castle",
        label: "Castle",
        description: "acier bleu, rouge sang — Wolfenstein 3D (1992)",
        background: Color::Rgb(10, 10, 18),
        accent: Color::Rgb(176, 32, 32),
        panel: Color::Rgb(20, 24, 36),
        dim: Color::Rgb(106, 114, 128),
        text: Color::Rgb(216, 216, 224),
        code: Color::Rgb(112, 184, 216),
        tool: Color::Rgb(128, 144, 176),
        border: Color::Rgb(60, 70, 88),
        highlight: Color::Rgb(224, 224, 240),
        selection_bg: Color::Rgb(176, 32, 32),
        selection_fg: Color::White,
        menu_selection_bg: Color::Rgb(140, 26, 26),
        popup_bg: Color::Rgb(14, 16, 24),
        success: Color::Rgb(80, 160, 80),
        warning: Color::Rgb(208, 160, 48),
        danger: Color::Rgb(224, 48, 48),
        banner: BannerStyle::Steel,
        sound_pack: "castle",
        rail_arrow: "◀☰",
        rail_drawer: false,
        rail_glow: false,
    },
];

static ACTIVE_INDEX: AtomicUsize = AtomicUsize::new(0);

#[cfg(test)]
pub(crate) static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub fn active() -> Theme {
    let index = ACTIVE_INDEX.load(Ordering::Relaxed).min(ALL.len() - 1);
    ALL[index]
}

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

pub fn activate(name: &str) -> Result<&'static Theme> {
    let selected = resolve(name)?;
    let index = ALL
        .iter()
        .position(|theme| theme.id == selected.id)
        .unwrap_or_default();
    ACTIVE_INDEX.store(index, Ordering::Relaxed);
    Ok(selected)
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
pub fn rail_arrow() -> &'static str {
    active().rail_arrow
}
pub fn rail_drawer() -> bool {
    active().rail_drawer
}
pub fn rail_glow() -> bool {
    active().rail_glow
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
