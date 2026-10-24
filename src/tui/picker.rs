//! Menu de sélection (overlay) pour `/model` et `/provider`.
//!
//! Logique volontairement pure (filtre, déplacement, sélection) pour être
//! testable sans terminal ; le rendu est dans `draw`.

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph};

#[derive(Clone, Debug)]
pub struct Item {
    /// Valeur appliquée si on valide (id de modèle, id de provider).
    pub id: String,
    /// Texte principal affiché.
    pub label: String,
    /// Information secondaire (contexte, transport, état de clé…).
    pub note: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Model,
    Provider,
    /// Palette de commandes (déclenchée en tapant `/`).
    Command,
}

#[derive(Debug)]
pub struct Picker {
    /// Cible optionnelle (ex. nom de rôle pour `/role set <role>`).
    pub target: Option<String>,
    pub title: String,
    pub items: Vec<Item>,
    pub filter: String,
    pub sel: usize,
    pub kind: Kind,
}

impl Picker {
    pub fn new(title: impl Into<String>, items: Vec<Item>, kind: Kind) -> Self {
        let sel = 0;
        Self {
            target: None,
            title: title.into(),
            items,
            filter: String::new(),
            sel,
            kind,
        }
    }

    /// Indices visibles : filtre insensible à la casse sur id/label/note.
    pub fn visible(&self) -> Vec<usize> {
        let f = self.filter.trim().to_lowercase();
        self.items
            .iter()
            .enumerate()
            .filter(|(_, it)| {
                f.is_empty()
                    || it.id.to_lowercase().contains(&f)
                    || it.label.to_lowercase().contains(&f)
                    || it.note.to_lowercase().contains(&f)
            })
            .map(|(i, _)| i)
            .collect()
    }

    /// Position du curseur dans la liste visible (0 si absent).
    fn cursor(&self, vis: &[usize]) -> usize {
        vis.iter().position(|&i| i == self.sel).unwrap_or(0)
    }

    /// Déplace la sélection de `delta` (cyclique) dans la liste filtrée.
    pub fn move_sel(&mut self, delta: isize) {
        let vis = self.visible();
        if vis.is_empty() {
            self.sel = 0;
            return;
        }
        let n = vis.len() as isize;
        let next = (self.cursor(&vis) as isize + delta).rem_euclid(n) as usize;
        self.sel = vis[next];
    }

    pub fn page(&mut self, delta: isize, page: usize) {
        self.move_sel(delta * page as isize);
    }

    pub fn push_char(&mut self, c: char) {
        self.filter.push(c);
        self.sel = self.visible().first().copied().unwrap_or(0);
    }

    pub fn pop_char(&mut self) {
        self.filter.pop();
        self.sel = self.visible().first().copied().unwrap_or(0);
    }

    pub fn selected(&self) -> Option<&Item> {
        // `sel` est un index dans `items`, mais il n'a de sens que s'il fait
        // partie des visibles. Sans ce contrôle, taper une commande inconnue
        // puis Entrée exécutait le **premier** élément de la liste.
        let vis = self.visible();
        if !vis.contains(&self.sel) {
            return None;
        }
        self.items.get(self.sel)
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        s.chars().take(max.saturating_sub(1)).collect::<String>() + "…"
    }
}

/// Affiche l'overlay (centré) : titre + filtre, liste, aide.
pub fn draw(f: &mut Frame, area: Rect, p: &Picker) {
    let w = (area.width * 3 / 4).clamp(40, 110).min(area.width);
    let h = (area.height * 2 / 3).clamp(8, 28).min(area.height);
    let r = Rect {
        x: area.x + area.width.saturating_sub(w) / 2,
        y: area.y + area.height.saturating_sub(h) / 2,
        width: w,
        height: h,
    };
    f.render_widget(Clear, r);

    let filtre = if p.filter.is_empty() {
        "(tape pour filtrer)".to_string()
    } else {
        p.filter.clone()
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" {} · {} ", p.title, filtre));
    let inner = block.inner(r);
    f.render_widget(block, r);
    if inner.height < 2 {
        return;
    }

    let vis = p.visible();
    let items: Vec<ListItem> = vis
        .iter()
        .map(|&i| {
            let it = &p.items[i];
            ListItem::new(format!(
                " {:<44} {}",
                truncate(&it.label, 44),
                truncate(&it.note, 60)
            ))
        })
        .collect();
    let mut st = ListState::default();
    if !vis.is_empty() {
        st.select(Some(p.cursor(&vis)));
    }
    let list = List::new(items).highlight_style(
        Style::default()
            .bg(Color::Rgb(40, 50, 110))
            .fg(Color::White),
    );
    let list_area = Rect {
        height: inner.height - 1,
        ..inner
    };
    f.render_stateful_widget(list, list_area, &mut st);

    let help = Paragraph::new(format!(
        " {} item(s) · ↑↓ naviguer · taper = filtrer · Entrée valider · Échap annuler",
        vis.len()
    ))
    .style(Style::default().fg(Color::DarkGray));
    f.render_widget(
        help,
        Rect {
            y: inner.y + inner.height - 1,
            height: 1,
            ..inner
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items(ids: &[&str]) -> Vec<Item> {
        ids.iter()
            .map(|id| Item {
                id: (*id).to_string(),
                label: (*id).to_string(),
                note: String::new(),
            })
            .collect()
    }

    #[test]
    fn filter_is_case_insensitive_and_matches_id_and_note() {
        let mut p = Picker::new("t", items(&["glm-4.6", "llama-3.3", "qwen3"]), Kind::Model);
        assert_eq!(p.visible().len(), 3);
        p.push_char('l');
        assert_eq!(p.visible(), vec![0, 1]); // glm + llama
        p.push_char('l');
        assert_eq!(p.visible(), vec![1]); // llama
        p.pop_char();
        p.pop_char();
        assert_eq!(p.visible().len(), 3);
    }

    #[test]
    fn move_is_cyclic_within_filtered_list() {
        let mut p = Picker::new("t", items(&["a", "b", "c"]), Kind::Model);
        assert_eq!(p.selected().map(|i| i.id.as_str()), Some("a"));
        p.move_sel(-1);
        assert_eq!(p.selected().map(|i| i.id.as_str()), Some("c"));
        p.move_sel(1);
        assert_eq!(p.selected().map(|i| i.id.as_str()), Some("a"));
    }

    #[test]
    fn filter_resets_selection_to_first_visible() {
        let mut p = Picker::new("t", items(&["a", "b", "c"]), Kind::Model);
        p.move_sel(2); // c
        p.push_char('b');
        assert_eq!(p.selected().map(|i| i.id.as_str()), Some("b"));
    }

    #[test]
    fn empty_filter_result_has_no_selection() {
        let mut p = Picker::new("t", items(&["a"]), Kind::Model);
        p.push_char('z');
        assert!(p.visible().is_empty());
        p.move_sel(1); // ne panique pas
                       // Régression : `selected()` renvoyait l'élément 0 malgré un filtre vide,
                       // donc Entrée exécutait une commande que l'utilisateur n'avait pas choisie.
        assert!(p.selected().is_none());
    }
}
