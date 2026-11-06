use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};

pub const REPO: &str = "Richerrail/zer0gh0st";

const API: &str = "https://api.github.com";

fn user_agent() -> String {
    format!("zer0/{}", crate::update::current_version())
}

const CACHE_TTL: Duration = Duration::from_secs(600);

fn cache() -> &'static Mutex<Option<(u64, Instant)>> {
    static C: OnceLock<Mutex<Option<(u64, Instant)>>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(None))
}

pub fn cached_stars() -> Option<u64> {
    cache().lock().ok().and_then(|c| c.map(|(n, _)| n))
}

pub fn repo_url() -> String {
    format!("https://github.com/{REPO}")
}

pub fn stargazers_url() -> String {
    format!("{}/stargazers", repo_url())
}

pub fn token() -> Option<String> {
    for var in ["ZER0_GITHUB_TOKEN", "GITHUB_TOKEN", "GH_TOKEN"] {
        if let Ok(v) = std::env::var(var) {
            if !v.trim().is_empty() {
                return Some(v.trim().to_string());
            }
        }
    }
    if let Some(k) = crate::vault::get_unlocked("github") {
        if !k.trim().is_empty() {
            return Some(k);
        }
    }
    crate::config::load_global()
        .keys
        .get("github")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

const MAX_INVITATIONS: u32 = 3;

fn invitation_deja_vue() -> bool {
    crate::config::load_global().github_prompted
}

fn rappels_effectues() -> u32 {
    crate::config::load_global().github_prompt_count
}

pub fn note_rappel() {
    let mut g = crate::config::load_global();
    g.github_prompt_count = g.github_prompt_count.saturating_add(1);
    g.github_prompted = true;
    let _ = crate::config::save_global(&g);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Invitation {
    Rien,
    Claire,
    Rappel,
}

pub fn invitation_pour(deja_vue: bool, rappels: u32, connecte: bool) -> Invitation {
    if connecte {
        return Invitation::Rien;
    }
    if deja_vue {
        return Invitation::Rappel;
    }
    if rappels < MAX_INVITATIONS {
        Invitation::Claire
    } else {
        Invitation::Rien
    }
}

pub fn self_cmd() -> String {
    self_cmd_from(
        std::env::var("ZER0_CMD_NAME").ok(),
        std::env::current_exe().ok(),
    )
}

pub fn self_cmd_from(env_name: Option<String>, exe: Option<std::path::PathBuf>) -> String {
    if let Some(v) = env_name {
        let v = v.trim();
        if !v.is_empty() {
            return v.to_string();
        }
    }
    exe.map(|p| p.display().to_string())
        .unwrap_or_else(|| "zer0".to_string())
}

pub fn invitation_startup() -> Option<String> {
    let connecte = token().is_some();
    let cmd = self_cmd();
    match invitation_pour(invitation_deja_vue(), rappels_effectues(), connecte) {
        Invitation::Rien => None,
        Invitation::Claire => Some(
            format!(
                "🤖 Tu peux voter pour zer0gh0st (le 👍 en bas a droite = une star GitHub) :\n\n\
Lance `{cmd} vote login` : GitHub affiche un code, tu le valides dans ton\n\
navigateur, et le jeton est range chiffre chez toi ; ensuite les 👍 votent\n\
directement, sans plus jamais de manipulation."
            ),
        ),
        Invitation::Rappel => Some(format!(
            "🤖 Vote : `{cmd} vote login` connecte ton compte GitHub (une fois) — le 👍 devient alors une vraie star."
        )),
    }
}

pub fn parse_count(body: &str) -> Option<u64> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    v.get("count").and_then(|c| c.as_u64())
}

pub const DEFAULT_CLIENT_ID: &str = "Ov23lioyZzAN5wtt0j0o";

pub fn client_id() -> Option<String> {
    client_id_from(
        std::env::var("ZER0_GITHUB_CLIENT_ID").ok(),
        crate::config::load_global().github_client_id,
    )
}

pub fn client_id_from(env: Option<String>, cfg: Option<String>) -> Option<String> {
    if let Some(v) = env {
        let v = v.trim();
        if !v.is_empty() {
            return Some(v.to_string());
        }
    }
    cfg.map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| Some(DEFAULT_CLIENT_ID.to_string()))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceCode {
    pub user_code: String,
    pub verification_uri: String,
    pub device_code: String,
    pub expires_in: u64,
    pub interval: u64,
}

pub async fn request_device_code(client_id: &str) -> Result<DeviceCode> {
    let r = crate::http_client::HTTP_CLIENT
        .post("https://github.com/login/device/code")
        .header("Accept", "application/json")
        .header("User-Agent", user_agent())
        .form(&[("client_id", client_id), ("scope", "public_repo")])
        .send()
        .await?;
    let status = r.status();
    let body = r.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(anyhow!(
            "GitHub a refusé la demande de code (HTTP {status})"
        ));
    }
    let v: serde_json::Value =
        serde_json::from_str(&body).map_err(|_| anyhow!("réponse GitHub illisible"))?;
    if let Some(err) = v.get("error").and_then(|e| e.as_str()) {
        let hint = if err == "not_enabled" {
            " — active « Enable Device Flow » dans les réglages de l'application OAuth"
        } else {
            ""
        };
        return Err(anyhow!("GitHub : {err}{hint}"));
    }
    let get = |k: &str| {
        v.get(k)
            .and_then(|x| x.as_str())
            .unwrap_or_default()
            .to_string()
    };
    let dc = DeviceCode {
        user_code: get("user_code"),
        verification_uri: get("verification_uri"),
        device_code: get("device_code"),
        expires_in: v.get("expires_in").and_then(|x| x.as_u64()).unwrap_or(900),
        interval: v.get("interval").and_then(|x| x.as_u64()).unwrap_or(5),
    };
    if dc.user_code.is_empty() || dc.device_code.is_empty() {
        return Err(anyhow!("réponse GitHub incomplète (pas de user_code)"));
    }
    Ok(dc)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PollOutcome {
    Token {
        access: String,
        refresh: Option<String>,
    },
    Pending,
    Expired,
}

const KEY_ACCESS: &str = "github";
const KEY_REFRESH: &str = "github_refresh";

pub fn refresh_token() -> Option<String> {
    crate::vault::get_unlocked(KEY_REFRESH).filter(|s| !s.trim().is_empty())
}

pub async fn refresh_access_token() -> Option<String> {
    let rt = refresh_token()?;
    let cid = client_id()?;
    let r = crate::http_client::HTTP_CLIENT
        .post("https://github.com/login/oauth/access_token")
        .header("Accept", "application/json")
        .header("User-Agent", user_agent())
        .form(&[
            ("client_id", cid.as_str()),
            ("refresh_token", rt.as_str()),
            ("grant_type", "refresh_token"),
        ])
        .send()
        .await
        .ok()?;
    let body = r.text().await.ok()?;
    let v: serde_json::Value = serde_json::from_str(&body).ok()?;
    let access = v.get("access_token").and_then(|t| t.as_str())?.to_string();
    if access.is_empty() {
        return None;
    }
    if let Some(new_rt) = v.get("refresh_token").and_then(|t| t.as_str()) {
        let _ = crate::vault::put(KEY_REFRESH, new_rt);
    }
    let _ = crate::vault::put(KEY_ACCESS, &access);
    Some(access)
}

pub async fn poll_device_token(client_id: &str, device_code: &str) -> Result<PollOutcome> {
    let r = crate::http_client::HTTP_CLIENT
        .post("https://github.com/login/oauth/access_token")
        .header("Accept", "application/json")
        .header("User-Agent", user_agent())
        .form(&[
            ("client_id", client_id),
            ("device_code", device_code),
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
        ])
        .send()
        .await?;
    let body = r.text().await.unwrap_or_default();
    let v: serde_json::Value =
        serde_json::from_str(&body).map_err(|_| anyhow!("réponse GitHub illisible"))?;
    if let Some(tok) = v.get("access_token").and_then(|t| t.as_str()) {
        if !tok.is_empty() {
            let refresh = v
                .get("refresh_token")
                .and_then(|t| t.as_str())
                .filter(|s| !s.is_empty())
                .map(str::to_string);
            return Ok(PollOutcome::Token {
                access: tok.to_string(),
                refresh,
            });
        }
    }
    match v.get("error").and_then(|e| e.as_str()) {
        Some("authorization_pending") | Some("slow_down") => Ok(PollOutcome::Pending),
        Some("expired_token") | Some("access_denied") => Ok(PollOutcome::Expired),
        Some(e) => Err(anyhow!("GitHub : {e}")),
        None => Err(anyhow!("réponse GitHub illisible")),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Credentials {
    pub access: String,
    pub refresh: Option<String>,
}

pub async fn device_login<F>(on_prompt: F) -> Result<Credentials>
where
    F: Fn(&str, &str),
{
    let cid = client_id().ok_or_else(|| {
        anyhow!(
            "pas de client_id OAuth : définis ZER0_GITHUB_CLIENT_ID (ou github_client_id dans la config)"
        )
    })?;
    let dc = request_device_code(&cid).await?;
    on_prompt(&dc.user_code, &dc.verification_uri);

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(dc.expires_in);
    let interval = dc.interval.max(1);
    while std::time::Instant::now() < deadline {
        tokio::time::sleep(std::time::Duration::from_secs(interval)).await;
        match poll_device_token(&cid, &dc.device_code).await? {
            PollOutcome::Token { access, refresh } => return Ok(Credentials { access, refresh }),
            PollOutcome::Pending => {}
            PollOutcome::Expired => return Err(anyhow!("code expiré — relance la connexion")),
        }
    }
    Err(anyhow!("délai dépassé — code expiré"))
}

pub async fn stars(force: bool) -> Option<u64> {
    if !force {
        if let Ok(c) = cache().lock() {
            if let Some((n, at)) = *c {
                if at.elapsed() < CACHE_TTL {
                    return Some(n);
                }
            }
        }
    }
    let Some(n) = fetch_stars().await else {
        return cached_stars();
    };
    if let Ok(mut c) = cache().lock() {
        *c = Some((n, Instant::now()));
    }
    Some(n)
}

async fn fetch_stars() -> Option<u64> {
    let r = crate::http_client::HTTP_CLIENT
        .get(format!("{API}/repos/{REPO}/stargazers/count"))
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", user_agent())
        .header("Cache-Control", "no-cache")
        .send()
        .await
        .ok()?;
    if !r.status().is_success() {
        return None;
    }
    parse_count(&r.text().await.ok()?)
}

pub async fn is_starred(token: &str) -> Result<bool> {
    let r = crate::http_client::HTTP_CLIENT
        .get(format!("{API}/user/starred/{REPO}"))
        .bearer_auth(token)
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", user_agent())
        .send()
        .await?;
    match r.status().as_u16() {
        204 => Ok(true),
        404 => Ok(false),
        s => Err(anyhow!("GitHub a répondu HTTP {s}")),
    }
}

pub async fn star(token: &str) -> Result<()> {
    let r = crate::http_client::HTTP_CLIENT
        .put(format!("{API}/user/starred/{REPO}"))
        .bearer_auth(token)
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", user_agent())
        .body(Vec::new())
        .send()
        .await?;
    let s = r.status();
    if s.is_success() {
        Ok(())
    } else {
        Err(anyhow!("GitHub a refusé la star (HTTP {s})"))
    }
}

pub async fn unstar(token: &str) -> Result<()> {
    let r = crate::http_client::HTTP_CLIENT
        .delete(format!("{API}/user/starred/{REPO}"))
        .bearer_auth(token)
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", user_agent())
        .send()
        .await?;
    let s = r.status();
    if s.is_success() {
        Ok(())
    } else {
        Err(anyhow!("GitHub a refusé le retrait de la star (HTTP {s})"))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Up,
    Down,
}

async fn vote_up(deja_starre: bool) -> String {
    if deja_starre {
        return "⭐ Zer0 est déjà starré par ton compte GitHub.".to_string();
    }
    let Some(tok) = token() else {
        return "Aucun jeton GitHub.".to_string();
    };
    match star(&tok).await {
        Ok(()) => match stars(true).await {
            Some(n) => format!(
                "⭐ Star posée sur GitHub ! ({n} stars — GitHub propage le compteur en ~1 min)"
            ),
            None => "⭐ Star posée sur GitHub !".to_string(),
        },
        Err(e) => format!("⚠️ Star impossible : {e}"),
    }
}

async fn vote_down() -> String {
    let Some(tok) = token() else {
        return "Aucun jeton GitHub.".to_string();
    };
    match unstar(&tok).await {
        Ok(()) => match stars(true).await {
            Some(n) => {
                format!("🖐️ Star retirée. ({n} stars — GitHub propage le compteur en ~1 min)")
            }
            None => "🖐️ Star retirée.".to_string(),
        },
        Err(e) => format!("⚠️ Retrait impossible : {e}"),
    }
}

pub async fn act(action: Action) -> String {
    let Some(tok) = token() else {
        let url = repo_url();
        let opened = crate::platform::open_url(&url).is_ok();
        crate::platform::restore_mouse_capture();
        return match action {
            Action::Up => {
                if opened {
                    format!("Aucun jeton GitHub : {url} ouvert — clique sur « ★ Star » pour voter.")
                } else {
                    format!("Aucun jeton GitHub et ouverture du navigateur impossible — pose la star sur {url}")
                }
            }
            Action::Down => {
                if opened {
                    format!("Aucun jeton GitHub : {url} ouvert — retire l'étoile (reclique sur « ★ Star »).")
                } else {
                    format!("Aucun jeton GitHub et ouverture du navigateur impossible — retire la star sur {url}")
                }
            }
        };
    };

    match action {
        Action::Up => match is_starred(&tok).await {
            Ok(ok) => vote_up(ok).await,
            Err(e) if is_unauthorized(&e) => match refresh_access_token().await {
                Some(new) => {
                    let ok = is_starred(&new).await.unwrap_or(false);
                    vote_up(ok).await
                }
                None => format!(
                    "⚠️ {e} — jeton expiré, reconnecte-toi : `{} vote login`",
                    self_cmd()
                ),
            },
            Err(e) => format!("⚠️ État GitHub inconnu : {e}"),
        },
        Action::Down => match unstar(&tok).await {
            Ok(()) => vote_down().await,
            Err(e) if is_unauthorized(&e) => match refresh_access_token().await {
                Some(new) => {
                    let _ = unstar(&new).await;
                    vote_down().await
                }
                None => format!(
                    "⚠️ {e} — jeton expiré, reconnecte-toi : `{} vote login`",
                    self_cmd()
                ),
            },
            Err(e) => format!("⚠️ Retrait impossible : {e}"),
        },
    }
}

fn is_unauthorized(e: &anyhow::Error) -> bool {
    e.to_string().contains("401")
}

pub async fn state() -> (Option<u64>, Option<bool>) {
    let stars = stars(false).await;
    let starred = match token() {
        Some(t) => is_starred(&t).await.ok(),
        None => None,
    };
    (stars, starred)
}

pub async fn status() -> String {
    let mut s = String::new();
    match stars(false).await {
        Some(n) => s.push_str(&format!("⭐ {n} star(s) sur GitHub\n")),
        None => s.push_str("⭐ compteur indisponible (API GitHub injoignable)\n"),
    }
    s.push_str(&format!("🔗 {}\n", repo_url()));
    s.push_str(&format!("   stargazers : {}\n", stargazers_url()));
    match token() {
        Some(_) => match is_starred(&token().unwrap_or_default()).await {
            Ok(true) => s.push_str("ton compte a starré le dépôt\n"),
            Ok(false) => s.push_str("ton compte n'a pas starré le dépôt\n"),
            Err(e) => s.push_str(&format!("état du compte inconnu : {e}\n")),
        },
        None => s.push_str(
            "aucun jeton GitHub (ZER0_GITHUB_TOKEN / GITHUB_TOKEN / GH_TOKEN, ou clé « github »)\n\
             → le vote se fait d'un clic sur « ★ Star » dans le navigateur\n",
        ),
    }
    s
}

use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

pub const WIDTH: u16 = 11;

pub fn draw(
    f: &mut Frame,
    area: Rect,
    stars: Option<u64>,
    starred: Option<bool>,
) -> (Option<Rect>, Option<Rect>) {
    if area.width < WIDTH {
        return (None, None);
    }
    let count = stars
        .map(|n| n.to_string())
        .unwrap_or_else(|| "?".to_string());
    let up_style = if starred == Some(true) {
        Style::default()
            .fg(Color::LightGreen)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Gray)
    };
    let s_up = Span::styled("👍", up_style);
    let s_cnt = Span::styled(format!(" {count}"), Style::default().fg(Color::DarkGray));
    let s_dn = Span::styled("👎", Style::default().fg(Color::DarkGray));
    let w_up = s_up.width() as u16;
    let w_cnt = s_cnt.width() as u16;
    let w_dn = s_dn.width() as u16;
    let total = w_up + w_cnt + w_dn;
    if total > area.width {
        return (None, None);
    }
    let line = Line::from(vec![s_up, s_cnt, s_dn]);
    f.render_widget(Paragraph::new(line).alignment(Alignment::Right), area);

    let x0 = area.x + area.width - total;
    let up = Rect {
        x: x0.saturating_sub(1),
        y: area.y,
        width: w_up + 1,
        height: 1,
    };
    let down = Rect {
        x: (x0 + w_up + w_cnt).saturating_sub(1),
        y: area.y,
        width: w_dn + 1,
        height: 1,
    };
    (Some(up), Some(down))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_point_to_the_repo() {
        assert_eq!(repo_url(), "https://github.com/Richerrail/zer0gh0st");
        assert_eq!(
            stargazers_url(),
            "https://github.com/Richerrail/zer0gh0st/stargazers"
        );
    }

    #[test]
    fn self_cmd_prefers_the_launcher_name() {
        assert_eq!(self_cmd_from(Some("zer0gh0st".into()), None), "zer0gh0st");
        assert_eq!(
            self_cmd_from(Some("   ".into()), Some("/usr/local/bin/zer0".into())),
            "/usr/local/bin/zer0"
        );
        assert_eq!(self_cmd_from(None, None), "zer0");
    }

    #[test]
    fn parse_count_reads_the_public_endpoint() {
        assert_eq!(parse_count(r#"{"count": 42}"#), Some(42));
        assert_eq!(parse_count(r#"{"count": 0}"#), Some(0));
        assert_eq!(parse_count("not json"), None);
        assert_eq!(parse_count(r#"{"count": "x"}"#), None);
        assert_eq!(parse_count(""), None);
    }

    #[test]
    fn default_client_id_is_embedded() {
        assert!(!DEFAULT_CLIENT_ID.is_empty());
        assert_eq!(
            client_id_from(None, None).as_deref(),
            Some(DEFAULT_CLIENT_ID)
        );
        assert_eq!(
            client_id_from(Some("ENV_ID".into()), Some("CFG_ID".into())).as_deref(),
            Some("ENV_ID")
        );
        assert_eq!(
            client_id_from(None, Some("CFG_ID".into())).as_deref(),
            Some("CFG_ID")
        );
        assert_eq!(
            client_id_from(Some("  ".into()), Some("CFG_ID".into())).as_deref(),
            Some("CFG_ID")
        );
    }

    #[test]
    fn refresh_is_opt_in() {
        assert!(refresh_token().is_none() || !refresh_token().unwrap().is_empty());
        assert_ne!(KEY_ACCESS, KEY_REFRESH);
    }

    #[test]
    fn action_names_are_distinct() {
        assert_ne!(Action::Up, Action::Down);
    }

    #[test]
    fn render_places_thumbs_at_bottom_right() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;

        let mut term = Terminal::new(TestBackend::new(80, 24)).unwrap();
        let area = Rect {
            x: 0,
            y: 23,
            width: 80,
            height: 1,
        };
        let mut up = None;
        let mut down = None;
        term.draw(|f| {
            let (u, d) = draw(f, area, Some(42), Some(true));
            up = u;
            down = d;
        })
        .unwrap();

        let up = up.expect("pouce haut absent");
        let down = down.expect("pouce bas absent");
        assert!(up.x > 60, "pouce haut trop à gauche : {up:?}");
        assert!(down.x > up.x, "les pouces ne sont pas ordonnés");
        assert!(down.x + down.width <= 80, "déborde à droite : {down:?}");

        let buf = term.backend().buffer();
        let line: String = (0..80).map(|x| buf[(x, 23)].symbol()).collect();
        assert!(line.contains("42"), "compteur absent : {line}");
    }
}

#[cfg(test)]
mod tests_prompt {
    use super::*;

    #[test]
    fn invitation_decision() {
        assert_eq!(invitation_pour(false, 0, true), Invitation::Rien);
        assert_eq!(invitation_pour(true, 5, true), Invitation::Rien);

        assert_eq!(invitation_pour(false, 0, false), Invitation::Claire);
        assert_eq!(invitation_pour(false, 1, false), Invitation::Claire);

        assert_eq!(invitation_pour(true, 1, false), Invitation::Rappel);

        assert_eq!(
            invitation_pour(false, MAX_INVITATIONS, false),
            Invitation::Rien
        );
    }
}
