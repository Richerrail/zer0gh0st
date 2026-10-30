## Non publié

- **Thèmes de couleurs TUI** : `/theme` ouvre un menu avec aperçus et persiste
  le choix. Thèmes `ghost` (historique), `noir` et `noir-rouge` ; le fond, le
  banner, les menus, le picker, l'assistant et Zero-chan suivent le thème.

## v0.7.15 — `t0p` dans le groupe mods (2026-10-30)

- Le menu range `t0p` avec radio/bintime/pong (il était listé hors groupe).

## v0.7.14 — `t0p` en mod latéral (2026-10-30)

- `/t0p` ouvre le moniteur dans le **panneau latéral** (même place que
  `/radio` et `/bintime`), au lieu d'un message dans le fil. Onglet `t0p`,
  rafraîchi 1×/s ; `/panel` ou `Esc` (panneau focus via Tab) le ferme.
- `zer0 t0p [--watch]` reste disponible en CLI.

## v0.7.13 — Moniteur système `zer0 t0p` (2026-10-29)

- **`zer0 t0p [--watch]`** et le **mod `/t0p`** (panneau latéral, comme
  radio/bintime) : btop-like sans btop (CPU global et par cœur, RAM/swap,
  charge, températures hwmon, top 8 processus) via `sysinfo`. Rafraîchi une
  fois par seconde. Températures : Linux ; macOS/Windows « indisponible ».

## v0.7.12 — Windows/macOS : GPU auto, shell adapté (2026-10-29)

- **GPU** : `-ngl` automatique. `ZER0_GPU_LAYERS=auto` interroge
  `llama-server --list-devices` (CUDA/Vulkan/ROCm/SYCL) ; macOS = Metal
  (`-ngl 999`) ; `0` force le CPU, `N` fixe les couches.
- **Windows** : `code_exec` préfère bash (Git Bash) → `wc`/`grep` marchent,
  sinon PowerShell, sinon cmd (`ZER0_SHELL` pour forcer). Shell annoncé dans
  le bloc Environnement.
- **OCR** : langues lues via `tesseract --list-langs` quand `tessdata` n'est
  pas dans `/usr/share` (macOS/Windows).
- README : section Windows/macOS.

## v0.7.11 — `knowledge_delete`, OCR multilingue (2026-10-28)

- **`knowledge_delete`** : supprime les chunks dont la source contient un motif
  (l'API existait, l'outil manquait).
- **`read_image`** : détecte les langues réellement installées, repli honnête
  `fra` → `eng` avec la commande d'installation ; défaut `fra+eng`.
- **Markdown** : liens `[texte](url)` → `texte (url)`, citations `> ` en barre,
  fin du BUG 4 partiel.
