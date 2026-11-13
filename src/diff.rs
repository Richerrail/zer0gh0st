//! Diff ligne à ligne pour montrer ce que `text_editor` a **réellement** changé.
//!
//! Pourquoi ici plutôt que dans le rendu : l'outil a l'avant et l'après en main,
//! la TUI n'a que du texte. Calculer le diff dans l'outil évite toute lecture de
//! fichier en plus (et tout I/O par frame).
//!
//! Le format sorti est du **diff unifié** lisible tel quel :
//!
//! ```text
//! @@ -58,8 +58,10 @@
//!    58    options: ChatOptions,
//!   - 61      crate::cancel::clear();
//!   + 61      // B5 : le drapeau n'est remis à zéro que pour un tour…
//! ```
//!
//! Choix assumés :
//! - **CRLF normalisé** avant comparaison : sinon un fichier Windows réécrit à
//!   l'identique apparaît entièrement modifié (faux « tout a changé »).
//! - **Préfixe/suffixe communs retirés** avant l'alignement : une retouche de
//!   3 lignes dans un fichier de 1000 lignes ne coûte pas 1000×1000 cellules de
//!   programmation dynamique.
//! - **Plafond dur** : au-delà de `MAX_DP_LINES`, on renonce à l'alignement fin
//!   (mémoire quadratique) et on signale un remplacement de bloc.
//! - Aucune dépendance ajoutée.

/// Nombre de lignes de contexte autour de chaque changement.
pub const CONTEXT: usize = 3;

/// Au-delà de cette taille (après retrait du préfixe/suffixe commun), on renonce
/// à l'alignement optimal : `n × m` entiers coûterait des centaines de Mo sur un
/// fichier de 20 000 lignes.
const MAX_DP_LINES: usize = 2000;

/// Statistiques d'un diff (ce qu'on affiche même replié).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Stats {
    pub added: usize,
    pub removed: usize,
}

impl Stats {
    /// `(+3 / -1)`, ou vide quand rien n'a changé.
    pub fn label(&self) -> String {
        if self.is_empty() {
            return String::new();
        }
        format!("(+{} / -{})", self.added, self.removed)
    }

    pub fn is_empty(&self) -> bool {
        self.added == 0 && self.removed == 0
    }

    /// Réécriture : beaucoup d'ajouts **et** beaucoup de suppressions.
    ///
    /// C'est le signal qui compte pour l'utilisateur : un modèle qui « corrige »
    /// en réécrivant tout le fichier. On le nomme au lieu de le laisser deviner.
    pub fn looks_like_rewrite(&self) -> bool {
        let max = self.added.max(self.removed);
        let min = self.added.min(self.removed);
        max >= 20 && min * 100 / max >= 60
    }
}

/// Nature d'une ligne de diff.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Added,
    Removed,
    Context,
}

