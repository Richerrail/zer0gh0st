//! Mod « 0cg » : watchlist crypto dans le panneau latéral (comme `t0p`).
//!
//! Choix de conception :
//! - **API publique sans clé** (`api.coingecko.com/api/v3`) : aucun secret à
//!   stocker, et ça marche dès l'installation. Les endpoints payants
//!   (historique long, WebSocket) ne sont pas utilisés.
//! - **Une seule requête** pour toute la liste (`/coins/markets` renvoie prix,
//!   variation 24 h **et** `sparkline_in_7d`), rafraîchie toutes les 15 s :
//!   4 req/min, très en dessous des limites publiques.
//! - **La watchlist appartient à l'utilisateur** : ordre conservé, ni tri par
//!   capitalisation ni liste par défaut imposée. `/0cg add|rm|list`.
//! - **Honnêteté** : le pied affiche l'âge des données ; un échec réseau est
//!   affiché comme tel (`prix indisponibles depuis N s`), jamais masqué par des
//!   valeurs figées présentées comme fraîches.

use std::time::{Duration, Instant};

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::symbols::Marker;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Axis, Chart, Dataset, GraphType};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::theme;

/// Intervalle de rafraîchissement par défaut (`ZER0_CG_SECS` pour changer).
pub const DEFAULT_SECS: u64 = 15;

/// Agent envoyé avec la requête.
///
/// **Requis** : sans `User-Agent`, l'API CoinGecko (derrière Cloudflare)
/// répond **403 Forbidden** à un client rustls — curl passe, reqwest non. Le
/// symptôme était un panneau vide avec « HTTP 403 » sans explication.
pub const USER_AGENT: &str = concat!("zer0gh0st/", env!("CARGO_PKG_VERSION"));

/// Au-delà de cette fraîcheur, les données sont dites **périmées**.
const STALE_AFTER: Duration = Duration::from_secs(60);

/// Une ligne de la watchlist.
#[derive(Debug, Clone, PartialEq)]
pub struct Quote {
    pub id: String,
    pub symbol: String,
    /// Prix en USD (0 si l'API n'en a pas donné).
    pub price: f64,
    /// Variation 24 h en pourcent (`None` si absente).
    pub change_24h: Option<f64>,
    /// Série 7 jours (168 points horaires), telle que fournie par l'API.
    pub spark: Vec<f64>,
}

impl Quote {
    /// Prix lisible, avec **espace insécable** comme séparateur de milliers
    /// (`64 947 $`, `0,5200 $`). La virgule décimale est française.
    pub fn price_label(&self) -> String {
        let s = if self.price >= 1000.0 {
            format!("{:.0}", self.price)
        } else if self.price >= 1.0 {
            format!("{:.2}", self.price)
        } else {
            format!("{:.4}", self.price)
        };
        let (int_part, dec_part) = match s.split_once('.') {
            Some((a, b)) => (a.to_string(), Some(b.to_string())),
            None => (s.clone(), None),
        };
        // Groupement des milliers par tranches de 3, depuis la droite.
        let mut grouped = String::new();
        let n = int_part.len();
        for (i, ch) in int_part.chars().enumerate() {
            if i > 0 && (n - i) % 3 == 0 {
                grouped.push(' '); // séparateur de milliers (largeur 1)
            }
            grouped.push(ch);
        }
        match dec_part {
            Some(d) => format!("{grouped},{d} $"),
            None => format!("{grouped} $"),
        }
    }

    /// Variation lisible : `+2,4 %` / `−1,1 %` (signe moins typographique).
    pub fn change_label(&self) -> String {
        match self.change_24h {
            None => "—".to_string(),
            Some(v) => {
                let sign = if v < 0.0 { "−" } else { "+" };
                format!("{sign}{:.1} %", v.abs()).replace('.', ",")
            }
        }
    }

    pub fn is_up(&self) -> bool {
        self.change_24h.map(|v| v >= 0.0).unwrap_or(true)
    }
}

