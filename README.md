# ZER0│GH0ST

> **Un agent IA qui vit dans ton terminal.** Rust · TUI · local ou cloud · aucune clé requise.

![ZER0│GH0ST](./zer0-ghost.png)

![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)
![Rust](https://img.shields.io/badge/rust-edition%202021-orange.svg)
![Platform](https://img.shields.io/badge/platform-Linux%20%7C%20macOS%20%7C%20Windows-lightgrey.svg)
![CLI only](https://img.shields.io/badge/interface-CLI%20%2F%20TUI-black.svg)
![CI](https://github.com/Richerrail/zer0gh0st/actions/workflows/ci.yml/badge.svg)

**Version** : 0.5.6 · **Licence** : MIT · **Interface** : terminal uniquement (pas de GUI web).

![Aperçu de ZER0│GH0ST](./screenshot.png)

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
15. [Soutenez le projet](#soutenez-le-projet)
16. [Licence](#16-licence)

---

## 1. Démarrage rapide

```bash
git clone https://github.com/Richerrail/zer0gh0st.git && cd zer0gh0st
./scripts/install.sh           # dépendances + build (ou : cargo build --release)
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
- **Déploiement durci (recommandé)** : la passerelle `zer0-gw` garde les clés **hors du
  processus agent**, l'egress nftables limite l'agent au loopback, et `code_exec` n'hérite
  plus des variables secrètes — voir `deploy/` et [`SAFETY.md`](./SAFETY.md) §7.
- `code_exec` **ne voit plus les secrets dans son environnement** : les variables
  `*API_KEY*`, `*TOKEN*`, `*PASSPHRASE*`, … sont retirées des processus enfants.
- Avec `/img`, l'image (base64) **part au provider** : elle n'est pas écrite dans
  l'historique, mais elle quitte la machine — ne l'utilise pas sur du contenu sensible.

Détail complet du modèle de menace : [`SAFETY.md`](./SAFETY.md).

---

## 4. Fonctionnalités

| Domaine | Détail |
|---|---|
| **TUI** | banner `ZER0│GH0ST` (`ghost in the shell`) animé, statut, **palette `/` filtrante**, **sélection navigable** `/model` et `/provider`, **molette** (conversation / panneaux), raisonnement affiché, **panneau latéral**, **saisie style Pi**, **historique ↑↓** |
| **Boucle d'agent** | appels d'outils, streaming SSE, max 8 itérations, budget 24 outils/tour, détection de boucle |
| **Containment** | réponse coupée à 1 Mo, timeout d'inactivité 120 s, `/stop` |
| **Multi-provider** | 10 providers intégrés + personnalisés, **transport automatique** via `zer0-gw` (clés hors de l'agent) ou direct, failover |
| **Résilience** | bascule d'endpoints, backoff 429, blacklist d'échecs 120 s, messages par code HTTP |
| **Gratuit** | découverte des modèles `:free` (OpenRouter public), `select` par **benchmarks réels** |
| **Rôles** | `main/code/reasoning/writing/vision/fast/utility/review`, sélection auto par catégorie · `/role off` = un seul modèle, sans bascule |
| **Mémoire** | store JSONL, recherche sémantique optionnelle, **enveloppe d'honnêteté** |
| **Connaissance** | base **SQLite + FTS5** (hybride), **injection auto** dans le prompt (+ outil `knowledge_search`) |
| **Zero-chan** | assistante vocale **intégrée à la TUI** : bloc d'état à droite du banner + boutons `0chan`/`micro` à droite de la saisie. **Réutilise le chat et l'input de l'agent**. STT = Whisper local. |
| **Menu cliquable** | panneau **Commandes** à droite (entre zero-chan et le badge 0chan/mic) : tous les groupes/sous-commandes **cliquables** (dépliage, exécution, ou insertion dans la saisie). Molette pour défiler. |
| **Sorties d'outils** | repliées à **3 lignes**, dépliage **par bloc** (clic droit) ou `Ctrl+O` (dernier) ; appels d'outils résumés sur une ligne (fini le script entier dans le fil) |
| **Vision** | `/img <fichier>` (TUI) et `--img` (CLI) : l'image part en **content-parts** vers un modèle multimodal ; **jamais persistée** dans l'historique |
| **Mise à jour** | `zer0 update` / `/update` : binaire brut + `SHA256SUMS` depuis les Releases GitHub, intégrité vérifiée, remplacement atomique |
| **Telegram** | bot à distance : `/telegram on\|off` dans la TUI ; chat, **vocal** (Whisper), **fichiers**, `/progress`, `/exec on\|off`, `/shutdown`. **Long polling** (aucune exposition réseau). |
| **Historique** | **conversation persistée** (JSONL) ; au démarrage = **nouvelle conversation** (l'ancienne est archivée, `--resume` pour reprendre) ; `/history`, `/new` ; **F6** liste/recharge les sessions |
| **Sélection** | clic + glisser dans le chat/saisie → surbrillance + **copie presse-papier** auto |
| **Contexte** | compaction automatique (~4000 tokens) via le rôle `utility` |
| **Tâches** | liste Markdown `.zer0/tasks.md`, `claim/done`, boucle `/work` par sous-agent ; **porte d'honnêteté** : `done` refusé si un livrable mentionné est absent ou vide |
| **Sous-agents** | `/ask`, `/review`, `/jev` en contexte isolé |
| **Outils** | `code_exec`, `text_editor`, `web_search`, `memory_*`, `task_*` |
| **Divers** | `/vault`, `/pdf`, personas `/whoami`, recherche `/search` |

---

## 5. Installation

Prérequis : **Rust** (cargo) + un **compilateur C** (`cc`) — `rusqlite` est compilé *bundled*,
rien à installer côté SQLite. `llama-server` (llama.cpp) seulement pour le mode **local**.

**Portabilité** : presse-papier via `wl-copy` (Wayland) → `xclip`/`xsel` (X11) → OSC 52 ;
micro via `pw-record` (PipeWire) → `parecord` (PulseAudio) → `arecord` (ALSA).

### Depuis GitHub

> 💡 **Installation automatique** Linux/macOS (dépendances selon la distro —
> Debian/Ubuntu/**Kali**/Fedora/Arch/openSUSE) : `./scripts/install.sh`
> (ou `--minimal` pour compiler seulement). Sous Windows, voir juste en dessous.

```bash
git clone https://github.com/Richerrail/zer0gh0st.git
cd zer0gh0st
./scripts/install.sh         # dépendances + build (ou : cargo build --release)
./target/release/zer0        # ou : ./scripts/run.sh
```

### Windows

**Option 1 — binaire précompilé** (le plus simple), depuis [Releases](https://github.com/Richerrail/zer0gh0st/releases/latest) :

- **`.zip`** (recommandé) : clic droit → **Extraire tout** (ou 7-Zip) → `zer0.exe`.
- **`.tar.gz`** (si tu n'as **que 7-Zip**) : **deux extractions** —
  1. clic droit sur `….tar.gz` → **7-Zip → Extraire vers « …\ »** → tu obtiens un `.tar` ;
  2. clic droit sur ce `.tar` → **7-Zip → Extraire ici** → tu obtiens `zer0.exe`.

Puis, dans **PowerShell** (pour voir la sortie) :

```powershell
.\zer0.exe
```

> Windows SmartScreen peut avertir (exe non signé) : *Informations complémentaires →
> Exécuter quand même*. Sur Windows 10+, la commande `tar -xzf …` marche aussi.

**Option 2 — depuis les sources** (PowerShell) :

```powershell
winget install Rustlang.Rustup          # Rust
winget install Git.Git                  # git
# Compilateur C++ requis pour rusqlite :
winget install Microsoft.VisualStudio.2022.BuildTools

git clone https://github.com/Richerrail/zer0gh0st.git
cd zer0gh0st
cargo build --release
.\target\release\zer0.exe
```

> - `install.sh` est un script **bash** : sous Windows, lance-le via **Git Bash**
>   (`bash scripts/install.sh`) ou **WSL**.
> - `code_exec` utilise **`cmd /C`** : installe **Git Bash** ou **WSL** pour disposer
>   des commandes bash.
> - Optionnel (voix/média) : `choco install sox ffmpeg` (Chocolatey) ou installe-les séparément.

### Binaires précompilés

À chaque tag `v*`, la CI publie des binaires **Linux (x86_64)**, **macOS (Intel + Apple
Silicon)** et **Windows (x86_64)** dans [Releases](https://github.com/Richerrail/zer0gh0st/releases).

### Plateformes

| OS | Prérequis | Notes |
|---|---|---|
| 🐧 **Linux** | `build-essential pkg-config` · audio `pipewire`/`pulseaudio-utils` · `wl-clipboard`\|`xclip` · `mpv`/`ffmpeg` | cible principale (testé) |
| 🍎 **macOS** | Xcode CLT · `brew install sox mpv ffmpeg` | via `pbcopy`, micro via `sox` |
| 🪟 **Windows** | **Visual Studio Build Tools (C++)** · `choco install sox ffmpeg` | `code_exec` = `cmd /C` → Git Bash/WSL conseillé |

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
zer0 provider add openrouter sk-or-...     # si zer0-gw tourne : déposée sur la passerelle, jamais ici
zer0 provider use openrouter
zer0 models --free                         # liste les modèles gratuits
zer0 select "Écris une fonction Rust"      # recommande un modèle :free
```

Voir [`SPEC.md`](./SPEC.md) §1 pour le périmètre exact.

---

### Mise à jour

```bash
zer0 update                 # télécharge et remplace le binaire (Releases GitHub)
zer0 update --check         # vérifie sans installer
zer0 update --force         # réinstalle même si à jour
./scripts/update.sh         # (depuis les sources) installe/rafraîchit ~/.local/bin/zer0gh0st
```

Le binaire est choisi selon la plateforme, le **SHA256 est vérifié** quand la release
publie `SHA256SUMS` (sinon c'est dit explicitement), et le remplacement est **atomique**.
Limites assumées : API GitHub non authentifiée (60 req/h), checksum issu du même dépôt
(protège d'une corruption, pas d'un dépôt compromis), et Windows ne peut pas écraser un
`.exe` en cours d'exécution (l'ancien est renommé puis supprimé). Si le binaire tourne
depuis `target/` (build Cargo), `zer0 update` **refuse** de le remplacer : pour un dev,
c'est `cargo build --release` qui fait foi (`--force` pour passer outre).

### Désinstaller

| Installation | Suppression |
|---|---|
| **Depuis les sources** | supprime le dossier cloné : `rm -rf zer0gh0st` |
| **Binaire précompilé** (Releases) | supprime le fichier : `rm zer0` (`zer0.exe` sous Windows) |
| **`cargo install --path .`** | `cargo uninstall zer0` |

**Données & configuration** — pour effacer aussi mémoire, historique, clés, base de connaissance :

| OS | À supprimer |
|---|---|
| 🐧 Linux | `rm -rf ~/.config/zer0 ~/.local/share/zer0` |
| 🍎 macOS | `rm -rf ~/Library/Application\ Support/zer0` |
| 🪟 Windows | `%APPDATA%\zer0` et `%LOCALAPPDATA%\zer0` |
| 🐧🍎🪟 `cargo install` | le binaire est dans `~/.cargo/bin/zer0` (`cargo uninstall zer0`) |

> `zer0` (ou `zer0.exe`) est un **binaire autonome** : le supprimer ne laisse rien derrière,
> sauf les dossiers de données ci-dessus.

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
| **taper `/`** | palette de commandes (filtre en tapant, ↑↓, Entrée) |
| `PgUp` `PgDn` · **molette** | faire défiler la conversation |
| `Ctrl+O` · **clic droit** | déplier/replier une sortie d'outil |
| `F4` | activer/désactiver `code_exec` |
| `F6` | gérer les conversations (sessions sauvegardées) |
| `F7` | relancer/poursuivre l'agent (nudge) |
| `F8` / `F10` | zero-chan (assistante) / micro |
| `Ctrl+C` / `/stop` | interrompre la génération |
| souris | clic + glisser → sélection, relâcher → copie ; **clic droit** = déplier un bloc |

---

## 7. Commandes

**TUI** (extrait) : `/help` `/think` `/provider` `/model` `/models` `/quota` `/ctx` `/free` `/select` `/route` `/telegram` `/tg`
`/role` `/project` `/memory` `/recall` `/compact` `/task` `/work` `/ask` `/review` `/search`
`/stop` `/context` `/whoami` `/vault` `/pdf` `/jev` `/kb` `/new` `/history` `/radio` `/bintime`
`/pong` `/kristal` `/streaming` `/retro` `/panel` `/clear` `/quit` `/img` `/update` `/embeddings`.

**CLI** : `zer0 provider|models|model|select|route|role|project|memory|embeddings|task|search|whoami|vault|pdf|jev|bintime|pong|kb|update`.

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
| `~/.config/zer0/config.toml` | **réglages** (provider/modèle, routes, rôles) — **aucun secret** (chmod 600) — modèle : [`config.example.toml`](./config.example.toml) |
| `~/.config/zer0/telegram.env` | token du bot Telegram (chmod 600) |
| `~/.config/zer0/vault.enc` | coffre **optionnel** de clés (Argon2 + ChaCha20-Poly1305) |
| `/etc/zer0-gw/gw.toml` | **passerelle** : upstreams + chemins autorisés (user `zer0-gw`) |
| `/etc/zer0-gw/keys.managed` | clés déposées via `/provider add` (0600, jamais lisibles par l'agent) |
| `/etc/zer0-gw/keys.env` | clés « style variable d'env » (injectées par systemd) |
| `<projet>/data/memory.jsonl` | mémoire persistante |
| `<projet>/data/conversation.jsonl` | historique de conversation (rechargé au démarrage) |
| `<projet>/data/personas/` | personas (`/whoami`) |
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

## 14. Crédits & remerciements

**Inspirations**
- **Agent Zero** (Python) — architecture d'agent et d'outils.
- **Spynel** — approche modulaire.
- **pi** — [pi.dev](https://pi.dev) — référence d'ergonomie pour la CLI/TUI.

**🙏 Remerciements appuyés**
- Un **immense merci à [kioptix](https://github.com/kioptix)**, un **ami**, pour
  **Synnoesis**. C'est de sa **philosophie d'honnêteté** que Zer0 s'inspire directement.
  **Soutenez son travail :**
  [github.com/kioptix/synnoesis](https://github.com/kioptix/synnoesis) ·
  [synnoesis.dev](https://synnoesis.dev)
  — **fork maintenu par nous :** [github.com/Richerrail/synnoesis](https://github.com/Richerrail/synnoesis)

**Divers**
- site : https://zer0gh0st.pages.dev
- Nom : référence à ***Ghost in the Shell***.
- Inférence : [llama.cpp](https://github.com/ggerganov/llama.cpp) · TUI : [ratatui](https://github.com/ratatui/ratatui).

---

## Soutenez le projet

<p align="center">
  <a href="https://www.buymeacoffee.com/richerrailk" target="_blank" rel="noopener noreferrer">
    <img src="https://cdn.buymeacoffee.com/buttons/v2/default-yellow.png" alt="Buy Me A Coffee">
  </a>
</p>

---

## 16. Licence

**MIT** — usage libre, modification encouragée. Copyright (c) 2026 richerrail.
Voir [`LICENSE`](./LICENSE).
