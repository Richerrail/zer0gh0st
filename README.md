# ZER0│GH0ST — agent IA en terminal (CLI-only)

**Zer0** est un agent d'ingénierie autonome qui vit dans un terminal. Il ne dépend d'aucune
interface web : une TUI (terminal), une boucle d'agent, des outils, du multi-provider et une
mémoire persistante. Réécrit de zéro en **Rust**, inspiré d'Agent Zero, Spynel et Synnoesis.

> **Multi-plateforme** : cœur portable. Détails de portage dans [`ECOSYSTEM.md`](./ECOSYSTEM.md) §5.
> **Limites et sécurité** : à lire avant le mode outils → [`SAFETY.md`](./SAFETY.md).

**Version du projet** : 0.5 · **Date** : 2026-10-06 · **Licence** : MIT (voir [`LICENSE`](./LICENSE))

---

## 1. Ce que c'est

Un binaire `zer0`. Une TUI. Une boucle : *utilisateur → modèle → outils → modèle → réponse*.
L'inférence est **déléguée** (llama-server local ou n'importe quel provider OpenAI-compatible) :
Zer0 ne réimplémente pas de modèle.

```
┌───────────────────────────────────────────── TUI (ratatui) ─────────────────────────────────────────────┐
│  status · banner ZER0│GH0ST · chat · input · footer            │  panneau latéral (/radio, /bintime)      │
└────────────────────────────────────────────────────────────────┴──────────────────────────────────────────┘
        │ Entrée
        ▼
   boucle d'agent ──► sélection de rôle ──► chaîne d'endpoints ──► modèle (SSE) ──► outils ──► …
        │                                            │
        └── mémoire (JSONL)                          └── failover · 429/backoff · mémoire d'échecs
```

---

## 2. Fonctionnalités

| Domaine | Détail |
|---|---|
| **TUI** | banner `ZER0│GH0ST` (`ghost in the shell`) animé, statut, scroll, raisonnement affiché, **panneau latéral**, **input multi-ligne**, **historique ↑↓** |
| **Boucle d'agent** | appels d'outils, streaming SSE, max 8 itérations, budget 24 outils/tour, détection de boucle |
| **Containment** | réponse coupée à 1 Mo, timeout d'inactivité 120 s, `/stop` |
| **Multi-provider** | 9 providers intégrés + personnalisés, clés en config ou **coffre chiffré** |
| **Résilience** | bascule d'endpoints, backoff 429, blacklist d'échecs 120 s, messages par code HTTP |
| **Gratuit** | découverte des modèles `:free` (OpenRouter public), `select` par **benchmarks réels** |
| **Rôles** | `main/code/reasoning/writing/vision/fast/utility/review`, sélection auto par catégorie |
| **Mémoire** | store JSONL, recherche sémantique optionnelle, **enveloppe d'honnêteté** |
| **Connaissance** | base **SQLite + FTS5** (hybride), **injection auto** dans le prompt (+ outil `knowledge_search`) |
| **Zero-chan** | assistante vocale **intégrée à la TUI** : bloc d'état à droite du banner + boutons `0chan`/`micro` à droite de la saisie. **Réutilise le chat et l'input de l'agent**. STT = Whisper local. |
| **Historique** | **conversation persistée** (JSONL), rechargée au démarrage ; `/history`, `/new`. |
| **Sélection** | clic + glisser dans le chat/saisie → surbrillance + **copie presse-papier** auto. |
| **Contexte** | compaction automatique (~4000 tokens) via le rôle `utility` |
| **Tâches** | liste Markdown `.zer0/tasks.md`, `claim/done`, boucle `/work` par sous-agent |
| **Sous-agents** | `/ask`, `/review`, `/jev` en contexte isolé |
| **Outils** | `code_exec`, `text_editor`, `web_search`, `memory_*`, `task_*` |
| **Divers** | `/vault`, `/pdf`, personas `/whoami`, recherche `/search` |

---

## 3. Installation & lancement

Prérequis : **Rust** (cargo) + un **compilateur C** (`cc`) — `rusqlite` est compilé *bundled*,
rien à installer côté SQLite. `llama-server` (llama.cpp) seulement pour le mode **local**.

### Depuis GitHub

```bash
git clone https://github.com/<toi>/zer0gh0st.git
cd zer0gh0st
cargo build --release        # ~5 min la première fois
./scripts/run.sh             # ou directement : ./target/release/zer0
```

### Mode 100 % local (aucune clé API)

```bash
./scripts/serve.sh           # démarre llama-server (2 cœurs sur 4)
./scripts/run.sh
```
Puis dans la TUI : `/provider local`. Backend rapide : `./scripts/serve-fast.sh` (modèle 1.5B).

### Providers cloud (clé optionnelle)

```bash
zer0 provider add openrouter sk-or-...     # ou export OPENROUTER_API_KEY=…
zer0 provider use openrouter
zer0 models --free                         # liste les modèles gratuits
zer0 select "Écris une fonction Rust"      # recommande un modèle :free
```

