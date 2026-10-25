# Zer0 — ECOSYSTEM.md

> Délimite le **noyau** (l'agent) des **outils intégrés** et de l'**écosystème** (modules fun),
> et documente la portabilité. Complète le README.

**Version** : 1.1 (2026-10-11) · **Projet** : 0.6.9

---

## 1. Découpage

| Altitude | Contenu |
|---|---|
| **Noyau** | boucle d'agent, TUI, providers, mémoire, tâches, sous-agents, config, coffre |
| **Outils intégrés** | `code_exec`, `text_editor`, `web_search`, `memory_*`, `task_*`, `/pdf`, `/search`, `/vault` |
| **Passerelle** | binaire `zer0-gw` séparé (3 modes de sécurité) — hors noyau de la TUI |

---

## 2. Modules (noyau)

Tous les modules restants vivent dans `src/fun/` : aucun programme externe, aucune
dépendance Python pour le noyau.

| Commande | Source | Techno | Mode |
|---|---|---|---|
| `/radio` | Rust (`src/fun/radio.rs`) | `ffplay`/`mpv`/`paplay` | panneau latéral |
| `/bintime` | Rust (`src/fun/bintime.rs`) | `date` (horloge) | panneau latéral |
| `/pong` | Rust (`src/fun/pong.rs`) | — | plein écran |

`src/mods.rs` ne sert plus qu'à résoudre la racine du projet (surchargeable par
`ZERO_ROOT`) pour localiser `base_connaissance/` et migrer les anciennes données.

---

## 3. Dépendances runtime (externes)

| Outil | Pour |
|---|---|
| `llama-server` (llama.cpp) | inférence locale |
| `ffplay` / `mpv` / `paplay` | `/radio` |
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
de bintime (BCD), Snake, assembleur Zer0-asm, enregistreur d'écran,
streaming minimal, modules `mods/` (kristal, streaming, retro) dont les sources n'étaient pas
versionnées.
