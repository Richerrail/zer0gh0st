# `mods/` — modules externes (optionnels)

Zer0v1 lance certains modules **dans une fenêtre séparée** depuis ce dossier :

| Commande | Attend | Techno |
|---|---|---|
| `/kristal` | `mods/zer0-kristal/zer0-kristal` | C++/Qt6 (lecteur audio) |
| `/streaming` | `mods/streaming/zero_video.py` | Python (libtorrent→mpv→pygame) |
| `/retro` | `mods/Zer0-Retr0/index.html` | HTML/JS (jeux) |

Ces programmes ne sont **pas fournis** ici (droits/licences propres, assets volumineux).
Si un module est absent, la commande affiche `introuvable` — le reste fonctionne.

Les modules **natifs** (`/radio`, `/bintime`, `/pong`) sont en Rust dans `src/fun/`.