Voir [`SPEC.md`](./SPEC.md) §1 pour le périmètre exact.

---

## 4. Commandes

TUI (extrait) : `/help` `/think` `/provider` `/model` `/models` `/free` `/select` `/route`
`/role` `/project` `/memory` `/recall` `/compact` `/task` `/work` `/ask` `/review` `/search`
`/stop` `/context` `/whoami` `/vault` `/pdf` `/jev` `/kb` `/new` `/history` `/radio` `/bintime` `/pong` `/kristal`
`/streaming` `/retro` `/panel` `/clear` `/quit`.

CLI : `zer0 provider|models|model|select|route|role|project|memory|embeddings|task|search|whoami|vault|pdf|jev|bintime|pong|kb`.

Détails : `zer0 --help`, ou `ROADMAP*.md`.

---

## 5. Modules fun (`mods/`, autonome)

| Commande | Quoi |
|---|---|
| `/radio [dossier\|fichier\|url]` | panneau audio à droite, playlist auto |
| `/bintime` | panneau horloge binaire CRT |
| `/pong` | Pong **contre l'agent** (plein écran) |
| `/kristal` · `/streaming` · `/retro` | apps lancées en fenêtre séparée depuis `mods/` |

Voir [`ECOSYSTEM.md`](./ECOSYSTEM.md).

---

## 6. Configuration

| Chemin | Rôle |
|---|---|
| `~/.config/zer0/config.toml` | clés, provider/modèle actifs, routes, rôles (chmod 600) |
| `~/.config/zer0/vault.enc` | clés chiffrées (Argon2 + ChaCha20-Poly1305) |
| `~/.local/share/zer0/memory.jsonl` | mémoire persistante |
| `~/.local/share/zer0/conversation.jsonl` | historique de conversation (rechargé au démarrage) |
| `~/.local/share/zer0/personas/` | personas (`/whoami`) |
| `<ZER0v1>/knowledge.db` | base de connaissance (SQLite + FTS5) |
| `<ZER0v1>/base_connaissance/` | sources indexables (texte/markdown) |
| `.zer0/project.toml` | surcharge par projet (provider, modèle, rôles, routes, prompt) |
| `./music/` | dossier audio par défaut (`/radio`) |
| `./mods/` | programmes fun copiés |

---

## 7. Zero-chan (voix)

Assistante vocale intégrée, **sans boîte chat ni barre dédiée** :

- **Bloc d'état** en haut à droite du banner (visible quand activé) : humeur, état
  (`en veille` / `écoute…` / `réfléchit…` / `parle…`), mini-visualiseur animé.
- **Deux boutons** à droite de la barre de saisie : `0chan:ON/off` et `MIC:REC/off`
  — cliquables à la souris, ou **`F8`** (0chan) / **`F10`** (micro) / **`Ctrl+O`** (micro).
- Le **micro** enregistre (`pw-record`, 16 kHz) puis transcrit (`faster-whisper`) ; le texte
  reconnu est envoyé **dans la chat de l'agent**, comme un message tapé.
- L'avatar/la voix ne dupliquent **pas** l'UI : ils réutilisent chat + input de l'agent.

Roadmap/détails : [`BRAINSTORM_ZEROCHAN.md`](./BRAINSTORM_ZEROCHAN.md).

---

## 8. Documentation

| Fichier | Contenu |
|---|---|
| [`SPEC.md`](./SPEC.md) | périmètre, modes de défaillance, critères d'acceptation |
| [`SAFETY.md`](./SAFETY.md) | modèle de menace, choix assumés, ce qui **n'est pas** protégé |
| [`ARCHITECTURE.md`](./ARCHITECTURE.md) | structure technique, flux, modules |
| [`ECOSYSTEM.md`](./ECOSYSTEM.md) | modules fun, portabilité Linux/macOS/Windows |
| [`BRAINSTORM_ZEROCHAN.md`](./BRAINSTORM_ZEROCHAN.md) | analyse & roadmap Zero-chan |
| [`CHANGELOG.md`](./CHANGELOG.md) | historique des versions |
| [`LICENSE`](./LICENSE) | MIT |
| `ROADMAP.md` · `ROADMAP_FUN.md` · `ROADMAP_PORT_SLASH.md` | feuilles de route |

---

## 9. Principe directeur

> **Ne jamais prétendre qu'une chose est sûre ou fiable quand elle ne l'est pas.**

Cette règle (reprise de Synnoesis) traverse le projet : la mémoire **déclare** ses dégradations,
le classement de modèles **dit** qu'il est heuristique, le coffre **ne prétend pas** protéger
contre l'exfiltration par le processus, et `SAFETY.md` liste ce que le bac à sable ne couvre pas.

---

## 10. Licence

**MIT** — usage libre, modification encouragée. Copyright (c) 2026 richerrail.
Voir [`LICENSE`](./LICENSE).