/// État du panneau.
pub struct Cg {
    /// Ids CoinGecko à suivre, **dans l'ordre choisi** par l'utilisateur.
    pub coins: Vec<String>,
    quotes: Vec<Quote>,
    /// Dernière mise à jour réussie (None = jamais).
    last_ok: Option<Instant>,
    last_attempt: Option<Instant>,
    /// Dernière erreur (affichée telle quelle, effacée au succès suivant).
    error: Option<String>,
    /// Une requête est en vol.
    pub loading: bool,
    secs: u64,
}

impl Cg {
    pub fn new(coins: Vec<String>) -> Self {
        Self {
            coins,
            quotes: Vec::new(),
            last_ok: None,
            last_attempt: None,
            error: None,
            loading: false,
            secs: crate::util::env_u64("ZER0_CG_SECS", DEFAULT_SECS, 5),
        }
    }

    /// Chargement bloquant (mode CLI/standalone, sans runtime async).
    ///
    /// On passe par `reqwest::blocking` : `tokio` tourne déjà (la boucle de la
    // TUI est un runtime), et `block_on` imbriqué paniquerait — d'où ce client
    /// dédié. Le mode TUI, lui, envoie sa requête dans une tâche `tokio` et
    /// reçoit les devises par `UiEvent`.
    pub fn tick_standalone(&mut self) {
        if !self.due() {
            return;
        }
        self.loading = true;
        let url = markets_url(&self.coins, "usd");
        let res = crate::http_client::BLOCKING_CLIENT
            .get(&url)
            .header("accept", "application/json")
            .header("user-agent", USER_AGENT)
            .timeout(Duration::from_secs(20))
            .send()
            .and_then(|r| {
                let status = r.status();
                r.text().map(|t| (status, t))
            });
        match res {
            Ok((status, body)) if status.is_success() => {
                match serde_json::from_str::<serde_json::Value>(&body) {
                    Ok(v) => {
                        let qs = parse_markets(&v);
                        if qs.is_empty() {
                            self.fail("réponse sans devise".to_string());
                        } else {
                            self.apply(qs);
                        }
                    }
                    Err(e) => self.fail(format!("réponse illisible : {e}")),
                }
            }
            Ok((status, _)) => self.fail(format!("HTTP {status}")),
            Err(e) => self.fail(format!("réseau : {e}")),
        }
    }

    pub fn quotes(&self) -> &[Quote] {
        &self.quotes
    }

    /// La liste a changé : on oublie l'intervalle pour recharger tout de suite.
    pub fn reload(&mut self) {
        self.last_attempt = None;
        self.loading = false;
    }

    /// Faut-il lancer une requête maintenant ? (intervalle + pas déjà en vol)
    pub fn due(&self) -> bool {
        if self.loading || self.coins.is_empty() {
            return false;
        }
        match self.last_attempt {
            None => true,
            Some(t) => t.elapsed() >= Duration::from_secs(self.secs),
        }
    }

    /// Applique une réponse : les devises sont réordonnées selon `self.coins`
    /// (l'API ne garantit pas l'ordre demandé).
    pub fn apply(&mut self, mut quotes: Vec<Quote>) {
        quotes.sort_by_key(|q| {
            self.coins
                .iter()
                .position(|c| c.eq_ignore_ascii_case(&q.id))
                .unwrap_or(usize::MAX)
        });
        if !quotes.is_empty() {
            self.quotes = quotes;
            self.last_ok = Some(Instant::now());
        }
        self.error = None;
        self.loading = false;
        self.last_attempt = Some(Instant::now());
    }

    /// Enregistre un échec et rend la main.
    pub fn fail(&mut self, msg: String) {
        self.error = Some(msg);
        self.loading = false;
        self.last_attempt = Some(Instant::now());
    }

    /// Âge lisible de la dernière mise à jour réussie.
    pub fn age_label(&self) -> String {
        match self.last_ok {
            None => "jamais".to_string(),
            Some(t) => {
                let s = t.elapsed().as_secs();
                if s < 60 {
                    format!("il y a {s} s")
                } else {
                    format!("il y a {} min", s / 60)
                }
            }
        }
    }

    pub fn is_stale(&self) -> bool {
        match (self.last_ok, self.error.as_ref()) {
            (_, Some(_)) => true,
            (None, None) => false,
            (Some(t), None) => t.elapsed() > STALE_AFTER,
        }
    }

