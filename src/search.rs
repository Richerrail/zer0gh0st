//! Recherche web sans clé API.
//!
//! Un seul moteur ne suffit pas : DuckDuckGo renvoie régulièrement une page de
//! vérification (202, anti-bot) à un client scripté, et l'API « Instant Answer »
//! ne contient presque jamais de résultats pour une requête générale. D'où une
//! **chaîne de repli** : DDG HTML → DDG Lite → Mojeek → DDG Lite via `curl`
//! (l'empreinte TLS de curl passe parfois là où reqwest est filtré) → Instant
//! Answer. Le premier moteur qui renvoie des résultats gagne, et son nom est
//! remonté pour que l'utilisateur sache d'où vient la réponse.

use crate::http_client;
use anyhow::Result;
use serde::Deserialize;

/// A single web search result.
#[derive(Debug, Clone)]
pub struct SearchHit {
    pub title: String,
    pub text: String,
    pub url: String,
}

/// UA de navigateur réel : un UA « bot » déclenche la page anti-robot de DDG.
const UA: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) \
                  Chrome/124.0.0.0 Safari/537.36";
const ACCEPT_LANG: &str = "fr-FR,fr;q=0.9,en;q=0.8";
const ACCEPT: &str = "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8";

/// Recherche web ; garde la signature historique (liste de résultats).
pub async fn web_search(query: &str, max: usize) -> Result<Vec<SearchHit>> {
    Ok(web_search_with_backend(query, max).await?.0)
}

/// Recherche web avec le nom du moteur qui a répondu (`"aucun"` si vide).
pub async fn web_search_with_backend(
    query: &str,
    max: usize,
) -> Result<(Vec<SearchHit>, &'static str)> {
    let max = max.clamp(1, 20);
    let q = urlencoding::encode(query);

    // 1. DuckDuckGo HTML (POST : c'est ce qu'utilise le formulaire du site).
    if let Some(hits) = ddg_html(&q, max).await {
        if !hits.is_empty() {
            return Ok((hits, "duckduckgo"));
        }
    }
    // 2. DuckDuckGo Lite (markup différent, souvent moins filtré).
    if let Some(hits) = ddg_lite(&q, max).await {
        if !hits.is_empty() {
            return Ok((hits, "duckduckgo-lite"));
        }
    }
    // 3. Mojeek : index indépendant, pas de clé, page simple à parser.
    if let Some(hits) = mojeek(&q, max).await {
        if !hits.is_empty() {
            return Ok((hits, "mojeek"));
        }
    }
    // 4. Repli via `curl` : autre pile TLS, autre empreinte → parfois accepté là
    //    où reqwest est bloqué. Mêmes parseurs.
    if let Some(body) = curl_fetch(&format!("https://lite.duckduckgo.com/lite/?q={q}")).await {
        let hits = parse_ddg_lite(&body, max);
        if !hits.is_empty() {
            return Ok((hits, "duckduckgo-lite (curl)"));
        }
    }
    if let Some(body) = curl_fetch(&format!("https://www.mojeek.com/search?q={q}")).await {
        let hits = parse_mojeek(&body, max);
        if !hits.is_empty() {
            return Ok((hits, "mojeek (curl)"));
        }
    }
    // 5. Instant Answer : utile pour les entités, pas pour une vraie requête.
    if let Ok(hits) = instant_answer(query, max).await {
        if !hits.is_empty() {
            return Ok((hits, "ddg-instant"));
        }
    }
    Ok((Vec::new(), "aucun"))
}

/// Format hits as plain text for a tool result or command output.
pub fn format_hits(hits: &[SearchHit]) -> String {
    if hits.is_empty() {
        return "Aucun résultat web.".to_string();
    }
    let mut s = String::new();
    for (i, h) in hits.iter().enumerate() {
        s.push_str(&format!(
            "{}. {} — {}\n   {}\n",
            i + 1,
            h.title,
            h.url,
            h.text
        ));
    }
    s
}

// --------------------------------------------------------------- transport

fn request_headers(req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
    req.header("User-Agent", UA)
        .header("Accept", ACCEPT)
        .header("Accept-Language", ACCEPT_LANG)
}

async fn get_text(url: &str) -> Option<String> {
    let resp = request_headers(http_client::HTTP_CLIENT.get(url))
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    resp.text().await.ok()
}

/// Repli `curl` (binaire système). Retourne le corps HTML si le code est 0.
async fn curl_fetch(url: &str) -> Option<String> {
    let out = tokio::process::Command::new("curl")
        .args([
            "-sL",
            "--max-time",
            "20",
            "-A",
            UA,
            "-H",
            &format!("Accept-Language: {ACCEPT_LANG}"),
            "-H",
            &format!("Accept: {ACCEPT}"),
            url,
        ])
        .output()
        .await
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).to_string())
}