impl Kind {
    /// Préfixe affiché (`+`, `-`, ` `) ; l'en-tête `@@` est en contexte.
    pub fn marker(self) -> char {
        match self {
            Kind::Added => '+',
            Kind::Removed => '-',
            Kind::Context => ' ',
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffLine {
    pub kind: Kind,
    pub text: String,
}

/// Résultat complet : lignes de diff (en-têtes `@@` compris) + statistiques.
#[derive(Debug, Clone, Default)]
pub struct Diff {
    pub lines: Vec<DiffLine>,
    pub stats: Stats,
    /// Nombre de lignes réellement changées, avant plafonnement d'affichage.
    pub changed: usize,
}

impl Diff {
    pub fn is_empty(&self) -> bool {
        self.stats.is_empty()
    }

    /// Rendu `text/diff` prêt à mettre dans un bloc ```diff```.
    ///
    /// `max` plafonne le nombre de lignes **affichées** ; les lignes en trop sont
    /// remplacées par un marqueur explicite (on ne tronque jamais en silence).
    pub fn render(&self, max: usize) -> String {
        let mut out = String::new();
        let mut shown = 0usize;
        let mut skipped = 0usize;
        for line in &self.lines {
            if max > 0 && shown >= max {
                skipped += 1;
                continue;
            }
            if !out.is_empty() {
                out.push('\n');
            }
            // L'en-tête `@@` n'a pas de préfixe dans un diff unifié.
            if line.text.starts_with("@@") {
                out.push_str(&line.text);
            } else {
                out.push(line.kind.marker());
                out.push(' ');
                out.push_str(&line.text);
            }
            shown += 1;
        }
        if skipped > 0 {
            out.push_str(&format!("\n… {skipped} ligne(s) masquée(s)"));
        }
        out
    }
}

/// Combien de lignes de diff un **modèle** reçoit dans un résultat d'outil.
///
/// L'utilisateur voit le diff complet (40 lignes repliables) ; le modèle n'a
/// aucun besoin de relire ce qu'il vient d'écrire. Garder 40 lignes dans le
/// contexte coûtait ~400 tokens par écriture, pour rien.
pub const MODEL_MAX_LINES: usize = 12;

/// Réduit les blocs ```diff``` d'un texte à `max` lignes de contenu.
///
/// Tout le reste du texte (et les autres blocs de code) est laissé intact :
/// cette fonction ne sert qu'à préparer la copie envoyée au modèle.
pub fn cap_diff_blocks(text: &str, max: usize) -> String {
    if !text.contains("```diff") {
        return text.to_string();
    }
    let mut out: Vec<String> = Vec::new();
    let mut in_diff = false;
    let mut shown = 0usize;
    let mut omitted = 0usize;
    for line in text.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") {
            if in_diff {
                if omitted > 0 {
                    out.push(format!("… {omitted} ligne(s) de diff omises"));
                    omitted = 0;
                }
                in_diff = false;
            } else if trimmed.trim_start_matches("```").trim() == "diff" {
                in_diff = true;
                shown = 0;
            }
            out.push(line.to_string());
            continue;
        }
        if in_diff {
            if shown < max {
                out.push(line.to_string());
                shown += 1;
            } else {
                omitted += 1;
            }
        } else {
            out.push(line.to_string());
        }
    }
    if omitted > 0 {
        out.push(format!("… {omitted} ligne(s) de diff omises"));
    }
    out.join("\n")
}

/// Normalise les fins de ligne : un fichier réécrit avec les mêmes lignes en
/// CRLF ne doit **pas** apparaître entièrement modifié.
fn normalize(text: &str) -> Vec<String> {
    text.replace("\r\n", "\n")
        .lines()
        .map(|l| l.to_string())
        .collect()
}

/// Diff unifié entre `old` et `new`.
pub fn unified(old: &str, new: &str) -> Diff {
    let a = normalize(old);
    let b = normalize(new);

    // 1. Retirer les lignes communes de tête et de queue.
    let mut start = 0usize;
    while start < a.len() && start < b.len() && a[start] == b[start] {
        start += 1;
    }
    let mut end = 0usize;
    while end + start < a.len()
        && end + start < b.len()
        && a[a.len() - 1 - end] == b[b.len() - 1 - end]
    {
        end += 1;
    }
    let mid_a = &a[start..a.len() - end];
    let mid_b = &b[start..b.len() - end];
    if mid_a.is_empty() && mid_b.is_empty() {
        return Diff::default();
    }

    // 2. Alignement du milieu (LCS) — ou remplacement de bloc si trop gros.
    let mid: Vec<(Kind, String)> = if mid_a.len() > MAX_DP_LINES || mid_b.len() > MAX_DP_LINES {
        let mut ops = Vec::with_capacity(mid_a.len() + mid_b.len());
        ops.extend(mid_a.iter().map(|l| (Kind::Removed, l.clone())));
        ops.extend(mid_b.iter().map(|l| (Kind::Added, l.clone())));
        ops
    } else {
        lcs_ops(mid_a, mid_b)
    };

    // 3. Reconstitue le fichier complet, puis ne garde que les hunks utiles.
    let mut ops: Vec<(Kind, String)> = Vec::with_capacity(a.len() + mid.len());
    ops.extend(a[..start].iter().map(|l| (Kind::Context, l.clone())));
    ops.extend(mid);
    ops.extend(
        a[a.len() - end..]
            .iter()
            .map(|l| (Kind::Context, l.clone())),
    );

    let mut stats = Stats::default();
    let mut changed = 0usize;
    for (kind, _) in &ops {
        match kind {
            Kind::Added => stats.added += 1,
            Kind::Removed => stats.removed += 1,
            Kind::Context => {}
        }
        if *kind != Kind::Context {
            changed += 1;
        }
    }

    // Positions cumulées : nombre de lignes avant l'indice `i` dans l'ancien et
    // dans le nouveau fichier (sert aux en-têtes `@@ -a,n +b,m @@`).
    let mut old_pos = Vec::with_capacity(ops.len() + 1);
    let mut new_pos = Vec::with_capacity(ops.len() + 1);
    let (mut o, mut n) = (0usize, 0usize);
    for (kind, _) in &ops {
        old_pos.push(o);
        new_pos.push(n);
        match kind {
            Kind::Added => n += 1,
            Kind::Removed => o += 1,
            Kind::Context => {
                o += 1;
                n += 1;
            }
        }
    }
    old_pos.push(o);
    new_pos.push(n);

    // Regroupe les changements proches (±CONTEXT lignes de contexte).
    let mut groups: Vec<(usize, usize)> = Vec::new();
    for (i, (kind, _)) in ops.iter().enumerate() {
        if *kind == Kind::Context {
            continue;
        }
        let from = i.saturating_sub(CONTEXT);
        let to = (i + CONTEXT).min(ops.len().saturating_sub(1));
        match groups.last_mut() {
            Some(g) if from <= g.1 + 1 => g.1 = g.1.max(to),
            _ => groups.push((from, to)),
        }
    }

    let mut lines = Vec::new();
    for (from, to) in groups {
        let old_count = old_pos[to + 1] - old_pos[from];
        let new_count = new_pos[to + 1] - new_pos[from];
        // Convention diff : un côté vide s'écrit `-0,0` (création) ou `+0,0`
        // (suppression), pas `-1,0`.
        let old_start = if old_count == 0 {
            old_pos[from]
        } else {
            old_pos[from] + 1
        };
        let new_start = if new_count == 0 {
            new_pos[from]
        } else {
            new_pos[from] + 1
        };
        lines.push(DiffLine {
            kind: Kind::Context,
            text: format!("@@ -{old_start},{old_count} +{new_start},{new_count} @@"),
        });
        for (kind, text) in &ops[from..=to] {
            lines.push(DiffLine {
                kind: *kind,
                text: text.clone(),
            });
        }
    }

    Diff {
        lines,
        stats,
        changed,
    }
}

/// Alignement LCS classique, rendu en opérations (contexte / ajout / suppression).
fn lcs_ops(a: &[String], b: &[String]) -> Vec<(Kind, String)> {
    let (n, m) = (a.len(), b.len());
    let mut dp = vec![vec![0u32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            dp[i][j] = if a[i] == b[j] {
                dp[i + 1][j + 1] + 1
            } else {
                dp[i + 1][j].max(dp[i][j + 1])
            };
        }
    }
    let mut ops = Vec::new();
    let (mut i, mut j) = (0usize, 0usize);
    while i < n && j < m {
        if a[i] == b[j] {
            ops.push((Kind::Context, a[i].clone()));
            i += 1;
            j += 1;
        } else if dp[i + 1][j] >= dp[i][j + 1] {
            ops.push((Kind::Removed, a[i].clone()));
            i += 1;
        } else {
            ops.push((Kind::Added, b[j].clone()));
            j += 1;
        }
    }
    ops.extend(a[i..].iter().map(|l| (Kind::Removed, l.clone())));
    ops.extend(b[j..].iter().map(|l| (Kind::Added, l.clone())));
    ops
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(d: &Diff) -> Vec<(char, String)> {
        d.lines
            .iter()
            .map(|l| (l.kind.marker(), l.text.clone()))
            .collect()
    }

    #[test]
    fn identical_text_has_no_diff() {
        let d = unified("a\nb\nc\n", "a\nb\nc\n");
        assert!(d.is_empty());
        assert!(d.lines.is_empty());
        assert_eq!(d.stats.label(), "");
    }

    #[test]
    fn one_added_line_is_reported_with_context() {
        let d = unified("a\nb\nc\n", "a\nb\nNEW\nc\n");
        assert_eq!(
            d.stats,
            Stats {
                added: 1,
                removed: 0
            }
        );
        let k = kinds(&d);
        assert_eq!(k[0].0, ' ', "l'en-tête @@ est une ligne de contexte");
        assert!(k[0].1.starts_with("@@ -"), "{:?}", k[0]);
        assert!(k.iter().any(|(m, t)| *m == '+' && t == "NEW"));
        assert!(
            k.iter().any(|(m, t)| *m == ' ' && t == "b"),
            "contexte absent"
        );
    }

    #[test]
    fn replacement_shows_both_sides() {
        let d = unified("x = 1\ny = 2\n", "x = 1\ny = 3\n");
        assert_eq!(
            d.stats,
            Stats {
                added: 1,
                removed: 1
            }
        );
        let k = kinds(&d);
        assert!(k.iter().any(|(m, t)| *m == '-' && t == "y = 2"));
        assert!(k.iter().any(|(m, t)| *m == '+' && t == "y = 3"));
    }

    #[test]
    fn crlf_rewrite_of_the_same_text_is_not_a_change() {
        // Le piège Windows : mêmes lignes, fins de ligne différentes.
        let d = unified("a\nb\nc\n", "a\r\nb\r\nc\r\n");
        assert!(
            d.is_empty(),
            "CRLF ne doit pas produire de faux diff : {d:?}"
        );
    }

    #[test]
    fn unchanged_head_and_tail_do_not_appear() {
        let old = (1..=50)
            .map(|i| format!("l{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let new = old.replace("l25", "l25-modifiee");
        let d = unified(&old, &new);
        assert_eq!(
            d.stats,
            Stats {
                added: 1,
                removed: 1
            }
        );
        // En-tête + 3 lignes avant + 1 - + 1 + + 3 après = 9.
        assert!(d.lines.len() <= 10, "trop de lignes : {}", d.lines.len());
        let out = d.render(0);
        assert!(out.contains("l25-modifiee"));
        assert!(
            !out.contains("l1\n"),
            "la tête du fichier ne doit pas apparaître"
        );
    }

    #[test]
    fn render_caps_and_says_how_many_lines_were_hidden() {
        let new = (1..=40)
            .map(|i| format!("l{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let d = unified("", &new);
        assert_eq!(d.stats.added, 40);
        let out = d.render(10);
        let body: Vec<&str> = out.lines().filter(|l| !l.starts_with('…')).collect();
        assert_eq!(body.len(), 10, "plafond non respecté : {out}");
        assert!(out.contains("31 ligne(s) masquée(s)"), "{out}");
        assert!(
            out.starts_with("@@ -0,0 +1,40 @@"),
            "en-tête absent : {out}"
        );
    }

    #[test]
    fn only_diff_fences_are_capped() {
        let text = "Remplacement (+3 / -1)\n```rust\nfn a() {}\n```\n```diff\n@@ -1,2 +1,2 @@\n a\n-b\n+c\n```\nfin";
        let capped = cap_diff_blocks(text, 2);
        // 2 lignes de diff gardées, le reste annoncé.
        assert!(capped.contains("@@ -1,2 +1,2 @@\n a"), "{capped}");
        assert!(capped.contains("2 ligne(s) de diff omises"), "{capped}");
        assert!(
            !capped.contains("+c"),
            "ligne au-delà du plafond conservée : {capped}"
        );
        // Le bloc rust et la prose sont intacts.
        assert!(capped.contains("fn a() {}"), "{capped}");
        assert!(capped.starts_with("Remplacement (+3 / -1)"), "{capped}");
        assert!(capped.ends_with("fin"), "{capped}");
    }

    #[test]
    fn text_without_diff_is_returned_unchanged() {
        let text = "sortie d'outil banale\nligne 2";
        assert_eq!(cap_diff_blocks(text, 12), text);
    }

    #[test]
    fn cap_under_the_limit_changes_nothing() {
        let text = "```diff\n@@ -1 +1 @@\n-a\n+b\n```";
        assert_eq!(cap_diff_blocks(text, 12), text);
    }

    #[test]
    fn rewrite_is_named_as_such() {
        let big = Stats {
            added: 40,
            removed: 38,
        };
        assert!(big.looks_like_rewrite());
        // Une vraie retouche ne crie pas au loup.
        let small = Stats {
            added: 4,
            removed: 2,
        };
        assert!(!small.looks_like_rewrite());
        // Un ajout massif sans suppression non plus.
        let add = Stats {
            added: 60,
            removed: 0,
        };
        assert!(!add.looks_like_rewrite());
    }

    #[test]
    fn huge_block_replacement_still_works_without_blowing_memory() {
        let old = (1..=3000)
            .map(|i| format!("a{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let new = (1..=3000)
            .map(|i| format!("b{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let d = unified(&old, &new);
        assert_eq!(
            d.stats,
            Stats {
                added: 3000,
                removed: 3000
            }
        );
        assert!(!d.lines.is_empty());
    }
}
