# Zer0 — ECOSYSTEM.md

> Délimite le **noyau** (l'agent) des **outils intégrés** et de l'**écosystème** (modules fun),
> et documente la portabilité. Complète le README.

**Version** : 1.0 (2026-10-06) · **Projet** : 0.5

---

## 1. Découpage

| Altitude | Contenu |
|---|---|
| **Noyau** | boucle d'agent, TUI, providers, mémoire, tâches, sous-agents, config, coffre |
| **Outils intégrés** | `code_exec`, `text_editor`, `web_search`, `memory_*`, `task_*`, `/pdf`, `/search`, `/vault` |
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
- `/retro` → `xdg-open mods/Zer0-Retr0/index.html`

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

---

## 4. Dossier audio

`/radio` lit `$ZER0_MUSIC_DIR`, sinon **`<ZER0v1>/music`** (livré vide, à remplir).
Formats : `mp3 wav ogg flac m4a opus aac wma`. Lecture en **playlist** (enchaînement auto).

---

## 5. Portabilité

**État : Linux (testé). macOS et Windows : analyse, non testé.**

| | Linux | macOS | Windows |
|---|---|---|---|
| Noyau Rust (TUI, client LLM, mémoire, coffre, PDF) | ✅ | ✅ | ✅ (unix `chmod` cfg-gardé) |
| `code_exec` (`bash -lc`) | ✅ | ✅ (`/bin/bash`) | ❌ (→ `cmd`/`powershell`) |
| `/bintime` (`date +%H %M %S`) | ✅ | ✅ (date BSD) | ❌ (→ lib temps Rust) |
| `/radio` | ✅ | ⚠️ sans `paplay` (ffplay/mpv OK) | ⚠️ sans `paplay` |
| `/kristal` | ✅ | ❌ recompiler Qt6 | ❌ recompiler Qt6 |
| `/streaming` (`python3`) | ✅ | ⚠️ `python3` parfois absent | ❌ `python` |
| `/retro` (`xdg-open`) | ✅ | ❌ `open` | ❌ `explorer`/`start` |
| `scripts/serve.sh` (`taskset`) | ✅ | ❌ | ❌ |
| Effort de portage | — | **petit** | **moyen** |

Pistes de portage multi-OS :
1. lanceurs par OS (`xdg-open`/`open`/`explorer`, `python3`/`python`) ;
2. horloge bintime en Rust pur (sans `date`) ;
3. `code_exec` selon l'OS ;
4. scripts `run.sh` + `run.command`/`.bat`.

---

## 6. Modules retirés (historique)

Tentés puis **rejetés** (hors périmètre, rendu insuffisant ou redondant) : premières versions
de bintime (BCD), Snake, analyseur FFT `kristal`, assembleur Zer0-asm, enregistreur d'écran,
streaming minimal. Voir [`ROADMAP_FUN.md`](./ROADMAP_FUN.md) pour le détail et les raisons.