    /// Grandeur du panneau : compact (1 rangée/actif) quand il y a beaucoup de
    /// lignes ou peu de hauteur.
    fn compact(&self, area: Rect) -> bool {
        let n = self.coins.len().max(1) as u16;
        let usable = area.height.saturating_sub(4); // cadre + en-tête + pied
        n.saturating_mul(3) > usable
    }

    pub fn draw(&self, f: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::border()))
            .title(format!(" 0cg · watchlist ({}) ", self.coins.len()));
        let inner = block.inner(area);
        f.render_widget(block, area);
        if inner.height < 3 {
            return;
        }

        let compact = self.compact(inner);
        let rows = (inner.height as usize).saturating_sub(2); // en-tête + pied
        let per: u16 = if compact { 1 } else { 3 };
        let visible = (rows / per as usize).max(1);

        let mut y = inner.y;
        if !compact {
            f.render_widget(
                Paragraph::new(Span::styled(
                    format!(" {:<6}{:>13}{:>9}", "ACTIF", "PRIX", "24 H"),
                    Style::default().fg(theme::dim()),
                )),
                Rect::new(inner.x, y, inner.width, 1),
            );
            y += 1;
        }

        if self.coins.is_empty() {
            f.render_widget(
                Paragraph::new(Span::styled(
                    " watchlist vide — `/0cg add btc`",
                    Style::default().fg(theme::dim()),
                )),
                Rect::new(inner.x, y, inner.width, 1),
            );
        }

        for (i, q) in self.quotes.iter().take(visible).enumerate() {
            let top = y + (i as u16) * per;
            if top >= inner.y + inner.height.saturating_sub(2) {
                break;
            }
            self.draw_quote(f, q, Rect::new(inner.x, top, inner.width, per), compact);
        }

        // Pied : fraîcheur **ou** panne, jamais un succès silencieux.
        let footer = if let Some(e) = &self.error {
            // Avant un premier succès, « depuis jamais » ne veut rien dire :
            // on distingue les deux cas.
            let msg = if self.last_ok.is_some() {
                format!(" ! prix indisponibles depuis {} ({e})", self.age_label())
            } else {
                format!(" ! prix indisponibles ({e})")
            };
            Span::styled(msg, Style::default().fg(theme::warning()))
        } else if self.loading || self.quotes.is_empty() {
            Span::styled(" … chargement", Style::default().fg(theme::dim()))
        } else {
            let base = format!(" maj {} · 1 requête / {} s", self.age_label(), self.secs);
            let style = if self.is_stale() {
                Style::default().fg(theme::warning())
            } else {
                Style::default().fg(theme::dim())
            };
            Span::styled(base, style)
        };
        let bottom = inner.y + inner.height - 2;
        f.render_widget(
            Paragraph::new(footer),
            Rect::new(inner.x, bottom, inner.width, 1),
        );
        let hint = if self.coins.is_empty() {
            " /0cg add <id|symbole>"
        } else {
            " /0cg add <id|sym> · rm · list"
        };
        f.render_widget(
            Paragraph::new(Span::styled(hint, Style::default().fg(theme::dim()))),
            Rect::new(inner.x, bottom + 1, inner.width, 1),
        );
    }

    fn draw_quote(&self, f: &mut Frame, q: &Quote, area: Rect, _compact: bool) {
        if area.width < 4 || area.height == 0 {
            return;
        }
        // Colonnes : symbole | prix | 24 h | sparkline.
        // Le sparkline prend ce qui reste ; les trois autres ont une largeur
        // fixe, donc prix et variation sont toujours alignés.
        let sym_w = 7u16;
        let price_w = 13u16;
        let chg_w = 9u16;
        let fixed = sym_w + price_w + chg_w;
        if area.width <= fixed + 3 {
            // Trop étroit pour la courbe : on n'affiche que les chiffres.
            let cols = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Length(sym_w),
                    Constraint::Length(price_w),
                    Constraint::Length(chg_w),
                ])
                .split(area);
            self.draw_fields(f, q, &cols, 1);
            return;
        }
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(sym_w),
                Constraint::Length(price_w),
                Constraint::Length(chg_w),
                Constraint::Min(6),
            ])
            .split(area);
        // Mode étendu : les champs sur la 1ʳᵉ ligne, la courbe sur le reste.
        let fields_h = 1;
        self.draw_fields(f, q, &cols[..3], fields_h);
        let spark_area = Rect::new(cols[3].x, cols[3].y, cols[3].width, cols[3].height);
        self.draw_spark(f, q, spark_area);
    }

    /// Les trois champs texte (symbole, prix, variation 24 h).
    fn draw_fields(&self, f: &mut Frame, q: &Quote, cols: &[Rect], height: u16) {
        if cols.len() < 3 {
            return;
        }
        // Couleur **explicite** : sans fg, le symbole prenait la couleur par
        // défaut du terminal — claire sur un terminal sombre, donc invisible sur
        // le fond papier de `paper`. (Même classe de bug que la palette `/`.)
        let sym = Span::styled(
            format!(
                " {:<width$}",
                q.symbol,
                width = cols[0].width.saturating_sub(1) as usize
            ),
            Style::default()
                .fg(theme::text())
                .add_modifier(Modifier::BOLD),
        );
        let price = Span::styled(
            format!(
                "{:>width$}",
                q.price_label(),
                width = cols[1].width as usize
            ),
            Style::default().fg(theme::text()),
        );
        let chg_color = if q.is_up() {
            theme::success()
        } else {
            theme::danger()
        };
        let chg = Span::styled(
            format!(
                "{:>width$}",
                q.change_label(),
                width = cols[2].width as usize
            ),
            Style::default().fg(chg_color),
        );
        f.render_widget(
            Paragraph::new(Line::from(vec![sym, price, chg])),
            Rect::new(
                cols[0].x,
                cols[0].y,
                cols[0].width + cols[1].width + cols[2].width,
                height,
            ),
        );
    }

    /// Courbe 7 jours en **braille** (Canvas), sans axes : l'échelle n'a pas
    /// d'intérêt à cette taille, seule la forme compte.
    fn draw_spark(&self, f: &mut Frame, q: &Quote, area: Rect) {
        if area.width < 4 || area.height == 0 || q.spark.len() < 2 {
            return;
        }
        // Sous-échantillonnage : la fenêtre graphique n'a que 2×width colonnes.
        let target = (area.width as usize * 2).max(8);
        let pts = downsample(&q.spark, target);
        let lo = pts.iter().cloned().fold(f64::INFINITY, f64::min);
        let hi = pts.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let (lo, hi) = if (hi - lo).abs() < f64::EPSILON {
            (lo - 1.0, hi + 1.0)
        } else {
            (lo, hi)
        };
        let data: Vec<(f64, f64)> = pts
            .iter()
            .enumerate()
            .map(|(i, v)| (i as f64, *v))
            .collect();
        let ds = Dataset::default()
            .data(&data)
            .marker(Marker::Braille)
            .graph_type(GraphType::Line)
            .style(Style::default().fg(theme::accent()));
        let chart = Chart::new(vec![ds])
            .x_axis(Axis::default().bounds([0.0, (pts.len() - 1).max(1) as f64]))
            .y_axis(Axis::default().bounds([lo, hi]));
        f.render_widget(chart, area);
    }
}

