use anyhow::{anyhow, Result};
use rusqlite::{params, Connection};
use std::path::{Path, PathBuf};

use crate::mods;

pub fn db_path() -> PathBuf {
    crate::config::data_dir().join("knowledge.db")
}

pub fn default_dir() -> PathBuf {
    std::env::var("ZER0_KB_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let root = mods::project_root();
            if root.join("Cargo.toml").is_file() {
                root.join("base_connaissance")
            } else {
                crate::config::data_dir().join("base_connaissance")
            }
        })
}

#[derive(Debug, Clone)]
pub struct Hit {
    pub id: i64,
    pub sujet: String,
    pub contenu: String,
    pub source: String,
    pub score: f32,
}

const KB_SCHEMA_VERSION: i64 = 1;

pub fn canonical_source(source: &str) -> String {
    let s = source.trim();
    if s.is_empty() {
        return s.to_string();
    }
    if let Some(colon) = s.find(':') {
        let is_windows_drive = colon == 1
            && s[..1].chars().all(|c| c.is_ascii_alphabetic())
            && s[colon + 1..].starts_with(['/', '\\']);
        if !is_windows_drive {
            let scheme = &s[..colon];
            if !scheme.is_empty()
                && scheme
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
            {
                return s.to_string();
            }
        }
    }
    let p = Path::new(s);
    if let Ok(canon) = std::fs::canonicalize(p) {
        return canon.to_string_lossy().to_string();
    }
    let abs = if p.is_absolute() {
        p.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(p)
    };
    lexical_clean(&abs).to_string_lossy().to_string()
}

fn lexical_clean(p: &Path) -> PathBuf {
    use std::path::Component;
    let mut out = PathBuf::new();
    for comp in p.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            c => out.push(c.as_os_str()),
        }
    }
    if out.as_os_str().is_empty() {
        out.push("/");
    }
    out
}

fn legacy_source_forms(canon: &str) -> Vec<String> {
    let mut out = Vec::new();
    let p = Path::new(canon);
    if let Ok(rel) = p.strip_prefix(std::env::current_dir().unwrap_or_default()) {
        out.push(rel.to_string_lossy().to_string());
    }
    if let Ok(rel) = p.strip_prefix(crate::mods::project_root()) {
        out.push(rel.to_string_lossy().to_string());
    }
    out
}

pub fn dedupe_sources_on(c: &Connection) -> Result<usize> {
    let mut stmt = c.prepare("SELECT DISTINCT source FROM knowledge")?;
    let sources: Vec<String> = stmt
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    drop(stmt);

    let mut groups: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    for src in &sources {
        groups
            .entry(canonical_source(src))
            .or_default()
            .push(src.clone());
    }

    let mut changed = 0usize;
    for (canon, group) in &groups {
        if group.len() == 1 && group[0] == *canon {
            continue;
        }
        let newest = group
            .iter()
            .map(|src| {
                let max_id: i64 = c
                    .query_row(
                        "SELECT COALESCE(MAX(id), 0) FROM knowledge WHERE source = ?1",
                        params![src],
                        |r| r.get(0),
                    )
                    .unwrap_or(0);
                (max_id, src.clone())
            })
            .max_by_key(|(id, _)| *id)
            .map(|(_, src)| src);
        let Some(keep) = newest else { continue };
        for src in group {
            if src == &keep {
                continue;
            }
            c.execute("DELETE FROM knowledge WHERE source = ?1", params![src])?;
            changed += 1;
        }
        if keep != *canon {
            c.execute(
                "UPDATE knowledge SET source = ?1 WHERE source = ?2",
                params![canon, keep],
            )?;
            changed += 1;
        }
    }
    if changed > 0 {
        c.execute(
            "INSERT INTO knowledge_fts(knowledge_fts) VALUES('rebuild')",
            [],
        )?;
    }
    Ok(changed)
}

pub fn source_exists(src: &str) -> bool {
    let p = Path::new(src);
    if p.is_absolute() {
        return p.exists();
    }
    p.exists() || crate::mods::project_root().join(p).exists()
}

