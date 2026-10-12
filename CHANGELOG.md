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
