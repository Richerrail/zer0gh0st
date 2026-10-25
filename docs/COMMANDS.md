# Commandes — ZER0|GH0ST

> Fichier généré depuis le CLI (`zer0 commands > docs/COMMANDS.md`).
> Ne pas éditer à la main : un test échoue si ce fichier diverge du binaire.

## `zer0`

ZER0|GH0ST - agent IA en terminal (CLI/TUI)

| Option | Description |
|---|---|
| `--server <SERVER>` | Override the OpenAI-compatible base URL (includes `/v1`) |
| `--model <MODEL>` | Override the model id |
| `--system-prompt <SYSTEM_PROMPT>` | Override the built-in system prompt |
| `--no-tools <NO_TOOLS>` | Start without any tools enabled |
| `--max-tokens <MAX_TOKENS>` | Cap the number of tokens generated per model call. Kept low by default: many providers count the *reserved* output against the rate limit, so a huge value burns free-tier quota fast |
| `--think <THINK>` | Enable the model's reasoning/thinking mode (much slower on weak CPUs) |
| `--allow-exec <ALLOW_EXEC>` | Start with the shell tool (`code_exec`, F4) enabled |
| `--resume <RESUME>` | Reprendre la conversation active au démarrage (sinon : nouvelle conversation) |
| `--once <ONCE>` | Run a single prompt headlessly, print the answer, then exit |
| `--img <IMG>` | Headless: attach an image (PNG/JPEG/WebP/GIF) to the prompt (vision) |

### `zer0 provider`

Manage providers and their API keys

#### `zer0 provider list`

List all known providers and whether a key is configured

#### `zer0 provider keys`

Show where the keys live: zer0-gw gateway status (never the values)

#### `zer0 provider add`

Store an API key for a known provider (reads $ENV if omitted)

| Option | Description |
|---|---|
| ` <ID>` |  |
| ` <KEY>` |  |

#### `zer0 provider add-custom`

Define a custom OpenAI-compatible provider

| Option | Description |
|---|---|
| ` <ID>` |  |
| ` <BASE_URL>` | Base URL including `/v1` (or equivalent), e.g. https://host/v1 |
| `--name <NAME>` |  |
| `--env <ENV>` |  |

#### `zer0 provider use`

Set the active provider

| Option | Description |
|---|---|
| ` <ID>` |  |

#### `zer0 provider remove`

Remove a stored API key

| Option | Description |
|---|---|
| ` <ID>` |  |

### `zer0 update`

Update the binary from the GitHub Releases

| Option | Description |
|---|---|
| `--check <CHECK>` | Only check whether a newer version exists |
| `--force <FORCE>` | Reinstall even when the version is already current |

### `zer0 models`

List the models of a provider

| Option | Description |
|---|---|
| `--free <FREE>` | Only show free (`:free` / $0) models |
| `--provider <PROVIDER>` | Provider id (defaults to the active provider) |

### `zer0 model`

Set the active model

#### `zer0 model use`

Set the active model

| Option | Description |
|---|---|
| ` <ID>` |  |

### `zer0 select`

Recommend free models for a given problem

| Option | Description |
|---|---|
| ` <PROBLEM>` | The problem statement |

### `zer0 route`

Ordered provider failover chain

#### `zer0 route list`

Show the ordered failover chain

#### `zer0 route add`

Append a route: `zer0 route add openrouter/qwen/qwen3.8-27b:free`

| Option | Description |
|---|---|
| ` <ROUTE>` |  |

#### `zer0 route auto`

Fill the chain from the provider's free models (ranked)

| Option | Description |
|---|---|
| `--provider <PROVIDER>` | Provider to pull free models from (default: openrouter) |
| `--top <TOP>` | How many models to keep |

#### `zer0 route clear`

Remove all failover routes

### `zer0 role`

Named model presets (role -> provider/model)

#### `zer0 role list`

Show roles and their models

#### `zer0 role off`

Disable role routing: one model (the active provider) for everything

#### `zer0 role on`

Re-enable automatic role routing

#### `zer0 role set`

