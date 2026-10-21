//! Vote de l'utilisateur sur zer0gh0st — 👍 = une star GitHub, 👎 = on la retire.
//!
//! ## Ce que « upvote » veut dire
//!
//! GitHub n'a pas de vote : il a des **stars**. Un vote positif est donc une
//! star sur `Richerrail/zer0gh0st`, et un vote négatif est son retrait
//! (`DELETE /user/starred/{owner}/{repo}`).
//!
//! Deux façons de la poser, dans cet ordre :
//!
//! 1. **Avec un jeton GitHub** (`ZER0_GITHUB_TOKEN`, `GITHUB_TOKEN`, `GH_TOKEN`,
//!    ou la clé `github` dans la config / le coffre) : l'API est appelée
//!    directement — `PUT /user/starred/{owner}/{repo}`. Jeton *fine-grained* :
//!    permission « Starring » en écriture + « Metadata » en lecture ; jeton
//!    *classique* : scope `public_repo`. Une star posée, une star retirée.
//! 2. **Sans jeton** : on ouvre la page du dépôt dans le navigateur, là où se
//!    trouve le bouton « ★ Star » — l'étoile est alors posée à la main.
//!    Aucun mensonge : le message dit exactement ce qui s'est passé.
//!
//! Le compteur affiché vient de l'API **publique**
//! (`GET /repos/{owner}/{repo}/stargazers/count`), mis en cache 10 minutes :
//! l'API GitHub sans jeton est plafonnée à 60 requêtes/heure.

use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};

/// Le dépôt votable (même source que les releases).
pub const REPO: &str = "Richerrail/zer0gh0st";

/// Base de l'API GitHub.
const API: &str = "https://api.github.com";

/// En-tête `User-Agent` : **obligatoire** sur l'API GitHub (sans lui : 403
/// « Request forbidden by administrative rules »).
fn user_agent() -> String {
    format!("zer0/{}", crate::update::current_version())
}

/// Durée de vie du compteur de stars en cache (API publique : 60 req/h).
const CACHE_TTL: Duration = Duration::from_secs(600);

/// Compteur observé : (nombre, moment de l'observation).
fn cache() -> &'static Mutex<Option<(u64, Instant)>> {
    static C: OnceLock<Mutex<Option<(u64, Instant)>>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(None))
}

/// Le dernier compteur connu, sans réseau.
pub fn cached_stars() -> Option<u64> {
    cache().lock().ok().and_then(|c| c.map(|(n, _)| n))
}

/// Page du dépôt — c'est là qu'est le bouton « ★ Star ».
pub fn repo_url() -> String {
    format!("https://github.com/{REPO}")
}

/// Page listant les stargazers.
pub fn stargazers_url() -> String {
    format!("{}/stargazers", repo_url())
}

/// Jeton GitHub optionnel : variable d'env → coffre → config.
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

/// Combien de fois l'invitation GitHub a deja ete proposee a cet utilisateur.
///
/// Au-dela, on ne propose plus : on se contente du rappel discret.
const MAX_INVITATIONS: u32 = 3;

/// L'utilisateur a-t-il deja eu droit a l'invitation de connexion GitHub ?
fn invitation_deja_vue() -> bool {
    crate::config::load_global().github_prompted
}

/// Nombre de rappels deja effectues (pour la config).
fn rappels_effectues() -> u32 {
    crate::config::load_global().github_prompt_count
}

/// Incremente le compteur de rappels et marque l'invitation comme vue.
///
/// Ecrit dans la config : l'utilisateur n'est donc pas relance a chaque
/// demarrage.
pub fn note_rappel() {
    let mut g = crate::config::load_global();
    g.github_prompt_count = g.github_prompt_count.saturating_add(1);
    g.github_prompted = true;
    let _ = crate::config::save_global(&g);
}

/// Ce qu'on dit a l'utilisateur au demarrage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Invitation {
    /// Rien a dire : deja connecte, ou rappels epuises.
    Rien,
    /// Premiere fois : invitation claire, avec la marche a suivre.
    Claire,
    /// Deja propose : rappel discret, on n'importune plus.
    Rappel,
}

/// Decide quoi dire au demarrage (logique pure, testable).
///
/// - deja connecte -> `Rien` (le vote est deja automatique) ;
/// - jamais propose et moins de `MAX_INVITATIONS` rappels -> `Claire` ;
/// - deja propose -> `Rappel` (discret) ;
/// - rappels epuises -> `Rien` (on laisse l'utilisateur tranquille).
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

/// Message a proposer au demarrage, selon la situation.
///
/// - **jamais connecte** et invitation disponible -> invitation claire
///   (une fois, puis elle devient un rappel discret) ;
/// - **deja connecte** -> rien (le vote est deja automatique) ;
/// - **rappels epuises** -> plus d'invitation, seulement un rappel discret.
///
/// Commande à taper pour réutiliser **ce** binaire.
///
/// Le nom « zer0 » codé en dur pouvait désigner un autre programme (chez
/// moi, `zer0` est une ancienne appli Python sans commande `vote`) : on
/// affiche donc le chemin réel de l'exécutant en cours — toujours
/// copiable et toujours juste.
pub fn self_cmd() -> String {
    std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "zer0".to_string())
}

