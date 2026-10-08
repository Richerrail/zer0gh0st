# Zer0 — SPEC (Spécification du système)

> **Portée** : ce que le système doit faire, comment il se comporte quand ça casse, et comment on
> sait qu'il marche. Complète le README ; ne le remplace pas.

**Version du document** : 1.0 (2026-10-06)
**Version du projet** : 0.5
**Licence** : MIT (voir `LICENSE`)

---

## 1. Périmètre

### 1.1 Noyau (produit principal)
- `main.rs` — CLI (`clap`), sous-commandes, bootstrap, `--once` headless.
- `tui/` — interface terminal (`ratatui` + `crossterm`), panneau latéral, modes.
- `agent/` — boucle de messages (`turn.rs`), historique (`history.rs`), compaction (`compact.rs`), sous-agents (`subagent.rs`).
- `llm/llama.rs` — client SSE OpenAI-compatible, assemblage des `tool_calls`, plafonds de flux.
- `providers.rs`, `models.rs`, `select.rs`, `route.rs` — multi-provider, découverte, sélection, bascule.
- `config.rs`, `vault.rs`, `memory.rs`, `tasks.rs`, `personas.rs`, `pdf.rs`, `search.rs`.

### 1.2 Outils intégrés
L'agent peut appeler (si activés) :

| Outil | Action | Activation |
|---|---|---|
| `code_exec` | exécute `bash -lc` | `F4` (OFF par défaut) |
| `text_editor` | `view/create/str_replace/insert/delete` | `F3` (ON par défaut) |
| `web_search` | DuckDuckGo, sans clé | toujours |
| `memory_save` / `memory_search` | mémoire persistante | toujours |
| `task_add` / `task_list` / `task_update` | tâches Markdown | toujours |

### 1.3 Modules fun (`mods/`) — hors noyau
`/radio`, `/bintime`, `/pong` (TUI) ; `/kristal`, `/streaming`, `/retro` (fenêtre séparée).
Périmètre et portabilité : [`ECOSYSTEM.md`](./ECOSYSTEM.md).

### 1.4 Hors périmètre
Web UI, Docker, plugins tiers, entraînement/fine-tuning, contrôle GUI, navigateur intégré.

---

## 2. Comportement en cas de défaillance

Toute dégradation doit être **visible**, jamais silencieuse.

| Situation | Comportement spécifié |
|---|---|
| Provider injoignable / 5xx | Bascule sur l'endpoint suivant, **annoncée** (`⚠ … → endpoint suivant`) |
| HTTP 429 | Backoff exponentiel (2 s, 4 s) puis bascule ; endpoint blacklisté 120 s |
| HTTP 401 | Message « clé API invalide ou absente », bascule |
| Réponse vide (modèle de raisonnement) | Considérée comme échec → bascule |
| Flux qui n'envoie plus rien (120 s) | Interrompu, message, pas de blocage RAM |
| Réponse > 1 Mo | Flux coupé, message « réponse trop longue » |
| Boucle d'outils (même outil+args 3×) | Arrêt, message `boucle détectée` |
| Budget d'outils (24/tour) dépassé | Arrêt, message |
| `/compact` sur conversation courte | Message `Rien à compacter`, **pas** une erreur |
| Coffre verrouillé, clé absente | Message explicite (env ou `vault unlock`) ; le reste continue |
| `text_editor create` sans contenu | **Refusé** (erreur), aucun fichier vide écrit |
| Inférence locale lente | Fonctionne ; le prompt est traité même à ~2 tok/s |

---

## 3. Critères d'acceptation (vérifiables)

| Affirmation | Comment on le sait | Statut |
|---|---|---|
| « Bascule automatique » | Test : provider actif sans clé + route valide → la réponse vient du 2ᵉ | ✅ observé |
| « Classement par benchmarks » | `select` affiche les indices Artificial Analysis et le nombre d'endpoints | ✅ observé |
| « Le classement est fiable » | **Non** — heuristique par mots-clés + benchmarks ; dit explicitement « non garantie » | ⚠️ non vérifié |
| « Mémoire honnête » | `degraded`, `ranking_meaningful`, `trust_enforced` affichés à chaque recherche | ✅ observé |
| « Compaction » | `/compact` remplace les anciens messages par un résumé du rôle `utility` | ✅ observé |
| « Containment de boucle » | 3 appels identiques → arrêt | ✅ à rejouer |
| « Playlist radio » | Piste suivante auto à la fin | ✅ observé |
| Portabilité macOS/Windows | Analyse statique, **non testé** (pas de toolchain) | ⚠️ voir ECOSYSTEM §5 |

> Les lignes ⚠️ sont des **limites déclarées**, pas des promesses. Toute métrique chiffrée
> (taux de bons routages, latence, taux d'échec) doit être publiée avec la version.

---

## 4. Frontières de confiance

Zer0 exécute du **vrai** bash (`code_exec`), écrit et supprime des fichiers (`text_editor`),
lit le web (`web_search`) et stocke des secrets (coffre). Il n'existe **aucun bac à sable** :
c'est le choix assumé, documenté en détail dans [`SAFETY.md`](./SAFETY.md) §1.

Points clés :
- Le modèle n'est **pas** de confiance : ses appels d'outils sont exécutés tels quels.
- Une page web peut tenter une injection de prompt (vecteur par construction).
- Le coffre protège le **disque**, pas contre l'exfiltration par le processus.

→ Lire `SAFETY.md` avant d'activer `code_exec`.

---

## 5. Invariants d'implémentation

- **Rôles de message** : seuls `user/assistant/tool/system` sont envoyés au serveur (les notices UI sont filtrées).
- **Enveloppe d'honnêteté** : toute recherche mémoire renvoie `degraded`, `ranking_meaningful`, `trust_enforced`.
- **Annonce de bascule** : jamais de changement d'endpoint silencieux.
- **Fichier vide** : `text_editor create` refuse un contenu vide.
- **Coffre** : `config.toml` et `vault.enc` sont `chmod 600` (Unix) ; ignoré sur les OS non-Unix.
