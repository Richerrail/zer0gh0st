pub fn floor_char_boundary(s: &str, max: usize) -> usize {
    if max >= s.len() {
        return s.len();
    }
    let mut i = max;
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

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

    #[test]
    fn multibyte_boundary_does_not_panic() {
        let mut s = String::from("a");
        s.push_str(&"é".repeat(20_000));
        assert!(truncate_utf8(&mut s, 16_000));
        assert_eq!(s.len(), 15_999);
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
        let mut s = String::from("🦀é🦀é");
        assert!(truncate_utf8(&mut s, 3));
        assert_eq!(s.len(), 0);
    }
}
