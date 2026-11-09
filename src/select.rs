use crate::models::ModelEntry;

/// Rough task categories used to rank models.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    Code,
    Reasoning,
    Math,
    Writing,
    Vision,
    Fast,
    General,
}

impl Category {
    pub fn label(&self) -> &'static str {
        match self {
            Category::Code => "code",
            Category::Reasoning => "raisonnement",
            Category::Math => "maths",
            Category::Writing => "rédaction",
            Category::Vision => "vision",
            Category::Fast => "rapide",
            Category::General => "général",
        }
    }
}

/// Whole-word (boundary) match, so `api` does not match inside `rapide`.
///
/// `start` must advance by one **char**, never one byte: `hay` is UTF-8 and
/// French text contains multibyte chars (`é`, `à`…). Slicing at a non-boundary
/// byte index panics ("byte index is not a char boundary").
fn contains_word(hay: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return false;
    }
    let mut start = 0;
    while let Some(pos) = hay[start..].find(needle) {
        let i = start + pos;
        let end = i + needle.len();
        // `i` and `end` are char boundaries (find/len of a match), so slicing is safe.
        // Accented letters count as word chars: `derivee` must not match in `prederivee`.
        let before_ok = hay[..i]
            .chars()
            .next_back()
            .is_none_or(|c| !c.is_alphanumeric());
        let after_ok = hay[end..]
            .chars()
            .next()
            .is_none_or(|c| !c.is_alphanumeric());
        if before_ok && after_ok {
            return true;
        }
        // advance one CHARACTER, not one byte (B1: byte advance panicked on `é`)
        let step = hay[i..].chars().next().map_or(1, |c| c.len_utf8());
        start = i + step;
    }
    false
}

/// Keyword-based task classifier (French + English).
pub fn classify(problem: &str) -> Category {
    // `to_lowercase`, NOT `to_ascii_lowercase`: the latter strips accents and
    // makes every accented keyword (`équation`, `dérivée`, `écris`...) dead.
    let p = problem.to_lowercase();
    let has = |words: &[&str]| words.iter().any(|w| contains_word(&p, w));

    if has(&[
        "image",
        "photo",
        "capture",
        "screenshot",
        "graphique",
        "dessin",
        "vision",
        "vois",
        "diagramme",
    ]) {
        return Category::Vision;
    }
    if has(&[
        "code",
        "coder",
        "programme",
        "fonction",
        "function",
        "bug",
        "compile",
        "rust",
        "python",
        "c++",
        "java",
        "script",
        "debug",
        "api",
        "sql",
        "regex",
        "algorithme",
        "implémente",
        "refactor",
    ]) {
        return Category::Code;
    }
    if has(&[
        "calcul",
        "calcule",
        "math",
        "équation",
        "equation",
        "intégrale",
        "dérivée",
        "résous",
        "resous",
        "pourcentage",
        "combien font",
        "somme",
        "produit",
        "probabilité",
    ]) {
        return Category::Math;
    }
    if has(&[
        "pourquoi",
        "explique",
        "raisonne",
        "analyse",
        "preuve",
        "plan",
        "stratégie",
        "compare",
        "logique",
        "démontre",
    ]) {
        return Category::Reasoning;
    }
    if has(&[
        "écris",
        "ecris",
        "rédige",
        "redige",
        "poème",
        "poeme",
        "histoire",
        "lettre",
        "résume",
        "resume",
        "traduis",
        "traduction",
        "email",
        "article",
    ]) {
        return Category::Writing;
    }
    if has(&["rapide", "vite", "court", "simple", "bref"]) {
        return Category::Fast;
    }
    Category::General
}

/// Published capability indices (0 when absent).
fn bench(m: &ModelEntry) -> (f64, f64, f64) {
    match m
        .benchmarks
        .as_ref()
        .and_then(|b| b.artificial_analysis.as_ref())
    {
        Some(a) => (
            a.intelligence_index.unwrap_or(0.0),
            a.coding_index.unwrap_or(0.0),
            a.agentic_index.unwrap_or(0.0),
        ),
        None => (0.0, 0.0, 0.0),
    }
}

fn has_image_input(m: &ModelEntry) -> bool {
    m.architecture
        .as_ref()
        .and_then(|a| a.input_modalities.as_ref())
        .map(|v| v.iter().any(|x| x.contains("image")))
        .unwrap_or(false)
}

/// Heuristique de secours (nom du modèle) quand le catalogue n'est pas disponible.
pub fn supports_images_by_name(model_id: &str) -> bool {
    let id = model_id.to_ascii_lowercase();
    [
        "vl",
        "vision",
        "omni",
        "multimodal",
        "llava",
        "pixtral",
        "image",
    ]
    .iter()
    .any(|k| id.contains(k))
}

/// Preference keywords per category (tie-break only; benchmarks dominate).
fn keyword_bonus(id: &str, cat: Category) -> i64 {
    let prefs: &[&str] = match cat {
        Category::Code => &[
            "coder",
            "code",
            "deepseek",
            "laguna",
            "north-mini-code",
            "poolside",
            "qwen",
            "ling",
        ],
        Category::Reasoning => &[
            "reasoning",
            "thinking",
            "think",
            "r1",
            "nemotron",
            "ultra",
            "super",
            "qwen",
            "magistral",
        ],
        Category::Math => &["math", "reasoning", "qwen", "nemotron", "r1", "thinking"],
        Category::Writing => &["instruct", "chat", "gemma", "qwen", "ling", "mistral"],
        Category::Vision => &["vl", "vision", "omni", "image", "multimodal"],
        Category::Fast => &["mini", "small", "flash", "lightning", "nano", "lfm", "tiny"],
        Category::General => &["instruct", "chat", "qwen", "gemma", "llama"],
    };
    let mut s = 0;
    for k in prefs {
        if id.contains(k) {
            s += 8;
        }
    }
    s
}

