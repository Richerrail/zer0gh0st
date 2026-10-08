# `mods/` — modules externes (optionnels)

Zer0v1 lance certains modules **dans une fenêtre séparée** depuis ce dossier :

| Commande | Attend | Techno |
|---|---|---|
| `/kristal` | `mods/zer0-kristal/zer0-kristal` | C++/Qt6 (lecteur audio) |
| `/streaming` | `mods/streaming/zero_video.py` | Python (libtorrent→mpv→pygame) |
| `/retro` | `mods/Zer0-Retr0/index.html` | HTML/JS (jeux) |

Ces programmes ne sont **pas fournis** ici (droits/licences propres, assets volumineux).
Si un module est absent, la commande affiche simplement `introuvable` — le reste de l'agent
fonctionne normalement.

## Ajouter un module

1. Dépose le programme dans un sous-dossier, p. ex. `mods/mon-module/`.
2. Ajoute un lanceur dans `src/mods.rs` (résout déjà la racine du projet).

## Modules intégrés (rien à faire)

`/radio` (lecteur audio terminal), `/bintime` (horloge), `/pong` sont **natifs en Rust**
dans `src/fun/` — pas dans `mods/`.
