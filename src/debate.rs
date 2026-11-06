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

const QUESTION_STARTS: &[&str] = &[
    "est-ce",
    "est ce",
    "pourquoi",
    "comment",
    "qu'est",
    "quest",
    "quel",
    "quelle",
    "explique",
    "analyser",
    "analyse",
    "compare",
    "débat",
    "debat",
    "debat",
    "avis",
    "penses-tu",
    "dois-je",
    "faut-il",
    "faut il",
];

pub fn topic_wants_actions(topic: &str) -> bool {
    let t = topic.trim().to_lowercase();
    let action = ACTION_MARKERS.iter().any(|m| t.contains(m));
    let question = t.ends_with('?') || QUESTION_STARTS.iter().any(|q| t.starts_with(q));
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
