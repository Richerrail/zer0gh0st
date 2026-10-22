//! Petits utilitaires de texte partagés.
//!
//! Pourquoi ce module existe : `String::truncate(n)` **panique** si `n` n'est pas
//! une frontière de caractère UTF-8. La moindre sortie d'outil accentuée de plus
//! de 16 000 octets pouvait donc faire tomber la tâche de génération (et laisser
//! la TUI bloquée en « streaming » pour toujours). Toute troncature passe
//! désormais par ici.

/// Plus grande longueur d'octets `<= max` qui tombe sur une frontière de
/// caractère. Ne panique jamais (au pire, renvoie `0`).
pub fn floor_char_boundary(s: &str, max: usize) -> usize {
    if max >= s.len() {
        return s.len();
    }
    let mut i = max;
    // Un caractère UTF-8 fait au plus 4 octets : au plus 3 itérations.
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

/// Tronque `s` à au plus `max` octets sans casser un caractère.
///
/// Retourne `true` si une troncature a effectivement eu lieu (l'appelant peut
/// alors ajouter un marqueur « … (tronqué) »).
pub fn truncate_utf8(s: &mut String, max: usize) -> bool {
    if s.len() <= max {
        return false;
    }
    let cut = floor_char_boundary(s, max);
    s.truncate(cut);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_string_untouched() {
        let mut s = String::from("bonjour");
        assert!(!truncate_utf8(&mut s, 100));
        assert_eq!(s, "bonjour");
    }

    #[test]
    fn ascii_truncation_is_exact() {
        let mut s = "x".repeat(50);
        assert!(truncate_utf8(&mut s, 10));
        assert_eq!(s.len(), 10);
    }

    /// Régression : `"a" + "é"*20_000` faisait paniquer `String::truncate(16_000)`
    /// (« assertion failed: self.is_char_boundary ») et tuait la tâche de génération.
    #[test]
    fn multibyte_boundary_does_not_panic() {
        let mut s = String::from("a");
        s.push_str(&"é".repeat(20_000));
        assert!(truncate_utf8(&mut s, 16_000));
        assert_eq!(s.len(), 15_999); // 1 + 2*7_999
        assert!(s.chars().all(|c| c == 'a' || c == 'é'));
    }

    #[test]
    fn zero_cut_is_safe() {
        let mut s = String::from("é");
        assert!(truncate_utf8(&mut s, 1));
        assert!(s.is_empty());
    }

    #[test]
    fn emoji_and_accent_mix() {
        // 4 octets (emoji) + 2 octets (é) : coupe demandée au milieu de l'emoji.
        let mut s = String::from("🦀é🦀é");
        assert!(truncate_utf8(&mut s, 3));
        assert_eq!(s.len(), 0);
    }
}
