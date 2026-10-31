//! Cadrage des débats 0box.
//!
//! Problème constaté : MAIN recevait l'instruction « tu DOIS appeler des
//! outils, si tu ne fais qu'écrire du texte tu ÉCHOUES ». Résultat : sur un
//! débat de test, il créait des tâches et des fichiers puis rendait une
//! réponse vide (la « synthèse » était le fichier).
//!
//! Ici : un classifieur **déterministe** du sujet. Une question ou un débat
//! n'ouvre pas les outils d'écriture ; une demande explicite de livrable, si.

/// Verbes/expressions qui demandent une action concrète (fichier, commande…).
const ACTION_MARKERS: &[&str] = &[
    "crée",
    "créer",
    "cree",
    "creer",
    "écris",
    "ecris",
    "écrire",
    "ecrire",
    "modifie",
    "modifier",
    "corrige",
    "corriger",
    "ajoute",
    "ajouter",
    "supprime",
    "supprimer",
    "génère",
    "genere",
    "générer",
    "generer",
    "installe",
    "installer",
    "exécute",
    "execute",
    "exécuter",
    "executer",
    "lance",
    "lancer",
    "compile",
    "compiler",
    "build",
    "refactor",
    "renomme",
    "renommer",
    "déplace",
    "deplace",
    "télécharge",
    "telecharge",
    "implémente",
    "implémente",
    "implemente",
    "développe",
    "developpe",
    "mets à jour",
    "mets a jour",
    "fix",
    "répare",
    "repare",
    "écris un fichier",
    "crée un fichier",
    "créer un fichier",
    "cree un fichier",
    "patch",
];

/// Marqueurs de question / débat : même avec un verbe d'action, on ne veut
/// qu'une réponse (ex. « comment créer un fichier ? »).
const QUESTION_MARKERS: &[&str] = &[
    "est-ce que",
    "est ce que",
    "pourquoi",
    "comment",
    "qu'est-ce",
    "quest-ce",
    "quelle",
    "quel ",
    "quels",
    "explique",
    "expliquer",
    "analyse",
    "analyser",
    "débat",
    "debat",
    "debate",
    "avis",
    "compare",
    "comparer",
    "différence",
    "difference",
    "penses-tu",
    "dois-je",
    "faut-il",
    "faut il",
    "vs",
];

/// Le sujet demande-t-il un livrable/une action, ou seulement une réponse ?
pub fn topic_wants_actions(topic: &str) -> bool {
    let t = format!(" {} ", topic.to_lowercase());
    let action = ACTION_MARKERS.iter().any(|m| t.contains(m));
    let question = QUESTION_MARKERS.iter().any(|m| t.contains(m));
    action && !question
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn questions_and_debates_are_text_only() {
        for t in [
            "est-ce que tu aimes ton environnement ?",
            "débat sur GPU vs CPU",
            "explique la différence entre box0 et 0box",
            "pourquoi le mode local est lent ?",
            "quel modèle choisir pour coder ?",
            "comment créer un fichier en Rust ?",
        ] {
            assert!(!topic_wants_actions(t), "devrait être texte: {t}");
        }
    }

    #[test]
    fn explicit_deliverables_enable_actions() {
        for t in [
            "crée un fichier /tmp/x.txt avec hello",
            "corrige le bug de parsing dans src/main.rs",
            "compile le projet et lance les tests",
            "écris un fichier notes.md qui résume le débat",
        ] {
            assert!(topic_wants_actions(t), "devrait être action: {t}");
        }
    }
}
