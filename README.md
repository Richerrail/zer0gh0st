# ZER0│GH0ST

> **Un agent IA qui vit dans ton terminal.** Rust · TUI · local ou cloud · zéro dépendance Python pour le noyau.

<img src="./logo.png" alt="ZER0│GH0ST" width="520">

![ZER0│GH0ST](./zer0-ghost.png)

![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)
![Rust](https://img.shields.io/badge/rust-edition%202021-orange.svg)
![Platform](https://img.shields.io/badge/platform-Linux%20%7C%20macOS%20%7C%20Windows-lightgrey.svg)
![CLI only](https://img.shields.io/badge/interface-CLI%20%2F%20TUI-black.svg)
![CI](https://github.com/Richerrail/zer0gh0st/actions/workflows/ci.yml/badge.svg)

**Version** : 0.7.12 · **Licence** : MIT · **Interface** : terminal uniquement (pas de GUI web).

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
4bis. [Windows et macOS](#4bis-windows-et-macos)
5. [Installation](#5-installation)
6. [Usage & exemples](#6-usage--exemples)
6bis. [Suggestions d'amélioration](#6bis-suggestions-damélioration)
7. [Commandes](#7-commandes)
8. [Zero-chan (voix)](#8-zero-chan-voix)
9. [Vote (👍 / 👎)](#8bis-vote-👍-)
10. [Configuration](#9-configuration)
11. [Modules fun](#10-modules-fun)
12. [Limites connues](#11-limites-connues)
13. [Documentation](#12-documentation)
14. [Contribuer](#13-contribuer)
15. [Crédits](#14-crédits)
16. [Soutenez le projet](#soutenez-le-projet)
17. [Licence](#16-licence)

---

## 1. Démarrage rapide

```bash
git clone https://github.com/Richerrail/zer0gh0st.git && cd zer0gh0st
./scripts/install.sh           # dépendances + build (ou : cargo build --release)
./target/release/zer0          # ou : ./scripts/run.sh
```

Mode **100 % local** : chat rapide, **sans outils** (aucune clé API).

```bash
# 1. dépose un GGUF (format llama.cpp, template de chat inclus) ici :
#    ~/ZER0v1/models/ollama_models/mon-modele.gguf
# 2. dans la TUI :
/provider use local      # llama-server démarre tout seul (port 8080)
/model                   # menu : tous les .gguf du dossier sont listés
```

Le local est volontairement un **chat conversationnel** : aucun outil n'est
envoyé au modèle, pas de base de connaissances, pas de prompt d'agent. C'est ce
qui le rend rapide sur une petite machine. L'agent outillé (`code_exec`,
fichiers, recherche, sous-agents) reste sur le **cloud** ; `/role`, `/route`,
`0box`, `/select` et les embeddings ne s'appliquent pas en local (rappelé au
démarrage).

Réglages par défaut du serveur local :

| Réglage | Valeur | Variable |
|---|---|---|
| Threads | **tous les cœurs** détectés | `ZER0_THREADS` |
| Contexte | 2048 | `ZER0_CTX` |
| Raisonnement | coupé (`--reasoning-budget 0`) | — |
| Streaming | activé (`/stream off` pour le couper) | config |

**Garde-fou thermique** (Linux) : la génération est interrompue à
`ZER0_TEMP_STOP` (88 °C) et `llama-server` est arrêté à `ZER0_TEMP_KILL`
(92 °C), avec un avertissement à `ZER0_TEMP_WARN` (80 °C). Sur macOS/Windows,
les capteurs ne sont pas exposés de façon portable : le garde-fou est inactif.

Le modèle doit contenir son **template de chat** dans le GGUF : un blob extrait
d'ollama ne suffit pas (ollama garde le template et le parseur d'outils dans son
manifeste). Les fichiers `mmproj` sont ignorés (projecteurs multimodaux).

Deux prompts système distincts sont maintenus : un **court** pour le local
(`/system local <texte>`, défaut adapté au CPU) et celui du cloud
(`/system cloud <texte>`). `… reset` revient au défaut. Le silence maximal entre
deux tokens est de 240 s (`ZER0_STREAM_IDLE_SECS`) ; en non-streaming le délai
total est de 600 s (`ZER0_HTTP_TIMEOUT_SECS`).

`scripts/serve.sh` reste disponible pour lancer le serveur à la main.

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
| **Containment** | réponse coupée à 1 Mo, timeout d'inactivité 240 s, `/stop` |
| **Multi-provider** | 10 providers intégrés + personnalisés, **transport automatique** via `zer0-gw` (clés hors de l'agent) ou direct, failover |
| **Résilience** | bascule d'endpoints, backoff 429, blacklist d'échecs 120 s, messages par code HTTP |

`code_exec` a un timeout par défaut de 300 s (max 3600), réglable globalement
par `ZER0_EXEC_TIMEOUT` ou par appel (`timeout_secs`).
| **Gratuit** | découverte des modèles `:free` (OpenRouter public), `select` par **benchmarks réels** |
| **Rôles** | `main/code/reasoning/writing/vision/fast/utility/review`, sélection auto par catégorie · `/role off` = un seul modèle, sans bascule |
| **Mémoire** | store JSONL, recherche sémantique optionnelle, **enveloppe d'honnêteté** |
| **Connaissance** | base **SQLite + FTS5** (hybride), **injection auto** dans le prompt (+ outil `knowledge_search`) |
| **Catalogue d'APIs** | `/apis <besoin>` — 2 000+ APIs publiques (source community, **datée**), avec colonne « clé requise » et filtre `keyless` |
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
| **Outils** | 18 outils : `code_exec`, `text_editor`, `read_image` (OCR), `web_search`, `web_read`, `memory_*`, `knowledge_*`, `task_*`, `delegate`, `0box` |
| **Local** | chat rapide **sans outils** (prompt d'une phrase, contexte 2048, `--no-warmup`, `-np 1`), GGUF scannés, serveur préchargé en 3 s |
| **GPU** | `-ngl` automatique : CUDA/Vulkan/ROCm via `llama-server --list-devices`, **Metal** sur macOS ; `ZER0_GPU_LAYERS=auto\|0\|N` |
| **Thermique** | Linux : alerte 80 °C, génération coupée à 88 °C, serveur arrêté à 92 °C (`ZER0_TEMP_*`) |
| **Moniteur** | mod `t0p` dans le panneau latéral (comme radio/bintime) : CPU (global + cœurs), RAM/swap, charge, températures, top 8 processus. Aussi `zer0 t0p [--watch]` en CLI |
| **Vote** | **👍 / 👎 en bas à droite de l'écran principal** (cliquables) : le pouce haut = **une star GitHub** sur [Richerrail/zer0gh0st](https://github.com/Richerrail/zer0gh0st), le bas retire la star. `/vote` ou `zer0 vote up\|down\|status\|login\|logout`. Compteur réel lu sur l'API GitHub. Connexion GitHub par device flow OAuth (`vote login`) → votes ensuite automatiques. |
| **Divers** | `/vault`, `/pdf`, personas `/whoami`, recherche `/search` |

---

## 4bis. Windows et macOS

Les trois plateformes compilent en CI (Ubuntu, macOS, Windows). Points
spécifiques :

| Sujet | Windows | macOS |
|---|---|---|
| `code_exec` | bash (Git Bash) si présent, sinon PowerShell, sinon cmd — forçable par `ZER0_SHELL=bash\|powershell\|cmd` | `bash -lc` |
| GPU local | `ZER0_GPU_LAYERS=auto` interroge `llama-server --list-devices` (CUDA/Vulkan/ROCm) | Metal par défaut (`-ngl 999`) |
| Garde-fou thermique | inactif (pas de capteurs portables) | inactif |
| OCR (`read_image`) | langues via `tesseract --list-langs` | idem (Homebrew) |
| Serveur local | `llama-server.exe` dans le `PATH` ou `ZER0_LLAMA_SERVER` | idem |

`ZER0_GPU_LAYERS=0` force le CPU, `=N` fixe le nombre de couches. Sans GPU
détecté, le message de démarrage dit « CPU seul » au lieu de le supposer.

## 5. Installation

Prérequis : **Rust** (cargo) + un **compilateur C** (`cc`) — `rusqlite` est compilé *bundled*,
rien à installer côté SQLite. `llama-server` (llama.cpp) seulement pour le mode **local**.

**Portabilité** : presse-papier via `wl-copy` (Wayland) → `xclip`/`xsel` (X11) → OSC 52 ;
micro via `pw-record` (PipeWire) → `parecord` (PulseAudio) → `arecord` (ALSA) → `sox_ng`/`sox`
(macOS et Windows).

### Depuis GitHub

> 💡 **Installation automatique** Linux/macOS (dépendances selon la distro —
> Debian/Ubuntu/**Kali**/Fedora/Arch/openSUSE) : `./scripts/install.sh`.
> Sous Windows : `.\scripts\install.cmd`. Les deux installent aussi les
> prérequis **Zero-chan** (micro + Whisper local), sauf en mode minimal
> (`--minimal` sous Unix, `-Minimal` sous Windows).

```bash
git clone https://github.com/Richerrail/zer0gh0st.git
cd zer0gh0st
./scripts/install.sh         # dépendances + Zero-chan + build (ou : cargo build --release)
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

**Option 2 — depuis les sources, installation automatique** (PowerShell) :

```powershell
winget install Git.Git                  # requis pour cloner le dépôt
git clone https://github.com/Richerrail/zer0gh0st.git
cd zer0gh0st
.\scripts\install.cmd
```

`install.cmd` installe et vérifie :

- Rust/Cargo, Git et Visual Studio Build Tools (compilateur C++ de `rusqlite`) ;
- FFmpeg et **SoX_ng** pour l'enregistrement du micro Windows ;
- Python 3.12 dans un venv `~\.venvs\zer0-whisper` ;
- `faster-whisper` + une version compatible de PyAV (`av<16`) ;
- la variable utilisateur `ZER0_PYTHON`, puis le binaire release.

Options utiles :

```powershell
.\scripts\install.cmd -VoiceOnly  # uniquement micro + Whisper (installation déjà faite)
.\scripts\install.cmd -Minimal    # outils de compilation seulement, sans voix/média
.\scripts\install.cmd -SkipBuild  # installe les dépendances sans compiler
```

Si l'automatisation n'est pas Possible, équivalent manuel :

```powershell
winget install Rustlang.Rustup
winget install Microsoft.VisualStudio.2022.BuildTools
winget install Gyan.FFmpeg
winget install sox_ng.sox_ng
winget install Python.Python.3.12
py -3.12 -m venv "$env:USERPROFILE\.venvs\zer0-whisper"
& "$env:USERPROFILE\.venvs\zer0-whisper\Scripts\python.exe" -m pip install "faster-whisper" "av<16"
cargo build --release
.\target\release\zer0.exe
```

> - `install.sh` est un script **bash** : sous Windows, lance-le via **Git Bash**
>   (`bash scripts/install.sh`) ou **WSL**.
> - `code_exec` utilise **`cmd /C`** : installe **Git Bash** ou **WSL** pour disposer
>   des commandes bash.
> - Installation **manuelle** de la voix (si `install.cmd` n'est pas utilisé) :
>   ```powershell
>   winget install Gyan.FFmpeg
>   winget install sox_ng.sox_ng
>   winget install Python.Python.3.12
>   py -3.12 -m venv "$env:USERPROFILE\.venvs\zer0-whisper"
>   & "$env:USERPROFILE\.venvs\zer0-whisper\Scripts\python.exe" -m pip install "faster-whisper" "av<16"
>   [Environment]::SetEnvironmentVariable("ZER0_PYTHON", "$env:USERPROFILE\.venvs\zer0-whisper\Scripts\python.exe", "User")
>   ```
>   Redémarre ensuite le terminal. Zer0 détecte aussi automatiquement le venv
>   `~/.venvs/zer0-whisper`, même si `ZER0_PYTHON` n'est pas encore exporté.

### Binaires précompilés

À chaque tag `v*`, la CI publie des binaires **Linux (x86_64)**, **macOS (Intel + Apple
Silicon)** et **Windows (x86_64)** dans [Releases](https://github.com/Richerrail/zer0gh0st/releases).

### Plateformes

| OS | Prérequis | Notes |
|---|---|---|
| 🐧 **Linux** | `build-essential pkg-config` · audio `pipewire`/`pulseaudio-utils` · `wl-clipboard`\|`xclip` · `mpv`/`ffmpeg` | cible principale (testé) |
| 🍎 **macOS** | Xcode CLT · `brew install sox mpv ffmpeg` | via `pbcopy`, micro via `sox` |
| 🪟 **Windows** | **Visual Studio Build Tools (C++)** · `winget install Gyan.FFmpeg` + `winget install sox_ng.sox_ng` | micro via `sox_ng` ; `code_exec` = bash (Git Bash) → PowerShell → cmd, surcharge `ZER0_SHELL` |

### Mode 100 % local (aucune clé API)

Le local est un **chat conversationnel** (pas d'outils) : c'est ce qui le rend
utilisable sur une petite machine. Voir le détail en
[§1 Démarrage rapide](#1-démarrage-rapide).

```bash
zer0gh0st                 # précharge llama-server en arrière-plan
# puis, dans la TUI :
/provider use local
/model                    # choisit un .gguf du dossier models/
```

Réglages : tous les cœurs détectés (génération), moitié en préfill, contexte
2048, raisonnement coupé, GPU automatique. Surcharges : `ZER0_THREADS`,
`ZER0_THREADS_BATCH`, `ZER0_CTX`, `ZER0_GPU_LAYERS`, `ZER0_TEMP_STOP`.

`scripts/serve.sh` reste disponible pour lancer le serveur à la main.

### Moniteur système

```bash
zer0 t0p              # un relevé
zer0 t0p --watch      # rafraîchi toutes les secondes (Ctrl+C pour sortir)
```

Dans la TUI : `/t0p` ouvre le **panneau latéral** (même place que `/radio` et `/bintime`) ; `/panel` le ferme, ou `Esc` une fois le panneau focus (Tab). Températures : Linux uniquement (ailleurs, « indisponible »).

### Providers cloud (clé optionnelle)

```bash
zer0 provider add openrouter sk-or-...     # si zer0-gw tourne : déposée sur la passerelle, jamais ici
zer0 provider use openrouter
zer0 models --free                         # liste les modèles gratuits
zer0 select "Écris une fonction Rust"      # recommande un modèle :free
```

---

### Catalogue d'APIs publiques

Le dépôt [public-apis/public-apis](https://github.com/public-apis/public-apis) (MIT) recense
2 000+ APIs gratuites. Un script l'importe dans une **table structurée** (recherche par
nom/description/catégorie, colonne « clé requise ») :

```bash
./scripts/kb-import-public-apis.sh          # importe (réf épinglée, date conservée)
./scripts/kb-import-public-apis.sh --kb     # indexe aussi pour l'agent (knowledge_search)
```

Puis, dans la TUI :

```
/apis                    → état : nb d'APIs, source, date d'import, catégories
/apis météo gratuit      → les APIs correspondantes
/apis crypto keyless     → seulement celles qui marchent SANS clé
```

CLI : `zer0 apis search "weather" --keyless`, `zer0 apis categories`, `zer0 apis stats`.

> **Honnêteté** : c'est une **liste communautaire de liens**, un *candidat* — pas une
> garantie. Beaucoup d'APIs sont mortes ; la source et la date d'import sont affichées
> avec chaque résultat, et le rappel « à vérifier » est explicite. Le catalogue n'est
> **pas** versionné dans le dépôt (il est réimporté quand on veut) → jamais périmé.

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
| 🐧 Linux | `rm -rf ~/.config/zer0 ~/.local/share/zer0gh0st` |
| 🍎 macOS | `rm -rf ~/Library/Application\ Support/zer0 ~/Library/Application\ Support/zer0gh0st` |
| 🪟 Windows | `%APPDATA%\zer0` et `%LOCALAPPDATA%\zer0gh0st` |
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

## 6bis. Suggestions d'amélioration

Idées **non implémentées** (TTS/M8, vision multimodale, etc.), avec leurs
contraintes : [docs/IMPROVEMENTS.md](docs/IMPROVEMENTS.md).

## 7. Commandes

**TUI** (extrait) : `/help` `/think` `/provider` `/model` `/models` `/quota` `/vote` `/ctx` `/free` `/select` `/route` `/telegram` `/tg`
`/role` `/project` `/memory` `/recall` `/compact` `/task` `/work` `/ask` `/review` `/search`
`/stop` `/context` `/whoami` `/vault` `/pdf` `/jev` `/kb` `/new` `/history` `/radio` `/bintime`
`/pong` `/panel` `/clear` `/quit` `/img` `/update` `/embeddings`.

**CLI** : `zer0 provider|models|model|select|route|role|project|memory|embeddings|task|search|whoami|vault|pdf|jev|bintime|pong|kb|update|quota|vote|0box` — alias débat : `zer0box "<sujet>"` (binaire-lien ou `scripts/zer0box.cmd` depuis les sources).

Détails : `zer0 --help`, ou [`docs/COMMANDS.md`](./docs/COMMANDS.md).

---

## 8. Zero-chan (voix)

Assistante vocale intégrée, **sans boîte chat ni barre dédiée** :

- **Bloc d'état** en haut à droite du banner (visible quand activé) : humeur, état
  (`en veille` / `écoute…` / `réfléchit…` / `parle…`), mini-visualiseur animé.
- **Deux boutons** à droite de la barre de saisie : `0chan:ON/off` et `MIC:REC/off`
  — cliquables à la souris, ou **`F8`** (0chan) / **`F10`** (micro).
- Le **micro** enregistre en WAV mono 16 kHz (`pw-record`, `parecord`, `arecord`,
  `sox_ng`/`sox`) puis transcrit (`faster-whisper`) ; le texte reconnu est envoyé
  **dans la chat de l'agent**, comme un message tapé. Sous Windows, `sox_ng` est
  recommandé : l'ancien SoX 14.4 ne sait pas toujours ouvrir le micro par défaut.
- L'avatar et la voix ne dupliquent **pas** l'UI : ils réutilisent chat + input de l'agent.

Roadmap et détails : [`BRAINSTORM_ZEROCHAN.md`](./BRAINSTORM_ZEROCHAN.md).

---

## 8bis. Vote (👍 / 👎)

Deux petits pouces **en bas à droite de l'écran principal** (dans le pied de page),
cliquables à la souris :

- **👍 (upvote) = une star GitHub** sur [Richerrail/zer0gh0st](https://github.com/Richerrail/zer0gh0st).
- **👎 (downvote) = le retrait de cette star.**
- Le nombre affiché est le **compteur réel** lu sur l'API GitHub (`stargazers/count`),
  pas un chiffre décoratif. S'il est inconnu, c'est `?`.

Deux modes, selon le jeton disponible :

| Jeton | Effet du 👍 |
|---|---|
| `ZER0_GITHUB_TOKEN` / `GITHUB_TOKEN` / `GH_TOKEN`, ou clé `github` dans la config / le coffre | **star posée via l'API** (`PUT /user/starred/...`) — 1 upvote = 1 star, automatiquement |
| aucun | la **page du dépôt s'ouvre** dans le navigateur : clique sur « ★ Star » |

> Le jeton n'est **jamais stocké** par Zer0 : il est lu dans l'environnement, le coffre
> chiffré ou `config.toml` (comme les autres clés). Le vote est aussi accessible sans
> souris : `/vote up`, `/vote down`, `/vote status`, ou `zer0 vote up|down|status`.

### Se connecter à GitHub (une fois)

Pour que le 👍 pose une vraie star sans intervention, il faut un jeton GitHub.
Zer0 l'obtient via le **device flow OAuth** :

```bash
zer0 vote login        # ou /vote login dans la TUI
# → ouvre https://github.com/login/device : colle le code affiché
```

Au **premier lancement**, zer0 te propose cette connexion dans la fenêtre
principale (et y glisse un rappel discret les fois suivantes). Sans jeton, le 👍
ouvre simplement la page du dépôt : pour une vraie star, connecte ton compte.

Le jeton est rangé **chiffré dans le coffre** (créé automatiquement au premier
`vote login`) : les votes suivants sont automatiques. Depuis la TUI, le coffre
doit déjà exister — le premier `vote login` se fait en CLI (une passphrase ne
saisit pas en mode raw). L'application OAuth **Zer0** est préconfigurée : `zer0 vote login` fonctionne
donc out of the box, sans rien configurer. (Le `client_id` est public — c'est un
identifiant, pas un secret — et le device flow n'a besoin d'aucun
`client_secret`. Pour pointer une autre application :
`ZER0_GITHUB_CLIENT_ID` ou `github_client_id` dans `config.toml`.)

```bash
zer0 vote logout       # retire le jeton du coffre
```

Le jeton est aussi cherché dans l'environnement
(`ZER0_GITHUB_TOKEN` / `GITHUB_TOKEN` / `GH_TOKEN`) et dans `config.toml`
(`[keys] github`) — pratique pour la CI.

---

## 9. Configuration

| Chemin | Rôle |
|---|---|
| `~/.config/zer0/config.toml` | **réglages** (provider/modèle, routes, rôles) — **aucun secret** (chmod 600) — modèle : [`config.example.toml`](./config.example.toml) |
| `~/.config/zer0/telegram.env` | token du bot Telegram (chmod 600) |
| `~/.config/zer0/vault.enc`<br>(Windows : `%APPDATA%\zer0\config\vault.enc`) | coffre **optionnel** de clés (Argon2 + ChaCha20-Poly1305) |
| `/etc/zer0-gw/gw.toml` | **passerelle** : upstreams + chemins autorisés (user `zer0-gw`) |
| `/etc/zer0-gw/keys.managed` | clés déposées via `/provider add` (0600, jamais lisibles par l'agent) |
| `/etc/zer0-gw/keys.env` | clés « style variable d'env » (injectées par systemd) |
| `<projet>/data/memory.jsonl` | mémoire persistante |
| `<projet>/data/conversation.jsonl` | historique de conversation (rechargé au démarrage) |
| `<projet>/data/personas/` | personas (`/whoami`) |
| `<projet>/knowledge.db` | base de connaissance (SQLite + FTS5) |
| `<projet>/base_connaissance/` | sources indexables (texte/markdown/tsv) |
| `<projet>/knowledge.db` | base de connaissance (SQLite + FTS5) **+ table `public_apis`** |
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
| [`SAFETY.md`](./SAFETY.md) | modèle de menace, choix assumés, ce qui **n'est pas** protégé |
| [`ARCHITECTURE.md`](./ARCHITECTURE.md) | structure technique, flux, modules |
| [`ECOSYSTEM.md`](./ECOSYSTEM.md) | modules fun, portabilité Linux/macOS/Windows |
| [`BRAINSTORM_ZEROCHAN.md`](./BRAINSTORM_ZEROCHAN.md) | analyse & roadmap Zero-chan |
| [`CHANGELOG.md`](./CHANGELOG.md) | historique des versions |
| [`docs/COMMANDS.md`](./docs/COMMANDS.md) | référence des commandes, générée depuis le CLI |
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
