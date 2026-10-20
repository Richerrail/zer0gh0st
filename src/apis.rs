//! Catalogue d'APIs publiques — source : <https://github.com/public-apis/public-apis>
//!
//! Le catalogue est **structuré** (une table SQLite dédiée), ce qui permet un
//! rendu en tableau avec la colonne « clé requise » — bien plus exploitable
//! qu'un chunk de Markdown.
//!
//! Honnêteté : le catalogue est un **instantané d'une liste communautaire**.
//! Beaucoup d'APIs meurent ; la date d'import est affichée avec chaque
//! résultat, et le rappel « à vérifier » est explicite.

use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use std::path::Path;

use crate::knowledge;

/// Une entrée du catalogue.
#[derive(Debug, Clone)]
pub struct ApiEntry {
    pub name: String,
    pub description: String,
    pub url: String,
    /// `No`, `apiKey`, `OAuth`… (colonne « Auth » du catalogue source).
    pub auth: String,
    pub https: String,
    pub cors: String,
    pub category: String,
}

impl ApiEntry {
    /// `true` si l'API s'utilise sans clé.
    pub fn keyless(&self) -> bool {
        self.auth.trim().eq_ignore_ascii_case("no")
    }
}

/// Métadonnées de l'import courant (source + date).
#[derive(Debug, Clone, Default)]
pub struct Meta {
    pub source: String,
    pub imported_at: String,
    pub count: usize,
}

fn init(c: &Connection) -> Result<()> {
    c.execute_batch(
        "CREATE TABLE IF NOT EXISTS public_apis(
             id         INTEGER PRIMARY KEY,
             name       TEXT NOT NULL,
             description TEXT NOT NULL,
             url        TEXT NOT NULL,
             auth       TEXT NOT NULL,
             https      TEXT NOT NULL,
             cors       TEXT NOT NULL,
             category   TEXT NOT NULL
         );
         CREATE INDEX IF NOT EXISTS public_apis_name ON public_apis(name);
         CREATE INDEX IF NOT EXISTS public_apis_category ON public_apis(category);
         CREATE TABLE IF NOT EXISTS public_apis_meta(
             key   TEXT PRIMARY KEY,
             value TEXT NOT NULL
         );",
    )?;
    Ok(())
}

fn conn() -> Result<Connection> {
    // Même fichier SQLite que la base de connaissance (tables FTS5 + les
    // nôtres) : on réutilise sa connexion initialisée.
    let c = knowledge::open()?;
    init(&c)?;
    Ok(c)
}

/// Importe un catalogue TSV (en-tête `name\tdescription\turl\tauth\thttps\tcors\tcategory`).
/// Les lignes commençant par `#` sont des commentaires (source, date).
pub fn import_tsv(path: &Path) -> Result<(usize, Meta)> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("lecture de {}", path.display()))?;
    let mut meta = Meta::default();
    let mut rows: Vec<ApiEntry> = Vec::new();

    for line in raw.lines() {
        let l = line.trim_end_matches(['\r', '\n']);
        if l.starts_with('#') {
            // Commentaire d'en-tête : « # source: … | imported: … »
            let body = l.trim_start_matches('#').trim();
            if let Some((k, v)) = body.split_once(':') {
                match k.trim() {
                    "source" => meta.source = v.trim().to_string(),
                    "imported" => meta.imported_at = v.trim().to_string(),
                    _ => {}
                }
            }
            continue;
        }
        if l.trim().is_empty() {
            continue;
        }
        let f: Vec<&str> = l.split('\t').collect();
        if f.len() < 3 {
            continue;
        }
        let get = |i: usize| f.get(i).unwrap_or(&"").trim().to_string();
        if get(0).is_empty() {
            continue;
        }
        rows.push(ApiEntry {
            name: get(0),
            description: get(1),
            url: get(2),
            auth: get(3),
            https: get(4),
            cors: get(5),
            category: get(6),
        });
    }

    let c = conn()?;
    c.execute("DELETE FROM public_apis", [])?;
    let tx = c.unchecked_transaction()?;
    {
        let mut st = tx.prepare(
            "INSERT INTO public_apis(name, description, url, auth, https, cors, category)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )?;
        for e in &rows {
            st.execute(params![
                e.name,
                e.description,
                e.url,
                e.auth,
                e.https,
                e.cors,
                e.category
            ])?;
        }
    }
    tx.commit()?;

    meta.count = rows.len();
    if meta.source.is_empty() {
        meta.source = "(source inconnue)".to_string();
    }
    if meta.imported_at.is_empty() {
        meta.imported_at = "(date inconnue)".to_string();
    }
    let c = conn()?;
    c.execute(
        "INSERT INTO public_apis_meta(key, value) VALUES('source', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![meta.source],
    )?;
    c.execute(
        "INSERT INTO public_apis_meta(key, value) VALUES('imported', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![meta.imported_at],
    )?;
    c.execute(
        "INSERT INTO public_apis_meta(key, value) VALUES('count', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![meta.count.to_string()],
    )?;
    Ok((rows.len(), meta))
}

