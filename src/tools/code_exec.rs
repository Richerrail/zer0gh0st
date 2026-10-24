use serde_json::json;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt};

use super::ToolSpec;

/// Octets capturés par flux (stdout/stderr) avant de jeter le reste.
///
/// On continue de **lire** au-delà du plafond (sinon l'enfant se bloque sur un
/// pipe plein), mais sans stocker : la mémoire du processus reste bornée.
const MAX_CAPTURE_BYTES: usize = 256 * 1024;
/// Octets réellement renvoyés au modèle (le contexte n'est pas gratuit).
const MAX_RETURN_BYTES: usize = 16_000;
/// Délai par défaut, et plafond dur : une commande ne peut pas figer l'agent.
const DEFAULT_TIMEOUT_SECS: u64 = 120;
const MAX_TIMEOUT_SECS: u64 = 3600;
/// Fréquence de scrutation du drapeau `/stop`.
const CANCEL_POLL: Duration = Duration::from_millis(150);

pub fn spec() -> ToolSpec {
    ToolSpec {
        name: "code_exec",
        description: "Run a bash command on the host and return stdout/stderr. \
                      Use for shell, compilers, tests, git, listing files, etc. \
                      The command is killed if it exceeds `timeout_secs` (default 120, max 3600).",
        parameters: json!({
            "type": "object",
            "properties": {
                "command": { "type": "string", "description": "The bash command to run." },
                "cwd": { "type": "string", "description": "Optional working directory." },
                "timeout_secs": {
                    "type": "integer",
                    "description": "Timeout in seconds (default 120, max 3600). The command is killed when exceeded."
                }
            },
            "required": ["command"]
        }),
    }
}

pub async fn run(args: &str) -> String {
    let v: serde_json::Value = match serde_json::from_str(args) {
        Ok(v) => v,
        Err(e) => return format!("Erreur: arguments JSON invalides ({e})"),
    };
    let command = v
        .get("command")
        .and_then(|x| x.as_str())
        .unwrap_or_default();
    if command.trim().is_empty() {
        return "Erreur: `command` manquant".into();
    }
    let timeout = v
        .get("timeout_secs")
        .and_then(|x| x.as_u64())
        .unwrap_or(DEFAULT_TIMEOUT_SECS)
        .clamp(1, MAX_TIMEOUT_SECS);
    let mut cmd = tokio::process::Command::from(crate::platform::shell_command(command));
    if let Some(dir) = v.get("cwd").and_then(|x| x.as_str()) {
        cmd.current_dir(dir);
    }
    // stdin fermé : une commande qui attend une saisie échoue au lieu de pendre.
    // `kill_on_drop` : si l'on abandonne (timeout ou /stop), l'enfant meurt.
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => return format!("Erreur d'exécution: {e}"),
    };
    let Some(mut stdout) = child.stdout.take() else {
        return "Erreur: stdout indisponible".into();
    };
    let Some(mut stderr) = child.stderr.take() else {
        return "Erreur: stderr indisponible".into();
    };

    let work = async move {
        let mut out = Vec::new();
        let mut err = Vec::new();
        let (out_trunc, err_trunc) = tokio::join!(
            drain(&mut stdout, &mut out, MAX_CAPTURE_BYTES),
            drain(&mut stderr, &mut err, MAX_CAPTURE_BYTES)
        );
        let status = child.wait().await.ok();
        (status, out, err, out_trunc, err_trunc)
    };

    let stop = async {
        while !crate::cancel::is_cancelled() {
            tokio::time::sleep(CANCEL_POLL).await;
        }
    };

    tokio::select! {
        res = tokio::time::timeout(Duration::from_secs(timeout), work) => match res {
            Ok((status, out, err, out_trunc, err_trunc)) => {
                render(status, out, err, out_trunc, err_trunc, timeout)
            }
            Err(_) => format!(
                "⏹ délai dépassé ({timeout} s) — commande tuée. \
                 Si c'est normal (build, test long), relance avec `timeout_secs` plus grand."
            ),
        },
        _ = stop => "⏹ interrompu (/stop) — commande tuée".to_string(),
    }
}

