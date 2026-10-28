use serde_json::json;
use std::process::Stdio;
use std::time::Duration;

use super::ToolSpec;

/// Plafond du texte OCR renvoyé au modèle.
const MAX_CHARS: usize = 12_000;
/// Un OCR de capture d'écran dépasse rarement quelques secondes.
const TIMEOUT_SECS: u64 = 90;

pub fn spec() -> ToolSpec {
    ToolSpec {
        name: "read_image",
        description: "Read the TEXT inside an image (screenshot, photo, diagram) via OCR \
                      (tesseract). Returns image metadata + extracted text. This is OCR, \
                      not visual reasoning: layout/colors are not described.",
        parameters: json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Absolute path to a png/jpg/webp/bmp/gif/tiff image." },
                "lang": { "type": "string", "description": "Tesseract language(s), e.g. `eng`, `fra+eng` (default: eng)." }
            },
            "required": ["path"]
        }),
    }
}

/// Extensions acceptées (tesseract lit via Leptonica).
pub fn is_image_path(path: &str) -> bool {
    let ext = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    matches!(
        ext.as_str(),
        "png" | "jpg" | "jpeg" | "webp" | "bmp" | "gif" | "tif" | "tiff"
    )
}

pub async fn run(args: &str) -> String {
    let v: serde_json::Value = match serde_json::from_str(args) {
        Ok(v) => v,
        Err(e) => return format!("Erreur: arguments JSON invalides ({e})"),
    };
    let path = v.get("path").and_then(|x| x.as_str()).unwrap_or("").trim();
    if path.is_empty() {
        return "Erreur: `path` manquant".into();
    }
    if !std::path::Path::new(path).is_file() {
        return format!("Erreur: fichier introuvable `{path}`");
    }
    if !is_image_path(path) {
        return "Erreur: extension non supportée (png, jpg, jpeg, webp, bmp, gif, tif, tiff)"
            .into();
    }
    let lang = v
        .get("lang")
        .and_then(|x| x.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("eng");

    // Métadonnées (best effort) : dimensions + format, utile pour un screenshot.
    let mut meta = String::new();
    if let Ok(out) = tokio::process::Command::new("identify")
        .args(["-format", "%wx%h %m", path])
        .output()
        .await
    {
        if out.status.success() {
            let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !s.is_empty() {
                meta = format!(" ({s})");
            }
        }
    }

    let cmd = {
        let mut c = tokio::process::Command::new("tesseract");
        c.arg(path)
            .arg("stdout")
            .arg("-l")
            .arg(lang)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        c
    };
    let out = match tokio::time::timeout(Duration::from_secs(TIMEOUT_SECS), cmd.output()).await {
        Ok(Ok(o)) => o,
        Ok(Err(e)) => {
            return format!(
                "Erreur: impossible de lancer `tesseract` ({e}). \
                 Installe-le : `sudo pacman -S tesseract` (+ `tesseract-data-fra` pour le français)."
            )
        }
        Err(_) => return format!("Erreur: OCR interrompu après {TIMEOUT_SECS}s."),
    };

    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let first = err.lines().next().unwrap_or("échec inconnu");
        let hint = if err.contains("Failed loading language") {
            " — langue absente : `sudo pacman -S tesseract-data-fra` (ou choisis `lang: eng`)."
        } else {
            ""
        };
        return format!("Erreur OCR ({first}){hint}");
    }

    let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let body = if text.is_empty() {
        "(aucun texte détecté — image sans texte, ou langue non installée)".to_string()
    } else {
        text.chars().take(MAX_CHARS).collect()
    };
    format!("Image `{path}`{meta}\nOCR tesseract (lang={lang}) :\n{body}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_extensions_are_accepted_case_insensitively() {
        assert!(is_image_path("/tmp/a.PNG"));
        assert!(is_image_path("capture.jpeg"));
        assert!(is_image_path("x.webp"));
        assert!(!is_image_path("notes.txt"));
        assert!(!is_image_path("archive.tar.gz"));
        assert!(!is_image_path("sans_extension"));
    }

    #[test]
    fn spec_is_well_formed() {
        let s = spec();
        assert_eq!(s.name, "read_image");
        assert_eq!(s.parameters["required"][0], "path");
    }
}
