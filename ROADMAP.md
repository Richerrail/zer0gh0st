# Roadmap Zer0

Agent IA en terminal, **CLI-only**, réécrit en **Rust** (inspiré d'Agent Zero, Spynel et Synnoesis).
État : **v0.1 — socle fonctionnel**.

Légende : ✅ fait · 🟡 en cours / proposé · ⬜ prévu

---

## v0.1 — Socle (✅ livré)

### TUI / interface
- ✅ TUI `ratatui` + `crossterm` : banner ASCII `AGENT ZERO`, barre de statut, chat, input, footer
- ✅ Raccourcis `F3` Read&Write, `F4` Code-exec, `F5` Aide, `F6`/`F7`, `Ctrl+C`
- ✅ Animation **arc-en-ciel** du banner pendant le travail du modèle
- ✅ **Scroll** du chat (`↑↓`, `PageUp/Down`, `Home/End`, suivi auto)
- ✅ Affichage du **raisonnement** (`reasoning_content`) en gris
- ✅ Mode headless `--once` (test / automatisation)

### Cœur agent
- ✅ Boucle de messages + **appels d'outils** (max 8 itérations), `break_loop`
- ✅ Streaming **SSE** OpenAI-compatible
- ✅ Filtrage des rôles internes (UI-only) avant envoi
- ✅ Modèle de raisonnement : `enable_thinking` (OFF par défaut), `max_tokens`, température

### Outils
- ✅ `code_exec` (bash réel)
- ✅ `text_editor` (`view`, `create`, `str_replace`, `insert`)

### Inférence locale
- ✅ Client `llama-server` (`/chat/completions`)
- ✅ Budget CPU **2 cœurs sur 4** (`taskset -c 0-1`, `-t 2`) — vérifié thread par thread
- ✅ Modèle `qwen3.5-4B-super-coder` (GGUF, arch `qwen35`)

### Multi-providers
- ✅ Catalogue **9 providers** : local, openrouter, google, nvidia, moonshotai, alibaba, xai, openai, anthropic
- ✅ Clés API en config (`~/.config/zer0/config.toml`, **chmod 600**) ou variables d'env
- ✅ **Providers personnalisés** (`add-custom <id> <base_url>`)
- ✅ En-têtes OpenRouter (`HTTP-Referer`, `X-Title`)

### Modèles gratuits
- ✅ Découverte `/models` + détection `:free` / prix 0 (OpenRouter public, sans clé)
- ✅ `select` par **benchmarks réels** (Artificial Analysis : intel / code / agentic)
- ✅ Honnêteté : « classement heuristique, non garantie »

### Routage / résilience
- ✅ **Failover** endpoints ordonnés, bascule saine, **annoncée**
- ✅ `route auto` : chaîne de repli remplie depuis les meilleurs `:free`
- ✅ Distinction erreurs `Setup` (bascule) vs `Mid` (flux en cours)

### Config
- ✅ Config globale + **par projet** (`.zer0/project.toml`, champs `roles` posés)

---

## v0.2 — Fiabilité des modèles ✅ *livré*

- ✅ **Mémoire des échecs** : endpoint gated/rate-limité blacklisté 120 s (ignoré au tour suivant)
- ✅ Retry avec **backoff exponentiel** sur 429 (2 essais : 2 s, 4 s)
- ✅ Messages clairs par code HTTP (401 clé, 402 crédit, 403 refus, 404 introuvable)
- ✅ Statut TUI : **endpoint réellement utilisé** (après bascule)
- ✅ Adapter **Google natif** (`/v1beta/models`), car la liste OpenAI-compat renvoie 404
- ✅ **Code non tronqué** : `max_tokens` par défaut 8192 + alerte si la réponse est coupée (`finish_reason = length`)
- ✅ Comportement **fichiers** : création via `text_editor` (Read&Write ON par défaut), chemins absolus, test via `code_exec`

## v0.3 — Routage par rôle ✅ *livré*

- ✅ Rôles nommés → `provider/model` : `main`, `code`, `reasoning`, `writing`, `vision`, `fast`, `utility`, `review`
- ✅ **Sélection automatique** par catégorie détectée (classifieur par mot entier, corrigé du piège « r*api*de »)
- ✅ **Chaîne par rôle** : le modèle du rôle passe en tête, puis l'endpoint actif + les routes de bascule
- ✅ **Config par projet** (`.zer0/project.toml`) : `provider`, `model`, `roles`, `routes`, `system_prompt`, `prompt_file`
- ✅ Commandes : `zer0 role list|set|unset|auto`, `zer0 project show|init` ; TUI : `/role`, `/project`
- ✅ Rôle affiché dans la barre de statut (`role:auto/code` ou `role:main`)
- ✅ `role auto` assigne chaque rôle au meilleur modèle `:free` de sa catégorie
- ✅ `fast`/`utility` favorisent les petits modèles (heuristique de nom, faute de données de latence)

## v0.4 — Mémoire honnête ✅ *livré* (insp. Synnoesis)

- ✅ Store **single-file** `~/.local/share/zer0/memory.jsonl` (chmod 600). *JSONL plutôt que SQLite (pas de dépendance C lourde) — choix déclaré.*
- ✅ `save` / recherche : **sémantique** si un embedder est configuré (`/embeddings` OpenAI-compatible), sinon **sous-chaîne**
- ✅ **Enveloppe d'honnêteté** : `degraded` + raison, `ranking_meaningful`, `trust_enforced: false`, `testables/probées/total`
- ✅ Chaque vecteur enregistre son embedder ; un classement traversant deux espaces → `ranking_meaningful: false`
- ✅ `trigger` optionnel → entrée **testable** ; sans trigger → `probeable: false`
- ✅ `trust` = `untrusted` sur toutes les entrées, jamais appliqué (affiché à chaque réponse)
- ✅ Outils `memory_save`/`memory_search` ; CLI `memory save|search|list|stats|forget|clear` ; TUI `/memory`, `/remember`, `/recall`, `/forget`
- ✅ **Compaction / résumé d'historique** : au-delà de ~6000 tokens, les anciens messages sont résumés par le rôle `utility` et remplacés ; `/compact` pour forcer. L'historique devient un message de résumé + les messages récents.

> Principe : la mémoire ne prétend jamais qu'un classement est fiable quand il ne l'est pas. Aucune entrée n'est auto-injectée dans le modèle pour l'instant (toutes `untrusted`), conformément à la position prudente de Synnoesis.

## v0.5 — Orchestration ✅ *livré*

- ✅ **Containment de boucle** : détection de répétition (même outil + mêmes arguments 3×) → arrêt ; budget de **24 appels d'outils/tour** ; plafond de flux **1 Mo** + timeout d'inactivité **120 s** (contre les modèles qui bouclent)
- ✅ **Tâches (division du travail)** : liste Markdown `.zer0/tasks.md`, statuts `[ ] / [~] / [x]`, claim/done ; outils `task_add`/`task_list`/`task_update` ; CLI `task` ; TUI `/task`
- ✅ **Boucle de travail** `/work` : prend la prochaine tâche en attente, la *claim*, l'exécute via un **sous-agent** du rôle associé, puis la marque *faite*
- ✅ **Sous-agents isolés** : `/ask <role> <question>` (contexte séparé, ne pollue pas le chat)
- ✅ **Revue / verdict** : `/review <fichier>` via le rôle `review`, se termine par `VERDICT: OK` ou `VERDICT: PROBLEMES` (premier pas vers la convergence sur assentiment)
- 🟡 Convergence multi-agents (assentiment formel à plusieurs) — prévue en v1.0

> Inspiré de **Spynel** (tâches Markdown + boucles plan/implémentation/revue) et **Synnoesis** (containment de boucle).

## v1.0 — Distribué ⬜

- ⬜ Identité **Ed25519** par agent, messages signés / vérifiés localement
- ⬜ Transport cross-machine (broker), présence, livraison durable
- ⬜ Multi-agents sur plusieurs machines

---

## Dette technique / qualité

- ⬜ Tests unitaires + d'intégration (client, select, config, route)
- ⬜ CI (build + tests) et releases versionnées
- ⬜ Gestion fine des erreurs réseau (timeouts, proxy)
- ⬜ Packaging (binaire `zer0`, install script)
- ⬜ Refactor `tui/mod.rs` (découper rendu / commandes / état)

## Décisions actées

- ❌ Assembly / C++ écartés → **Rust** (sécurité mémoire + écosystème)
- ❌ Réimplémenter l'inférence → **déléguée** (`llama-server` / providers)
- ✅ Modèle local **bridé à 2 cœurs** pour laisser la machine réactive
- ✅ Philosophie **honnêteté** (déclarer ce qui n'est pas vérifié) reprise de Synnoesis