/// `None` = rien a dire (l'utilisateur est deja connecte).
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

/// Extrait le compteur d'un corps JSON `{"count": 42}`.
pub fn parse_count(body: &str) -> Option<u64> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    v.get("count").and_then(|c| c.as_u64())
}

/// `client_id` de l'application OAuth *Zer0* (device flow du vote).
///
/// C'est un **identifiant public** : GitHub l'affiche publiquement sur la page
/// de l'application, et le device flow n'a besoin d'aucun `client_secret`.
/// Il permet à tout utilisateur de connecter son compte en un clic
/// (`{cmd} vote login`) sans rien configurer.
pub const DEFAULT_CLIENT_ID: &str = "Ov23lioyZzAN5wtt0j0o";

/// Client ID de l'application OAuth du projet (device flow).
///
/// L'application OAuth est créée par le mainteneur du dépôt ; son `client_id`
/// est public par nature (c'est un identifiant, pas un secret). Il peut être
/// fourni par `ZER0_GITHUB_CLIENT_ID` ou stocké dans la config ; à défaut on
/// utilise celui de l'application *Zer0* (constante ci-dessus).
pub fn client_id() -> Option<String> {
    if let Ok(v) = std::env::var("ZER0_GITHUB_CLIENT_ID") {
        if !v.trim().is_empty() {
            return Some(v.trim().to_string());
        }
    }
    crate::config::load_global()
        .github_client_id
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| Some(DEFAULT_CLIENT_ID.to_string()))
}

/// Réponse du device flow : le code à saisir et l'URL de vérification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceCode {
    pub user_code: String,
    pub verification_uri: String,
    pub device_code: String,
    pub expires_in: u64,
    pub interval: u64,
}

/// Demande un code de device flow à GitHub.
///
/// `scope = "public_repo"` : c'est le scope qui autorise à **starrer** un
/// dépôt public (documenté par GitHub). C'est le minimum qui permet le vote.
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
        // `not_enabled` = la case « Enable Device Flow » n'est pas cochée sur
        // la page de l'application OAuth.
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

/// Résultat d'un échange de code contre un jeton.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PollOutcome {
    /// Jeton obtenu.
    Token(String),
    /// L'utilisateur n'a pas encore validé le code.
    Pending,
    /// Le code a expiré.
    Expired,
}

/// Échange le `device_code` contre un jeton (interroge le service de jetons).
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
            return Ok(PollOutcome::Token(tok.to_string()));
        }
    }
    match v.get("error").and_then(|e| e.as_str()) {
        Some("authorization_pending") | Some("slow_down") => Ok(PollOutcome::Pending),
        Some("expired_token") | Some("access_denied") => Ok(PollOutcome::Expired),
        Some(e) => Err(anyhow!("GitHub : {e}")),
        None => Err(anyhow!("réponse GitHub illisible")),
    }
}

/// Connexion complète (device flow) : affiche le code, attend la validation,
/// puis renvoie le jeton. `on_prompt` reçoit le code et l'URL à montrer.
pub async fn device_login<F>(on_prompt: F) -> Result<String>
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
            PollOutcome::Token(t) => return Ok(t),
            PollOutcome::Pending => {}
            PollOutcome::Expired => return Err(anyhow!("code expiré — relance la connexion")),
        }
    }
    Err(anyhow!("délai dépassé — code expiré"))
}

/// Compteur de stars observé (API publique, cache 10 min).
///
/// `force` ignore le cache (après un vote). En cas d'échec réseau, la
/// dernière valeur connue est renvoyée (elle peut être datée : le message
/// indique alors qu'elle est ancienne).
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
        // Le CDN de GitHub met ce endpoint en cache 60 s : sans cette demande,
        // le compteur affiché peut être périmé après un vote.
        .header("Cache-Control", "no-cache")
        .send()
        .await
        .ok()?;
    if !r.status().is_success() {
        return None;
    }
    parse_count(&r.text().await.ok()?)
}

/// Le compte authentifié a-t-il starré le dépôt ? (jeton requis)
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

/// Pose une star (`PUT /user/starred/…`).
pub async fn star(token: &str) -> Result<()> {
    let r = crate::http_client::HTTP_CLIENT
        .put(format!("{API}/user/starred/{REPO}"))
        .bearer_auth(token)
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", user_agent())
        // L'API exige Content-Length: 0 sur un PUT sans corps.
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

/// Retire une star (`DELETE /user/starred/…`).
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

/// Le vote : `Up` = star, `Down` = retrait de star.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Up,
    Down,
}