pub fn purge_missing_on<F: Fn(&str) -> bool>(c: &Connection, exists: F) -> Result<usize> {
    let mut stmt = c.prepare("SELECT DISTINCT source FROM knowledge")?;
    let sources: Vec<String> = stmt
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let missing: Vec<String> = sources.into_iter().filter(|s| !exists(s)).collect();
    for src in &missing {
        c.execute("DELETE FROM knowledge WHERE source = ?1", params![src])?;
    }
    if !missing.is_empty() {
        c.execute(
            "INSERT INTO knowledge_fts(knowledge_fts) VALUES('rebuild')",
            [],
        )?;
    }
    Ok(missing.len())
}

pub fn purge_missing() -> Result<usize> {
    let c = open()?;
    purge_missing_on(&c, source_exists)
}

pub fn drop_source_on(c: &Connection, pattern: &str) -> Result<usize> {
    let pat = format!("%{}%", pattern.trim());
    let n = c.execute("DELETE FROM knowledge WHERE source LIKE ?1", params![pat])?;
    if n > 0 {
        c.execute(
            "INSERT INTO knowledge_fts(knowledge_fts) VALUES('rebuild')",
            [],
        )?;
    }
    Ok(n)
}

pub fn dedupe_sources() -> Result<usize> {
    let c = open()?;
    dedupe_sources_on(&c)
}

pub fn drop_source(pattern: &str) -> Result<usize> {
    if pattern.trim().is_empty() {
        return Err(anyhow!("source vide"));
    }
    let c = open()?;
    drop_source_on(&c, pattern)
}

pub fn one_per_source(hits: Vec<Hit>) -> Vec<Hit> {
    let mut seen: Vec<String> = Vec::new();
    let mut out = Vec::with_capacity(hits.len());
    for h in hits {
        if seen.contains(&h.source) {
            continue;
        }
        seen.push(h.source.clone());
        out.push(h);
    }
    out
}

pub(crate) fn open() -> Result<Connection> {
    let path = db_path();
    let c = Connection::open(&path)?;
    init(&c)?;
    check_schema(&c, &path)?;
    import_legacy_if_empty(&c, &path);
    Ok(c)
}

fn import_legacy_if_empty(c: &Connection, current: &Path) {
    let local: i64 = c
        .query_row("SELECT COUNT(*) FROM knowledge", [], |r| r.get(0))
        .unwrap_or(0);
    if local > 0 {
        return;
    }
    let legacy = crate::mods::project_root().join("knowledge.db");
    if !legacy.is_file() || legacy == current {
        return;
    }
    let attach = c.execute(
        "ATTACH DATABASE ?1 AS legacy",
        params![legacy.to_string_lossy().to_string()],
    );
    if attach.is_err() {
        return;
    }
    let imported: i64 = c
        .query_row("SELECT COUNT(*) FROM legacy.knowledge", [], |r| r.get(0))
        .unwrap_or(0);
    if imported > 0 {
        let _ = c.execute(
            "INSERT INTO knowledge (sujet, contenu, source)
             SELECT sujet, contenu, source FROM legacy.knowledge",
            [],
        );
        let _ = c.execute(
            "INSERT INTO knowledge_fts(knowledge_fts) VALUES('rebuild')",
            [],
        );
        eprintln!(
            "📚 base de connaissance importée depuis {} ({imported} chunks) — \
             vecteurs à recalculer : `zer0 kb ingest --embed`",
            legacy.display()
        );
    }
    let _ = c.execute("DETACH DATABASE legacy", []);
}

fn check_schema(c: &Connection, path: &Path) -> Result<()> {
    let v: i64 = c.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if v > KB_SCHEMA_VERSION {
        return Err(anyhow!(
            "base de connaissance {} créée par une version plus récente \
             (schéma {v} > {KB_SCHEMA_VERSION}) — mets à jour ZER0|GH0ST",
            path.display()
        ));
    }
    if v < KB_SCHEMA_VERSION {
        c.pragma_update(None, "user_version", KB_SCHEMA_VERSION)?;
    }
    Ok(())
}

