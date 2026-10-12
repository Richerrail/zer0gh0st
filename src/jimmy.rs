//! **Jimmy** — le cancre.
//!
//! Un faux agent volontairement inutile : il ne réfléchit pas, ne code pas, et
//! répond « je sais pas » / « hein ?? » / « ZzZzzZzzz… ». Il sert de **faire-valoir**
//! (`/provoke`) pour provoquer les vrais agents — et de gag.

use std::time::{SystemTime, UNIX_EPOCH};

const LINES: &[&str] = &[
    "je sais pas.",
    "hein ?? quoi ??",
    "ZzZzzZzzz… 💤",
    "c'est quoi la question déjà ?",
    "j'ai pas eu le temps, désolé.",
    "bof. demande à quelqu'un d'autre.",
    "… (Jimmy regarde le plafond)",
    "attends, je finis ma sieste.",
    "ça a l'air compliqué, non ?",
    "j'étais parti chercher un café.",
    "euh… on peut pas faire ça demain ?",
    "réponse : 42. (j'ai rien compris)",
];

/// Une réplique de Jimmy (pseudo-aléatoire).
pub fn reply() -> &'static str {
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as usize)
        .unwrap_or(0);
    LINES[n % LINES.len()]
}

/// Délai « fainéant » (quelques secondes) pour jouer le cancre qui traîne.
pub fn lazy_delay() -> u64 {
    3 + (reply().len() as u64 % 6)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jimmy_always_answers_something() {
        for _ in 0..20 {
            assert!(!reply().trim().is_empty());
        }
        assert!((3..=8).contains(&lazy_delay()));
    }
}