/// Métadonnées de l'import courant.
pub fn meta() -> Result<Meta> {
    let c = conn()?;
    let mut m = Meta::default();
    let mut st = c.prepare("SELECT key, value FROM public_apis_meta")?;
    let mut rows = st.query([])?;
    while let Some(r) = rows.next()? {
        let k: String = r.get(0)?;
        let v: String = r.get(1)?;
        match k.as_str() {
            "source" => m.source = v,
            "imported" => m.imported_at = v,
            "count" => m.count = v.parse().unwrap_or(0),
            _ => {}
        }
    }
    Ok(m)
}

/// Recherche dans le catalogue (nom, description, catégorie).
/// Tri : correspondance exacte du nom d'abord, puis nom, puis description.
/// `keyless` ne garde que les APIs utilisables **sans clé**.
pub fn search_opts(query: &str, limit: usize, keyless: bool) -> Result<Vec<ApiEntry>> {
    let c = conn()?;
    let q = query.trim();
    if q.is_empty() {
        return Ok(Vec::new());
    }
    let like = format!("%{}%", q.to_lowercase());
    let mut st = c.prepare(
        "SELECT name, description, url, auth, https, cors, category FROM public_apis
         WHERE (lower(name) LIKE ?1 OR lower(description) LIKE ?1 OR lower(category) LIKE ?1)
           AND (?4 = 0 OR lower(auth) = 'no')
         ORDER BY CASE
             WHEN lower(name) = ?2 THEN 0
             WHEN lower(name) LIKE ?1 THEN 1
             WHEN lower(category) LIKE ?1 THEN 2
             ELSE 3 END,
             length(name), name
         LIMIT ?3",
    )?;
    let mut rows = st.query(params![like, q.to_lowercase(), limit as i64, if keyless {1i64} else {0i64}])?;
    let mut out = Vec::new();
    while let Some(r) = rows.next()? {
        out.push(ApiEntry {
            name: r.get(0)?,
            description: r.get(1)?,
            url: r.get(2)?,
            auth: r.get(3)?,
            https: r.get(4)?,
            cors: r.get(5)?,
            category: r.get(6)?,
        });
    }
    Ok(out)
}

/// Recherche (toutes les APIs) — raccourci.
#[allow(dead_code)]
pub fn search(query: &str, limit: usize) -> Result<Vec<ApiEntry>> {
    search_opts(query, limit, false)
}

/// Catégories et nombre d'APIs par catégorie.
pub fn categories() -> Result<Vec<(String, usize)>> {
    let c = conn()?;
    let mut st =
        c.prepare("SELECT category, COUNT(*) FROM public_apis GROUP BY category ORDER BY category")?;
    let mut rows = st.query([])?;
    let mut out = Vec::new();
    while let Some(r) = rows.next()? {
        out.push((r.get(0)?, r.get(1)?));
    }
    Ok(out)
}