/// Réduit une série à `n` points en gardant la forme (moyenne par seau).
pub fn downsample(series: &[f64], n: usize) -> Vec<f64> {
    if n == 0 || series.is_empty() {
        return Vec::new();
    }
    if series.len() <= n {
        return series.to_vec();
    }
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let a = i * series.len() / n;
        let b = ((i + 1) * series.len() / n).max(a + 1);
        let slice = &series[a..b.min(series.len())];
        out.push(slice.iter().sum::<f64>() / slice.len() as f64);
    }
    out
}

/// Réponse `/coins/markets` → quotes. Fonction pure : testable sans réseau.
pub fn parse_markets(json: &serde_json::Value) -> Vec<Quote> {
    let Some(arr) = json.as_array() else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|m| {
            let id = m.get("id")?.as_str()?.to_string();
            let symbol = m
                .get("symbol")
                .and_then(|s| s.as_str())
                .unwrap_or(&id)
                .to_uppercase();
            let price = m
                .get("current_price")
                .and_then(|p| p.as_f64())
                .unwrap_or(0.0);
            let change_24h = m
                .get("price_change_percentage_24h")
                .and_then(|p| p.as_f64());
            let spark = m
                .get("sparkline_in_7d")
                .and_then(|s| s.get("price"))
                .and_then(|p| p.as_array())
                .map(|a| a.iter().filter_map(|v| v.as_f64()).collect())
                .unwrap_or_default();
            Some(Quote {
                id,
                symbol,
                price,
                change_24h,
                spark,
            })
        })
        .collect()
}

