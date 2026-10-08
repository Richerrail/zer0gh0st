# ZER0v1 — Modules fun (sources réelles + re-création)

Programmes fournis par l'utilisateur, **copiés dans `ZER0v1/mods/`** (autonome, aucun lien externe).
Tout est **validé** et **câblé en commandes slash**.

## Sources dans `mods/`

| Commande | Source | Techno |
|---|---|---|
| `/kristal` | `mods/zer0-kristal/` | C++ / Qt6 (Winamp + FFT + EQ) |
| `/streaming` | `mods/streaming/zero_video.py` | Python (libtorrent → mpv → pygame) |
| `/retro` | `mods/Zer0-Retr0/` | HTML/JS (5 jeux) |
| `/bintime` | `mods/bintime/zero_bintime.py` | Python/pygame (référence) |

## Re-créations Rust (intégrées en TUI)

| Commande | Contenu |
|---|---|
| `/bintime` | horloge binaire CRT : 3 groupes HEURE/MIN/SEC, **6 bits** (32…1), LEDs vertes, poids, date, ligne binaire |
| `/pong` | Pong **contre l'agent** : grille entière, **pas fixe ~22 fps**, raquette continue, premier à 7 |

## Lancements fenêtre séparée

| Commande | Lance |
|---|---|
| `/kristal` | `mods/zer0-kristal/zer0-kristal` |
| `/streaming` | `python3 mods/streaming/zero_video.py` |
| `/retro` | `xdg-open mods/Zer0-Retr0/index.html` (Pong = « Toi contre ZER0 ») |

Résolution des chemins : racine projet déduite de l'exécutable (`target/release/zer0` → `ZER0v1`).

## Historique des itérations

- 1res tentatives (bintime BCD, Snake, FFT kristal, Zer0-asm, screen, streaming) → **rejetées**.
- Pong : 60 fps flottant = dédoublement/tearing → corrigé en **grille + pas fixe**.
- Repris sur les **vraies sources** fournies, version par version, validées une à une.

## Fichiers code

- `src/fun/mod.rs` — `Mode { Chat, Radio, BinTime, Pong }` + runner standalone (previews).
- `src/fun/bintime.rs`, `src/fun/pong.rs`, `src/fun/radio.rs`
- `src/mods.rs` — lanceurs `/kristal`, `/streaming`, `/retro`
