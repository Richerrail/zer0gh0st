# `mods/` — modules externes de ZER0│GH0ST

Un **mod** est un programme séparé, lancé par Zer0 mais **hors de son noyau** :
il peut être écrit en shell, Python, Rust, ce qu'on veut. Le noyau ne parle aucun
protocole exotique ; il délègue.

| Mod | Commande | Ce qu'il fait | Statut |
|---|---|---|---|
| `zer0do` | `/0do` | appareils du réseau local (Roku ECP) : découverte, état, touches | fourni |

## `zer0do` — appareils du réseau

```bash
/0do scan                     # découvre les appareils Roku du sous-réseau
/0do status 192.168.1.42      # modèle, nom, série, logiciel, alimentation
/0do key 192.168.1.42 Home    # envoie une touche (allowlist requise)
/0do power 192.168.1.42 off   # éteint (allowlist requise)
/0do list-allow               # état de la liste blanche
```

En CLI, sans passer par la TUI :

```bash
./mods/zer0do/zer0do scan --subnet 192.168.1.0/24
```

### Périmètre assumé (v1)

- **Roku ECP uniquement** (port 8060). Un téléviseur Samsung (Tizen), LG
  (webOS), Sony (Bravia) ou un Chromecast **ne sera pas piloté** ici : chaque
  marque est un protocole différent, avec appairage et identifiants. Le mod le
  dit au lieu de faire semblant.
- **Aucune action sans allowlist** : les adresses autorisées vivent dans
  `.zer0/zer0do.allow` (une IP par ligne, `#` pour commenter). Le fichier est
  dans **ton** dépôt, pas dans celui du projet.
- **Pas de « réparation »** : ce mod envoie des touches. Redémarrer un service
  d'un appareil demanderait SSH et des identifiants — hors périmètre.
- **Allumer** une TV n'est pas possible via ECP (il faudrait Wake-on-LAN) :
  `power on` est refusé avec cette explication, plutôt qu'un faux succès.

### Dépendances

`bash` et `curl` (les deux sont présents partout où `code_exec` fonctionne).
`avahi-browse` est optionnel : il sert seulement à **lister** les Chromecast
visibles en mDNS, pour information (ils ne sont pas pilotés).

### Journalisation

Chaque appel via `/0do` est écrit dans le journal d'audit
(`zer0 audit`, `data/audit.jsonl`) avec ses arguments complets : une action sur
un appareil reste relisible après coup.

## Écrire un mod

1. Un dossier `mods/<nom>/` avec un exécutable.
2. Un `README.md` qui dit **ce qu'il ne fait pas** (règle du projet : ne jamais
   prétendre qu'une chose est sûre ou fiable quand elle ne l'est pas).
3. Un branchement dans `src/tui/mod.rs` (`handle_command`) si tu veux une
   commande `/…`, et une entrée dans `src/tui/menu.rs` pour la palette.

Les anciens lanceurs (`/kristal`, `/streaming`, `/retro`) ont été retirés parce
que leurs sources n'étaient pas versionnées — la commande était morte après un
clone. Un mod fourni doit donc vivre **dans ce dépôt**.