/// Score one model for a category. Higher is better.
///
/// When OpenRouter publishes Artificial Analysis indices, they dominate the
/// score; keyword preferences are only a fallback / tie-break.
/// `true` si le modèle n'est pas utilisable en chat (embedding ou reranker).
/// Ces modèles ne répondent que sur `/embeddings` ; les envoyer à
/// `/chat/completions` produit un **400** (constaté avec
/// `liquid/lfm-2.5-embedding-350m:free`).
pub fn is_embedding_or_rerank(id: &str) -> bool {
    id.contains("embedding") || id.contains("rerank")
}

pub fn score(m: &ModelEntry, cat: Category) -> i64 {
    let id = m.id.to_ascii_lowercase();
    let (intel, code, agentic) = bench(m);

    let mut s = match cat {
        // No latency data available: use naming heuristics, not intelligence.
        Category::Fast => fast_score(&id),
        Category::Code => (code.max(agentic) * 4.0) as i64,
        _ => (intel * 4.0) as i64,
    };
    s += keyword_bonus(&id, cat);

    // modality constraints
    if cat == Category::Vision && !has_image_input(m) {
        s -= 300;
    }

    // never useful as a chat/agent model
    if is_embedding_or_rerank(&id) {
        s -= 1000;
    }
    if id.contains("content-safety") {
        s -= 200;
    }
    if (id.contains("lyria") || id.contains("whisper") || id.contains("tts"))
        || (id.ends_with("image") && cat != Category::Vision)
    {
        s -= 300;
    }

    if let Some(c) = m.context_length {
        s += (c as f64).log2() as i64;
    }
    s
}

/// Naming-based heuristic for "fast" models (no latency data in the catalog).
fn fast_score(id: &str) -> i64 {
    let mut s = 0i64;
    for k in [
        "flash",
        "mini",
        "small",
        "lightning",
        "nano",
        "lfm",
        "tiny",
        "north-mini",
        "2.6b",
        "1.5b",
        "3b",
        "4b",
        "7b",
        "8b",
        "12b",
    ] {
        if id.contains(k) {
            s += 40;
        }
    }
    for k in [
        "550b", "120b", "ultra", "super", "pro", "27b", "31b", "26b", "72b",
    ] {
        if id.contains(k) {
            s -= 40;
        }
    }
    s
}

/// Rank models for a problem, best first.
pub fn rank(
    problem: &str,
    models: &[ModelEntry],
    top: usize,
) -> (Category, Vec<(i64, ModelEntry)>) {
    let cat = classify(problem);
    let mut scored: Vec<(i64, ModelEntry)> = models
        .iter()
        .cloned()
        .map(|m| (score(&m, cat), m))
        .collect();
    scored.sort_by_key(|a| std::cmp::Reverse(a.0));
    scored.truncate(top);
    (cat, scored)
}

/// Human-readable capability summary for a model.
pub fn capability_note(m: &ModelEntry) -> String {
    let (intel, code, agentic) = bench(m);
    if intel == 0.0 && code == 0.0 && agentic == 0.0 {
        "pas de benchmark publié".to_string()
    } else {
        format!("intel {intel:.0} · code {code:.0} · agentic {agentic:.0}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contains_word_boundaries() {
        assert!(contains_word("api", "api"));
        assert!(!contains_word("rapide", "api"));
        assert!(contains_word("un api ici", "api"));
        // les lettres accentuées sont des caractères de mot, pas des séparateurs
        assert!(!contains_word("les dérivées de f", "dérivée"));
        assert!(contains_word("les dérivées de f", "dérivées"));
    }

    /// Régression B1 : une borne droite refusée sur un mot-clé multioctet
    /// faisait avancer `start` d'un octet, en plein milieu d'un `é` -> panic.
    #[test]
    fn contains_word_no_panic_on_multibyte() {
        // Le panic exige un mot-clé COMMENÇANT par un accent : le match est
        // refusé (borne droite), puis l'ancien code faisait start = i + 1,
        // soit le 2e octet du 'é' -> "byte index is not a char boundary".
        assert!(!contains_word("les équations du système", "équation"));
        assert!(!contains_word("il écrit vite", "écris"));
        assert!(!contains_word("les dérivées de f", "dérivée"));
        assert!(!contains_word("des probabilités faibles", "probabilité"));
        assert!(!contains_word("deux intégrales", "intégrale"));
        // aucun match, texte accentué : doit terminer sans panic
        assert!(!contains_word("éèàçûô", "zzz"));
        assert!(!contains_word("réponse brève", "api"));
        // et les cas positifs fonctionnent toujours
        assert!(contains_word("les équations du système", "équations"));
    }

    #[test]
    fn classify_french_sentence() {
        assert_eq!(
            classify("j'ai un bug dans mon programme, ça plante"),
            Category::Code
        );
        assert_eq!(classify("réponds vite, bref"), Category::Fast);
        assert_eq!(
            classify("explique-moi pourquoi ça marche"),
            Category::Reasoning
        );
        // les mots-clés accentués doivent survivre au lowercase
        // (to_ascii_lowercase les tuait)
        assert_eq!(classify("résous cette équation"), Category::Math);
        assert_eq!(classify("écris un poème"), Category::Writing);
    }
}