/// URL de la requête (une seule pour toute la liste).
pub fn markets_url(coins: &[String], vs: &str) -> String {
    format!(
        "https://api.coingecko.com/api/v3/coins/markets?vs_currency={vs}&ids={}&sparkline=true&price_change_percentage=24h",
        coins.join(",")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn json() -> serde_json::Value {
        serde_json::json!([
            {"id":"ethereum","symbol":"eth","current_price":1822.22,
             "price_change_percentage_24h":-1.1,
             "sparkline_in_7d":{"price":[1800.0,1810.0,1822.2]}},
            {"id":"bitcoin","symbol":"btc","current_price":64947.0,
             "price_change_percentage_24h":2.4,
             "sparkline_in_7d":{"price":[63000.0,64000.0,64947.0]}}
        ])
    }

    #[test]
    fn parse_keeps_the_fields_we_show() {
        let q = parse_markets(&json());
        assert_eq!(q.len(), 2);
        let eth = q.iter().find(|x| x.id == "ethereum").unwrap();
        assert_eq!(eth.symbol, "ETH");
        assert!((eth.price - 1822.22).abs() < 1e-6);
        assert_eq!(eth.spark.len(), 3);
        assert!(!eth.is_up());
    }

    #[test]
    fn applying_a_response_follows_the_users_order() {
        // L'API ne garantit pas l'ordre : c'est la watchlist qui décide.
        let mut cg = Cg::new(vec!["bitcoin".into(), "ethereum".into()]);
        cg.loading = true;
        cg.apply(parse_markets(&json()));
        let ids: Vec<&str> = cg.quotes().iter().map(|q| q.id.as_str()).collect();
        assert_eq!(ids, vec!["bitcoin", "ethereum"]);
        assert!(!cg.is_stale());
        assert!(cg.age_label().starts_with("il y a"));
    }

    #[test]
    fn labels_are_human_readable() {
        let q = Quote {
            id: "bitcoin".into(),
            symbol: "BTC".into(),
            price: 64947.0,
            change_24h: Some(2.4),
            spark: vec![],
        };
        // Séparateur de milliers : espace ordinaire (largeur 1, portable).
        assert_eq!(q.price_label(), format!("64{}947 $", ' '));
        assert_eq!(q.change_label(), "+2,4 %");
        assert!(q.is_up());
        let small = Quote {
            price: 0.52,
            change_24h: Some(-3.25),
            ..q.clone()
        };
        assert_eq!(small.price_label(), "0,5200 $");
        assert_eq!(small.change_label(), "−3,2 %");
        assert!(!small.is_up());
    }

    #[test]
    fn a_failure_is_reported_and_marks_the_panel_stale() {
        let mut cg = Cg::new(vec!["bitcoin".into()]);
        cg.loading = true;
        cg.fail("429 Too Many Requests".into());
        assert!(cg.is_stale(), "une panne doit être dite périmée");
        assert_eq!(cg.error.as_deref(), Some("429 Too Many Requests"));
    }

    #[test]
    fn due_follows_the_interval_and_never_overlaps() {
        let mut cg = Cg::new(vec!["bitcoin".into()]);
        cg.secs = 5;
        assert!(cg.due(), "première requête attendue");
        cg.loading = true;
        assert!(!cg.due(), "pas de doublon pendant une requête");
        cg.apply(parse_markets(&json()));
        assert!(!cg.due(), "intervalle pas encore écoulé");
    }

    #[test]
    fn downsample_keeps_the_shape_and_the_size() {
        let series: Vec<f64> = (0..168).map(|i| i as f64).collect();
        let d = downsample(&series, 24);
        assert_eq!(d.len(), 24);
        assert!(d[0] < d[23], "la tendance doit rester croissante");
        // Série plus courte que la cible : rendue telle quelle.
        assert_eq!(downsample(&[1.0, 2.0], 24), vec![1.0, 2.0]);
    }

    #[test]
    fn the_row_shows_symbol_price_and_change() {
        // Régression : la ligne n'affichait que le symbole (prix et 24 H vides)
        // alors que la sparkline, elle, était bien rendue.
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;

        let q = Quote {
            id: "bitcoin".into(),
            symbol: "BTC".into(),
            price: 64910.0,
            change_24h: Some(0.8),
            spark: vec![1.0, 2.0, 1.5, 3.0, 2.5],
        };
        let cg = Cg::new(vec!["bitcoin".into()]);
        let mut term = Terminal::new(TestBackend::new(60, 8)).unwrap();
        term.draw(|f| cg.draw_quote(f, &q, f.area(), false))
            .unwrap();
        let buf = term.backend().buffer();
        let mut text = String::new();
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                text.push_str(buf[(x, y)].symbol());
            }
            text.push('\n');
        }
        assert!(text.contains("BTC"), "symbole absent : {text}");
        assert!(text.contains("64 910 $"), "prix absent : {text}");
        assert!(text.contains("+0,8 %"), "variation absente : {text}");
    }

    #[test]
    fn the_row_falls_back_to_numbers_on_a_narrow_panel() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;

        let q = Quote {
            id: "ethereum".into(),
            symbol: "ETH".into(),
            price: 1815.0,
            change_24h: Some(-2.3),
            spark: vec![1.0, 2.0],
        };
        let cg = Cg::new(vec!["ethereum".into()]);
        let mut term = Terminal::new(TestBackend::new(28, 3)).unwrap();
        term.draw(|f| cg.draw_quote(f, &q, f.area(), true)).unwrap();
        let buf = term.backend().buffer();
        let mut text = String::new();
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                text.push_str(buf[(x, y)].symbol());
            }
            text.push('\n');
        }
        assert!(text.contains("ETH"), "{text}");
        assert!(text.contains("1 815 $"), "{text}");
        assert!(
            text.contains("-2,3") || text.contains("\u{2212}2,3"),
            "{text}"
        );
    }

    #[test]
    fn the_symbol_is_coloured_by_the_theme() {
        // Régression : le symbole était rendu sans couleur explicite, donc avec
        // la couleur par défaut du terminal — illisible sur le fond clair de
        // `paper`. On vérifie la couleur réelle de la cellule.
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;

        let q = Quote {
            id: "bitcoin".into(),
            symbol: "BTC".into(),
            price: 64910.0,
            change_24h: Some(0.8),
            spark: vec![1.0, 2.0, 3.0],
        };
        let cg = Cg::new(vec!["bitcoin".into()]);

        for theme_name in ["paper", "noir", "castle"] {
            crate::theme::activate(theme_name).unwrap();
            let mut term = Terminal::new(TestBackend::new(60, 6)).unwrap();
            term.draw(|f| cg.draw_quote(f, &q, f.area(), true)).unwrap();
            let buf = term.backend().buffer();
            let mut found = false;
            for y in 0..buf.area.height {
                for x in 0..buf.area.width {
                    let cell = &buf[(x, y)];
                    if cell.symbol().contains('B') {
                        assert_eq!(
                            cell.fg,
                            crate::theme::text(),
                            "symbole sans couleur du thème ({theme_name})"
                        );
                        found = true;
                    }
                }
            }
            assert!(found, "symbole absent du rendu ({theme_name})");
        }
        crate::theme::activate("ghost").unwrap();
    }

    #[test]
    fn the_user_agent_is_present_and_versioned() {
        // Sans agent, CoinGecko répond 403 (mesuré) : ce n'est pas cosmétique.
        assert!(USER_AGENT.starts_with("zer0gh0st/"), "{USER_AGENT}");
        assert!(USER_AGENT.len() > "zer0gh0st/".len(), "{USER_AGENT}");
    }

    #[test]
    fn the_url_lists_every_coin_once() {
        let url = markets_url(&["btc".into(), "eth".into()], "usd");
        assert!(url.contains("ids=btc,eth"), "{url}");
        assert!(url.contains("vs_currency=usd"), "{url}");
        assert!(url.contains("sparkline=true"), "{url}");
    }
}