fn init(c: &Connection) -> Result<()> {
    c.execute_batch(
        "PRAGMA journal_mode=WAL;
         PRAGMA synchronous=NORMAL;
         CREATE TABLE IF NOT EXISTS knowledge(
             id      INTEGER PRIMARY KEY,
             sujet   TEXT NOT NULL,
             contenu TEXT NOT NULL,
             source  TEXT NOT NULL
         );
         CREATE INDEX IF NOT EXISTS knowledge_source ON knowledge(source);
         CREATE TABLE IF NOT EXISTS embeddings(
             knowledge_id INTEGER PRIMARY KEY,
             embedder     TEXT NOT NULL,
             vector       BLOB NOT NULL
         );",
    )?;
    c.execute_batch(
        "CREATE VIRTUAL TABLE IF NOT EXISTS knowledge_fts
         USING fts5(sujet, contenu, content='knowledge', content_rowid='id');",
    )?;
    Ok(())
}

fn collect_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            collect_files(&p, out);
        } else if p.is_file() {
            let ext = p
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if matches!(ext.as_str(), "txt" | "md" | "rst" | "org" | "tsv" | "csv") {
                out.push(p);
            }
        }
    }
}

fn chunks(text: &str, max: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for ch in text.chars() {
        if cur.len() + ch.len_utf8() > max && !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
        }
        cur.push(ch);
    }
    if !cur.trim().is_empty() {
        out.push(cur);
    }
    out
}

fn f32_to_blob(v: &[f32]) -> Vec<u8> {
    let mut b = Vec::with_capacity(v.len() * 4);
    for f in v {
        b.extend_from_slice(&f.to_le_bytes());
    }
    b
}

fn blob_to_f32(b: &[u8]) -> Vec<f32> {
    b.as_chunks::<4>()
        .0
        .iter()
        .map(|c| f32::from_le_bytes(*c))
        .collect()
}

const STOPWORDS: &[&str] = &[
    "salut",
    "bonjour",
    "bonsoir",
    "coucou",
    "hello",
    "hey",
    "allo",
    "slt",
    "bjr",
    "merci",
    "oui",
    "non",
    "le",
    "la",
    "les",
    "un",
    "une",
    "des",
    "de",
    "du",
    "au",
    "aux",
    "et",
    "ou",
    "où",
    "que",
    "qui",
    "quoi",
    "dont",
    "pour",
    "par",
    "sur",
    "dans",
    "avec",
    "sans",
    "en",
    "vers",
    "chez",
    "ce",
    "cet",
    "cette",
    "ces",
    "mon",
    "ma",
    "mes",
    "ton",
    "ta",
    "tes",
    "son",
    "sa",
    "ses",
    "notre",
    "nos",
    "votre",
    "vos",
    "leur",
    "leurs",
    "je",
    "tu",
    "il",
    "elle",
    "on",
    "nous",
    "vous",
    "ils",
    "elles",
    "me",
    "te",
    "se",
    "moi",
    "toi",
    "lui",
    "est",
    "sont",
    "etre",
    "être",
    "ete",
    "été",
    "avoir",
    "ai",
    "as",
    "avons",
    "avez",
    "ont",
    "parle",
    "parler",
    "dis",
    "dire",
    "explique",
    "expliquer",
    "donne",
    "donner",
    "fais",
    "faire",
    "comment",
    "pourquoi",
    "quel",
    "quelle",
    "quels",
    "the",
    "an",
    "of",
    "to",
    "in",
    "is",
    "are",
    "and",
    "or",
    "for",
    "with",
    "what",
    "how",
    "why",
    "salut",
    "bonjour",
    "hello",
    "merci",
    "stp",
    "svp",
    "raconte",
    "raconter",
    "tout",
    "tous",
    "toute",
    "toutes",
    "monde",
    "chose",
    "choses",
    "quelque",
    "beaucoup",
    "peux",
    "peut",
    "veux",
    "voudrais",
    "aimerais",
    "svp",
    "sais",
    "voir",
    "juste",
];

pub fn tokens(raw: &str) -> Vec<String> {
    raw.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.len() >= 3 && !STOPWORDS.contains(t))
        .map(|t| t.to_string())
        .collect()
}

fn fts_query(raw: &str, op: &str) -> String {
    let ts = tokens(raw);
    if ts.is_empty() {
        return String::new();
    }
    ts.iter()
        .map(|t| format!("\"{t}\""))
        .collect::<Vec<_>>()
        .join(op)
}

