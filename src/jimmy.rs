//! **Jimmy** — le cancre.
//!
//! Un faux agent volontairement inutile : il ne réfléchit pas, ne code pas, et
//! répond « je sais pas » / « hein ?? » / « ZzZzzZzzz… ». Il sert de **faire-valoir**
//! (`/provoke`) pour provoquer les vrais agents — et de gag.

use std::time::{SystemTime, UNIX_EPOCH};

const LINES: &[&str] = &[
    // ── Le cancre ──
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
    // ── Les gags « j'ai tout cassé » ──
    "j'ai arrangé ton code : y'avait trop de lignes, j'en ai effacé 300.",
    "j'ai supprimé les warnings en supprimant le code. 👍",
    "j'ai commenté tout le fichier, comme ça plus de bugs.",
    "j'ai enlevé les tests : ils échouaient.",
    "j'ai push direct sur main, c'est bon non ?",
    "j'ai fait un force-push. j'espère que personne avait pull.",
    "j'ai mis un `rm -rf` au début pour nettoyer, ça devrait aller.",
    "j'ai déplacé tout dans /tmp, c'était mieux rangé.",
    "j'ai remplacé ta base de données par un .txt, plus simple.",
    "j'ai renommé tes fichiers en `final_v2_final_definitif`.",
    "j'ai ajouté 47 dépendances, ça compile toujours ?",
    "j'ai mis ta clé API dans le README, c'est plus pratique.",
    "j'ai pas testé mais ça devrait marcher. (ça marche pas)",
    "ça marche chez moi. 🤷",
    "j'ai supprimé `.git` : ça prenait de la place pour rien.",
    "j'ai déployé en prod à 3h du mat, tranquille.",
    "j'ai « optimisé » en retirant les vérifications d'erreurs.",
    "j'ai renommé la variable `data` en `d`, c'est plus court.",
    "j'ai mis le mot de passe en dur, comme ça plus de config.",
    "j'ai désactivé le pare-feu, ça bloquait mes tests.",
    "j'ai fait `git reset --hard`, t'avais rien commité de toute façon ?",
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
