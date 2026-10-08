//! Base de connaissance locale : SQLite + FTS5 (+ recherche vectorielle optionnelle).
//!
//! Reproduit `zero_knowledge.db` du Zer0 Python, en Rust. Le magasin est un fichier
//! SQLite embarqué (`rusqlite`, feature `bundled`). La recherche est **hybride** :
//! FTS5 (full-text) + cosinus sur des embeddings stockés en BLOB — si un embedder est
//! configuré (sinon FTS5 seul, et on le dit).

use anyhow::{anyhow, Result};
use rusqlite::{params, Connection};
use std::path::{Path, PathBuf};

use crate::mods;

/// Fichier SQLite de la base de connaissance.
pub fn db_path() -> PathBuf {
    mods::project_root().join("knowledge.db")
}

/// Dossier source par défaut (`<ZER0v1>/base_connaissance`).
pub fn default_dir() -> PathBuf {
    std::env::var("ZER0_KB_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| mods::project_root().join("base_connaissance"))
}

#[derive(Debug, Clone)]
pub struct Hit {
    pub id: i64,
    pub sujet: String,
    pub contenu: String,
    pub source: String,
    pub score: f32,
}

fn open() -> Result<Connection> {
    let c = Connection::open(db_path())?;
    init(&c)?;
    Ok(c)
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
    // FTS5 (contenu externe = table knowledge).
    c.execute_batch(
        "CREATE VIRTUAL TABLE IF NOT EXISTS knowledge_fts
         USING fts5(sujet, contenu, content='knowledge', content_rowid='id');",
    )?;
    Ok(())
}

// ------------------------------------------------------------------ ingestion

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
            if matches!(ext.as_str(), "txt" | "md" | "rst" | "org") {
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
    b.chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

// --------------------------------------------------------------------- search

/// Mots vides ignorés dans la requête FTS (FR + EN).
const STOPWORDS: &[&str] = &[
    "le", "la", "les", "un", "une", "des", "de", "du", "au", "aux", "et", "ou", "où", "que",
    "qui", "quoi", "dont", "pour", "par", "sur", "dans", "avec", "sans", "en", "vers", "chez", "ce",
    "cet", "cette", "ces", "mon", "ma", "mes", "ton", "ta", "tes", "son", "sa", "ses", "notre",
    "nos", "votre", "vos", "leur", "leurs", "je", "tu", "il", "elle", "on", "nous", "vous", "ils",
    "elles", "me", "te", "se", "moi", "toi", "lui", "est", "sont", "etre", "être", "ete", "été",
    "avoir", "ai", "as", "avons", "avez", "ont", "parle", "parler", "dis", "dire", "explique",
    "expliquer", "donne", "donner", "fais", "faire", "comment", "pourquoi", "quel", "quelle", "quels",
    "the", "an", "of", "to", "in", "is", "are", "and", "or", "for", "with", "what", "how", "why",
    "salut", "bonjour", "hello", "merci", "stp", "svp", "raconte", "raconter", "tout", "tous",
    "toute", "toutes", "monde", "chose", "choses", "quelque", "beaucoup", "peux", "peut", "veux",
    "voudrais", "aimerais", "svp", "sais", "voir", "juste",
];

/// Mots significatifs d'une requête (minuscule, >=3 lettres, sans stopword).
pub fn tokens(raw: &str) -> Vec<String> {
    raw.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.len() >= 3 && !STOPWORDS.contains(t))
        .map(|t| t.to_string())
        .collect()
}

/// Construit une requête FTS5 (guillemets par terme, liaison `op`).
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
    // d'abord AND (tous les termes significatifs), sinon OR (un des termes).
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

/// Recherche hybride FTS5 + vectoriel. `degraded` = pas d'embedder / pas de vecteurs.
pub async fn search(query: &str, limit: usize) -> (Vec<Hit>, Option<String>) {
    let c = match open() {
        Ok(c) => c,
        Err(e) => return (Vec::new(), Some(format!("DB: {e}"))),
    };

    let mut hits = fts_search(&c, query, limit).unwrap_or_default();
    let mut degraded = None;

    // vectoriel si un embedder est configuré
    if let Some(embedder) = crate::memory::embedder_id() {
        match crate::memory::embed(query).await {
            Ok(qv) => {
                let mut stmt = c
                    .prepare("SELECT knowledge_id, vector FROM embeddings WHERE embedder = ?1")
                    .unwrap();
                let rows = stmt.query_map(params![embedder], |r| {
                    let id: i64 = r.get(0)?;
                    let v: Vec<u8> = r.get(1)?;
                    Ok((id, v))
                });
                if let Ok(rows) = rows {
                    let mut vec_scores: Vec<(i64, f32)> = Vec::new();
                    for row in rows.flatten() {
                        let v = blob_to_f32(&row.1);
                        vec_scores.push((row.0, cosine(&qv, &v)));
                    }
                    vec_scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
                    for (id, s) in vec_scores.into_iter().take(limit) {
                        if let Some(h) = hits.iter_mut().find(|h| h.id == id) {
                            h.score = (h.score + s).max(h.score);
                        } else if let Ok(mut stmt) = c.prepare(
                            "SELECT id, sujet, contenu, source FROM knowledge WHERE id = ?1",
                        ) {
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
            Err(e) => degraded = Some(format!("embedding requête échoué: {e}")),
        }
    } else if hits.is_empty() {
        hits = like_search(&c, query, limit).unwrap_or_default();
        degraded = Some("aucun embedder configuré → recherche FTS5/LIKE".into());
    }

    hits.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
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

// ------------------------------------------------------------------- ingestion

/// Indexe un dossier (récursif). `embed=true` calcule les embeddings (si embedder configuré).
/// Retourne (fichiers, chunks).
pub async fn ingest_dir(dir: &Path, embed: bool) -> Result<(usize, usize)> {
    let mut files = Vec::new();
    collect_files(dir, &mut files);
    if files.is_empty() {
        return Err(anyhow!("aucun fichier texte dans {}", dir.display()));
    }

    let c = open()?;
    // Une seule transaction pour tout l'ingest : sinon 1 fsync par chunk (très lent).
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
        let source = f.to_string_lossy().to_string();
        // remplace les entrées précédentes de ce fichier
        c.execute("DELETE FROM knowledge WHERE source = ?1", params![source])?;

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

/// Reconstruit l'index FTS5.
pub fn rebuild_fts() -> Result<()> {
    let c = open()?;
    c.execute("INSERT INTO knowledge_fts(knowledge_fts) VALUES('rebuild')", [])?;
    Ok(())
}

pub fn stats() -> Result<String> {
    let c = open()?;
    let entries: i64 = c.query_row("SELECT COUNT(*) FROM knowledge", [], |r| r.get(0))?;
    let files: i64 = c.query_row("SELECT COUNT(DISTINCT source) FROM knowledge", [], |r| r.get(0))?;
    let vecs: i64 = c.query_row("SELECT COUNT(*) FROM embeddings", [], |r| r.get(0))?;
    Ok(format!(
        "Base de connaissance : {}\n  entrées (chunks) : {entries}\n  fichiers         : {files}\n  vecteurs         : {vecs}\n  embedder         : {}\n  FTS5             : activé",
        db_path().display(),
        crate::memory::embedder_id().unwrap_or_else(|| "(aucun)".into())
    ))
}

// ---------------------------------------------------------------------- log

/// Fichier de log des injections (`<ZER0v1>/knowledge.log`).
pub fn log_path() -> PathBuf {
    mods::project_root().join("knowledge.log")
}

fn now() -> String {
    std::process::Command::new("date")
        .arg("+%Y-%m-%d %H:%M:%S")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

/// Journalise une injection : requête + sources (fichier + score).
pub fn log_injection(query: &str, hits: &[Hit]) {
    if hits.is_empty() {
        return;
    }
    use std::io::Write;
    let mut line = format!("[{}] query={query:?}", now());
    for h in hits {
        line.push_str(&format!(" | {} <{}> score={:.3}", h.sujet, h.source, h.score));
    }
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path())
    {
        let _ = writeln!(f, "{line}");
    }
}

/// Dernières lignes du log (pour `/kb log`).
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
            h.contenu.replace('\n', " ").chars().take(300).collect::<String>()
        ));
    }
    s
}