fn fts_search(c: &Connection, query: &str, limit: usize) -> Result<Vec<Hit>> {
    for op in [" AND ", " OR "] {
        let q = fts_query(query, op);
        if q.is_empty() {
            return Ok(Vec::new());
        }
        let mut stmt = c.prepare(
            "SELECT k.id, k.sujet, k.contenu, k.source, bm25(knowledge_fts) AS rank
             FROM knowledge_fts
             JOIN knowledge k ON k.id = knowledge_fts.rowid
             WHERE knowledge_fts MATCH ?1
             ORDER BY rank
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![q, limit as i64], |r| {
            let rank: f64 = r.get(4).unwrap_or(0.0);
            Ok(Hit {
                id: r.get(0)?,
                sujet: r.get(1)?,
                contenu: r.get(2)?,
                source: r.get(3)?,
                score: (1.0 / (1.0 + rank.abs())) as f32,
            })
        })?;
        let hits: Vec<Hit> = rows.filter_map(|x| x.ok()).collect();
        if !hits.is_empty() {
            return Ok(hits);
        }
    }
    Ok(Vec::new())
}

fn like_search(c: &Connection, query: &str, limit: usize) -> Result<Vec<Hit>> {
    let pat = format!("%{}%", query);
    let mut stmt = c.prepare(
        "SELECT id, sujet, contenu, source FROM knowledge
         WHERE contenu LIKE ?1 OR sujet LIKE ?1 LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![pat, limit as i64], |r| {
        Ok(Hit {
            id: r.get(0)?,
            sujet: r.get(1)?,
            contenu: r.get(2)?,
            source: r.get(3)?,
            score: 0.5,
        })
    })?;
    Ok(rows.filter_map(|x| x.ok()).collect())
}

pub async fn search(query: &str, limit: usize) -> (Vec<Hit>, Option<String>) {
    let embedder = crate::memory::embedder_id();
    let (qv, embed_err) = match &embedder {
        Some(_) => match crate::memory::embed(query).await {
            Ok(v) => (Some(v), None),
            Err(e) => (None, Some(format!("embedding requête échoué: {e}"))),
        },
        None => (None, None),
    };

    let q = query.to_string();
    if let Ok((hits, degraded)) =
        tokio::task::spawn_blocking(move || search_blocking(&q, limit, embedder, qv)).await
    {
        return (hits, degraded.or(embed_err));
    }
    (
        Vec::new(),
        Some("recherche interrompue (tâche bloquante)".into()),
    )
}

fn search_blocking(
    query: &str,
    limit: usize,
    embedder: Option<String>,
    qv: Option<Vec<f32>>,
) -> (Vec<Hit>, Option<String>) {
    let c = match open() {
        Ok(c) => c,
        Err(e) => return (Vec::new(), Some(format!("DB: {e}"))),
    };

    let mut hits = fts_search(&c, query, limit).unwrap_or_default();
    let mut degraded = None;

    match (embedder, qv) {
        (Some(embedder), Some(qv)) => {
            let rows = c
                .prepare("SELECT knowledge_id, vector FROM embeddings WHERE embedder = ?1")
                .and_then(|mut stmt| {
                    let mapped = stmt
                        .query_map(params![embedder], |r| {
                            let id: i64 = r.get(0)?;
                            let v: Vec<u8> = r.get(1)?;
                            Ok((id, v))
                        })?
                        .collect::<std::result::Result<Vec<_>, _>>();
                    mapped
                });
            if let Ok(rows) = rows {
                let mut vec_scores: Vec<(i64, f32)> = rows
                    .into_iter()
                    .map(|(id, v)| (id, cosine(&qv, &blob_to_f32(&v))))
                    .collect();
                vec_scores
                    .sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
                for (id, s) in vec_scores.into_iter().take(limit) {
                    if let Some(h) = hits.iter_mut().find(|h| h.id == id) {
                        h.score = (h.score + s).max(h.score);
                    } else if let Ok(mut stmt) =
                        c.prepare("SELECT id, sujet, contenu, source FROM knowledge WHERE id = ?1")
                    {
                        if let Ok(r) = stmt.query_row(params![id], |r| {
                            Ok(Hit {
                                id: r.get(0)?,
                                sujet: r.get(1)?,
                                contenu: r.get(2)?,
                                source: r.get(3)?,
                                score: s,
                            })
                        }) {
                            hits.push(r);
                        }
                    }
                }
            }
        }
        _ => {
            if hits.is_empty() {
                hits = like_search(&c, query, limit).unwrap_or_default();
                degraded = Some("aucun embedder configuré → recherche FTS5/LIKE".into());
            }
        }
    }

    hits.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    hits.truncate(limit);
    (hits, degraded)
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let mut dot = 0.0;
    let mut na = 0.0;
    let mut nb = 0.0;
    for (x, y) in a.iter().zip(b.iter()) {
        dot += x * y;
        na += x * x;
        nb += y * y;
    }
    if na == 0.0 || nb == 0.0 {
        0.0
    } else {
        dot / (na.sqrt() * nb.sqrt())
    }
}

