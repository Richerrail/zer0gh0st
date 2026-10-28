use serde_json::json;

use super::ToolSpec;

/// Longueur maximale renvoyée au modèle.
const MAX_CHARS: usize = 16_000;

pub fn spec() -> ToolSpec {
    ToolSpec {
        name: "web_read",
        description: "Read a web page as plain text. HTML is stripped; pages that need \
                      JavaScript can be rendered by a reader proxy with `render: true` \
                      (sends the URL to r.jina.ai) or ZER0_WEB_READER=jina.",
        parameters: json!({
            "type": "object",
            "properties": {
                "url": { "type": "string", "description": "http(s) URL." },
                "render": { "type": "boolean", "description": "Render JavaScript via r.jina.ai (third-party)." }
            },
            "required": ["url"]
        }),
    }
}

/// Convertit du HTML en texte lisible (sans dépendance) : retire scripts,
/// styles, balises et commentaires, décode les entités usuelles.
pub fn html_to_text(html: &str) -> String {
    let mut out = String::with_capacity(html.len() / 2);
    let mut rest = html;
    let mut skip: Option<&'static str> = None;
    while !rest.is_empty() {
        if let Some((idx, _)) = rest.char_indices().find(|(_, c)| *c == '<') {
            if idx > 0 {
                if skip.is_none() {
                    push_text(&mut out, &rest[..idx]);
                }
                rest = &rest[idx..];
            }
        } else {
            if skip.is_none() {
                push_text(&mut out, rest);
            }
            break;
        }

        // Balise : jusqu'au prochain '>'.
        let Some(end) = rest.find('>') else {
            break; // balise non fermée : on ignore la fin
        };
        let tag = &rest[1..end];
        rest = &rest[end + 1..];

        let lower = tag.to_ascii_lowercase();
        let name = lower
            .trim_start_matches('/')
            .split(|c: char| c.is_whitespace() || c == '/')
            .next()
            .unwrap_or("");
        if lower.starts_with("--") || lower.starts_with('!') {
            continue; // commentaire / doctype
        }
        if !lower.starts_with('/') {
            if name == "script" {
                skip = Some("script");
                continue;
            }
            if name == "style" {
                skip = Some("style");
                continue;
            }
        } else if skip.is_some_and(|s| s == name) {
            skip = None;
            continue;
        }
        if skip.is_none() && matches!(name, "p" | "br" | "div" | "li" | "tr" | "h1" | "h2" | "h3") {
            out.push('\n');
        }
    }
    normalize(&out)
}

fn push_text(out: &mut String, s: &str) {
    // Décodage des entités les plus courantes (`&amp;` en dernier pour ne pas
    // décoder deux fois une entité échappée).
    let decoded = s
        .replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&amp;", "&");
    out.push_str(&decoded);
}

fn normalize(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut blank = 0usize;
    for line in s.lines() {
        let line = line.trim();
        if line.is_empty() {
            blank += 1;
            if blank > 1 {
                continue;
            }
        } else {
            blank = 0;
        }
        out.push_str(line);
        out.push('\n');
    }
    out.trim().to_string()
}

pub async fn run(args: &str) -> String {
    let v: serde_json::Value = match serde_json::from_str(args) {
        Ok(v) => v,
        Err(e) => return format!("Erreur: arguments JSON invalides ({e})"),
    };
    let url = v.get("url").and_then(|x| x.as_str()).unwrap_or("").trim();
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return "Erreur: `url` doit commencer par http:// ou https://".into();
    }
    let render = v
        .get("render")
        .and_then(|x| x.as_bool())
        .unwrap_or_else(|| {
            std::env::var("ZER0_WEB_READER")
                .map(|v| v.eq_ignore_ascii_case("jina"))
                .unwrap_or(false)
        });
    let target = if render {
        format!("https://r.jina.ai/{url}")
    } else {
        url.to_string()
    };

    let resp = match crate::http_client::HTTP_CLIENT
        .get(&target)
        .timeout(std::time::Duration::from_secs(30))
        .header("user-agent", "Mozilla/5.0 (compatible; Zer0GH0ST)")
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => return format!("Erreur réseau: {e}"),
    };
    let status = resp.status();
    let body = match resp.text().await {
        Ok(t) => t,
        Err(e) => return format!("Erreur lecture: {e}"),
    };
    if !status.is_success() {
        return format!(
            "HTTP {status} : {}",
            body.chars().take(300).collect::<String>()
        );
    }
    let text = if render { body } else { html_to_text(&body) };
    let text = text.trim();
    let note = if render {
        "\n\n(rendu JavaScript via r.jina.ai)"
    } else {
        ""
    };
    format!(
        "{}\n\n{}{note}",
        url,
        text.chars().take(MAX_CHARS).collect::<String>()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_to_text_strips_tags_and_scripts() {
        let html = "<html><head><style>body{color:red}</style></head><body>\
                    <script>alert('x')</script><h1>Titre</h1>\
                    <p>Bonjour &amp; bienvenue.</p></body></html>";
        let t = html_to_text(html);
        assert!(t.contains("Titre"), "{t}");
        assert!(t.contains("Bonjour & bienvenue."), "{t}");
        assert!(!t.contains("alert"), "{t}");
        assert!(!t.contains("color:red"), "{t}");
        assert!(!t.contains('<'), "{t}");
    }

    #[test]
    fn spec_is_well_formed() {
        let s = spec();
        assert_eq!(s.name, "web_read");
        assert_eq!(s.parameters["required"][0], "url");
    }
}