Associate a role with a model: `zer0 role set code openrouter/qwen/qwen3.8-27b:free`

| Option | Description |
|---|---|
| ` <ROLE>` |  |
| ` <ROUTE>` |  |

#### `zer0 role unset`

Remove a role preset

| Option | Description |
|---|---|
| ` <ROLE>` |  |

#### `zer0 role auto`

Auto-assign each role to the best free model for its category

| Option | Description |
|---|---|
| `--provider <PROVIDER>` |  |

### `zer0 project`

Project configuration

#### `zer0 project show`

Show the current project config

#### `zer0 project init`

Create a `.zer0/project.toml` template

### `zer0 memory`

Persistent memory (honest store)

#### `zer0 memory list`

List stored entries

| Option | Description |
|---|---|
| `--limit <LIMIT>` |  |

#### `zer0 memory save`

Save a fact: `zer0 memory save "text" [--trigger "future query"]`

| Option | Description |
|---|---|
| ` <TEXT>` |  |
| `--trigger <TRIGGER>` |  |

#### `zer0 memory search`

Search memory: `zer0 memory search "query"`

| Option | Description |
|---|---|
| ` <QUERY>` |  |
| `--k <K>` |  |

#### `zer0 memory stats`

Show the store path and counts

#### `zer0 memory forget`

Delete an entry by id

| Option | Description |
|---|---|
| ` <ID>` |  |

#### `zer0 memory clear`

Delete all entries

### `zer0 embeddings`

Configure the embedding backend for semantic memory

#### `zer0 embeddings set`

Configure the embedding backend

| Option | Description |
|---|---|
| ` <PROVIDER>` |  |
| ` <MODEL>` |  |

#### `zer0 embeddings unset`

Remove the embedding backend

#### `zer0 embeddings show`

Show the current embedding backend

### `zer0 apis`

Catalogue of public APIs (source: public-apis/public-apis)

#### `zer0 apis import`

Import a TSV catalogue (see scripts/kb-import-public-apis.sh)

| Option | Description |
|---|---|
| ` <PATH>` |  |

#### `zer0 apis search`

Search the catalogue

| Option | Description |
|---|---|
| ` <QUERY>` |  |
| `--k <K>` |  |
| `--keyless <KEYLESS>` | Only APIs usable without an API key |

#### `zer0 apis categories`

Categories and counts

#### `zer0 apis stats`

Catalogue status (count, source, import date)

### `zer0 task`

Markdown task list (division of work)

#### `zer0 task list`

List tasks

#### `zer0 task add`

Add a task

| Option | Description |
|---|---|
| ` <TEXT>` |  |
| `--role <ROLE>` |  |

#### `zer0 task done`

Mark a task as done

| Option | Description |
|---|---|
| ` <ID>` |  |

#### `zer0 task claim`

Mark a task as in progress

| Option | Description |
|---|---|
| ` <ID>` |  |

#### `zer0 task rm`

Remove a task by id

| Option | Description |
|---|---|
| ` <ID>` |  |

#### `zer0 task clear`

Remove all completed ([x]) tasks

### `zer0 search`

Web search (DuckDuckGo)

| Option | Description |
|---|---|
| ` <QUERY>` |  |
| `--max <MAX>` |  |

### `zer0 whoami`

Personas (system-prompt presets)

#### `zer0 whoami list`

List available personas

#### `zer0 whoami show`

Show a persona's content

| Option | Description |
|---|---|
| ` <NAME>` |  |

#### `zer0 whoami add`

Create a persona from a file (`-` for stdin)

| Option | Description |
|---|---|
| ` <NAME>` |  |
| ` <FILE>` |  |

### `zer0 vault`

Encrypted API-key vault

#### `zer0 vault status`

Show vault status

#### `zer0 vault setup`

Encrypt the keys from config.toml into the vault

#### `zer0 vault unlock`

Unlock the vault for this session

#### `zer0 vault lock`

Lock the vault

#### `zer0 vault reset`

Delete the vault (keys stay forgotten)

### `zer0 pdf`

Local PDF tools