- CHANGELOG : 0.6.15 → 0.7.11 documentés (il s'arrêtait à 0.6.14).

## v0.7.10 — Lire le texte d'une image (2026-10-28)

- **`read_image`** : OCR (tesseract) + métadonnées (dimensions/format) pour les
  captures d'écran et photos. Repli honnête quand une langue manque
  (`fra` absent → `eng`, avec la commande d'installation).

## v0.7.9 — Sous-agent isolé, lecture web, échéances (2026-10-28)

- **`delegate`** : sous-agent à contexte isolé (outils moins `delegate`/`0box`,
  donc pas de récursion), synthèse courte.
- **`web_read`** : HTML → texte ; `render: true` passe par r.jina.ai pour les
  pages qui exigent JavaScript.
- **Échéances de tâches** : `task_add due="+2h"` (ou epoch), stockage
  rétro-compatible, rappel TUI chaque minute, une fois par tâche.

## v0.7.8 — Écrire dans la base de connaissances (2026-10-28)

- **`knowledge_add`** : chunks + FTS5 + embeddings, même pipeline que
  l'ingestion de fichiers, idempotent par `source`. Remplace le contournement
  `sqlite3` qui cassait l'index hybride.

## v0.7.7 — Mémoire corrigeable, lecture paginée (2026-10-28)

- **`memory_update`** / **`memory_delete`** : une entrée fausse n'est plus
  définitive ; le texte modifié est ré-embeddé.
- **`task_delete`**, et **`text_editor view`** accepte `offset`/`limit`
  (400 lignes par défaut) au lieu d'avaler le fichier entier.

## v0.7.6 — Correctifs TUI de l'audit (2026-10-27)

- ctx réel (`0,7 %` au lieu de `0 %`), état des interrupteurs dans le footer
  (sécurité `code_exec`), `project_root()` depuis le dossier courant,
  wrap aux espaces + largeur Unicode, titres markdown, 13 commandes
  manquantes au menu + test anti-dérive.

## v0.7.5 — Prompt local d'une phrase (2026-10-27)

- Local : « Je suis Zer0, ton AI conversationnel ultra rapide. » — rien d'autre.

## v0.7.4 — Démarrage local en 3 s (2026-10-27)

- `-np 1 -b 512 -ub 256` en plus de `--no-warmup` : 37,8 s → 3,0 s.

## v0.7.3 — Pas de warmup, préchargement (2026-10-27)

- `--no-warmup` (37,8 → 4,4 s), génération sur tous les cœurs, préfill à
  moitié, serveur préchargé au lancement de la TUI, verrou anti-double-démarrage.

## v0.7.2 — Matériel réel dans le prompt (2026-10-27)

- Bloc Environnement : nombre de cœurs réel + GPU lu sur le bus PCI (fini les
  hallucinations `nvidia-smi`).

## v0.7.1 — Cœurs réels, `code_exec` 300 s (2026-10-27)

- Prompt local sans « 2 cœurs » codé en dur ; `code_exec` 300 s par défaut
  (`ZER0_EXEC_TIMEOUT`), docs de flux à 240 s.

## v0.7.0 — Local = chat rapide, garde-fou thermique (2026-10-27)

- Le mode local n'est plus un agent : aucun outil, aucun KB, prompt court.
- Tous les cœurs détectés, contexte 2048 ; coupure à 88 °C, serveur arrêté
  à 92 °C (Linux).

## v0.6.20 — Noyau d'outils local (2026-10-27)

- `ZER0_LOCAL_TOOLS=minimal` (4 outils) — retiré en 0.7.0 avec le mode agent local.

## v0.6.19 — Raisonnement, passerelle, timeout (2026-10-27)

- `--reasoning-budget 0` (content vide sinon), `local` contourne zer0-gw,
  timeout total adaptatif (600 s non-stream).

## v0.6.18 — `/stream` et correctif `/system` (2026-10-26)

- Bascule streaming/non-streaming persistée ; `/system` sans argument ne
  ferme plus la TUI (panic `args[1..]`).

## v0.6.17 — Timeout de flux 240 s (2026-10-26)

- Un préfill local peut dépasser 2 min : `ZER0_STREAM_IDLE_SECS`.

## v0.6.16 — Prompts local/cloud séparés (2026-10-26)

- `/system local|cloud <texte>`, prompt local court, persistance.

## v0.6.15 — Mode local : llama-server auto, GGUF, outils natifs (2026-10-26)

- Démarrage automatique de llama-server, scan récursif des GGUF (`mmproj`
  exclus), résolution de modèle, limitations annoncées, outils natifs.

## v0.6.14 — Mode local : diagnostics honnêtes (2026-10-25)

- **« aucun modèle » / « /model use ne marche pas »** : `models::list` annonçait « URL invalide »
  pour une simple connexion refusée. Il distingue désormais URL invalide (builder) et provider
  injoignable (connect/timeout), avec la commande à lancer pour `local` (`scripts/serve.sh`).
- **Message d'échec vide** : quand tous les endpoints étaient sautés (marqués en échec récent),
  l'erreur affichait « dernier: » sans rien. Elle liste maintenant les endpoints concernés et
  le délai d'attente.
- **`--once` imprime le raisonnement** : un modèle « thinking » qui ne remplit que
  `reasoning_content` n'affichait rien du tout en headless.
- **`scripts/serve.sh`** : `--reasoning off` retiré — sur llama-server b10809 ce drapeau fige le
  serveur (vérifié avec des prompts minuscules).

## v0.6.13 — Installer Zero-chan : choisir un Python compatible (2026-10-25)

- Sur une machine où un shim `mise`/`pyenv` expose `python3` en **3.15**, l'installeur prenait
  le premier `python3` venu puis refusait (garde `< 3.13`) : il était inutilisable alors que le
  Python système 3.14 fonctionne. Il teste désormais chaque candidat (3.12, 3.13, 3.14, 3.11,
  3, python) et retient le premier compatible (3.9–3.14).
- Cause du message `ModuleNotFoundError: No module named 'faster_whisper'` : sans venv
  `~/.venvs/zer0-whisper`, `platform::python()` retombait sur `python3` (le shim 3.15), dépourvu
  du module. Le venv créé avec le Python système est détecté automatiquement.

## v0.6.12 — Micro Windows et installation des prérequis (2026-10-25)

Contribution de **darkdober4** (PR #5) :

- **Zero-chan sous Windows** : prise en charge de `sox_ng`, qui ouvre réellement le périphérique
  d'enregistrement (l'ancien SoX 14.4 peut répondre « no default audio device »). L'arrêt est
  suivi d'une réparation des tailles RIFF du WAV pour que `faster-whisper` puisse le lire.
- Le message « micro indisponible » affiche la **cause réelle** et la commande d'installation
  adaptée au système.
- Le venv STT `~/.venvs/zer0-whisper` est détecté automatiquement (`ZER0_PYTHON` reste prioritaire).
- **Installation des prérequis** : `scripts/install.sh` (Linux/macOS) installe l'enregistreur et
  le venv Whisper ; `scripts/install-windows.ps1` + `install.cmd` font de même sous Windows
  (Rust, Build Tools, FFmpeg, SoX_ng, Python 3.12, `faster-whisper` + `av<16`). Les scripts sont
  validés en CI (`bash -n`, parseur PowerShell).

Suites apportées après la revue :

- `repair_wav()` ne réécrit plus un WAV **déjà cohérent** : il n'intervient que si la taille du
  chunk `data` est provisoire (0 ou supérieure aux octets disponibles), donc il ne peut pas
  gonfler un fichier qui contient des chunks annexes après `data`.
- Le `stderr` de l'enregistreur est **drainé en continu** dans un thread : un enregistreur
  bavard ne peut plus bloquer sur un tube plein (enregistrement vide sans message).

## v0.6.11 — Tâches en cours animées (2026-10-24)

- Dans la TUI, le marqueur d'une tâche `en cours` cycle `* ¤ o O 0 O o ¤` (~480 ms par pas)
  au lieu de rester `[~]` : on voit immédiatement ce qui tourne, même sans couleur.
- Liste chargée une fois puis re-rendue depuis un cache (`App::task_view`) : aucun accès
  disque par frame. Toute autre commande `task` invalide la vue.
- `format_tasks()` pur et `SPINNER` testés (cycle, bouclage, marqueurs `[ ]`/`[x]` intacts).

## v0.6.10 — Persona persistante et diagnostic (2026-10-24)

- **La persona active est persistée** (`config.toml`, champ `persona`) et restaurée au
  démarrage. Avant, `/whoami zero-vamp` puis un relancement perdait le persona sans le dire —
  ce qui donnait l'impression que le persona n'agissait pas.
- **`/system`** : affiche la persona active, si son texte est réellement dans le prompt, la
  longueur du prompt système et son début. Vérifier au lieu de supposer.
- `compose_system()` testé (base + persona), pour garantir le câblage.

## v0.6.9 — Contexte : chiffres exacts, salutations silencieuses (2026-10-24)

- **`/context` ne ment plus** : il affichait « compaction auto à : 6000 tokens » (valeur codée
  en dur) ; il affiche la fenêtre réelle du modèle, l'historique estimé, le pourcentage
  utilisé et le vrai seuil (60 % de la fenêtre, min 4 000).
- **Fenêtre inconnue = 32 768** par défaut (au lieu de 200 000) : surestimer la fenêtre
  faisait envoyer un historique trop long, refusé par le provider (400).
- **Salutations ignorées par la base de connaissance** : « salut », « bonjour », « merci »…
  ne produisent plus de tokens significatifs, donc aucune requête SQLite et aucune injection.

## v0.6.8 — Recherche web : quatre moteurs au lieu d'un (2026-10-24)

- **Chaîne de repli** : DuckDuckGo HTML (POST + en-têtes de navigateur) → DuckDuckGo Lite →
  Mojeek → DDG Lite/Mojeek via `curl` → Instant Answer. Le premier qui répond gagne, et
  `/search` indique le moteur utilisé.
- Cause du « aucun résultat » : un seul backend en GET, un UA qui déclenchait l'anti-bot, et
  une API Instant Answer presque toujours vide sur une requête générale.
- Parseur commun aux trois markups (attributs dans n'importe quel ordre), déduplication par
  URL, avance correcte entre résultats. 5 tests sur fixtures HTML.

## v0.6.7 — Base de connaissance : fantômes, poids et retrait (2026-10-24)

- **Purge des sources fantômes** (`/kb purge`, `zer0 kb purge`) : la base indexait 104 sources
  dont **aucune** n'existait encore sur disque — un livre fournissait à lui seul 68 % des
  chunks. La commande supprime les chunks dont le fichier a disparu et reconstruit le FTS.
- **Un seul chunk par source** dans l'injection automatique : un document volumineux ne peut
  plus occuper les trois extraits à lui seul.
- **`/kb drop <motif>`** (`zer0 kb drop`) : retirer de l'index les sources dont le chemin
  contient le motif, sans tout réindexer.

## v0.6.6 — Séparation stricte des deux applications (2026-10-24)

- **Plus d'import depuis l'application Python** : `migrate_old_data()` relisait
  `~/.local/share/zer0` à chaque lancement et recopiait conversation, mémoire, personas et
  archives manquants. Conséquences : la conversation Python revenait après chaque `/new`
  (33 copies du même fichier), et les personas supprimées réapparaissaient.
- La migration depuis notre propre checkout (`<projet>/data`, `knowledge.db`) est conservée.
  L'application Python garde ses données, sa config (`~/.config/zer0cod`) et ses personas
  (`whoami/`) : chaque produit reste chez lui.

## v0.6.5 — Quotas observables et bascule raisonnée (2026-10-24)

- **`/quota` persiste** : cumul par jour et par endpoint dans
  `<données>/quota.json` (14 jours), écrit au plus toutes les 2 s, immédiat sur 429.
  `zer0 quota` lancé à part affiche enfin la journée réelle au lieu d'une page blanche.
- **`max_tokens` plafonné à 2 048 pour les modèles `:free`** : la sortie réservée
  n'est plus comptée contre le quota pour rien.
- **Bascule ciblée** : une erreur déterministe (400/422) n'est plus rejouée sur tous les
  endpoints — sauf mention d'outils, où un autre modèle peut les supporter.

## v0.6.4 — Quotas gratuits et palette de commandes (2026-10-24)

- **429 définitif reconnu** : un `free-models-per-day` / quota journalier / crédits épuisés ne
  déclenche plus 3 réessais puis une bascule. Message immédiat, avec la marche à suivre.
- **Pas de bascule sur 429 d'un modèle `:free`** : tous partagent le quota du compte, changer
  de modèle ne rend pas de quota — ça ne faisait que multiplier les requêtes.
- **Budget d'itérations réduit** à 4 (au lieu de 8) quand la chaîne contient un `:free`.
- **Palette de commandes** : `Espace` valide la frappe et rend la main à la saisie, `Entrée`
  sans résultat réinjecte le texte tapé. `/whoami zero-vamp` et `/task add …` sont enfin
  saisissables. `Picker::selected()` respecte le filtre (avant, Entrée pouvait exécuter le
  premier élément de la liste).
- Personas compactées côté données (8 Ko → < 1 Ko), avec une section « Exécution » qui
  rappelle de répondre d'abord et de n'appeler un outil que si nécessaire.

## v0.6.3 — Assistant de première connexion (2026-10-23)

- **Assistant en 8 étapes** (`/setup`, proposé au premier lancement) : inférence (cloud/local),
  provider, clé API, modèle, Telegram, raisonnement, `code_exec`, visite des commandes.
  Chaque écran explique le **pourquoi** avant le **comment**, et `Échap` enregistre l'étape
  pour reprendre plus tard.
- **Saisie masquée** pour les clés et jetons : l'écran n'affiche que `●`, le secret ne passe
  ni dans l'historique ni dans le journal d'audit.
- **Menus sélectionnables** au lieu de syntaxes à mémoriser : `/model`, `/model use`
  (sans identifiant), `/free`, `/provider`, `/role set <rôle>` (le modèle choisi alimente
  le rôle).
- **`/login` / `/logout`** : connexion complète (provider → clé → modèle) en une commande.
- **`/help` court** (14 commandes utiles) ; `/help all` conserve la liste exhaustive.
- **Choix persistants** : `think` et `code_exec` sont enregistrés dans `config.toml`
  (`false` par défaut pour les deux) ; `--think` / `--allow-exec` restent prioritaires.
- **Pas d'intrusion chez l'existant** : une config qui a déjà un provider/modèle ne
  déclenche pas l'assistant (`setup_done` migré).

## v0.6.2 — Liste de tâches globale (2026-10-22)

- **`tasks::path()` ne dépend plus du dossier courant.** Il renvoyait
  `.zer0/tasks.md` **relatif au cwd** : lancé depuis `~`, l'agent écrivait dans
  `~/.zer0/tasks.md` ; depuis le dépôt, ailleurs — listes différentes selon
  l'endroit de lancement, et des `.zer0/` dispersés.
  Désormais `~/.local/share/zer0gh0st/tasks.md`, surchargeable par
  `ZERO_TASKS_FILE` pour une liste propre à un projet.
- **Migration unique** : reprend la liste la plus récente parmi `.zer0/tasks.md`
  (cwd), `<racine du projet>/.zer0/tasks.md` et `~/.zer0/tasks.md`, sans jamais
  écraser une liste existante.
- **Prompt système** : le bloc « Environnement » annonçait encore
  `<projet>/data`, `<projet>/knowledge.db` et `<projet>/.zer0` ; il donne
  maintenant les chemins XDG réels (données, base, tâches).

## v0.6.1 — Correctifs issus de la revue (2026-10-22)

- **TUI — cache de rendu** : chaque frame re-découpait et re-parsait toute la conversation
  (O(conversation) par frame, donc par token). Rendu mémoïsé par message (index, largeur,
  empreinte du contenu), invalidé au changement du nombre de messages.
- **I/O hors du runtime async** : `knowledge::search` (appelée à chaque tour) exécute FTS5
  et le scan des vecteurs en `spawn_blocking` ; idem pour la lecture/écriture de
  `memory.jsonl`. Un `prepare().unwrap()` du chemin vectoriel devient une erreur gérée.
- **Migration des données** : les journaux JSONL sont repris s'ils sont **plus récents**
  que la copie XDG. Corrige une mémoire amputée (21 entrées → 5) après le passage en XDG.
- **Journal d'audit** : purge d'ancienneté (90 jours par défaut, `ZER0_AUDIT_RETENTION_DAYS`,
  `0` = désactivée), une fois par processus ; les lignes illisibles sont conservées.
- **Référence des commandes générée** : `zer0 commands` produit `docs/COMMANDS.md`
  (559 lignes) depuis clap ; un test échoue si le fichier diverge du binaire.
- **Usage stable** : `bin_name("zer0")` — l'asset brut n'affiche plus
  `Usage: zer0-x86_64-unknown-linux-gnu`.
- `.gitattributes` : fins de ligne normalisées, markdown en LF (le test anti-dérive
  échouait sur un checkout Windows en CRLF).

## v0.6.0 — Noyau testable, correctifs critiques et garde-fous (2026-10-22)

### Corrigé — pertes de données et crashs silencieux
- **Troncature UTF-8** : `String::truncate` paniquait au milieu d'un caractère (toute
  sortie d'outil accentuée > 16 Ko) et tuait la tâche de génération sans message ; la TUI
  restait bloquée en « streaming ». Nouveau module `util` + tests de régression.
- **Markdown aplati** : `sanitize_content` écrasait tous les blancs (retours à la ligne,
  indentation) de **chaque** réponse assistant, contenu ensuite persisté et renvoyé au
  modèle. Ne touche plus qu'aux tokens `<|…|>` et aux lignes vides excédentaires.
- **`code_exec` non borné** : `output()` bloquant, sans timeout ni plafond, insensible à
  `/stop`. Désormais `tokio::process` + timeout (120 s, max 3600) + kill + capture bornée.
- **`--resume`** : la coupe du JSONL pouvait produire un historique refusé par les
  providers (400 partout). `convo::repair` + `load_repaired`.
- **Compaction non persistée** : le résumé ne survivait pas au redémarrage.
- **Fin de flux** sans `[DONE]`/`finish_reason` : signalée (`eof`), plus acceptée en silence.
- **Double backoff 429** : un seul endroit décide, dans le client.
- **Outils `0box` figés** : `Runtime` en `RwLock`, `F3`/`F4` s'appliquent réellement.
- **Config illisible** : sauvegarde `.corrupt-*.bak` et écritures refusées (le
  `save_global` suivant écrasait les clés API).

### Sécurité (droits utilisateur assumés, sans sandbox)
- `zer0-gw` : jeton admin (`x-zer0-admin`, gw.toml ou `ZER0_GW_ADMIN_TOKEN`), comparaison
  à temps constant, avertissement au démarrage si absent.
- Telegram : `code_exec` **et** `text_editor` derrière `/exec` ; `/exec on` refusé sur un
  chat lié automatiquement (`ZER0_TELEGRAM_ALLOW_EXEC=1` pour forcer).
- Journal d'audit append-only (`data/audit.jsonl`) : `zer0 audit`, `/audit`.
- Sauvegardes `<fichier>.zer0.bak` avant écrasement ou suppression.
- Budget d'appels LLM par tour (120, `ZER0_MAX_LLM_CALLS`), sous-agents inclus.

### Architecture
- Noyau des commandes dans `src/commands.rs` : `select`, `search`, `task`, `model`, `route`,
  `role`, `memory`, `kb`, `provider`, `quota`, `pdf`, `apis`, `whoami`, `vault`, `embeddings`.
  Le CLI et la TUI n'ont plus qu'un adaptateur chacun — plusieurs commandes avaient déjà
  divergé (`role auto` avait deux sens opposés, `select` 8 vs 5 modèles, `task add --role`
  ignoré en TUI).
- `provider list` distingue le **transport** (direct / zer0-gw) de la **présence d'une clé**.
- Données hors du dépôt : `~/.local/share/zer0gh0st` (XDG), `ZER0_DATA_DIR` pour surcharger.
  Migration unique depuis `data/` et `knowledge.db` du checkout, plus reprise d'une base
  vide laissée par un lancement antérieur.
- Schémas versionnés : `schema_version` dans `config.toml`, `PRAGMA user_version` en SQLite.
- Modules `/kristal`, `/streaming` et `/retro` retirés : leurs sources n'étaient pas
  versionnées, donc les commandes étaient mortes après un clone (`/bintime`, `/radio`,
  `/pong` restent, dans `src/fun/`).

### Tests
- Noyau extrait dans `src/lib.rs` + `src/cli.rs` ; `src/main.rs` n'est plus qu'un lanceur.
- `src/commands.rs` : source unique CLI/TUI pour `select`, `search`, `task` (les deux
  implémentations avaient divergé : 8 vs 5 modèles, `--role` ignoré en TUI).
- `tests/streaming.rs` : faux serveur SSE local, 7 tests (tool_calls fragmentés, flux
  tronqué, 429, bascule d'endpoint, outils transmis).
- 152 tests (127 lib + 14 gateway + 11 intégration) ; `cargo fmt --check` bloquant en CI.

## v0.5.9 — Rafraîchissement du jeton GitHub (opt-in) (2026-10-21)

- **Support des jetons expirables (opt-in)** : si l'app OAuth a l'option
  « Expire user access tokens », GitHub renvoie un `refresh_token` en plus du jeton
  d'accès. Zer0 le conserve **chiffré** (coffre, clé `github_refresh`) et renouvelle
  automatiquement le jeton d'accès sur un 401, puis rejoue l'action → le 👍 reste
  automatique au-delà des 8 h.
- **Opt-in par nature** : sans `refresh_token` stocké, rien ne change (aucune requête
  supplémentaire, aucun comportement modifié). `vote logout` retire les deux.
- Test rendu **indépendant de l'environnement** (il lisait la config réelle de
  l'utilisateur) + test de l'opt-in.

## v0.5.8 — Vote 👍 (vraie star GitHub), /quota live, correctifs (2026-10-21)

Points saillants :

- **👍 dans la TUI = une vraie étoile GitHub** sur `Richerrail/zer0gh0st`, posée avec le
  compte de l'utilisateur (OAuth *device flow* : un code à coller, **aucun
  `client_secret`** dans le code). Sans jeton, la page du dépôt s'ouvre dans le
  navigateur — **rien n'est simulé**. Compteur d'étoiles réel (API publique, cache 10 min).
  Le jeton n'est **jamais** stocké en clair : env → **coffre chiffré** (Argon2 +
  ChaCha20-Poly1305, fichier 0600) ; aucun champ `github_token` dans `config.toml`.
- **`/quota`** : quotas et crédits OpenRouter en direct.
- **Correctif capture souris (Windows)** : `cmd /C start` réinheritait le mode console
  qui porte la capture de la souris de crossterm → après un clic, plus aucun événement
  souris (menu compris). Mesuré : `mouse=true` → `mouse=false`. Correctif :
  `rundll32 url.dll,FileProtocolHandler` (aucune console annexe) + stdio détachés.
- **Les messages nomment le bon binaire** : « `zer0` codé en dur » pouvait désigner un
  autre programme (une ancienne appli Python, p. ex.) → la commande indiquée était
  fausse. `ZER0_CMD_NAME` (posé par le lanceur) est prioritaire, sinon le chemin réel
  de l'exécutant.
- **Fichiers communities** GitHub (`CONTRIBUTING`, `CODE_OF_CONDUCT`, `SECURITY`,
  `ACCESSIBILITY`, templates d'issues et de PR) + description et topics du dépôt.
- Corrige aussi : la base de connaissances partait à `/` pour un binaire installé hors
  checkout source → désormais dans `~/.local/share/zer0gh0st/`.

### Détail — vote 👍/👎

- **Nouveau module `vote.rs`** : deux petits pouces **en bas à droite de l'écran
  principal** (pied de page, clicables à la souris) — le **pouce haut = une star
  GitHub** sur `Richerrail/zer0gh0st`, le **pouce bas = le retrait de cette star**.
- **1 upvote = 1 star, vraiment** : avec un jeton GitHub (`ZER0_GITHUB_TOKEN`,
  `GITHUB_TOKEN`, `GH_TOKEN`, ou clé `github` dans la config / le coffre), la star
  est posée via l'API GitHub (`PUT /user/starred/{owner}/{repo}`) — pas un simple
  lien. **Sans jeton**, la page du dépôt s'ouvre dans le navigateur, là où se
  trouve le bouton « ★ Star » (le message dit explicitement ce qui s'est passé :
  rien n'est simulé).
- **Le compteur affiché est réel** : lu sur l'API publique
  (`GET /repos/{owner}/{repo}/stargazers/count`). Inconnu → `?`, jamais un chiffre
  inventé.
- **CLI** : `zer0 vote up|down|status` · **TUI** : `/vote [up|down|status]`, plus
  les clics sur les pouces. Le vote est aussi dans la palette `/` et dans l'aide.
- **Invitation à la connexion GitHub** : au **premier lancement**, la fenêtre
  principale propose `zer0 vote login` (device flow OAuth : un code à valider
  dans le navigateur, une fois pour toutes). Les lancements suivants se
  contentent d'un **rappel discret** d'une ligne. Le compteur vit dans la
  config (`github_prompted`, `github_prompt_count`) : l'utilisateur n'est ni
  harcelé, ni relancé à chaque démarrage.
- Tests : parsing du compteur, URLs, et un test de rendu (les deux pouces sont
  bien cliquables, ordonnés et alignés à droite).

### Correctif : le menu et les boutons de vote ne répondaient plus aux clics

- **Cause racine (mesurée)** : ouvrir un navigateur via `cmd /C start` — l'ancien
  `platform::open()` sous Windows — **hérite de la console** du TUI et lui
  **reprend le mode « capture souris »** de la console à sa sortie. Après un
  simple clic sur le 👍, le terminal ne recevait **plus aucun événement souris** :
  ni les pouces, ni le menu « Commandes ». Mesuré : `mouse=true` avant →
  `mouse=false` après `cmd /C start`.
- **Correctif** : `platform::open_url()` passe désormais par
  `rundll32 url.dll,FileProtocolHandler` (aucune console annexe) et détache les
  flux stdio ; `platform::restore_mouse_capture()` **réarme** mode brut + capture
  souris après l'ouverture. Mesuré : `mouse=true` après `rundll32`.
- **`open_file()` (le mod `/retro`) corrigé aussi** : il utilisait encore
  `cmd /C start`, qui réinvoquait le même problème. Il reçoit maintenant
  `CREATE_NO_WINDOW` : mesuré, `cmd /C start` **sans** ce drapeau perd la capture
  souris, **avec** il la conserve.
- **Multiplateforme** : `open_url()` / `open_file()` couvrent Linux
  (`xdg-open`, repli `gio`), macOS (`open`) et Windows (`rundll32` pour les URL,
  `cmd /C start` + `CREATE_NO_WINDOW` pour les fichiers) ; stdio détaché partout
  pour qu'un enfant ne corrompt pas l'affichage du TUI.
  `restore_mouse_capture()` est défini pour les 3 systèmes (le mode souris est
  porté par la console sous Windows, par les séquences ANSI ailleurs).
- **Ordre de routage des clics** : le menu passe désormais **en premier** (ses
  zones sont des lignes entières et ne croisent pas les pouces), puis les pouces,
  puis 0chan/mic, et enfin la sélection de texte.
- **Zones des pouces élargies d'une cellule** : un emoji occupe 1 ou 2 cellules
  selon le terminal, un clic « à côté » doit compter.
- Test de régression : `tui::tests::clicks_reach_menu_and_vote_buttons`.

### Deux pièges de l'API GitHub rencontrés et corrigés

- l'API **exige un en-tête `User-Agent`** → sans lui, `403 Request forbidden by
  administrative rules` (le compteur restait invisible) ;
- GitHub **met cet endpoint en cache 60 s** (CDN) et **propage le compteur en
  ~1 min** → on demande `Cache-Control: no-cache` et le message de vote annonce
  le délai, pour ne pas afficher un compte faux.

### Connexion GitHub automatique (device flow OAuth)

- `zer0 vote login` (ou `/vote login`) : GitHub affiche un code, l'utilisateur
  le valide dans son navigateur, le jeton est rangé **chiffré dans le coffre**
  (`vault::put("github", …)`). Ensuite, chaque 👍 est une vraie star, sans rien
  demander à l'utilisateur.
- L'application OAuth **Zer0** est **préconfigurée** (`client_id` embarqué) :
  `zer0 vote login` fonctionne out of the box, sans configuration. Le `client_id`
  est **public** (ce n'est pas un secret) ; le `client_secret` n'est **jamais**
  nécessaire (device flow). Pour pointer une autre application :
  `ZER0_GITHUB_CLIENT_ID` ou `github_client_id` dans `config.toml`.
- Le jeton est cherché dans cet ordre : `ZER0_GITHUB_TOKEN` → `GITHUB_TOKEN` →
  `GH_TOKEN` → coffre → `config.toml` (`[keys] github`). Rien n'est stocké en
  clair par Zer0 si le coffre est utilisé.
- `zer0 vote logout` retire le jeton du coffre.
- **Le coffre se crée tout seul** au premier `vote login` (il n'existait pas
  forcément) et `zer0 vault setup` accepte une config sans clé API (le coffre
  peut ne contenir que le jeton GitHub).
- **Le jeton n'est jamais perdu** : si l'écriture chiffrée échoue, il est copié
  dans le presse-papier et la façon de le stocker est expliquée. Depuis la TUI,
  le coffre doit être initialisé en CLI (`zer0 vault setup`) : on ne peut pas
  saisir une passphrase en mode raw.

## v0.5.7 — Catalogue d'APIs, fichiers communities (2026-10-20)

- **`/apis <besoin>`** : catalogue de **2 000+ APIs publiques** (source
  [public-apis/public-apis](https://github.com/public-apis/public-apis), MIT), importé par
  `scripts/kb-import-public-apis.sh` (réf épinglée, **date d'import conservée**) dans une
  **table SQLite dédiée** (`public_apis`) : recherche par nom/description/catégorie,
  colonne **« clé : oui/non »**, filtre `keyless`, rappel « candidat, à vérifier ».
  L'agent peut s'en servir seul (le catalogue est aussi indexé pour
  `knowledge_search`).
- **Deux bugs corrigés au passage** : `kb ingest` ignorait les `.tsv`/`.csv` ; l'appel
  d'**embeddings** partait en direct (401 « clé absente ») au lieu de passer par `zer0-gw`.
- **Fichiers communities** (checklist « Repository health » de GitHub) :
  `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, `SECURITY.md`, `ACCESSIBILITY.md`,
  templates d'issues (YAML) et de PR. Description du dépôt + topics posés via l'API.
- Le tag `v0.5.6` reste sur un commit **antérieur** aux correctifs 0box : la 0.5.7 est
  la version de référence.

## Catalogue d'APIs publiques : `/apis` (2026-10-20)

- **Script d'import** `scripts/kb-import-public-apis.sh` : télécharge le catalogue
  [public-apis/public-apis](https://github.com/public-apis/public-apis) (MIT) à une **réf
  épinglée**, saute l'en-tête publicitaire, convertit les tableaux markdown en **TSV**
  (une API par ligne : nom, description, url, auth, https, cors, catégorie) et l'importe
  dans une **table SQLite dédiée** (`public_apis`) avec la **source et la date d'import**.
  `--kb` indexe aussi le TSV dans la base de connaissance (pour l'outil `knowledge_search`).
- **`/apis [besoin]`** (TUI) et **`zer0 apis search|import|categories|stats`** (CLI) :
  recherche structurée (nom > description > catégorie), colonne **« clé : oui/non »**,
  filtre `keyless` (uniquement les APIs sans clé), rappel « candidat, à vérifier » et
  source/date à chaque résultat.
- **Deux bugs trouvés au passage** :
  - `kb ingest` ignorait les `.tsv`/`.csv` (« aucun fichier texte ») → extension acceptée ;
  - l'appel d'**embeddings** partait **en direct** (401 « clé absente ») au lieu de passer
    par `zer0-gw` → `config::endpoint_for()` utilisé aussi pour l'embedder.

## v0.5.6 — 0box réel, CI verte, transport par la passerelle (2026-10-20)

Points saillants :

- **L'outil `0box` est RÉEL** : il renvoyait une chaîne prefabricée (« 3 agents
  confinés ont débattu ») sans qu'aucun débat ait lieu. Il lance maintenant le vrai
  débat (`tui::run_0box_debate`, le même que `/0box` et `zer0 0box`) et renvoie la
  transcription au modèle.
- **CI remise au vert** (`cargo clippy -D warnings` échouait : la CI de `main` était
  rouge) + test presse-papier rendu indépendant de la machine.
- **Bug de transport** : les `routes` et les rôles partaient en direct alors que le
  provider actif passait par `zer0-gw` → replis en 401 « clé absente ».
- **0box testable de bout en bout** : CLI `zer0 0box <sujet>` headless, résultats
  d'outils visibles, résumés compacts, historique du débat enrichi des actions,
  budget d'endiguement paramétrable, arrêt d'endiguement ≠ échec.
- **Porte d'honnêteté sur les tâches** : `task_update → done` refuse si un livrable
  mentionné est absent ou vide.
- **Suivi d'usage des modèles** (`/quota`) : ce que l'agent **observe** (appels OK,
  429 réels, limites `X-RateLimit-*`).
- Clés hors de l'agent (`zer0-gw`), un seul identifiant par provider, menus (palette
  `/`, sélection `/model` et `/provider`, dépliage par bloc, molette), vision réelle
  (`/img`, `--img`), provider Z.ai, `zer0 update` — détails dans l'historique.

### Détail des correctifs

- **CI remise au vert après le travail 0box** (`cargo clippy -D warnings` échouait :
  3 erreurs → la CI de `main` était rouge) :
  - `iter().cloned().collect()` → `to_vec()` ; `run_0box_debate` et
    `run_0box_turn_with_retry` regroupés en `DebateSetup` / `DebateTurn`
    (9 arguments → trop pour clippy, et illisibles).
  - **Test presse-papier** : `wl-copy` ajoute TOUJOURS un `\n` final (le drapeau
    `--trim-newline` concerne l'*entrée*, pas l'ajout) → `paste()` retire
    **exactement un** retour à la ligne (une fin de ligne de l'utilisateur reste
    intacte) ; + test dédié.
- **L'outil `0box` est désormais RÉEL** : il renvoyait une chaîne prefabricée
  (« 3 agents confinés ont débattu ») alors qu'aucun débat n'avait lieu — le
  modèle croyait à un résultat inexistant. Il lance maintenant le **vrai** débat
  (`tui::run_0box_debate`, le même que `/0box` et `zer0 0box`) et renvoie la
  transcription au modèle. Contexte d'exécution partagé (`tools::init_runtime`).
  *Cycle de types async* (outil → débat → sous-agent → outil) cassé par
  `tools::execute_boxed` (effacement de type) — sinon E0391 « cycle detected ».
- **Bug de transport** : les `routes` et les rôles (`provider/model`) passaient
  **en direct** alors que le provider actif passait par `zer0-gw` → les replis
  échouaient en **401 « clé absente »**. `config::endpoint_for()` est désormais
  le point unique : passerelle si elle tourne (clé jamais dans l'agent), sinon
  direct.

- **Suivi d'usage des modèles** (`/quota`, `zer0 quota [--provider id]`) :
  - **format chiffré** : `0/50 (0 %) restant`, `47/50 (94 %)`, etc. — à partir des
    limites révélées par le fournisseur (corps des 429 OpenRouter, en-têtes
    `X-RateLimit-*` quand un provider les envoie) ;
  - **solde de clé en direct** : OpenRouter `/auth/key` interrogeable à la
    demande (usage total/journalier/mensuel, crédit restant en $ et %) ;
  - **Cline ne publie rien** (404 sur tous les endpoints d'usage testés) : seuls
    les appels observés et 429 y sont comptés — dit explicitement.

- **Alias débat `zer0box`** : `zer0box "<sujet>"` == `zer0 0box "<sujet>"` — via
  `visible_alias` clap et détection du nom du binaire (lien dur/copie nommé
  `zer0box.exe`), ou `scripts/zer0box.cmd` depuis les sources.

- **0box testable de bout en bout + corrections issues du run réel** (0BOX-TEST
  complet via le module 0box uniquement) :
  - **CLI `zer0 0box <sujet> [--perspectives a,b,c]`** : le débat séquentiel
    exécutable headless — jusqu'ici réservé à la TUI, donc invérifiable en
    automatisation.
  - **Résultats d'outils visibles** : `SubAgentEvent::ToolResult` créé et
    relayé (`⚙ appel` suivi de `✓ résultat`) ; l'affichage tronquait avant à
    100 chars le JSON brut, donnant l'impression d'un flux coupé.
  - **Résumés compacts** : les appels du débat passent par `summarize_tool_call`
    (plus jamais le `file_text` brut).
  - **Historique du débat enrichi des actions** : chaque tour transmet ses
    actions (task_add, fichiers…) aux suivants — sinon MAIN recréait en double
    des tâches déjà posées, et ne voyait jamais le travail des autres.
  - **MAIN vérifie `task_list` d'abord** (anti-duplication, étape 0 de sa procédure).
  - **`chat_template_kwargs` non envoyé à Google** (400 « Unknown name ») —
    paramètre llama.cpp/vLLM rejeté par l'endpoint OpenAI-compat de Gemini.
  - **Sous-agent** : dernier message assistant AYANT du texte (un tour qui
    finit sur des tool calls ne donnait plus « (aucune réponse) »).
  - **Budget d'endiguement paramétrable** (`run_turn_budgeted` /
    `run_subagent_budgeted`) : tours 0box = 20 itérations / 64 appels (le
    8/24 par défaut ne laissait même pas place aux `task_update` après
    vérification). Toujours borné — jamais illimité.
  - **Arrêt d'endiguement ≠ échec** : une détection de boucle ou un budget
    dépassé conserve le tour accompli au lieu de REJOUER tout le tour
    (run 4 : 15 minutes perdues en retry intégral).
  - **`enrich_system` adapté Windows** : `python` (pas `python3`), scripts
    temporaires plutôt que one-liners multi-instructions cassés par le quoting
    `cmd`.
- **Porte d'honnêteté sur les tâches** : `task_update → done` (outil agent, CLI,
  TUI, boucle `/work`) **refuse** de marquer une tâche « fait » si un livrable
  mentionné dans son texte (chemin ancré `~/…`, `/…`, `X:\…`) est **absent ou vide** —
  conséquence directe du run 0BOX-TEST, qui a révélé des tâches `[x]` sans livrable.
  Heuristique prudente (jamais de faux positif bloquant) ; contournement assumé :
  édition manuelle de `.zer0/tasks.md`.

- **Suivi d'usage des modèles** (`/quota`) : Zer0 affiche ce qu'il **observe** — appels
  réussis, 429 réels, et les limites (`X-RateLimit-*`) quand un fournisseur les révèle
  dans le corps d'un 429 (OpenRouter). Les listes `/models` et `/free` portent un badge
  par modèle (⚠ 429, · appels ok). Honnêteté : les API ne publient pas leur quota
  (Cline n'expose rien) — rien n'est inventé, les compteurs sont locaux à la session.

## v0.5.5 — Clés hors de l'agent, vision, menus (2026-10-17)

Points saillants depuis v0.5.4 (détail daté plus bas) :

- **Clés hors du processus agent** : `zer0-gw` (passerelle, utilisateur dédié) détient
  les clés ; `/provider add` les y dépose en écriture seule ; `code_exec` n'hérite plus
  d'aucun secret ; egress nftables (agent → loopback) ; unités systemd durcies.
- **Un seul identifiant par provider** (fin du préfixe `gw-`) ; transport
  passerelle/direct automatique ; migration des configurations existantes.
- **Menus** : palette `/` filtrante, sélection navigable `/model` et `/provider`,
  dépliage **par bloc** des sorties d'outils (clic droit / Ctrl+O), molette souris.
- **Vision réelle** : `/img` (TUI) et `--img` (CLI) envoient l'image à un modèle multimodal.
- **Nouveau provider** : Z.ai (Zhipu GLM).
- **Mise à jour** : `zer0 update` — la CI publie désormais le binaire brut de chaque
  plateforme et un `SHA256SUMS` (vérifié au téléchargement).
- **Corrections** : URL upstream avec segment de version (z.ai), écriture des clés sous
  systemd (`ReadWritePaths`), modèles d'embedding refusés en chat, `/role off` = un seul
  modèle sans bascule, redémarrage du service à la mise à jour.

## Mise à jour automatique : `zer0 update` (2026-10-17)

- **`zer0 update`** (CLI) et **`/update`** (TUI) : interroge la dernière release GitHub,
  télécharge le **binaire brut** de la plateforme, **vérifie le SHA256** (si
  `SHA256SUMS` est publié — sinon le dit), puis remplace le binaire en place (rename
  atomique). Options `--check` et `--force` ; les notes de version sont résumées.
- **CI** : chaque release publie désormais le **binaire brut** (en plus des archives)
  et des **checksums par cible**, agrégés en un `SHA256SUMS` unique.
- **`scripts/update.sh`** : installe/rafraîchit un lanceur stable
  (`~/.local/bin/zer0gh0st`) depuis les Releases — utile quand on construit depuis
  les sources (cargo écrit dans `target/release`, pas dans le PATH).
- Limites assumées : API GitHub non authentifiée (60 req/h), checksum issu du même
  dépôt (pas de signature), Windows ne peut pas écraser un `.exe` en cours.
- **Garde-fou dev** : si le binaire tourne depuis `target/` (build Cargo), `zer0 update`
  **refuse** de le remplacer — pour un dev, c'est `cargo build --release` qui fait foi
  (`--force` pour passer outre).

## Fil compact : sorties repliées, dépliage par bloc (2026-10-17)

- Les **sorties d'outils** (`role: tool`) sont repliées dans le fil : **3 lignes** +
  « … N ligne(s) de plus — clic droit ou Ctrl+O ». Le dépliage est **par bloc** :
  **clic droit** sur une sortie la déplie/replie (plafond 200 lignes), **Ctrl+O** agit
  sur la dernière — comme `pi`, sans noyer le fil.
- Les **appels d'outils** tiennent sur une ligne : `text_editor create /tmp/x.py
  (42 lignes)`, `code_exec` → 1ʳᵉ ligne de la commande, `web_search` → la requête.
  Fini le script Python entier déversé dans le fil.
- Pied de page et aide : `Ctrl+O` / clic droit.

## Vision : `/img` et `--img` (2026-10-17)

- **`/img <chemin>`** (TUI) joint une image (PNG/JPEG/WebP/GIF) au **prochain** message ;
  `/img clear` la retire, `/img` montre l'état. En CLI : `--img <chemin> --once "…"`.
- Le message part en **content parts** OpenAI (`[{type:text}, {type:image_url}]`, data URL
  base64) via `ChatMessage::to_wire_json()` ; sans image, `content` reste une chaîne.
- **L'image n'est pas persistée** (`serde(skip)`) : jamais écrite dans le JSONL de
  conversation (taille, données sensibles) — après `--resume`, refaire `/img`.
- **Garde-fou honnête** : `/img` compare le modèle actif au catalogue `/models`
  (`architecture.input_modalities`, marqué « 🖼 images » dans `/model`) et avertit si le
  modèle n'accepte pas d'image, au lieu de laisser partir une requête vouée à l'échec.
- Vérifié de bout en bout : agent → passerelle → `nvidia/nemotron-3.5-super-vl:free`
  décrit réellement une capture d'écran (noms de stations lus dans l'image).

## Défilement à la molette (2026-10-17)

- **Molette souris** : défile la **conversation** (3 lignes/cran, retour en bas =
  suivi auto réactivé). Au-dessus du panneau « Commandes », elle défile le panneau ;
  sélecteur ouvert (`/model`, `/provider`, palette `/`), elle navigue dans la liste.
  `PgUp`/`PgDn` restent disponibles.

## Menus de sélection + un seul identifiant provider (2026-10-17)

- **`/model`** (sans argument) et **`/provider`** (sans argument) ouvrent un **menu de
  sélection** : ↑↓ / PageUp·PageDown, **filtre en tapant**, Entrée pour valider,
  Échap pour annuler. Le panneau « Commandes » cliquable reste.
- **Palette `/`** : taper `/` en début de saisie ouvre la **liste filtrante des
  commandes** (↑↓, taper = filtrer, Entrée : exécute — ou insère si la commande attend
  un argument, Échap : annuler). Le panneau « Commandes » cliquable reste.
- **Fin du préfixe `gw-`** : un seul identifiant par provider (`openrouter`, `nvidia`,
  `zai`…). Le transport (passerelle ou direct) est décidé automatiquement : si
  `zer0-gw` répond, les providers intégrés passent par elle et **aucune clé n'est
  résolue dans l'agent**. Les configs `gw-*` existantes sont migrées au démarrage.

## Correctif zer0-gw : dépôt de clés (2026-10-16)

- **`ProtectSystem=strict` rendait `/etc` en lecture seule** pour le service :
  `/provider add` échouait en **500** avec `Read-only file system (os error 30)`.
  Ajout de `ReadWritePaths=-/etc/zer0-gw` à l'unité, et droits de dossier
  explicites dans `scripts/gw-setup.sh` (`chown root:zer0-gw`, `chmod 1770`).
- La réponse HTTP 500 (et le journal) indique désormais la **cause système**
  (`Permission denied`, `Read-only file system`…) au lieu d'un message générique.

## `/role off|on` — tester un seul modèle (2026-10-15)

- **`/role off`** (TUI) / **`zer0 role off`** (CLI) : désactive le routage par rôle —
  tout (chat, sous-agents, `/work`, compaction) passe par le **modèle du provider
  actif**. **`/role on`** réactive. Persisté dans `config.toml` (`roles_enabled`,
  défaut `true` : les configs existantes restent actives). Barre de statut : `role:off`.
- **Sans bascule** : en mode `off`, les `routes` de failover sont ignorées — un échec
  donne une erreur au lieu de changer silencieusement de modèle (avant : la chaîne de
  bascule continuait à s'appliquer, d'où des modèles différents d'un message à l'autre).
  `/role on` rétablit **routage par rôle + bascule**.
- `/role use <role>` réactive automatiquement les rôles ; `/role list` indique l'état.

## Découpage de sécurité : zer0-gw + egress (2026-10-15)

- **`zer0-gw`** (nouveau binaire, `cargo build --release --features gateway`) : passerelle
  de clés. Les clés vivent dans `/etc/zer0-gw/` ; l'agent n'appelle que le loopback
  **sans aucune clé** — l'exfiltration de clé devient impossible. Liste blanche de
  chemins, rate-limit par provider, refus des redirections, journal JSON **sans clés**.
- **`platform::scrub_secrets`** : `code_exec` (et tout shell enfant) n'hérite plus des
  variables « secrets » (`*API_KEY*`, `*TOKEN*`, `*PASSPHRASE*`, `ZER0_VAULT*`, …).
- **`/provider add <id> <clé>`** (TUI + `zer0 provider add`) : quand `zer0-gw` tourne,
  la clé est **déposée sur la passerelle** (endpoint admin loopback, écriture seule) et
  l'agent bascule sur `gw-<id>` **sans clé locale** ; sinon repli local averti. Nouvelle
  commande **`/provider keys`** (état des clés détenues par `zer0-gw`) ; `provider remove`
  oublie aussi la clé côté passerelle.
- **`deploy/`** : unités systemd durcies (`IPAddressAllow=localhost` pour l'agent
  headless), règles nftables d'egress par utilisateur (`zer0` → loopback uniquement ;
  `zer0-gw` → 443 sortant), `scripts/gw-setup.sh`.
- **Limites déclarées** (SAFETY.md §7) : l'agent peut toujours utiliser le relais
  (quota) — plafonds de dépense côté provider recommandés.

## Projet test 0box (2026-10-13)

- **`0BOX-TEST.md`** : projet test « compteur » (8 tâches, 1 par rôle) pour valider le
  module `0box` de @darkdober4 — dispatch, dépendances, livrables vérifiables, verdict
  final du rôle `review`.

## Conscience de l'environnement (2026-10-13)

- Bloc **## Environnement** injecté dans le prompt : **date**, **OS/arch**, **dossier
  courant (pwd)**, **racine du projet** (data/knowledge/tasks), **git** — plus une consigne
  « ne devine pas les chemins ».
- Corrige les chemins inventés (`/a0/usr/workdir`, HOME…) et les erreurs de date.

## Reprise auto + dossier de données (2026-10-13)

- **Fin des réponses tronquées** : `max_tokens` par défaut **2048 → 8192**, et **reprise
  automatique** quand `finish_reason=length` (jusqu'à 4 fois) → **plus besoin de F7**.
- **Dossier de données dédié** : `~/.local/share/zer0gh0st` (avant `…/zer0`, qui se
  mélangeait avec l'ancien projet Python). Migration auto (conversations, mémoire, personas).

# Zer0 — CHANGELOG

> Même court, il doit exister. Format : versions du projet (`ROADMAP.md`).

---

## v0.5 — Orchestration (2026-10-06)

- **Containment de boucle** : détection de répétition (même outil + mêmes arguments 3×),
  budget de 24 appels d'outils par tour.
- **Plafonds de flux** : réponse coupée à 1 Mo, timeout d'inactivité 120 s (contre les modèles qui bouclent).
- **Tâches** : `.zer0/tasks.md` (statuts `[ ]`/`[~]`/`[x]`), outils `task_add`/`task_list`/`task_update`,
  CLI `task`, TUI `/task`.
- **Boucle `/work`** : prochaine tâche → *claim* → sous-agent du rôle associé → *fait*.
- **Sous-agents** : `/ask <role> <question>` (contexte isolé).
- **Revue** : `/review <fichier>` → `VERDICT: OK|PROBLEMES`.

## v0.4 — Mémoire honnête (2026-10-06)

- Store persistant `~/.local/share/zer0/memory.jsonl` (chmod 600).
- Recherche : cosinus si embedder configuré, sinon sous-chaîne.
- **Enveloppe d'honnêteté** : `degraded` (+ raison), `ranking_meaningful`, `trust_enforced: false`,
  `testables/probées/total`. Chaque vecteur enregistre son embedder (inter-espaces → non significatif).
- `trigger` optionnel → entrée testable ; `trust` = `untrusted`, jamais appliqué.
- Outils `memory_save`/`memory_search` ; CLI `memory …` ; TUI `/memory`, `/remember`, `/recall`, `/forget`.
- **Compaction d'historique** : résumé via le rôle `utility` au-delà de ~6000 tokens ; `/compact`.

## v0.3 — Routage par rôle (2026-10-06)

- Rôles `main/code/reasoning/writing/vision/fast/utility/review` → `provider/model`.
- Sélection **automatique** par catégorie (classifieur par **mot entier** ; corrige le piège « r*api*de »).
- Chaîne par rôle : modèle du rôle en tête, puis endpoint actif + routes.
- Config par projet `.zer0/project.toml` (`provider/model/roles/routes/system_prompt/prompt_file`).
- `role auto` assigne chaque rôle au meilleur modèle `:free` de sa catégorie ; `fast`/`utility` favorisent les petits modèles.

## v0.2 — Fiabilité des modèles (2026-10-06)

- **Mémoire des échecs** : endpoint défaillant blacklisté 120 s.
- **Backoff 429** (2 s, 4 s) puis bascule ; messages dédiés par code HTTP (401/402/403/404).
- Statut : endpoint **réellement** utilisé après bascule.
- **Google natif** `/v1beta/models` (la liste OpenAI-compat renvoyait 404).
- **Code non tronqué** : `max_tokens` 8192 par défaut + alerte `finish_reason = length`.
- Fichiers : création via `text_editor` (ON par défaut), chemins absolus, test via `code_exec`.
- **Réponse vide** (modèle de raisonnement) traitée comme échec → bascule.
- Contrôle du raisonnement : `chat_template_kwargs.enable_thinking` (Qwen) + `reasoning.enabled` (OpenRouter).

## v0.1 — Socle (2026-10-06)

- TUI ratatui : banner `AGENT ZERO`, statut, chat, input, footer, F3–F7.
- Animation arc-en-ciel du banner, scroll du chat, affichage du raisonnement.
- Boucle de messages + appels d'outils, streaming SSE.
- Outils `code_exec`, `text_editor`.
- Inférence locale `llama-server`, **bridée à 2 cœurs** (`taskset`).
- **9 providers** + personnalisés ; clés en config ou env ; en-têtes OpenRouter.
- Découverte des modèles `:free` ; `select` par benchmarks (Artificial Analysis).
- **Failover** d'endpoints annoncé ; `route auto`.
- Mode headless `--once`.

## Portages depuis le Zer0 Python (2026-10-06)

- `/search` + outil `web_search` (DuckDuckGo instant answer).
- `/stop` (annulation), `/context`, `/whoami` (personas).
- `/vault` (Argon2 + ChaCha20-Poly1305), `/pdf` (info/text/merge/split), `/jev` (score 0-3).

## Modules fun (`mods/`, 2026-10-06)

- `/radio` (panneau latéral, playlist auto), `/bintime` (panneau), `/pong` (vs agent, plein écran).
- `/kristal` (Qt6), `/streaming` (libtorrent/mpv/pygame), `/retro` (HTML/JS) — lancés en fenêtre séparée.
- Sources **copiées dans `mods/`** (autonome). Pong rétro rebaptisé « Toi contre ZER0 ».

## Ajustements quota & modèles (2026-10-06)

- **`max_tokens` par défaut 8192 → 2048** : beaucoup de providers comptent la sortie
  *réservée* dans le quota, donc un plafond élevé brûle le rate-limit très vite.
- **Compaction** : seuil 6000 → **4000** tokens (historique re-envoyé moins long).
- **Normalisation du nom de modèle** : un `model` qui commence par `{provider}/`
  (slug copié d'OpenRouter, ex. `nvidia/deepseek-ai/…`) est nettoyé pour l'API native du
  provider (sauf OpenRouter, dont l'espace de noms est significatif). Corrige le 404
  `nvidia/nvidia/…`.
- **Repli non-stream** : certains modèles (vLLM « diffusion », ex. `google/diffusiongemma-26b`)
  renvoient un `content` **vide en streaming** mais répondent en non-stream. Le client retente
  alors **une fois sans streaming**. Corrige les modèles qui marchent ailleurs mais pas ici.
- **Repli raisonnement** : si le `content` reste vide mais que `reasoning_content` est présent
  (modèles de raisonnement type `meta/muse-glimmer-30b`), le raisonnement est utilisé comme
  réponse au lieu d'échouer.

## STT : aide au diagnostic (2026-10-11)

- **`ZER0_PYTHON`** : force l'interpréteur Python (utile si `faster-whisper` est
  dans un **venv** plutôt que dans le Python système — Ubuntu/WSL).
- **`zer0 stt <fichier.wav>`** : teste la transcription **sans micro** (diagnostic).
- Message d'erreur STT plus explicite (interpréteur utilisé + piste d'installation).

## Démo automatisée + menu au clavier (2026-10-11)

- **Menu « Commandes » au clavier** : **`F2`** donne le focus, **`↑`/`↓`** naviguent,
  **`Entrée`** déplie/exécute, **`Esc`** quitte le menu. Le clic souris fonctionne toujours.
  (Permet d'automatiser le menu dans une vidéo.)
- **`scripts/demo.tape`** : scénario **VHS** qui rejoue commandes, raccourcis (F3–F10),
  menu (au clavier) et une petite tâche → génère un `demo.gif`.
- **`scripts/demo.sh`** : lance VHS (message d'installation si absent) et produit la vidéo.
- **VHS ne gère pas les touches F ni `Escape`** → commandes équivalentes ajoutées :
  `/menu`, `/readwrite`, `/codeexec`, `/chats`, `/nudge`, `/0chan`, `/mic` (l'aperçu vidéo
  les utilise ; elles servent aussi sans clavier F).
- Le `demo.gif` généré **n'est pas versionné** (`.gitignore`).

## Curseur de saisie (2026-10-11)

- **Déplacement du curseur** dans la barre de saisie : `←` `→`, `Suppr`, `Ctrl+A` (début),
  `Ctrl+E` (fin). L'insertion, le `Backspace` et le collage se font **au niveau du curseur**.
- Le rendu **suit le curseur** (défilement si le texte dépasse la hauteur visible).
- Opère par **caractères** (jamais par octets).

## `/jev` lit le code complet (2026-10-11)

- **Correction** : `/jev` (et `zer0 jev`) **tronquait à 20 000 octets** → analyse partielle
  et score erroné. Il envoie désormais **tout le fichier**.
- Fichiers volumineux : **découpage en parties** (par **caractères**, jamais par octets →
  plus de risque de panic UTF-8), puis **score global = moyenne** des parties.
- Nouvelle fonction `agent::subagent::run_jev` + parsing `SCORE:` (testé).

## Ergonomie saisie & corrections (2026-10-11)

- **Saisie simplifiée** : plus de cadre/règles autour de la zone de saisie — juste
  `❯ texte` (1..=8 lignes). L'info `ctx`/modèle reste dans la barre de statut.
- **Collage multi-ligne corrigé** : activation du **bracketed paste** + gestion de
  `Event::Paste` → un texte collé (avec retours à la ligne) est inséré d'un bloc au lieu
  d'envoyer le message au premier saut de ligne.
- **`/stop` réparé** : les commandes slash sont désormais traitées **pendant une génération**
  (avant, la garde `streaming` bloquait `/stop`).
- **README** : section **Désinstaller** (sources, binaires précompilés, `cargo install`,
  dossiers de données par OS).

## Qualité & CI (2026-10-11)

- **CI GitHub Actions** : `cargo build --release` + `cargo clippy --all-targets -- -D warnings`
  à chaque push/PR, badge dans le README.
- **Nettoyage clippy** : les 14 avertissements sont résolus (`is_multiple_of`, `as_chunks`,
  `&Path` au lieu de `&PathBuf`, `sort_by_key`, `while let`, etc.) → **0 warning**.
- **`config.example.toml`** : modèle de configuration commenté.

## Portabilité Linux (2026-10-10)

- **Presse-papier** : `wl-copy` (Wayland) → `xclip` / `xsel` (X11) → **OSC 52** (repli terminal).
- **Micro** : `pw-record` (PipeWire) → `parecord` (PulseAudio) → `arecord` (ALSA) — choisit
  automatiquement le premier disponible. Message clair si aucun.
- **`scripts/install.sh`** : installe les dépendances selon la distribution
  (Debian/Ubuntu/Kali, Fedora, Arch/omarchy, openSUSE), puis Rust (rustup) et compile.
  Mode `--minimal` pour ne prendre que le nécessaire à la compilation.

## Menu de commandes cliquable (2026-10-10)

- Panneau **Commandes** à droite de la TUI, **entre** le bloc zero-chan (haut) et le badge
  `0chan/mic` (bas). Tous les groupes et sous-commandes, **cliquables** :
  - clic sur un groupe (`▸ provider`) → déplie/replie ;
  - clic sur une commande sans argument → l'exécute ;
  - clic sur une commande à argument (`add `, `use `) → l'insère dans la saisie.
- **Molette** au-dessus du menu pour **défiler** (l'arbre complet dépasse la hauteur).
- Les **mods** (`radio`, `bintime`, `pong`…) s'ouvrent **à droite du menu** (≥ 130 colonnes).
- Nouveau fichier `src/tui/menu.rs` (arbre + rendu + zones cliquables).

## Bot Telegram (2026-10-10)

- **`zer0 telegram`** (mode CLI) ou **`/telegram on|off`** dans la TUI : bot **long polling**
  (aucun port à ouvrir), **allowlist** (auto-claim du premier chat).
- Texte, **vocal** (transcrit par Whisper), **fichiers** (dossier `drop/incoming`).
- **Progression** : message `⏳` édité (appels d'outils), puis `✅ terminé` + réponse
  finale dans un **nouveau message** (→ notification).
- Commandes : `/help` `/new` `/status` `/progress` `/exec on|off` `/stop` `/shutdown`.
- `/exec` désactive `code_exec` par défaut (shell à distance = sensible).
- Token dans `~/.config/zer0/telegram.env` (chmod 600) — **jamais** dans le dépôt.

## Contexte du modèle (2026-10-09)

- Le compteur `ctx N%/XXX` utilise désormais la **vraie fenêtre de contexte** du modèle,
  au lieu de 200k codé en dur (NVIDIA ne la rapporte pas → Kimi-k3 s'affichait à tort à 200k).
- Résolution par priorité : **override manuel** (`/ctx`) > fenêtre rapportée par le
  provider (`/models`) > **table de modèles connus** (Kimi-k2/k3 = 1M, Gemini = 1M,
  Claude = 200k, GPT-4o/4.1 = 128k…) > 200k.
- Nouvelle commande **`/ctx [N|1M|200k|auto]`** (affiche, force et persiste le contexte).

## UI « Pi-like » (2026-10-09)

- **Chat** : suppression du cadre ; messages rendus en blocs — `❯` (fond panneau) pour
  l'utilisateur, `◕` pour l'assistant, `⚙`/`·` discrets. Coloration `code`, **gras**,
  et blocs ``` en surbrillance.
- **Saisie** : règles haut/bas façn Pi (`─`), infos `provider/model · think` à gauche et
  `ctx N%/200k · cwd` à droite. Plus de ligne « modèle » séparée, saisie resserrée.
- **Historique restauré affiché** (avant : chargé en mémoire seulement, invisible).
- Palette alignée sur le thème Pi « omarchy-system ».
- **Badge 0chan/mic** compact en encadré (`┌ 0chan/mic ┐` / `│ off / off │` / `└──┘`)
  au lieu de deux boutons empilés ; les deux `off` restent cliquables (0chan / micro).

## Sessions de conversation : F6 / F7 (2026-10-08)

- **F6 — Chats** : pop-up listant les sessions sauvegardées (`conversation_*.jsonl`),
  navigation ↑↓, `Entrée` pour charger, `Esc`/`q`/`F6` pour fermer. Charger une session
  archive l'actuelle puis la rend active (rien n'est perdu).
- **F7 — Nudge** : relance l'agent (« Continue ton raisonnement ou ton exécution. »)
  sans rien taper — utile après un arrêt, un stream vide ou une coupure.
- `/new` **archive** désormais la conversation active (`conversation_<timestamp>.jsonl`)
  au lieu de la supprimer ; les fichiers vides ne sont pas conservés.
- Refactor : `send_message` délègue à `start_turn` (réutilisé par le nudge).

## Nom : ZER0│GH0ST (2026-10-08)

- Le bandeau passe de « AGENT ZERO » à **`ZER0│GH0ST`**, avec le sous-titre **`ghost in the shell`**.
- Référence *Ghost in the Shell* : le **shell** = ton terminal Unix, le **ghost** = l'IA dedans.
  Leet cohérent (`0` dans ZER0 et GH0ST), unique, tech/dev/hacker.
- Rendu : glyphes `H`, `S`, séparateur `│` ajoutés au banner ASCII (`src/tui/banner.rs`).

## Sélection souris & nettoyage (2026-10-08)

- **Sélection au glisser** : clic + glisser dans le chat ou la barre de saisie → surbrillance,
  et **copie automatique** dans le presse-papier (`wl-copy`, sinon OSC 52) au relâchement.
- **Ctrl+O retiré** (le micro reste sur **F10**).
- **Conversation corrompue** : une réponse charabia d'un modèle précédent était persistée dans
  `conversation.jsonl` et rechargée → polluait le contexte. Fichier nettoyé ; le modèle actuel
  (`moonshotai/kimi-k3`) répond correctement.

## Historique de conversation (2026-10-08)

- **Conversation persistée** dans `~/.local/share/zer0/conversation.jsonl` (une ligne par message).
- **Rechargée au démarrage** (200 derniers messages) → l'agent se souvient entre les sessions.
- `/history` : montre les derniers messages + le chemin du fichier.
- `/new` : efface la conversation (affichage **et** fichier).

## Recherche web & injection (2026-10-08)

- **`web_search` corrigé** : passe de l'API *Instant Answer* (qui ne renvoie rien pour
  la plupart des requêtes, ex. « bananes ») au **HTML de DuckDuckGo** parsé
  (`result__a` / `result__snippet`), avec décodage des liens `uddg=` et repli sur l'API.
- **Injection de connaissance : filtre de pertinence** — un extrait n'est injecté que si le
  **nom de fichier** contient un mot significatif de la requête. Fini le gros fichier de 1 Mo
  injecté pour « Salut tout le monde ». Stopwords étendus (salut, tout, monde, raconte…).

## Zero-chan — voix intégrée (2026-10-08)

- **UI sans doublon** : bloc d'état à droite du banner + boutons `0chan` / `micro` à droite de la
  barre de saisie ; **réutilise le chat et l'input de l'agent** (choix de conception explicite).
- **Boutons cliquables** (capture souris crossterm) + raccourcis `F8` (0chan) / `F9` (micro).
- **STT** : `pw-record` (16 kHz mono) → `faster-whisper` (base, CPU int8, local) → le texte
  reconnu est envoyé dans la chat de l'agent.
- État animé : `en veille` / `écoute…` / `réfléchit…` / `parle…` (suivi de l'agent).
- Module `src/zerochan.rs`.

## Confort TUI (2026-10-06)

- **Zone de saisie multi-ligne** : le texte passe à la ligne quand il atteint le bord
  (hauteur dynamique 1→8 lignes, curseur suivi) au lieu de devenir invisible.
- **Historique des messages** : `↑`/`↓` rappellent les entrées précédentes (brouillon restauré
  quand on redescend). Le défilement du chat passe à `PageUp`/`PageDown` (+ `Home`/`End`).

## Base de connaissance (2026-10-06)

- **`knowledge.rs`** : base locale **SQLite + FTS5** (SQLite embarqué via `rusqlite`), avec
  recherche **hybride** FTS5 + cosinus sur embeddings stockés en BLOB (vectoriel si un embedder
  est configuré, sinon FTS5/LIKE — dégradation annoncée).
- Sources copiées dans **`<ZER0v1>/base_connaissance`** (105 fichiers / 202 chunks indexés).
- Outil agent **`knowledge_search`** ; CLI `kb stats|search|ingest|rebuild` ; TUI `/kb`.
- **Correctif ingestion** : chaque chunk était auto-commité → **un fsync par chunk** (≈90 s pour un
  fichier de 1 Mo, désormais **~0,7 s**). Ingest dans une **transaction unique** +
  `PRAGMA synchronous=NORMAL`. Chunker refait (coupe aussi les lignes longues).
- **Log d'injection** : `knowledge.log` + notice TUI `📚 connaissance injectée : …` + `/kb log`.
- Requête FTS améliorée : **stopwords** FR/EN + logique **AND** puis OR (moins de bruit).
- **Injection automatique** : avant chaque tour, recherche FTS5 sur le message → les 3 meilleurs
  extraits sont ajoutés au prompt (bloc « Connaissance locale — DONNÉES ») ; `/kb auto` pour couper.

## Docs (2026-10-06)

- Ajout de `SPEC.md`, `SAFETY.md`, `ARCHITECTURE.md`, `ECOSYSTEM.md`, `CHANGELOG.md`.
- Réécriture du `README.md`.
- Ajout de `LICENSE` (MIT, © 2026 richerrail) et des mentions de licence (`Cargo.toml`).

---

## Prochain (à décider)

- Mesure chiffrée : taux de bons routages de rôle, latence, taux d'échec (à publier par version).
- Portage **macOS** (petit) puis **Windows** (moyen) — voir `ECOSYSTEM.md` §5.
- `v0.5`/`v1.0` : orchestration multi-agents, identité Ed25519 (`ROADMAP.md`).