// ------------------------------------------------------------ DuckDuckGo

async fn ddg_html(q: &str, max: usize) -> Option<Vec<SearchHit>> {
    let resp = request_headers(http_client::HTTP_CLIENT.post("https://html.duckduckgo.com/html/"))
        .form(&[("q", q), ("kl", "fr-fr")])
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let body = resp.text().await.ok()?;
    Some(parse_ddg(&body, max))
}

async fn ddg_lite(q: &str, max: usize) -> Option<Vec<SearchHit>> {
    let body = get_text(&format!("https://lite.duckduckgo.com/lite/?q={q}")).await?;
    Some(parse_ddg_lite(&body, max))
}

async fn mojeek(q: &str, max: usize) -> Option<Vec<SearchHit>> {
    let body = get_text(&format!("https://www.mojeek.com/search?q={q}")).await?;
    Some(parse_mojeek(&body, max))
}

// --------------------------------------------------------------- parsing

/// Strip tags and decode the main HTML entities.
fn clean(s: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#x27;", "'")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Lit un attribut `key="…"` ou `key='…'`.
fn attr_any(s: &str, key: &str) -> Option<String> {
    for quote in ['"', '\''] {
        let pat = format!("{key}={quote}");
        if let Some(start) = s.find(&pat) {
            let start = start + pat.len();
            if let Some(end) = s[start..].find(quote) {
                return Some(s[start..start + end].to_string());
            }
        }
    }
    None
}

/// Texte entre le premier `>` et la première occurrence de `close`.
fn between(s: &str, open: char, close: &str) -> Option<String> {
    let start = s.find(open)? + 1;
    let end = s[start..].find(close)? + start;
    Some(s[start..end].to_string())
}

/// DDG wraps links in `//duckduckgo.com/l/?uddg=<encoded>`.
fn real_url(href: &str) -> String {
    if let Some(pos) = href.find("uddg=") {
        let rest = &href[pos + 5..];
        let enc = rest.split('&').next().unwrap_or(rest);
        if let Ok(dec) = urlencoding::decode(enc) {
            return dec.to_string();
        }
    }
    if let Some(rest) = href.strip_prefix("//") {
        return format!("https://{rest}");
    }
    href.to_string()
}

fn dedupe(hits: &mut Vec<SearchHit>, max: usize) {
    let mut seen: Vec<String> = Vec::new();
    hits.retain(|h| {
        if h.url.is_empty() || seen.contains(&h.url) {
            return false;
        }
        seen.push(h.url.clone());
        true
    });
    hits.truncate(max);
}

/// Parseur commun : repère un marqueur de résultat, remonte au début de la
/// balise (`<`), puis lit `href`, le titre et le résumé.
///
/// Remonter au `<` est indispensable : selon le moteur, `href` peut être écrit
/// **avant** ou **après** la classe (`<a href=… class=…>` vs `<a class=… href=…>`).
fn parse_results(
    html: &str,
    max: usize,
    marker: &str,
    title_close: &str,
    snippet_marker: &str,
    snippet_close: &str,
) -> Vec<SearchHit> {
    let mut hits = Vec::new();
    let mut search_from = 0usize;
    while hits.len() < max {
        let Some(rel) = html[search_from..].find(marker) else {
            break;
        };
        let marker_pos = search_from + rel;
        let start = html[..marker_pos].rfind('<').unwrap_or(marker_pos);
        let block = &html[start..];
        let title = between(block, '>', title_close)
            .map(|t| clean(&t))
            .unwrap_or_default();
        let url = attr_any(block, "href")
            .map(|h| real_url(&h))
            .unwrap_or_default();
        let after = marker_pos - start + marker.len();
        let next = block[after..]
            .find(marker)
            .map(|p| p + after)
            .unwrap_or(block.len());
        let snippet = block[..next]
            .find(snippet_marker)
            .and_then(|p| between(&block[p..], '>', snippet_close))
            .map(|t| clean(&t))
            .unwrap_or_default();
        if !title.is_empty() {
            hits.push(SearchHit {
                title,
                text: snippet,
                url,
            });
        }
        // Avancer **après** le marqueur, sinon on reparse le même résultat en
        // boucle (et `dedupe` le réduisait à un seul).
        search_from = marker_pos + marker.len();
    }
    dedupe(&mut hits, max);
    hits
}

/// Page HTML classique : `class="result__a"` + `class="result__snippet"`.
fn parse_ddg(html: &str, max: usize) -> Vec<SearchHit> {
    parse_results(
        html,
        max,
        "class=\"result__a\"",
        "</a>",
        "class=\"result__snippet\"",
        "</a>",
    )
}

/// Page « Lite » : `class='result-link'` puis `class='result-snippet'`.
fn parse_ddg_lite(html: &str, max: usize) -> Vec<SearchHit> {
    parse_results(html, max, "result-link", "</a>", "result-snippet", "</td>")
}

/// Mojeek : `<h2><a class="ob" href="…">Titre</a></h2>` + `<p class="s">résumé</p>`.
fn parse_mojeek(html: &str, max: usize) -> Vec<SearchHit> {
    parse_results(html, max, "class=\"ob\"", "</a>", "class=\"s\"", "</p>")
}

// ------------------------------------------------------- Instant Answer API

#[derive(Deserialize)]
struct DdgResponse {
    #[serde(default, rename = "AbstractText")]
    abstract_text: String,
    #[serde(default, rename = "AbstractURL")]
    abstract_url: String,
    #[serde(default, rename = "Heading")]
    heading: String,
    #[serde(default, rename = "RelatedTopics")]
    related: Vec<serde_json::Value>,
}

async fn instant_answer(query: &str, max: usize) -> Result<Vec<SearchHit>> {
    let q = urlencoding::encode(query);
    let url = format!("https://api.duckduckgo.com/?q={q}&format=json&no_html=1&skip_disambig=1");
    let resp = request_headers(http_client::HTTP_CLIENT.get(&url))
        .send()
        .await?;
    let data: DdgResponse = resp.json().await?;
    let mut hits = Vec::new();
    if !data.abstract_text.is_empty() {
        hits.push(SearchHit {
            title: if data.heading.is_empty() {
                query.to_string()
            } else {
                data.heading.clone()
            },
            text: data.abstract_text.clone(),
            url: data.abstract_url.clone(),
        });
    }
    for topic in data.related {
        if hits.len() >= max {
            break;
        }
        let text = topic.get("Text").and_then(|v| v.as_str()).unwrap_or("");
        if text.is_empty() {
            continue;
        }
        hits.push(SearchHit {
            title: text.chars().take(80).collect(),
            text: text.to_string(),
            url: topic
                .get("FirstURL")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
        });
    }
    Ok(hits)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ddg_html_parses_title_url_and_snippet() {
        let html = r##"
        <div class="result">
          <a rel="nofollow" class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com%2Fa&amp;rut=x">Example A</a>
          <a class="result__snippet" href="#">Premier <b>résultat</b> utile.</a>
        </div>
        <div class="result">
          <a rel="nofollow" class="result__a" href="https://example.org/b">Example B</a>
          <a class="result__snippet" href="#">Deuxième.</a>
        </div>"##;
        let hits = parse_ddg(html, 5);
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].title, "Example A");
        assert_eq!(hits[0].url, "https://example.com/a");
        assert_eq!(hits[0].text, "Premier résultat utile.");
        assert_eq!(hits[1].url, "https://example.org/b");
    }

    #[test]
    fn ddg_lite_parses_single_quoted_markup() {
        let html = r##"
        <a rel="nofollow" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com%2Fc" class='result-link'>Lite C</a>
        <td class='result-snippet'>Résumé <b>lite</b>.</td>
        <a rel="nofollow" href="https://example.net/d" class='result-link'>Lite D</a>
        <td class='result-snippet'>Autre.</td>"##;
        let hits = parse_ddg_lite(html, 5);
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].title, "Lite C");
        assert_eq!(hits[0].url, "https://example.com/c");
        assert_eq!(hits[0].text, "Résumé lite.");
        assert_eq!(hits[1].url, "https://example.net/d");
    }

    #[test]
    fn mojeek_parses_results() {
        let html = r##"
        <li><h2><a class="ob" href="https://example.com/e">Mojeek E</a></h2>
        <p class="s">Extrait <b>E</b>.</p></li>
        <li><h2><a class="ob" href="https://example.org/f">Mojeek F</a></h2>
        <p class="s">Extrait F.</p></li>"##;
        let hits = parse_mojeek(html, 5);
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].title, "Mojeek E");
        assert_eq!(hits[0].url, "https://example.com/e");
        assert_eq!(hits[0].text, "Extrait E.");
    }

    #[test]
    fn anti_bot_page_yields_no_hits() {
        let html = "<html><body>Unfortunately, bots use DuckDuckGo too.</body></html>";
        assert!(parse_ddg(html, 5).is_empty());
        assert!(parse_ddg_lite(html, 5).is_empty());
        assert!(parse_mojeek(html, 5).is_empty());
    }

    #[test]
    fn duplicates_are_removed() {
        let html = r##"
        <h2><a class="ob" href="https://same.example/x">Un</a></h2><p class="s">a</p>
        <h2><a class="ob" href="https://same.example/x">Deux</a></h2><p class="s">b</p>"##;
        assert_eq!(parse_mojeek(html, 5).len(), 1);
    }
}
