# `mods/` — modules externes de ZER0│GH0ST

Un **mod** est un programme séparé, lancé par Zer0 mais **hors de son noyau** :
il peut être écrit en shell, Python, Rust, ce qu'on veut. Le noyau ne parle aucun
protocole exotique ; il délègue.

| Mod | Commande | Ce qu'il fait | Statut |
|---|---|---|---|
| `zer0do` | `/0do` | appareils du réseau local (Roku ECP) : découverte, état, touches | fourni |

## `zer0do` — appareils du réseau

```bash
/0do scan                        # découvre les appareils Roku du sous-réseau
/0do status 192.168.1.42         # modèle, nom, série, logiciel, alimentation
/0do apps 192.168.1.42           # applications installées (id + nom)
/0do active 192.168.1.42         # application au premier plan
/0do launch 192.168.1.42 Netflix # ouvre une application (allowlist requise)
/0do search 192.168.1.42 météo   # recherche globale du Roku (allowlist requise)
/0do key 192.168.1.42 Home       # envoie une touche (allowlist requise)
/0do power 192.168.1.42 off      # éteint (allowlist requise)
/0do power 192.168.1.42 on       # allume par Wake-on-LAN (allowlist + MAC)
/0do wake 192.168.1.42 --dry-run # montre le paquet sans l'envoyer
/0do list-allow                  # état de la liste blanche
```

L'allowlist accepte une **MAC** en seconde colonne, indispensable à l'allumage.
`scan` affiche cette MAC (celle de l'interface active) et donne la ligne prête
à coller :

```
IP               MODÈLE              ÉTAT       MAC                NOM
192.168.1.42     RTRU5528-CA         PowerOn    ac:ac:e2:2a:fe:4c  55" RCA Roku TV

Pour autoriser le réveil (Wake-on-LAN), ajoute dans .zer0/zer0do.allow :
  192.168.1.42  ac:ac:e2:2a:fe:4c
```

### Touches (`/0do key <ip> <TOUCHE>`)

| Famille | Touches |
|---|---|
| Navigation | `Up` `Down` `Left` `Right` `Select` `Back` `Home` `Info` |
| Lecture | `Play` `Rev` `Fwd` `InstantReplay` |
| Claviers | `Enter` `Backspace` `Search` |
| Son | `VolumeUp` `VolumeDown` `VolumeMute` |
| Entrées | `InputHDMI1` … `InputHDMI4`, `InputTuner`, `InputAV1` |
| Alimentation | `Power` (bascule), `PowerOff` |

Codes observés sur un appareil réel :

| Code | Signification |
|---|---|
| `200` / `204` | touche acceptée |
| `400` | touche inconnue de ce modèle |
| `000` | aucune réponse : touche non gérée en ECP sur ce modèle (le volume passe souvent **en infrarouge**), ou appareil hors ligne |

Le mod **rapporte le code** au lieu de prétendre avoir réussi. La saisie
caractère par caractère (`Lit_*`) **n'est pas gérée** : utilise
`/0do search <ip> <texte>`.

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
  `.zer0/zer0do.allow` (une IP par ligne, `#` pour commenter, MAC facultative).
  Le fichier est dans **ton** dépôt, pas dans celui du projet.
- **Pas de « réparation »** : ce mod envoie des touches et ouvre des
  applications. Redémarrer un service d'un appareil demanderait SSH et des
  identifiants — hors périmètre.
- **Allumer** passe par **Wake-on-LAN** (paquet magique UDP, port 9), envoyé en
  broadcast **et** en unicast. Il faut (a) la MAC dans l'allowlist — `scan`
  l'affiche, (b) que l'appareil garde sa carte réseau alimentée en veille
  (« Fast TV start » sur Roku).
  **Le broadcast dépend du shell** : `bash` ne pose pas `SO_BROADCAST`, donc
  l'envoi en `255.255.255.255` peut échouer en `Permission denied` — le mod le
  dit ligne par ligne (✅/❌ par destination) au lieu d'annoncer un succès. S'il
  échoue, l'unicast suffit souvent ; sinon `socat` est utilisé automatiquement
  s'il est installé. `--dry-run` montre le paquet sans l'envoyer.

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