/// Rendu lisible (TUI + CLI).
pub fn format_entries(entries: &[ApiEntry], meta: &Meta) -> String {
    if entries.is_empty() {
        // Distingue « catalogue vide » de « aucun résultat » : le message
        // doit être vrai dans les deux cas.
        if meta.count == 0 {
            return "Catalogue d'APIs publiques vide.\n\
                    Importe-le : scripts/kb-import-public-apis.sh"
                .to_string();
        }
        return format!(
            "Aucune API ne correspond dans le catalogue ({} entrées, importé le {}).\n\
             ⚠ Le catalogue est en ANGLAIS : cherche « weather », « crypto », « translate »…\n\
             Catégories : {}",
            meta.count,
            meta.imported_at,
            categories()
                .map(|c| {
                    c.iter()
                        .map(|(n, k)| format!("{n} ({k})"))
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .unwrap_or_default()
        );
    }
    let mut s = String::new();
    for e in entries {
        let key = if e.keyless() {
            "clé: non"
        } else if e.auth.trim().is_empty() {
            "clé: ?"
        } else {
            &format!("clé: {}", e.auth.trim())
        };
        s.push_str(&format!(
            "• {} — {}\n   {}\n   [{} · https:{} · cors:{} · {}]\n",
            e.name,
            e.description,
            e.url,
            key,
            e.https,
            e.cors,
            e.category
        ));
    }
    s.push_str(&format!(
        "\n{} API(s) — source : {} — importé le {} — ⚠ candidat, à vérifier (beaucoup d'APIs meurent).",
        entries.len(),
        meta.source,
        meta.imported_at
    ));
    s
}

/// État du catalogue (pour `/apis` sans argument).
pub fn stats() -> Result<String> {
    let m = meta()?;
    let cats = categories()?;
    if m.count == 0 {
        return Ok(
            "Catalogue d'APIs publiques vide.\nImporte-le : scripts/kb-import-public-apis.sh"
                .to_string(),
        );
    }
    let mut s = format!(
        "Catalogue d'APIs publiques : {} entrées · {} catégories\nsource : {}\nimporté le : {}\n",
        m.count,
        cats.len(),
        m.source,
        m.imported_at
    );
    s.push_str("\ncatégories :\n");
    for (c, n) in cats.iter().take(40) {
        s.push_str(&format!("  {c:<28} {n}\n"));
    }
    s.push_str("\n/utilisation : /apis <besoin>   (ex. /apis météo gratuit)\n");
    s.push_str("⚠ liste communautaire : les APIs peuvent être mortes — à vérifier avant usage.\n");
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(name: &str, desc: &str, auth: &str) -> ApiEntry {
        ApiEntry {
            name: name.into(),
            description: desc.into(),
            url: "https://example.org".into(),
            auth: auth.into(),
            https: "Yes".into(),
            cors: "No".into(),
            category: "Test".into(),
        }
    }

    #[test]
    fn keyless_detection() {
        assert!(e("A", "d", "No").keyless());
        assert!(!e("A", "d", "apiKey").keyless());
        assert!(!e("A", "d", "OAuth").keyless());
    }

    #[test]
    fn format_mentions_source_date_and_warning() {
        let m = Meta {
            source: "public-apis@abc".into(),
            imported_at: "2026-10-20".into(),
            count: 1,
        };
        let s = format_entries(&[e("Dog API", "chiens", "No")], &m);
        assert!(s.contains("Dog API"));
        assert!(s.contains("clé: non"));
        assert!(s.contains("public-apis@abc"));
        assert!(s.contains("2026-10-20"));
        assert!(s.contains("à vérifier"), "l'avertissement d'honnêteté doit être là");
    }

    #[test]
    fn empty_result_points_to_the_import_script() {
        let s = format_entries(&[], &Meta::default());
        assert!(s.contains("kb-import-public-apis.sh"));
        assert!(s.contains("vide"), "catalogue vide → message explicite");
    }

    #[test]
    fn no_match_says_so_and_hints_english() {
        let m = Meta {
            source: "public-apis@x".into(),
            imported_at: "2026-10-20".into(),
            count: 2161,
        };
        let s = format_entries(&[], &m);
        assert!(s.contains("Aucune API ne correspond"));
        assert!(s.contains("ANGLAIS"), "l'astuce doit être donnée");
        assert!(!s.contains("kb-import-public-apis.sh"), "le catalogue est là : pas de rappel d'import");
    }
}
