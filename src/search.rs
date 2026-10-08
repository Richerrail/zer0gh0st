use anyhow::Result;
use serde::Deserialize;

/// A single web search result.
#[derive(Debug, Clone)]
pub struct SearchHit {
    pub title: String,
    pub text: String,
    pub url: String,
}

const UA: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Zer0/0.1";

/// Search DuckDuckGo. Uses the HTML endpoint (real results), then falls back to
/// the Instant Answer API.
pub async fn web_search(query: &str, max: usize) -> Result<Vec<SearchHit>> {
    let mut hits = html_search(query, max).await.unwrap_or_default();
    if hits.is_empty() {
        hits = instant_answer(query, max).await.unwrap_or_default();
    }
    hits.truncate(max);
    Ok(hits)
}

/// Format hits as plain text for a tool result or command output.
pub fn format_hits(hits: &[SearchHit]) -> String {
    if hits.is_empty() {
        return "Aucun résultat web.".to_string();
    }
    let mut s = String::new();
    for (i, h) in hits.iter().enumerate() {
        s.push_str(&format!("{}. {} — {}\n   {}\n", i + 1, h.title, h.url, h.text));
    }
    s
}

// ------------------------------------------------------------- HTML endpoint

async fn html_search(query: &str, max: usize) -> Result<Vec<SearchHit>> {
    let q = urlencoding::encode(query);
    let url = format!("https://html.duckduckgo.com/html/?q={q}");
    let resp = reqwest::Client::new()
        .get(&url)
        .header("User-Agent", UA)
        .send()
        .await?;
    if !resp.status().is_success() {
        return Ok(Vec::new());
    }
    let body = resp.text().await?;
    Ok(parse_ddg(&body, max))
}

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
        .trim()
        .to_string()
}

fn attr(s: &str, key: &str) -> Option<String> {
    let pat = format!("{key}=\"");
    let start = s.find(&pat)? + pat.len();
    let end = s[start..].find('"')? + start;
    Some(s[start..end].to_string())
}

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

fn parse_ddg(html: &str, max: usize) -> Vec<SearchHit> {
    let mut hits = Vec::new();
    let mut from = 0usize;
    while hits.len() < max {
        let Some(rel) = html[from..].find("class=\"result__a\"") else {
            break;
        };
        let start = from + rel;
        let block = &html[start..];
        let title = between(block, '>', "</a>").map(|t| clean(&t)).unwrap_or_default();
        let url = attr(block, "href").map(|h| real_url(&h)).unwrap_or_default();

        // snippet = first result__snippet before the next result__a
        let next = block[12..]
            .find("class=\"result__a\"")
            .map(|p| p + 12)
            .unwrap_or(block.len());
        let snippet = block[..next]
            .find("class=\"result__snippet\"")
            .and_then(|p| between(&block[p..], '>', "</a>"))
            .map(|t| clean(&t))
            .unwrap_or_default();

        if !title.is_empty() {
            hits.push(SearchHit {
                title,
                text: snippet,
                url,
            });
        }
        from = start + 12;
    }
    hits
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
    let resp = reqwest::Client::new()
        .get(&url)
        .header("User-Agent", UA)
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