#### `zer0 pdf info`

Page count and metadata

| Option | Description |
|---|---|
| ` <PATH>` |  |

#### `zer0 pdf text`

Extract text

| Option | Description |
|---|---|
| ` <PATH>` |  |

#### `zer0 pdf merge`

Merge: `zer0 pdf merge out.pdf a.pdf b.pdf`

| Option | Description |
|---|---|
| ` <OUTPUT>` |  |
| ` <INPUTS>` |  |

#### `zer0 pdf split`

Keep pages first..=last: `zer0 pdf split in.pdf out.pdf 1 3`

| Option | Description |
|---|---|
| ` <PATH>` |  |
| ` <OUTPUT>` |  |
| ` <FIRST>` |  |
| ` <LAST>` |  |

### `zer0 jev`

Calibrated code-quality score (rubric via the review role)

| Option | Description |
|---|---|
| ` <FILE>` |  |

### `zer0 commands`

Référence Markdown des commandes (générée depuis le CLI)

### `zer0 audit`

Journal d'audit des actions de l'agent (commandes exécutées, fichiers écrits)

| Option | Description |
|---|---|
| `--tail <TAIL>` | Nombre de lignes à afficher (les plus récentes) |

### `zer0 bintime`

Preview: CRT binary clock (full screen)

### `zer0 pong`

Preview: Pong vs the agent (full screen)

### `zer0 pong-frame`

Hidden: dump one Pong frame (diagnostic)

### `zer0 stt`

Teste la transcription STT sur un fichier audio (WAV) — sans micro

| Option | Description |
|---|---|
| ` <FILE>` |  |

### `zer0 jimmy`

Jimmy le cancre — répond « je sais pas »

| Option | Description |
|---|---|
| ` <TASK>` |  |

### `zer0 provoke`

Jimmy d'abord, puis provoque un vrai agent (rôle `code` par défaut)

| Option | Description |
|---|---|
| ` <ROLE>` |  |
| ` <TASK>` |  |

### `zer0 kb`

Knowledge base (SQLite + FTS5)

#### `zer0 kb stats`

Show store statistics

#### `zer0 kb search`

Search the knowledge base

| Option | Description |
|---|---|
| ` <QUERY>` |  |
| `--k <K>` |  |

#### `zer0 kb ingest`

Index a directory (default: <ZER0v1>/base_connaissance)

| Option | Description |
|---|---|
| ` <DIR>` |  |
| `--embed <EMBED>` |  |

#### `zer0 kb rebuild`

Rebuild the FTS5 index

#### `zer0 kb purge`

Retire de l'index les sources dont le fichier n'existe plus

#### `zer0 kb drop`

Retire de l'index les sources dont le chemin contient ce motif

| Option | Description |
|---|---|
| ` <SOURCE>` |  |

#### `zer0 kb log`

Show the knowledge-injection log

| Option | Description |
|---|---|
| `--lines <LINES>` |  |

### `zer0 telegram`

Bot Telegram : converse avec Zer0 à distance (long polling)

| Option | Description |
|---|---|
| `--token <TOKEN>` | Token du bot (sinon config ou ZER0_TELEGRAM_TOKEN) |
| `--allow <ALLOW>` | Identifiants Telegram autorisés (sinon auto-claim du premier chat) |

### `zer0 0box`

Débat séquentiel multi-agents (0box) — headless, sans TUI

| Option | Description |
|---|---|
| ` <TOPIC>` | Le sujet du débat (une phrase) |
| `--perspectives <PERSPECTIVES>` | Perspectives du débat, séparées par des virgules (défaut : code,reasoning,main) |

### `zer0 quota`

Usage observé des modèles + solde de la clé si le provider le publie

| Option | Description |
|---|---|
| `--provider <PROVIDER>` | Provider à interroger (défaut : provider actif) |

### `zer0 vote`

Vote pour zer0gh0st : le pouce haut = une star GitHub, le bas = son retrait

| Option | Description |
|---|---|
| ` <ACTION>` | Action : `up` (star), `down` (retrait), `status`, `login`, `logout` |

