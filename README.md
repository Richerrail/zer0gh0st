# ZER0│GH0ST

> **Un agent IA qui vit dans ton terminal.** Rust · TUI · local ou cloud · aucune clé requise.

![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)
![Rust](https://img.shields.io/badge/rust-edition%202021-orange.svg)
![Platform](https://img.shields.io/badge/platform-Linux%20%7C%20macOS%20%7C%20Windows-lightgrey.svg)
![CLI only](https://img.shields.io/badge/interface-CLI%20%2F%20TUI-black.svg)

**Version** : 0.5.0 · **Licence** : MIT · **Interface** : terminal uniquement (pas de GUI web).

**Zer0** est un agent d'ingénierie qui vit dans un terminal. Réécrit de zéro en **Rust**,
inspiré d'Agent Zero, Spynel et Synnoesis. Son nom — *ghost in the shell* — dit tout :
le **shell** est ton terminal Unix, le **ghost** est l'IA dedans.

> ⚠️ **Avant le mode outils** : `code_exec` exécute de **vraies commandes shell avec tes droits**.
> Il n'y a **pas de bac à sable**. Lis [`SAFETY.md`](./SAFETY.md) avant de l'activer.

---

## Sommaire

1. [Démarrage rapide](#1-démarrage-rapide)
2. [Ce que c'est](#2-ce-que-cest)
3. [Sécurité](#3-sécurité)
4. [Fonctionnalités](#4-fonctionnalités)
5. [Installation](#5-installation)
6. [Usage & exemples](#6-usage--exemples)
7. [Commandes](#7-commandes)
8. [Zero-chan (voix)](#8-zero-chan-voix)
9. [Configuration](#9-configuration)
10. [Modules fun](#10-modules-fun)
11. [Limites connues](#11-limites-connues)
12. [Documentation](#12-documentation)
13. [Contribuer](#13-contribuer)
14. [Crédits](#14-crédits)
15. [Licence](#15-licence)

---

## 1. Démarrage rapide

```bash
git clone https://github.com/<toi>/zer0gh0st.git && cd zer0gh0st
cargo build --release          # ~5 min la première fois
./target/release/zer0          # ou : ./scripts/run.sh
```

Mode **100 % local** (aucune clé API) :

```bash
export ZER0_MODEL=/chemin/vers/mon-modele.Q4_0.gguf
./scripts/serve.sh             # llama-server sur :8080
./target/release/zer0          # puis, dans la TUI : /provider local
```

Mode **cloud** (clé optionnelle) :

```bash
zer0 provider add openrouter sk-or-...
zer0 models --free             # liste les modèles gratuits
```

Prérequis : **Rust** (cargo) + un **compilateur C** (`cc`). `llama-server` (llama.cpp)
uniquement pour le mode local.

---

## 2. Ce que c'est

Un binaire `zer0`. Une TUI. Une boucle : *utilisateur → modèle → outils → modèle → réponse*.
L'inférence est **déléguée** (llama-server local ou n'importe quel provider OpenAI-compatible) :
Zer0 ne réimplémente **aucun** modèle.

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

## 3. Sécurité

> **Zer0 n'est pas isolé.** Quand `code_exec` est activé, le modèle peut lancer n'importe
> quelle commande que **toi** tu peux lancer — supprimer des fichiers, installer des paquets,
> envoyer des données sur le réseau.

- Les outils sont **désactivés par défaut**. `F4` bascule `code_exec` en direct ; `--allow-exec`
  démarre avec l'exécution activée.
- Le coffre (`/vault`) **chiffre les clés au repos** mais **ne protège pas** contre
  l'exfiltration par le processus lui-même.
- Aucune clé API n'est stockée dans le dépôt : elles vivent dans `~/.config/zer0/` (chmod 600).

Détail complet du modèle de menace : [`SAFETY.md`](./SAFETY.md).

---

## 4. Fonctionnalités

| Domaine | Détail |
|---|---|
| **TUI** | banner `ZER0│GH0ST` (`ghost in the shell`) animé, statut, scroll, raisonnement affiché, **panneau latéral**, **input multi-ligne** (1–8 lignes), **historique ↑↓** |
| **Boucle d'agent** | appels d'outils, streaming SSE, max 8 itérations, budget 24 outils/tour, détection de boucle |
| **Containment** | réponse coupée à 1 Mo, timeout d'inactivité 120 s, `/stop` |
| **Multi-provider** | 9 providers intégrés + personnalisés, clés en config ou **coffre chiffré** |
| **Résilience** | bascule d'endpoints, backoff 429, blacklist d'échecs 120 s, messages par code HTTP |
| **Gratuit** | découverte des modèles `:free` (OpenRouter public), `select` par **benchmarks réels** |
| **Rôles** | `main/code/reasoning/writing/vision/fast/utility/review`, sélection auto par catégorie |
| **Mémoire** | store JSONL, recherche sémantique optionnelle, **enveloppe d'honnêteté** |
| **Connaissance** | base **SQLite + FTS5** (hybride), **injection auto** dans le prompt (+ outil `knowledge_search`) |
| **Zero-chan** | assistante vocale **intégrée à la TUI** : bloc d'état à droite du banner + boutons `0chan`/`micro` à droite de la saisie. **Réutilise le chat et l'input de l'agent**. STT = Whisper local. |
| **Historique** | **conversation persistée** (JSONL), rechargée au démarrage ; `/history`, `/new` |
| **Sélection** | clic + glisser dans le chat/saisie → surbrillance + **copie presse-papier** auto |
| **Contexte** | compaction automatique (~4000 tokens) via le rôle `utility` |
| **Tâches** | liste Markdown `.zer0/tasks.md`, `claim/done`, boucle `/work` par sous-agent |
| **Sous-agents** | `/ask`, `/review`, `/jev` en contexte isolé |
| **Outils** | `code_exec`, `text_editor`, `web_search`, `memory_*`, `task_*` |
| **Divers** | `/vault`, `/pdf`, personas `/whoami`, recherche `/search` |

---

## 5. Installation

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

Place un modèle `.gguf` dans `models/` (ou pointe-le) :

```bash
export ZER0_MODEL=/chemin/vers/mon-modele.Q4_0.gguf   # optionnel (défaut dans serve.sh)
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

## 6. Usage & exemples

Zer0 agit : tu décris un but, il appelle les outils nécessaires.

```text
> analyse mes processus et dis-moi lesquels fermer
> refactore src/tools/mod.rs puis lance les tests
> cherche les actus Rust 2026 et résume-les en 5 points
> indexe base_connaissance/ dans la base de connaissance
> crée la tâche « ajouter le support PDF » et traite-la
```

### Raccourcis clavier

| Touche | Action |
|---|---|
| `Entrée` | envoyer · `Maj+Entrée` / `Alt+Entrée` : nouvelle ligne |
| `↑` `↓` | historique de saisie |
| `PgUp` `PgDn` | faire défiler la conversation |
| `F4` | activer/désactiver `code_exec` |
| `F8` / `F10` | zero-chan (assistante) / micro |
| `Ctrl+C` / `/stop` | interrompre la génération |
| souris | clic + glisser → sélection, relâcher → copie |

---

## 7. Commandes

**TUI** (extrait) : `/help` `/think` `/provider` `/model` `/models` `/free` `/select` `/route`
`/role` `/project` `/memory` `/recall` `/compact` `/task` `/work` `/ask` `/review` `/search`
`/stop` `/context` `/whoami` `/vault` `/pdf` `/jev` `/kb` `/new` `/history` `/radio` `/bintime`
`/pong` `/kristal` `/streaming` `/retro` `/panel` `/clear` `/quit`.

**CLI** : `zer0 provider|models|model|select|route|role|project|memory|embeddings|task|search|whoami|vault|pdf|jev|bintime|pong|kb`.

Détails : `zer0 --help`, ou les `ROADMAP*.md`.

---

## 8. Zero-chan (voix)

Assistante vocale intégrée, **sans boîte chat ni barre dédiée** :

- **Bloc d'état** en haut à droite du banner (visible quand activé) : humeur, état
  (`en veille` / `écoute…` / `réfléchit…` / `parle…`), mini-visualiseur animé.
- **Deux boutons** à droite de la barre de saisie : `0chan:ON/off` et `MIC:REC/off`
  — cliquables à la souris, ou **`F8`** (0chan) / **`F10`** (micro).
- Le **micro** enregistre (`pw-record`, 16 kHz) puis transcrit (`faster-whisper`) ; le texte
  reconnu est envoyé **dans la chat de l'agent**, comme un message tapé.
- L'avatar et la voix ne dupliquent **pas** l'UI : ils réutilisent chat + input de l'agent.

Roadmap et détails : [`BRAINSTORM_ZEROCHAN.md`](./BRAINSTORM_ZEROCHAN.md).

---

## 9. Configuration

| Chemin | Rôle |
|---|---|
| `~/.config/zer0/config.toml` | clés, provider/modèle actifs, routes, rôles (chmod 600) |
| `~/.config/zer0/vault.enc` | clés chiffrées (Argon2 + ChaCha20-Poly1305) |
| `~/.local/share/zer0/memory.jsonl` | mémoire persistante |
| `~/.local/share/zer0/conversation.jsonl` | historique de conversation (rechargé au démarrage) |
| `~/.local/share/zer0/personas/` | personas (`/whoami`) |
| `<projet>/knowledge.db` | base de connaissance (SQLite + FTS5) |
| `<projet>/base_connaissance/` | sources indexables (texte/markdown) |
| `.zer0/project.toml` | surcharge par projet (provider, modèle, rôles, routes, prompt) |
| `./music/` | dossier audio par défaut (`/radio`) |
| `./mods/` | programmes fun copiés |

---

## 10. Modules fun

| Commande | Quoi | Où |
|---|---|---|
| `/radio [dossier\|fichier\|url]` | panneau audio à droite, playlist auto | natif Rust |
| `/bintime` | panneau horloge binaire CRT | natif Rust |
| `/pong` | Pong **contre l'agent** (plein écran) | natif Rust |
| `/kristal` · `/streaming` · `/retro` | apps lancées en fenêtre séparée | `mods/` (optionnel) |

Les modules `mods/` ne sont **pas fournis** (voir [`mods/README.md`](./mods/README.md)).
Détails : [`ECOSYSTEM.md`](./ECOSYSTEM.md).

---

## 11. Limites connues

Le projet assume ses limites (principe directeur, §12) :

- **`code_exec` n'est pas isolé** : accès complet à ta machine avec tes droits.
- **Dépend du modèle** : un petit modèle local peut mal enchaîner les outils ; certains tags
  propriétaires (ex. `<|…|>` de Kimi) sont nettoyés mais restent imparfaits.
- **Rate-limits fournisseurs** : les paliers gratuits (NVIDIA, OpenRouter) renvoient par
  moments des `429` → failover, mais la réponse peut être vide.
- **Whisper local requis** pour la voix (`faster-whisper`), sinon STT indisponible.
- **Linux testé** ; macOS/Windows sont *portables* mais non validés (voir [`ECOSYSTEM.md`](./ECOSYSTEM.md) §5).

---

## 12. Documentation

| Fichier | Contenu |
|---|---|
| [`SPEC.md`](./SPEC.md) | périmètre, modes de défaillance, critères d'acceptation |
| [`SAFETY.md`](./SAFETY.md) | modèle de menace, choix assumés, ce qui **n'est pas** protégé |
| [`ARCHITECTURE.md`](./ARCHITECTURE.md) | structure technique, flux, modules |
| [`ECOSYSTEM.md`](./ECOSYSTEM.md) | modules fun, portabilité Linux/macOS/Windows |
| [`BRAINSTORM_ZEROCHAN.md`](./BRAINSTORM_ZEROCHAN.md) | analyse & roadmap Zero-chan |
| [`CHANGELOG.md`](./CHANGELOG.md) | historique des versions |
| `ROADMAP.md` · `ROADMAP_FUN.md` · `ROADMAP_PORT_SLASH.md` | feuilles de route |
| [`LICENSE`](./LICENSE) | MIT |

**Principe directeur :**

> **Ne jamais prétendre qu'une chose est sûre ou fiable quand elle ne l'est pas.**

Cette règle (reprise de Synnoesis) traverse le projet : la mémoire **déclare** ses dégradations,
le classement de modèles **dit** qu'il est heuristique, le coffre **ne prétend pas** protéger
contre l'exfiltration par le processus, et `SAFETY.md` liste ce que le bac à sable ne couvre pas.

---

## 13. Contribuer

1. Fork + branche (`git checkout -b feat/ma-feature`).
2. `cargo build` et `cargo clippy` doivent passer sans avertissement.
3. Commits clairs ; ouvre une *pull request* en décrivant le **pourquoi**.
4. Par principe : documente les **limites** de ce que tu ajoutes (pas de promesse non tenue).

Idées ouvertes : voir `ROADMAP*.md`.

---

## 14. Crédits

- Inspirations : **Agent Zero** (Python), **Spynel**, **Synnoesis** (principe d'honnêteté).
- Nom : référence à ***Ghost in the Shell***.
- Inférence : [llama.cpp](https://github.com/ggerganov/llama.cpp) · TUI : [ratatui](https://github.com/ratatui/ratatui).

---

## 15. Licence

**MIT** — usage libre, modification encouragée. Copyright (c) 2026 richerrail.
Voir [`LICENSE`](./LICENSE).