pub async fn ingest_dir(dir: &Path, embed: bool) -> Result<(usize, usize)> {
    let mut files = Vec::new();
    collect_files(dir, &mut files);
    if files.is_empty() {
        return Err(anyhow!("aucun fichier texte dans {}", dir.display()));
    }

    let c = open()?;
    c.execute_batch("BEGIN")?;
    let embedder = if embed {
        crate::memory::embedder_id()
    } else {
        None
    };
    let mut nfiles = 0usize;
    let mut nchunks = 0usize;

    for f in &files {
        let Ok(text) = std::fs::read_to_string(f) else {
            continue;
        };
        let source = canonical_source(&f.to_string_lossy());
        c.execute("DELETE FROM knowledge WHERE source = ?1", params![source])?;
        for old in legacy_source_forms(&source) {
            if old != source {
                c.execute("DELETE FROM knowledge WHERE source = ?1", params![old])?;
            }
        }

        let sujet = f
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        nfiles += 1;

        for ch in chunks(&text, 1200) {
            if ch.trim().is_empty() {
                continue;
            }
            c.execute(
                "INSERT INTO knowledge(sujet, contenu, source) VALUES (?1, ?2, ?3)",
                params![sujet, ch, source],
            )?;
            let id = c.last_insert_rowid();
            c.execute(
                "INSERT INTO knowledge_fts(rowid, sujet, contenu) VALUES (?1, ?2, ?3)",
                params![id, sujet, ch],
            )?;
            if let Some(emb) = &embedder {
                if let Ok(v) = crate::memory::embed(&ch).await {
                    c.execute(
                        "INSERT OR REPLACE INTO embeddings(knowledge_id, embedder, vector) VALUES (?1, ?2, ?3)",
                        params![id, emb, f32_to_blob(&v)],
                    )?;
                }
            }
            nchunks += 1;
        }
    }
    c.execute_batch("COMMIT")?;
    Ok((nfiles, nchunks))
}

pub async fn add(source: &str, sujet: &str, text: &str, embed: bool) -> Result<(usize, bool)> {
    if text.trim().is_empty() {
        return Err(anyhow!("texte vide"));
    }
    let parts: Vec<String> = chunks(text, 1200)
        .into_iter()
        .filter(|c| !c.trim().is_empty())
        .collect();
    if parts.is_empty() {
        return Err(anyhow!("aucun contenu à indexer"));
    }

    let embedder = if embed {
        crate::memory::embedder_id()
    } else {
        None
    };
    let mut vectors: Vec<Option<Vec<f32>>> = vec![None; parts.len()];
    if embedder.is_some() {
        for (i, ch) in parts.iter().enumerate() {
            if let Ok(v) = crate::memory::embed(ch).await {
                vectors[i] = Some(v);
            }
        }
    }
    let embedded = vectors.iter().any(|v| v.is_some());

    let source = canonical_source(source);
    let sujet = sujet.to_string();
    let n = parts.len();
    let embedder = embedder.filter(|_| embedded);
    tokio::task::spawn_blocking(move || -> Result<()> {
        let c = open()?;
        c.execute_batch("BEGIN")?;
        c.execute("DELETE FROM knowledge WHERE source = ?1", params![source])?;
        for (i, ch) in parts.iter().enumerate() {
            c.execute(
                "INSERT INTO knowledge(sujet, contenu, source) VALUES (?1, ?2, ?3)",
                params![sujet, ch, source],
            )?;
            let id = c.last_insert_rowid();
            c.execute(
                "INSERT INTO knowledge_fts(rowid, sujet, contenu) VALUES (?1, ?2, ?3)",
                params![id, sujet, ch],
            )?;
            if let (Some(e), Some(v)) = (&embedder, vectors.get(i).and_then(|v| v.as_ref())) {
                c.execute(
                    "INSERT OR REPLACE INTO embeddings(knowledge_id, embedder, vector) \
                     VALUES (?1, ?2, ?3)",
                    params![id, e, f32_to_blob(v)],
                )?;
            }
        }
        c.execute_batch("COMMIT")?;
        Ok(())
    })
    .await
    .map_err(|e| anyhow!("écriture KB interrompue: {e}"))??;

    Ok((n, embedded))
}

