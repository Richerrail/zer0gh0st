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
fn contains_word(hay: &str, needle: &str) -> bool {
    let bytes = hay.as_bytes();
    let mut start = 0;
    while let Some(pos) = hay[start..].find(needle) {
        let i = start + pos;
        let end = i + needle.len();
        let before_ok = i == 0 || !bytes[i - 1].is_ascii_alphanumeric();
        let after_ok = end >= bytes.len() || !bytes[end].is_ascii_alphanumeric();
        if before_ok && after_ok {
            return true;
        }
        start = i + 1;
    }
    false
}

/// Keyword-based task classifier (French + English).
pub fn classify(problem: &str) -> Category {
    let p = problem.to_ascii_lowercase();
    let has = |words: &[&str]| words.iter().any(|w| contains_word(&p, w));

    if has(&["image", "photo", "capture", "screenshot", "graphique", "dessin", "vision", "vois", "diagramme"]) {
        return Category::Vision;
    }
    if has(&["code", "coder", "programme", "fonction", "function", "bug", "compile", "rust", "python", "c++", "java", "script", "debug", "api", "sql", "regex", "algorithme", "implémente", "refactor"]) {
        return Category::Code;
    }
    if has(&["calcul", "calcule", "math", "équation", "equation", "intégrale", "dérivée", "résous", "resous", "pourcentage", "combien font", "somme", "produit", "probabilité"]) {
        return Category::Math;
    }
    if has(&["pourquoi", "explique", "raisonne", "analyse", "preuve", "plan", "stratégie", "compare", "logique", "démontre"]) {
        return Category::Reasoning;
    }
    if has(&["écris", "ecris", "rédige", "redige", "poème", "poeme", "histoire", "lettre", "résume", "resume", "traduis", "traduction", "email", "article"]) {
        return Category::Writing;
    }
    if has(&["rapide", "vite", "court", "simple", "bref"]) {
        return Category::Fast;
    }
    Category::General
}

/// Published capability indices (0 when absent).
fn bench(m: &ModelEntry) -> (f64, f64, f64) {
    match m.benchmarks.as_ref().and_then(|b| b.artificial_analysis.as_ref()) {
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

/// Preference keywords per category (tie-break only; benchmarks dominate).
fn keyword_bonus(id: &str, cat: Category) -> i64 {
    let prefs: &[&str] = match cat {
        Category::Code => &["coder", "code", "deepseek", "laguna", "north-mini-code", "poolside", "qwen", "ling"],
        Category::Reasoning => &["reasoning", "thinking", "think", "r1", "nemotron", "ultra", "super", "qwen", "magistral"],
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
    if id.contains("embedding") || id.contains("rerank") {
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
        "flash", "mini", "small", "lightning", "nano", "lfm", "tiny", "north-mini", "2.6b", "1.5b",
        "3b", "4b", "7b", "8b", "12b",
    ] {
        if id.contains(k) {
            s += 40;
        }
    }
    for k in ["550b", "120b", "ultra", "super", "pro", "27b", "31b", "26b", "72b"] {
        if id.contains(k) {
            s -= 40;
        }
    }
    s
}

/// Rank models for a problem, best first.
pub fn rank(problem: &str, models: &[ModelEntry], top: usize) -> (Category, Vec<(i64, ModelEntry)>) {
    let cat = classify(problem);
    let mut scored: Vec<(i64, ModelEntry)> = models
        .iter()
        .cloned()
        .map(|m| (score(&m, cat), m))
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0));
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