/// Lit un flux jusqu'à EOF en stockant au plus `cap` octets.
/// Retourne `true` si la sortie a dépassé le plafond.
async fn drain<R: AsyncRead + Unpin>(r: &mut R, buf: &mut Vec<u8>, cap: usize) -> bool {
    let mut tmp = [0u8; 8 * 1024];
    let mut truncated = false;
    loop {
        match r.read(&mut tmp).await {
            Ok(0) => break,
            Ok(n) => {
                let room = cap.saturating_sub(buf.len());
                if room == 0 {
                    truncated = true;
                } else if n <= room {
                    buf.extend_from_slice(&tmp[..n]);
                } else {
                    buf.extend_from_slice(&tmp[..room]);
                    truncated = true;
                }
            }
            Err(_) => break,
        }
    }
    truncated
}

fn render(
    status: Option<std::process::ExitStatus>,
    out: Vec<u8>,
    err: Vec<u8>,
    out_trunc: bool,
    err_trunc: bool,
    _timeout: u64,
) -> String {
    let mut text = String::new();
    if !out.is_empty() {
        text.push_str(&String::from_utf8_lossy(&out));
    }
    if !err.is_empty() {
        if !text.is_empty() {
            text.push('\n');
        }
        text.push_str("[stderr]\n");
        text.push_str(&String::from_utf8_lossy(&err));
    }
    if text.trim().is_empty() {
        text = format!(
            "(commande terminée, code {})",
            status.and_then(|s| s.code()).unwrap_or(-1)
        );
    }
    let mut notes: Vec<String> = Vec::new();
    if out_trunc || err_trunc {
        notes.push(format!(
            "sortie dépassée ({} Ko capturés par flux)",
            MAX_CAPTURE_BYTES / 1024
        ));
    }
    // Troncature sûre (frontière UTF-8) avant de polluer le contexte.
    if crate::util::truncate_utf8(&mut text, MAX_RETURN_BYTES) {
        notes.push(format!("affichage limité à {} Ko", MAX_RETURN_BYTES / 1000));
    }
    if !notes.is_empty() {
        text.push_str(&format!("\n… ({})", notes.join(" · ")));
    }
    if status.map(|s| s.code().unwrap_or(0) != 0).unwrap_or(false) && !text.contains("[stderr]") {
        text.push_str(&format!(
            "\n(code de sortie {})",
            status.and_then(|s| s.code()).unwrap_or(-1)
        ));
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_advertises_timeout() {
        let s = spec();
        assert!(s.parameters["properties"]["timeout_secs"].is_object());
    }

    #[test]
    fn render_marks_truncation() {
        let r = render(None, b"hello".to_vec(), Vec::new(), false, false, 120);
        assert!(r.contains("hello"));
    }

    #[test]
    fn render_reports_stderr() {
        let r = render(None, Vec::new(), b"boom".to_vec(), false, false, 120);
        assert!(r.contains("[stderr]\nboom"));
    }

    #[test]
    fn render_empty_output_has_code() {
        let r = render(None, Vec::new(), Vec::new(), false, false, 120);
        assert!(r.contains("commande terminée"));
    }

    /// Régression : une sortie accentuée de plus de 16 Ko déclenchait
    /// `panic!("assertion failed: self.is_char_boundary")`.
    #[test]
    fn render_multibyte_output_does_not_panic() {
        let mut out = String::from("a");
        out.push_str(&"é".repeat(20_000));
        let r = render(None, out.into_bytes(), Vec::new(), false, false, 120);
        assert!(r.contains("affichage limité"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn runs_and_captures() {
        let r = run(r#"{"command":"printf 'ok'"}"#).await;
        assert_eq!(r.trim(), "ok");
    }

    #[tokio::test]
    async fn missing_command_is_an_error() {
        assert!(run("{}").await.contains("manquant"));
    }

    #[tokio::test]
    async fn bad_json_is_an_error() {
        assert!(run("{oops").await.contains("JSON invalides"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn timeout_kills_the_command() {
        let started = std::time::Instant::now();
        let r = run(r#"{"command":"sleep 10","timeout_secs":1}"#).await;
        assert!(r.contains("délai dépassé"), "got: {r}");
        assert!(started.elapsed() < std::time::Duration::from_secs(5));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn stdin_is_closed_so_reads_fail_fast() {
        // `cat` sans stdin doit terminer immédiatement (pas de blocage).
        let r = run(r#"{"command":"cat","timeout_secs":5}"#).await;
        assert!(!r.contains("délai dépassé"), "got: {r}");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn big_output_is_capped() {
        let r =
            run(r#"{"command":"head -c 900000 /dev/zero | tr '\\0' 'x'","timeout_secs":20}"#).await;
        assert!(r.len() <= MAX_RETURN_BYTES + 200, "len {}", r.len());
        assert!(r.contains("dépassée") || r.contains("limité"));
    }
}