pub fn rebuild_fts() -> Result<()> {
    let c = open()?;
    c.execute(
        "INSERT INTO knowledge_fts(knowledge_fts) VALUES('rebuild')",
        [],
    )?;
    Ok(())
}

pub fn stats() -> Result<String> {
    let c = open()?;
    let entries: i64 = c.query_row("SELECT COUNT(*) FROM knowledge", [], |r| r.get(0))?;
    let files: i64 = c.query_row("SELECT COUNT(DISTINCT source) FROM knowledge", [], |r| {
        r.get(0)
    })?;
    let vecs: i64 = c.query_row("SELECT COUNT(*) FROM embeddings", [], |r| r.get(0))?;
    Ok(format!(
        "Base de connaissance : {}\n  entrées (chunks) : {entries}\n  fichiers         : {files}\n  vecteurs         : {vecs}\n  embedder         : {}\n  FTS5             : activé",
        db_path().display(),
        crate::memory::embedder_id().unwrap_or_else(|| "(aucun)".into())
    ))
}

pub fn log_path() -> PathBuf {
    crate::config::data_dir().join("knowledge.log")
}

fn now() -> String {
    crate::platform::now()
}

pub fn log_injection(query: &str, hits: &[Hit]) {
    if hits.is_empty() {
        return;
    }
    use std::io::Write;
    let mut line = format!("[{}] query={query:?}", now());
    for h in hits {
        line.push_str(&format!(
            " | {} <{}> score={:.3}",
            h.sujet, h.source, h.score
        ));
    }
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path())
    {
        let _ = writeln!(f, "{line}");
    }
}

pub fn tail_log(n: usize) -> String {
    let Ok(s) = std::fs::read_to_string(log_path()) else {
        return format!("Aucun log encore ({})", log_path().display());
    };
    let lines: Vec<&str> = s.lines().collect();
    let start = lines.len().saturating_sub(n);
    format!("{}\n({})", lines[start..].join("\n"), log_path().display())
}

