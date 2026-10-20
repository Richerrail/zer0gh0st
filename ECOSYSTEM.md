# Zer0 — ECOSYSTEM.md

> Délimite le **noyau** (l'agent) des **outils intégrés** et de l'**écosystème** (modules fun),
> et documente la portabilité. Complète le README.

**Version** : 1.1 (2026-10-11) · **Projet** : 0.5.5

---

## 1. Découpage

| Altitude | Contenu |
|---|---|
| **Noyau** | boucle d'agent, TUI, providers, mémoire, tâches, sous-agents, config, coffre |
| **Outils intégrés** | `code_exec`, `text_editor`, `web_search`, `memory_*`, `task_*`, `/pdf`, `/search`, `/vault` |
| **Passerelle** | binaire `zer0-gw` séparé (3 modes de sécurité) — hors noyau de la TUI |
| **Écosystème** | modules `mods/` (audio, horloge, jeu, vidéo, jeux rétro) — **hors noyau** |

Les modules de l'écosystème sont **autonomes** : tous les programmes sont **copiés dans
`ZER0v1/mods/`** (aucun lien vers un dossier externe).

---

## 2. Modules (`mods/`)

| Commande | Source | Techno | Mode |
|---|---|---|---|
| `/radio` | Rust (`src/fun/radio.rs`) | `ffplay`/`mpv`/`paplay` | panneau latéral |
| `/bintime` | Rust (`src/fun/bintime.rs`) | `date` (horloge) | panneau latéral |
| `/pong` | Rust (`src/fun/pong.rs`) | — | plein écran |
| `/kristal` | `mods/zer0-kristal/` | C++ / Qt6 | fenêtre séparée |
| `/streaming` | `mods/streaming/zero_video.py` | Python / libtorrent / mpv / pygame | fenêtre séparée |
| `/retro` | `mods/Zer0-Retr0/` | HTML / JS (navigateur) | fenêtre séparée |
| `/bintime` (référence) | `mods/bintime/zero_bintime.py` | Python / pygame | non lancé |

Détails et historique : [`ROADMAP_FUN.md`](./ROADMAP_FUN.md).

### `mods.rs` — lanceurs
Résout la racine projet depuis l'exécutable (`target/release/zer0` → `ZER0v1`) puis lance :
- `/kristal` → `mods/zer0-kristal/zer0-kristal`
- `/streaming` → `python3 mods/streaming/zero_video.py`
- `/retro` → ouvre `mods/Zer0-Retr0/index.html` (via le navigateur par défaut, multiplateforme)

---

## 3. Dépendances runtime (externes)

| Outil | Pour |
|---|---|
| `llama-server` (llama.cpp) | inférence locale |
| `ffplay` / `mpv` / `paplay` | `/radio` |
| `ffmpeg` / `yt-dlp` | `/kristal`, `/streaming` |
| `python3` | `/streaming` (+ libtorrent, pygame) |
| `xdg-open` (+ navigateur) | `/retro` |
| Qt6 (`Qt6Widgets`, `Qt6Multimedia`) | compilation de `mods/zer0-kristal` |
| `taskset` (Linux) | limite 2 cœurs du serveur local |
| `sox` | enregistrement micro (macOS/Windows, et Linux sans PipeWire/Pulse) |
| `pbcopy` / `clip` | presse-papier macOS / Windows |

---

## 4. Dossier audio

`/radio` lit `$ZER0_MUSIC_DIR`, sinon **`<ZER0v1>/music`** (livré vide, à remplir).
Formats : `mp3 wav ogg flac m4a opus aac wma`. Lecture en **playlist** (enchaînement auto).

---

## 5. Portabilité

**État : Linux, macOS et Windows — compilés et vérifiés par la CI**
(`.github/workflows/ci.yml`, matrice 3 OS). Des **binaires précompilés** sont publiés à chaque
`tag` (`release.yml`). Tout passe par `src/platform.rs`.

| | Linux | macOS | Windows |
|---|---|---|---|
| Noyau Rust (TUI, client LLM, mémoire, coffre, PDF, Telegram) | ✅ | ✅ | ✅ |
| `code_exec` | `bash -lc` | `bash -lc` | **`cmd /C`** |
| Presse-papier | `wl-copy`/`xclip`/`xsel` | **`pbcopy`** | **`clip`** |
| Ouvrir un fichier | `xdg-open`/`gio` | `open` | `cmd start` |
| Python | `python3`/`python` | `python3` | `python` |
| Horodatage (`/bintime`, logs) | `date` | `date` | **PowerShell** |
| Micro (zero-chan) | pw-record / parecord / arecord | **sox** | **sox** |
| `/radio` | ✅ | ✅ (ffplay/mpv) | ✅ (ffplay/mpv) |
| `/kristal` | ✅ (Qt6) | ⚠️ recompiler Qt6 | ⚠️ recompiler Qt6 |
| `/streaming` | ✅ | ⚠️ libtorrent/pygame | ⚠️ libtorrent/pygame |
| `/retro` | ✅ | ✅ (`open`) | ✅ (`start`) |
| `scripts/serve.sh` (`taskset`) | ✅ | ❌ (llama-server direct) | ❌ |

### Prérequis par OS
- **Linux (Debian/Ubuntu/Kali)** : `build-essential pkg-config` ; audio
  `pipewire` / `pulseaudio-utils` / `alsa-utils` ; `wl-clipboard` **ou** `xclip` ; `mpv`/`ffmpeg`.
- **macOS** : Xcode Command Line Tools (`xcode-select --install`) ; `brew install sox mpv ffmpeg`.
- **Windows** : **Visual Studio Build Tools (C++)** ; `choco install sox ffmpeg`.

### Passerelle `zer0-gw`
Le binaire `zer0-gw` est **autonome** (hors noyau) : il fait transiter le trafic vers un dossier
distant en mode **standard**, **anonyme** ou **zero-knowledge** (chiffrement de bout en bout,
passphrase exigée à chaque lancement). Il n'est **pas** prévu pour être lancé par la TUI elle-même
— c'est un processus à part, lancé sur le dossier distant.

### Limites connues (Windows)
- `code_exec` utilise **`cmd /C`** : installe **Git Bash** ou **WSL** pour les commandes bash.
- **Pas de `taskset`** → lance `llama-server` sans limite de cœurs.
- Les modules `mods/` (Qt6, torrent) ne sont **pas prévus** pour Windows.

### Installation automatique
`./scripts/install.sh` détecte la distribution Linux (Debian/Ubuntu/Kali, Fedora, Arch,
openSUSE) et installe les dépendances ; `--minimal` ne prend que le nécessaire à la compilation.

---

## 6. Modules retirés (historique)

Tentés puis **rejetés** (hors périmètre, rendu insuffisant ou redondant) : premières versions
de bintime (BCD), Snake, analyseur FFT `kristal`, assembleur Zer0-asm, enregistreur d'écran,
streaming minimal. Voir [`ROADMAP_FUN.md`](./ROADMAP_FUN.md) pour le détail et les raisons.