/// Exécute le vote et renvoie le message à montrer à l'utilisateur.
pub async fn act(action: Action) -> String {
    match token() {
        Some(tok) => match action {
            Action::Up => match is_starred(&tok).await {
                Ok(true) => "⭐ Zer0 est déjà starré par ton compte GitHub.".to_string(),
                Ok(false) => match star(&tok).await {
                    Ok(()) => {
                        let n = stars(true).await;
                        match n {
                            Some(n) => format!(
                                "⭐ Star posée sur GitHub ! ({n} stars — GitHub propage le compteur en ~1 min)"
                            ),
                            None => "⭐ Star posée sur GitHub !".to_string(),
                        }
                    }
                    Err(e) => format!("⚠️ Star impossible : {e}"),
                },
                Err(e) => format!("⚠️ État GitHub inconnu : {e}"),
            },
            Action::Down => match unstar(&tok).await {
                Ok(()) => {
                    let n = stars(true).await;
                    match n {
                        Some(n) => format!(
                            "🖐️ Star retirée. ({n} stars — GitHub propage le compteur en ~1 min)"
                        ),
                        None => "🖐️ Star retirée.".to_string(),
                    }
                }
                Err(e) => format!("⚠️ Retrait impossible : {e}"),
            },
        },
        None => {
            // Pas de jeton : on ouvre la page où se trouve le bouton ★ Star.
            let url = repo_url();
            let opened = crate::platform::open_url(&url).is_ok();
            // Le programme lancé a pu rendre le mode « souris » à la console :
            // on le réactive, sinon le TUI ne recevrait plus aucun clic.
            crate::platform::restore_mouse_capture();
            match action {
                Action::Up => {
                    if opened {
                        format!(
                            "Aucun jeton GitHub : {url} ouvert — clique sur « ★ Star » pour voter."
                        )
                    } else {
                        format!(
                            "Aucun jeton GitHub et ouverture du navigateur impossible — pose la star sur {url}"
                        )
                    }
                }
                Action::Down => {
                    if opened {
                        format!(
                            "Aucun jeton GitHub : {url} ouvert — retire l'étoile (reclique sur « ★ Star »)."
                        )
                    } else {
                        format!(
                            "Aucun jeton GitHub et ouverture du navigateur impossible — retire la star sur {url}"
                        )
                    }
                }
            }
        }
    }
}

/// État du vote : compteur de stars observé + star du compte (si jeton).
pub async fn state() -> (Option<u64>, Option<bool>) {
    let stars = stars(false).await;
    let starred = match token() {
        Some(t) => is_starred(&t).await.ok(),
        None => None,
    };
    (stars, starred)
}

/// État des lieux : compteur, star de ton compte, jeton, URL du vote.
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

// ------------------------------------------------------------------ rendu TUI

use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

/// Largeur du bloc de vote (bas à droite de l'écran principal).
pub const WIDTH: u16 = 11;

/// Dessine le bloc 👍 / 👎 et renvoie les zones cliquables.
///
/// `stars` : compteur observé (`None` = inconnu → on affiche « ? »).
/// `starred` : ton compte a-t-il starré ? (`None` = inconnu, pas de jeton).
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
    // Les largeurs sont calculées à partir des spans : un emoji peut occuper
    // 1 ou 2 colonnes selon le terminal — la zone cliquable suit.
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

    // Zones cliquables : le pouce haut et le pouce bas.
    // On élargit la zone d'un cheveu de chaque côté : un emoji peut occuper
    // 1 ou 2 cellules selon le terminal, et un clic « à côté » doit compter.
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
    fn parse_count_reads_the_public_endpoint() {
        assert_eq!(parse_count(r#"{"count": 42}"#), Some(42));
        assert_eq!(parse_count(r#"{"count": 0}"#), Some(0));
        // Rien n'est inventé : un corps inattendu ne donne pas de chiffre.
        assert_eq!(parse_count("not json"), None);
        assert_eq!(parse_count(r#"{"count": "x"}"#), None);
        assert_eq!(parse_count(""), None);
    }

    #[test]
    fn default_client_id_is_embedded() {
        // Sans client_id, `{cmd} vote login` est impossible : le device flow
        // est le seul moyen d'obtenir un jeton sans configuration.
        assert!(!DEFAULT_CLIENT_ID.is_empty());
        assert_eq!(client_id().as_deref(), Some(DEFAULT_CLIENT_ID));
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

        // Les deux pouces sont cliquables, ordonnés, et à droite de l'écran.
        let up = up.expect("pouce haut absent");
        let down = down.expect("pouce bas absent");
        assert!(up.x > 60, "pouce haut trop à gauche : {up:?}");
        assert!(down.x > up.x, "les pouces ne sont pas ordonnés");
        assert!(down.x + down.width <= 80, "déborde à droite : {down:?}");

        // Le compteur observé est affiché.
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
        // Deja connecte : on ne dit rien.
        assert_eq!(invitation_pour(false, 0, true), Invitation::Rien);
        assert_eq!(invitation_pour(true, 5, true), Invitation::Rien);

        // Jamais propose : invitation claire.
        assert_eq!(invitation_pour(false, 0, false), Invitation::Claire);
        assert_eq!(invitation_pour(false, 1, false), Invitation::Claire);

        // Deja propose : rappel discret (une seule ligne).
        assert_eq!(invitation_pour(true, 1, false), Invitation::Rappel);

        // Rappels epuises : silence, on n'importune plus.
        assert_eq!(
            invitation_pour(false, MAX_INVITATIONS, false),
            Invitation::Rien
        );
    }
}