pub fn format_hits(hits: &[Hit], degraded: &Option<String>) -> String {
    let mut s = format!("Connaissance : {} résultat(s)", hits.len());
    if let Some(d) = degraded {
        s.push_str(&format!("  (dégradé : {d})"));
    }
    s.push('\n');
    for h in hits {
        s.push_str(&format!(
            "\n[{}] {}  — {}\n  {}\n",
            h.score,
            h.sujet,
            h.source,
            h.contenu
                .replace('\n', " ")
                .chars()
                .take(300)
                .collect::<String>()
        ));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mem_db() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        init(&c).unwrap();
        c
    }

    fn add(c: &Connection, sujet: &str, contenu: &str, source: &str) {
        c.execute(
            "INSERT INTO knowledge (sujet, contenu, source) VALUES (?1, ?2, ?3)",
            params![sujet, contenu, source],
        )
        .unwrap();
    }

    #[test]
    fn purge_removes_only_missing_sources() {
        let c = mem_db();
        add(&c, "a", "contenu a", "/existe/a.txt");
        add(&c, "b", "contenu b", "/absent/b.txt");
        add(&c, "b2", "contenu b2", "/absent/b.txt");
        let removed = purge_missing_on(&c, |s| s.starts_with("/existe")).unwrap();
        assert_eq!(removed, 1, "une source distincte absente");
        let n: i64 = c
            .query_row("SELECT COUNT(*) FROM knowledge", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1);
        let fts: i64 = c
            .query_row("SELECT COUNT(*) FROM knowledge_fts", [], |r| r.get(0))
            .unwrap();
        assert_eq!(fts, 1);
    }

    #[test]
    fn drop_source_matches_a_substring() {
        let c = mem_db();
        add(&c, "a", "x", "/kb/livre_pirate.txt");
        add(&c, "b", "y", "/kb/autre.txt");
        assert_eq!(drop_source_on(&c, "pirate").unwrap(), 1);
        let n: i64 = c
            .query_row("SELECT COUNT(*) FROM knowledge", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn one_chunk_per_source_keeps_the_best_one() {
        let mk = |id: i64, src: &str, score: f32| Hit {
            id,
            sujet: "s".into(),
            contenu: "c".into(),
            source: src.into(),
            score,
        };
        let hits = vec![
            mk(1, "/livre", 0.9),
            mk(2, "/livre", 0.8),
            mk(3, "/autre", 0.7),
            mk(4, "/livre", 0.6),
        ];
        let out = one_per_source(hits);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].id, 1);
        assert_eq!(out[1].id, 3);
    }

    #[test]
    fn canonical_source_keeps_free_ids() {
        assert_eq!(canonical_source("note:projet-x"), "note:projet-x");
        assert_eq!(canonical_source("manuel:123"), "manuel:123");
        assert_eq!(
            canonical_source("https://example.com/x"),
            "https://example.com/x"
        );
    }

    #[test]
    fn canonical_source_unifies_relative_and_absolute() {
        let cwd = std::env::current_dir().unwrap();
        let rel = "base_connaissance/public-apis.tsv";
        let abs_path = cwd.join(rel);
        let abs = canonical_source(&abs_path.to_string_lossy());
        assert!(
            std::path::Path::new(&abs).is_absolute(),
            "forme non absolue : {abs}"
        );
        assert_eq!(
            std::path::Path::new(&abs)
                .file_name()
                .and_then(|n| n.to_str()),
            Some("public-apis.tsv")
        );
        if abs_path.is_file() {
            assert_eq!(canonical_source(rel), abs);
        }
        assert_eq!(canonical_source("./x/../y.txt"), canonical_source("y.txt"));
    }

    #[test]
    fn dedupe_merges_relative_and_absolute_forms() {
        let c = mem_db();
        let cwd = std::env::current_dir().unwrap();
        let rel = "base_connaissance/public-apis.tsv";
        let abs = cwd.join(rel);
        let other = cwd.join("base_connaissance/autre.txt");
        add(&c, "a", "contenu absolu", &abs.to_string_lossy());
        add(&c, "a", "contenu relatif", rel);
        add(&c, "b", "autre fichier", &other.to_string_lossy());

        let changed = dedupe_sources_on(&c).unwrap();
        assert!(changed >= 1, "au moins une source fusionnée");
        let n: i64 = c
            .query_row("SELECT COUNT(*) FROM knowledge", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 2);
        let srcs: Vec<String> = c
            .prepare("SELECT DISTINCT source FROM knowledge ORDER BY source")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert_eq!(srcs.len(), 2);
        assert!(
            srcs.iter().all(|s| std::path::Path::new(s).is_absolute()),
            "sources non absolues : {srcs:?}"
        );
    }

    #[test]
    fn greetings_produce_no_tokens() {
        assert!(tokens("salut").is_empty());
        assert!(tokens("bonjour").is_empty());
        assert!(tokens("merci beaucoup").iter().all(|t| t != "merci"));
        assert_eq!(tokens("salut rust"), vec!["rust".to_string()]);
    }

    #[test]
    fn tokens_filters_short_and_stopwords() {
        assert_eq!(
            tokens("algorithmique boussole"),
            vec!["algorithmique", "boussole"]
        );
        assert!(tokens("a bb ccc").contains(&"ccc".to_string()));
        assert!(!tokens("a bb").iter().any(|t| t.len() < 3));
        assert!(!tokens("les des dans").iter().any(|t| t == "les"));
    }
}
