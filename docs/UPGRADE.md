# `/upgrade` — outil de développement

Réservé aux **checkouts de développement** (dossier avec `Cargo.toml` + `.git`,
ou `ZER0_DEV=1`). Un utilisateur du binaire release reçoit un refus explicite.

## `/upgrade scan`

Audit **lecture seule** du dépôt. Ce qu'il regarde :

| Source | Ce qui est signalé |
|---|---|
| `src/**/*.rs` | `unwrap()`/`expect()` sur les chemins chauds (`local`, `vault`, `config`, `knowledge`, `zer0-gw`), `unsafe`, fichiers > 900 lignes, TODO/FIXME |
| `knowledge.db` | même fichier indexé sous plusieurs formes (`zer0 kb dedupe`) |
| dépôt | sauvegardes `*.zer0.bak`, fichiers modifiés non commités |
| git | fichiers modifiés depuis le dernier tag `v*` |

Sorties écrites dans `~/.local/share/zer0gh0st/upgrade/` :

```
audit-AAAAMMJJ.md    # lisible (priorités, preuves, effort)
audit-AAAAMMJJ.json  # machine, pour le futur `/upgrade fix`
```

`/upgrade` sans argument (ou `status`) affiche le dernier audit.

## Ce qui n'est pas encore là

- `/upgrade fix` : suivre la TODO, demander confirmation à chaque étape ;
- `/upgrade idea` : débat 0box, idées numérotées ;
- `/upgrade apply <n>` : implémenter l'idée choisie ;
- `/upgrade ship` : commit + PR (merge/release **manuels**).

Rien de tout cela ne modifie le dépôt pour l'instant.
