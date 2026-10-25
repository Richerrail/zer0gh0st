use crate::{
    agent, commands, config, enrich_system, fun, http_client, jimmy, models, providers, route,
    telegram, tools, tui, update, vault, zerochan, DEFAULT_SYSTEM,
};
use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand};

use crate::agent::history::ChatMessage;
use crate::event::UiEvent;
use crate::llm::llama::ChatOptions;

#[derive(Parser, Debug)]
#[command(
    name = "zer0",
    // L'usage affiché ne doit pas dépendre du nom du fichier téléchargé
    // (`zer0-x86_64-unknown-linux-gnu` avant installation).
    bin_name = "zer0",
    version,
    about = "ZER0|GH0ST - agent IA en terminal (CLI/TUI)"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    /// Override the OpenAI-compatible base URL (includes `/v1`).
    #[arg(long)]
    server: Option<String>,

    /// Override the model id.
    #[arg(long)]
    model: Option<String>,

    /// Override the built-in system prompt.
    #[arg(long)]
    system_prompt: Option<String>,

    /// Start without any tools enabled.
    #[arg(long, default_value_t = false)]
    no_tools: bool,

    /// Cap the number of tokens generated per model call.
    /// Kept low by default: many providers count the *reserved* output against
    /// the rate limit, so a huge value burns free-tier quota fast.
    #[arg(long, default_value_t = 8192)]
    max_tokens: u32,

    /// Enable the model's reasoning/thinking mode (much slower on weak CPUs).
    #[arg(long, default_value_t = false)]
    think: bool,

    /// Start with the shell tool (`code_exec`, F4) enabled.
    #[arg(long, default_value_t = false)]
    allow_exec: bool,

    /// Reprendre la conversation active au démarrage (sinon : nouvelle conversation).
    #[arg(long, default_value_t = false)]
    resume: bool,

    /// Run a single prompt headlessly, print the answer, then exit.
    #[arg(long)]
    once: Option<String>,

    /// Headless: attach an image (PNG/JPEG/WebP/GIF) to the prompt (vision).
    #[arg(long)]
    img: Option<String>,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Manage providers and their API keys.
    Provider {
        #[command(subcommand)]
        action: ProviderAction,
    },
    /// Update the binary from the GitHub Releases.
    Update {
        /// Only check whether a newer version exists.
        #[arg(long)]
        check: bool,
        /// Reinstall even when the version is already current.
        #[arg(long)]
        force: bool,
    },
    /// List the models of a provider.
    Models {
        /// Only show free (`:free` / $0) models.
        #[arg(long)]
        free: bool,
        /// Provider id (defaults to the active provider).
        #[arg(long)]
        provider: Option<String>,
    },
    /// Set the active model.
    Model {
        #[command(subcommand)]
        action: ModelAction,
    },
    /// Recommend free models for a given problem.
    Select {
        /// The problem statement.
        problem: Vec<String>,
    },
    /// Ordered provider failover chain.
    Route {
        #[command(subcommand)]
        action: RouteAction,
    },
    /// Named model presets (role -> provider/model).
    Role {
        #[command(subcommand)]
        action: RoleAction,
    },
    /// Project configuration.
    Project {
        #[command(subcommand)]
        action: ProjectAction,
    },
    /// Persistent memory (honest store).
    Memory {
        #[command(subcommand)]
        action: MemoryAction,
    },
    /// Configure the embedding backend for semantic memory.
    Embeddings {
        #[command(subcommand)]
        action: EmbeddingsAction,
    },
    /// Catalogue of public APIs (source: public-apis/public-apis).
    Apis {
        #[command(subcommand)]
        action: ApisAction,
    },
    /// Markdown task list (division of work).
    Task {
        #[command(subcommand)]
        action: TaskAction,
    },
    /// Web search (DuckDuckGo).
    Search {
        query: Vec<String>,
        #[arg(long)]
        max: Option<usize>,
    },
    /// Personas (system-prompt presets).
    Whoami {
        #[command(subcommand)]
        action: WhoamiAction,
    },
    /// Encrypted API-key vault.
    Vault {
        #[command(subcommand)]
        action: VaultAction,
    },
    /// Local PDF tools.
    Pdf {
        #[command(subcommand)]
        action: PdfAction,
    },
    /// Calibrated code-quality score (rubric via the review role).
    Jev { file: String },
    /// Référence Markdown des commandes (générée depuis le CLI).
    Commands,
    /// Journal d'audit des actions de l'agent (commandes exécutées, fichiers écrits).
    Audit {
        /// Nombre de lignes à afficher (les plus récentes).
        #[arg(long, default_value_t = 50)]
        tail: usize,
    },
    /// Preview: CRT binary clock (full screen).
    Bintime,
    /// Preview: Pong vs the agent (full screen).
    Pong,
    /// Hidden: dump one Pong frame (diagnostic).
    #[command(hide = true)]
    PongFrame,
    /// Teste la transcription STT sur un fichier audio (WAV) — sans micro.
    Stt { file: String },
    /// Jimmy le cancre — répond « je sais pas ».
    Jimmy { task: Vec<String> },
    /// Jimmy d'abord, puis provoque un vrai agent (rôle `code` par défaut).
    Provoke {
        #[arg(default_value = "code")]
        role: String,
        task: Vec<String>,
    },
    /// Knowledge base (SQLite + FTS5).
    Kb {
        #[command(subcommand)]
        action: KbAction,
    },
    /// Bot Telegram : converse avec Zer0 à distance (long polling).
    Telegram {
        /// Token du bot (sinon config ou ZER0_TELEGRAM_TOKEN).
        #[arg(long)]
        token: Option<String>,
        /// Identifiants Telegram autorisés (sinon auto-claim du premier chat).
        #[arg(long)]
        allow: Vec<i64>,
    },
    /// Débat séquentiel multi-agents (0box) — headless, sans TUI.
    #[command(name = "0box", visible_alias = "zer0box")]
    ZeroBox {
        /// Le sujet du débat (une phrase).
        topic: Vec<String>,
        /// Perspectives du débat, séparées par des virgules (défaut : code,reasoning,main).
        #[arg(long, value_delimiter = ',')]
        perspectives: Vec<String>,
    },
    /// Usage observé des modèles + solde de la clé si le provider le publie.
    Quota {
        /// Provider à interroger (défaut : provider actif).
        #[arg(long)]
        provider: Option<String>,
    },
    /// Vote pour zer0gh0st : le pouce haut = une star GitHub, le bas = son retrait.
    Vote {
        /// Action : `up` (star), `down` (retrait), `status`, `login`, `logout`.
        action: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
enum ProviderAction {
    /// List all known providers and whether a key is configured.
    List,
    /// Show where the keys live: zer0-gw gateway status (never the values).
    Keys,
    /// Store an API key for a known provider (reads $ENV if omitted).
    Add { id: String, key: Option<String> },
    /// Define a custom OpenAI-compatible provider.
    AddCustom {
        id: String,
        /// Base URL including `/v1` (or equivalent), e.g. https://host/v1
        base_url: String,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        env: Option<String>,
    },
    /// Set the active provider.
    Use { id: String },
    /// Remove a stored API key.
    Remove { id: String },
}

#[derive(Subcommand, Debug)]
enum ModelAction {
    /// Set the active model.
    Use { id: String },
}

#[derive(Subcommand, Debug)]
enum RouteAction {
    /// Show the ordered failover chain.
    List,
    /// Append a route: `zer0 route add openrouter/qwen/qwen3.8-27b:free`
    Add { route: String },
    /// Fill the chain from the provider's free models (ranked).
    Auto {
        /// Provider to pull free models from (default: openrouter).
        #[arg(long)]
        provider: Option<String>,
        /// How many models to keep.
        #[arg(long)]
        top: Option<usize>,
    },
    /// Remove all failover routes.
    Clear,
}

#[derive(Subcommand, Debug)]
enum RoleAction {
    /// Show roles and their models.
    List,
    /// Disable role routing: one model (the active provider) for everything.
    Off,
    /// Re-enable automatic role routing.
    On,
    /// Associate a role with a model: `zer0 role set code openrouter/qwen/qwen3.8-27b:free`
    Set { role: String, route: String },
    /// Remove a role preset.
    Unset { role: String },
    /// Auto-assign each role to the best free model for its category.
    Auto {
        #[arg(long)]
        provider: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
enum ProjectAction {
    /// Show the current project config.
    Show,
    /// Create a `.zer0/project.toml` template.
    Init,
}

#[derive(Subcommand, Debug)]
enum MemoryAction {
    /// List stored entries.
    List {
        #[arg(long)]
        limit: Option<usize>,
    },
    /// Save a fact: `zer0 memory save "text" [--trigger "future query"]`
    Save {
        text: Vec<String>,
        #[arg(long)]
        trigger: Option<String>,
    },
    /// Search memory: `zer0 memory search "query"`
    Search {
        query: Vec<String>,
        #[arg(long)]
        k: Option<usize>,
    },
    /// Show the store path and counts.
    Stats,
    /// Delete an entry by id.
    Forget { id: String },
    /// Delete all entries.
    Clear,
}

#[derive(Subcommand, Debug)]
enum EmbeddingsAction {
    /// Configure the embedding backend.
    Set { provider: String, model: String },
    /// Remove the embedding backend.
    Unset,
    /// Show the current embedding backend.
    Show,
}

#[derive(Subcommand, Debug)]
enum TaskAction {
    /// List tasks.
    List,
    /// Add a task.
    Add {
        text: Vec<String>,
        #[arg(long)]
        role: Option<String>,
    },
    /// Mark a task as done.
    Done { id: String },
    /// Mark a task as in progress.
    Claim { id: String },
    /// Remove a task by id.
    Rm { id: String },
    /// Remove all completed ([x]) tasks.
    Clear,
}

#[derive(Subcommand, Debug)]
enum WhoamiAction {
    /// List available personas.
    List,
    /// Show a persona's content.
    Show { name: String },
    /// Create a persona from a file (`-` for stdin).
    Add { name: String, file: String },
}

#[derive(Subcommand, Debug)]
enum VaultAction {
    /// Show vault status.
    Status,
    /// Encrypt the keys from config.toml into the vault.
    Setup,
    /// Unlock the vault for this session.
    Unlock,
    /// Lock the vault.
    Lock,
    /// Delete the vault (keys stay forgotten).
    Reset,
}

#[derive(Subcommand, Debug)]
enum PdfAction {
    /// Page count and metadata.
    Info { path: String },
    /// Extract text.
    Text { path: String },
    /// Merge: `zer0 pdf merge out.pdf a.pdf b.pdf`
    Merge { output: String, inputs: Vec<String> },
    /// Keep pages first..=last: `zer0 pdf split in.pdf out.pdf 1 3`
    Split {
        path: String,
        output: String,
        first: u32,
        last: u32,
    },
}

#[derive(Subcommand, Debug)]
enum KbAction {
    /// Show store statistics.
    Stats,
    /// Search the knowledge base.
    Search {
        query: Vec<String>,
        #[arg(long)]
        k: Option<usize>,
    },
    /// Index a directory (default: <ZER0v1>/base_connaissance).
    Ingest {
        dir: Option<String>,
        #[arg(long)]
        embed: bool,
    },
    /// Rebuild the FTS5 index.
    Rebuild,
    /// Retire de l'index les sources dont le fichier n'existe plus.
    Purge,
    /// Retire de l'index les sources dont le chemin contient ce motif.
    Drop { source: String },
    /// Show the knowledge-injection log.
    Log {
        #[arg(long, default_value_t = 15)]
        lines: usize,
    },
}

/// Catalogue d'APIs publiques (public-apis/public-apis).
#[derive(Subcommand, Debug)]
enum ApisAction {
    /// Import a TSV catalogue (see scripts/kb-import-public-apis.sh).
    Import { path: String },
    /// Search the catalogue.
    Search {
        query: Vec<String>,
        #[arg(long)]
        k: Option<usize>,
        /// Only APIs usable without an API key.
        #[arg(long)]
        keyless: bool,
    },
    /// Categories and counts.
    Categories,
    /// Catalogue status (count, source, import date).
    Stats,
}

/// Référence Markdown des commandes, **générée depuis clap**.
///
/// La doc des commandes était écrite à la main dans le README : elle dérivait à
/// chaque ajout. Ici, la source est le CLI lui-même ; `docs/COMMANDS.md` est
/// comparé à cette sortie par un test, donc toute divergence casse la CI.
pub fn commands_markdown() -> String {
    use clap::CommandFactory;
    let mut cmd = Cli::command();
    let mut out = String::new();
    out.push_str("# Commandes — ZER0|GH0ST\n\n");
    out.push_str("> Fichier généré depuis le CLI (`zer0 commands > docs/COMMANDS.md`).\n");
    out.push_str(
        "> Ne pas éditer à la main : un test échoue si ce fichier diverge du binaire.\n\n",
    );
    render_command(&mut cmd, "zer0", &mut out, 2);
    out
}

fn render_command(cmd: &mut clap::Command, path: &str, out: &mut String, level: usize) {
    out.push_str(&format!("{} `{}`\n\n", "#".repeat(level), path));
    if let Some(about) = cmd.get_about().map(|a| a.to_string()) {
        out.push_str(&format!("{about}\n\n"));
    }
    let mut rows: Vec<(String, String)> = Vec::new();
    for a in cmd.get_arguments() {
        let id = a.get_id().as_str();
        if id == "help" || id == "version" {
            continue;
        }
        let mut usage = format!("`--{}`", a.get_long().unwrap_or(id));
        if let Some(v) = a.get_value_names() {
            let names: Vec<&str> = v.iter().map(|n| n.as_str()).collect();
            usage = format!(
                "`{} <{}>`",
                a.get_long().map(|l| format!("--{l}")).unwrap_or_default(),
                names.join(" ")
            );
        }
        let help = a.get_help().map(|h| h.to_string()).unwrap_or_default();
        rows.push((usage, help.replace('\n', " ")));
    }
    if !rows.is_empty() {
        out.push_str("| Option | Description |\n|---|---|\n");
        for (u, h) in rows {
            out.push_str(&format!("| {u} | {h} |\n"));
        }
        out.push('\n');
    }
    let subs: Vec<(String, String)> = cmd
        .get_subcommands()
        .map(|s| (s.get_name().to_string(), String::new()))
        .collect();
    if !subs.is_empty() {
        for (name, _) in subs {
            if let Some(mut sub) = cmd.find_subcommand(&name).cloned() {
                render_command(&mut sub, &format!("{path} {name}"), out, level + 1);
            }
        }
    }
}

/// Point d'entrée du binaire `zer0` (CLI + TUI).
pub async fn entry() -> Result<()> {
    // Ancien schéma `gw-<id>` → identifiant simple (migration silencieuse, une fois).
    let _ = config::migrate_gateway_prefix();
    // Une config illisible ne doit jamais être silencieuse : on sauvegarde, on
    // refuse les écritures, et on le dit ici (avant toute commande).
    if let Some(msg) = config::corruption() {
        eprintln!("⚠ configuration : {msg}");
    }
    // Alias binaire : une copie ou un lien dur de `zer0(.exe)` nommé `zer0box(.exe)`
    // lance directement le débat — `zer0box "sujet"` == `zer0 0box "sujet"`.
    // Créer le lien : New-Item -ItemType HardLink -Path zer0box.exe -Target zer0.exe
    let mut args: Vec<String> = std::env::args().collect();
    let invoked = std::path::Path::new(&args[0])
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if invoked.starts_with("zer0box") {
        args.insert(1, "0box".to_string());
    }
    let cli = Cli::parse_from(args);

    if let Some(cmd) = cli.command {
        if !matches!(cmd, Command::Vault { .. }) {
            ensure_vault();
        }
        return run_command(cmd).await;
    }
    ensure_vault();

    let system = cli
        .system_prompt
        .unwrap_or_else(|| DEFAULT_SYSTEM.to_string());
    let system = config::system_prompt_override().unwrap_or(system);
    let all_tools = if cli.no_tools {
        Vec::new()
    } else {
        tools::all()
    };
    let options = ChatOptions {
        max_tokens: cli.max_tokens,
        thinking: cli.think,
        temperature: 0.3,
    };

    let mut settings = config::resolve();
    if let Some(server) = cli.server {
        settings.base_url = normalize_server(&server);
    }
    if let Some(model) = cli.model {
        settings.model = model;
    }

    if let Some(prompt) = cli.once.clone() {
        // Contexte runtime : l'outil `0box` (appelé par le modèle) a besoin des
        // mêmes réglages que l'agent pour lancer un vrai débat.
        tools::set_runtime(tools::Runtime {
            settings: settings.clone(),
            system: system.clone(),
            options: options.clone(),
            all_tools: all_tools.clone(),
            read_write: true,
            code_exec: true,
        });
        return run_once(
            settings,
            system,
            prompt,
            all_tools,
            options,
            cli.img.clone(),
        )
        .await;
    }

    // Même contexte pour la TUI (l'outil 0box y est atteignable par le modèle).
    tools::set_runtime(tools::Runtime {
        settings: settings.clone(),
        system: system.clone(),
        options: options.clone(),
        all_tools: all_tools.clone(),
        read_write: true,
        code_exec: cli.allow_exec,
    });

    tui::run(
        settings,
        system,
        all_tools,
        options,
        cli.allow_exec,
        cli.resume,
    )
    .await
}

/// Accept both `http://host:port` and `http://host:port/v1` forms.
fn normalize_server(s: &str) -> String {
    let s = s.trim_end_matches('/');
    if s.contains("/v1") || s.contains("/v1beta") || s.contains("/compatible-mode") {
        s.to_string()
    } else {
        format!("{s}/v1")
    }
}

async fn run_command(cmd: Command) -> Result<()> {
    match cmd {
        Command::Provider { action } => provider_cmd(action),
        Command::Models { free, provider } => list_models_cmd(free, provider).await,
        Command::Model { action } => model_cmd(action),
        Command::Select { problem } => select_cmd(problem).await,
        Command::Route { action } => route_cmd(action).await,
        Command::Role { action } => role_cmd(action).await,
        Command::Project { action } => project_cmd(action),
        Command::Memory { action } => memory_cmd(action).await,
        Command::Embeddings { action } => embeddings_cmd(action),
        Command::Task { action } => task_cmd(action),
        Command::Search { query, max } => search_cmd(query, max).await,
        Command::Whoami { action } => whoami_cmd(action),
        Command::Update { check, force } => {
            println!("{}", update::run(check, force).await?);
            Ok(())
        }
        Command::Vault { action } => vault_cmd(action),
        Command::Pdf { action } => pdf_cmd(action),
        Command::Jev { file } => jev_cmd(file).await,
        Command::Commands => {
            print!("{}", commands_markdown());
            Ok(())
        }
        Command::Audit { tail } => {
            for line in commands::audit_tail(tail) {
                println!("{line}");
            }
            Ok(())
        }
        Command::Bintime => fun::run_standalone(fun::Mode::BinTime),
        Command::Pong => fun::run_standalone(fun::Mode::Pong),
        Command::PongFrame => fun::debug_pong_frame(),
        Command::Stt { file } => stt_cmd(file).await,
        Command::Jimmy { task } => jimmy_cmd(task).await,
        Command::Provoke { role, task } => provoke_cmd(role, task).await,
        Command::Kb { action } => kb_cmd(action).await,
        Command::Apis { action } => apis_cmd(action),
        Command::Telegram { token, allow } => telegram::run(token, allow).await,
        Command::ZeroBox {
            topic,
            perspectives,
        } => zerobox_cmd(topic, perspectives).await,
        Command::Quota { provider } => {
            for line in commands::quota_report(provider.as_deref()).await? {
                println!("{line}");
            }
            Ok(())
        }
        Command::Vote { action } => {
            let act = action
                .unwrap_or_else(|| "status".to_string())
                .to_ascii_lowercase();
            match act.as_str() {
                "up" | "👍" | "upvote" => {
                    println!("{}", crate::vote::act(crate::vote::Action::Up).await);
                }
                "down" | "👎" | "downvote" => {
                    println!("{}", crate::vote::act(crate::vote::Action::Down).await);
                }
                "login" | "connect" => {
                    // Device flow OAuth : l'utilisateur valide un code dans son
                    // navigateur, une fois. Le jeton est ensuite rangé dans le
                    // coffre → les votes suivants sont automatiques.
                    let creds = crate::vote::device_login(|code, url| {
                        println!("Ouvre {url} et saisis le code : {code}");
                    })
                    .await?;
                    crate::vault::put("github", &creds.access)?;
                    match creds.refresh {
                        // App à jetons expirables : on garde aussi le refresh
                        // token (chiffré) → le vote reste automatique après
                        // expiration (8 h). **Opt-in** : absent = rien à faire.
                        Some(rt) => {
                            crate::vault::put("github_refresh", &rt)?;
                            println!("✅ Jeton GitHub enregistré (chiffré) — avec renouvellement automatique.");
                        }
                        None => println!("✅ Jeton GitHub enregistré dans le coffre."),
                    }
                    println!("   Le vote est désormais automatique (👍 = une star).");
                }
                "logout" => {
                    let had = crate::vault::remove("github").unwrap_or(false);
                    let had_rt = crate::vault::remove("github_refresh").unwrap_or(false);
                    if had || had_rt {
                        println!("Jeton GitHub retiré du coffre.");
                    } else {
                        println!("Aucun jeton GitHub dans le coffre.");
                    }
                }
                "status" | "info" => {
                    println!("{}", crate::vote::status().await);
                }
                other => {
                    return Err(anyhow!(
                        "action inconnue : {other}\nusage : zer0 vote [up|down|status|login|logout]"
                    ));
                }
            }
            Ok(())
        }
    }
}

async fn jimmy_cmd(task: Vec<String>) -> Result<()> {
    let t = task.join(" ");
    if t.trim().is_empty() {
        println!("🐌 Jimmy : {}", jimmy::reply());
    } else {
        let d = jimmy::lazy_delay();
        println!("🐌 Jimmy se gratte la tête… (tâche : « {t} »)");
        tokio::time::sleep(std::time::Duration::from_secs(d)).await;
        println!("🐌 Jimmy (après {d}s) : {}", jimmy::reply());
    }
    Ok(())
}

async fn provoke_cmd(role: String, task: Vec<String>) -> Result<()> {
    // Le 1er mot n'est un rôle que s'il est connu ; sinon il fait partie de la tâche.
    let (role, t) = if route::KNOWN_ROLES.contains(&role.as_str()) {
        (role, task.join(" "))
    } else {
        let mut all = vec![role];
        all.extend(task);
        let joined = all.join(" ");
        (route::auto_role(&joined).to_string(), joined)
    };
    if t.trim().is_empty() {
        eprintln!("usage: zer0 provoke [rôle] <tâche>");
        return Ok(());
    }
    let jab = jimmy::reply();
    println!("🐌 Jimmy devait « {t} » → il a répondu : « {jab} »");
    println!("⚡ provocation → rôle {role}\n");
    let settings = config::resolve();
    let endpoints = route::chain_for_role(&settings, &role);
    let system = DEFAULT_SYSTEM.to_string();
    let options = ChatOptions {
        max_tokens: 2048,
        thinking: false,
        temperature: 0.3,
    };
    let prompt = format!(
        "Le cancre Jimmy devait « {t} » et a seulement répondu : « {jab} ». \
         Une IA compétente ferait mieux. Fais le travail correctement : {t}"
    );
    let res =
        agent::subagent::run_subagent(endpoints, system, prompt, tools::all(), options, None).await;
    println!("{res}");
    Ok(())
}

/// Débat 0box en mode headless : les mêmes règles que la TUI (`/0box`), sans interface.
/// Permet de tester/scripter le module de débat (0BOX-TEST l'exigeait — jusqu'ici
/// le débat n'était exécutable qu'interactivement, donc invérifiable en CI).
async fn zerobox_cmd(topic: Vec<String>, perspectives: Vec<String>) -> Result<()> {
    use std::io::Write;

    let topic = topic.join(" ");
    if topic.trim().is_empty() {
        anyhow::bail!("usage: zer0 0box <sujet> (alias : zer0box <sujet>, zer0 zer0box <sujet>) [--perspectives code,reasoning,main]");
    }
    let perspectives = if perspectives.is_empty() {
        vec![
            "code".to_string(),
            "reasoning".to_string(),
            "main".to_string(),
        ]
    } else {
        perspectives
    };

    let settings = config::resolve();
    let system = config::system_prompt_override().unwrap_or_else(|| DEFAULT_SYSTEM.to_string());
    // Même enrichissement que la TUI (tui:172) et --once : sans le bloc Environnement
    // (OS, dossiers), les modèles croyaient être sous Linux et perdaient leurs
    // itérations sur `cat`/`ls -la`/`mkdir -p` sous cmd.exe.
    let system = enrich_system(system);
    let options = ChatOptions {
        max_tokens: 8192,
        thinking: false,
        temperature: 0.3,
    };

    // En headless, tous les outils sont armés (même politique que --once).
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<UiEvent>();
    tokio::spawn(async move {
        tui::run_0box_debate(
            topic,
            perspectives,
            tui::DebateSetup {
                settings,
                system,
                options,
                read_write: true, // text_editor
                code_exec: true,  // code_exec
                all_tools: tools::all(),
                ui: tx,
            },
        )
        .await;
    });

    while let Some(ev) = rx.recv().await {
        match ev {
            UiEvent::Info(t) => {
                println!("{t}");
                let _ = std::io::stdout().flush();
            }
            UiEvent::Token(t) => {
                print!("{t}");
                let _ = std::io::stdout().flush();
            }
            UiEvent::Active(label) => eprintln!("→ endpoint: {label}"),
            UiEvent::Error(e) => {
                eprintln!("⚠ {e}");
                break;
            }
            _ => {}
        }
    }
    println!();
    Ok(())
}

async fn stt_cmd(file: String) -> Result<()> {
    let p = std::path::PathBuf::from(&file);
    match zerochan::transcribe(&p).await {
        Ok(t) => {
            println!("{t}");
            Ok(())
        }
        Err(e) => Err(e),
    }
}

async fn kb_cmd(action: KbAction) -> Result<()> {
    let lines = match action {
        KbAction::Stats => commands::kb_stats()?,
        KbAction::Search { query, k } => {
            commands::kb_search(&query.join(" "), k.unwrap_or(5)).await?
        }
        KbAction::Ingest { dir, embed } => {
            commands::kb_ingest(dir.map(std::path::PathBuf::from), embed).await?
        }
        KbAction::Rebuild => commands::kb_rebuild()?,
        KbAction::Purge => commands::kb_purge()?,
        KbAction::Drop { source } => commands::kb_drop(&source)?,
        KbAction::Log { lines } => commands::kb_log(lines),
    };
    for line in lines {
        println!("{line}");
    }
    Ok(())
}
fn apis_cmd(action: ApisAction) -> Result<()> {
    let lines = match action {
        ApisAction::Import { path } => commands::apis_import(&path)?,
        ApisAction::Search { query, k, keyless } => commands::apis_search(
            &query.join(" "),
            k.unwrap_or(commands::APIS_DEFAULT_K),
            keyless,
        )?,
        ApisAction::Categories => commands::apis_categories()?,
        ApisAction::Stats => commands::apis_stats()?,
    };
    for line in lines {
        println!("{line}");
    }
    Ok(())
}
fn pdf_cmd(action: PdfAction) -> Result<()> {
    let cmd = match action {
        PdfAction::Info { path } => commands::PdfCmd::Info(path),
        PdfAction::Text { path } => commands::PdfCmd::Text(path),
        PdfAction::Merge { output, inputs } => commands::PdfCmd::Merge { output, inputs },
        PdfAction::Split {
            path,
            output,
            first,
            last,
        } => commands::PdfCmd::Split {
            path,
            output,
            first,
            last,
        },
    };
    for line in commands::pdf(cmd)? {
        println!("{line}");
    }
    Ok(())
}
async fn jev_cmd(file: String) -> Result<()> {
    let content = std::fs::read_to_string(&file)?;
    let settings = config::resolve();
    let endpoints = route::chain_for_role(&settings, "review");
    let system = DEFAULT_SYSTEM.to_string();
    let options = ChatOptions {
        max_tokens: 512,
        thinking: false,
        temperature: 0.2,
    };
    let nchars = content.chars().count();
    eprintln!("📊 jev : {file} ({nchars} caractères)…");
    let res = agent::subagent::run_jev(endpoints, system, &file, &content, options, None).await;
    println!("{res}");
    Ok(())
}

/// If a vault exists and is locked, try env then an interactive prompt.
fn ensure_vault() {
    if !vault::enabled() || vault::is_unlocked() {
        return;
    }
    if vault::try_env_unlock().unwrap_or(false) {
        return;
    }
    use std::io::IsTerminal;
    if !std::io::stdin().is_terminal() {
        eprintln!("⚠ coffre verrouillé (définis $ZER0_VAULT_PASSPHRASE ou `zer0 vault unlock`)");
        return;
    }
    if let Ok(p) = rpassword::prompt_password("Zer0 — coffre verrouillé, passphrase : ") {
        if !p.is_empty() {
            match vault::unlock(&p) {
                Ok(n) => eprintln!("🔓 coffre déverrouillé ({n} clé(s))"),
                Err(e) => eprintln!("⚠ {e}"),
            }
        }
    }
}

fn vault_cmd(action: VaultAction) -> Result<()> {
    // `setup`/`unlock`/`reset` exigent une saisie de passphrase : impossible en
    // TUI, donc ils restent pilotés par le CLI. La logique (vault::*) est
    // commune ; seul l'affichage de `status`/`lock` est partagé.
    let lines = match action {
        VaultAction::Status => commands::vault_status(),
        VaultAction::Lock => commands::vault_lock(),
        VaultAction::Setup => {
            let p = rpassword::prompt_password("Nouvelle passphrase : ")?;
            let p2 = rpassword::prompt_password("Confirme : ")?;
            if p != p2 {
                return Err(anyhow!("les passphrases ne correspondent pas"));
            }
            let n = vault::setup(&p)?;
            vec![format!(
                "Coffre créé ({n} clé(s) chiffrée(s)). Les clés ont été retirées de config.toml."
            )]
        }
        VaultAction::Unlock => {
            let p = rpassword::prompt_password("Passphrase : ")?;
            let n = vault::unlock(&p)?;
            vec![format!(
                "Coffre déverrouillé ({n} clé(s)) — pour cette session."
            )]
        }
        VaultAction::Reset => {
            vault::reset()?;
            vec!["Coffre supprimé. Les clés doivent être re-saisies.".to_string()]
        }
    };
    for line in lines {
        println!("{line}");
    }
    Ok(())
}
fn whoami_cmd(action: WhoamiAction) -> Result<()> {
    let lines = match action {
        WhoamiAction::List => commands::persona_list(),
        WhoamiAction::Show { name } => commands::persona_show(&name)?,
        WhoamiAction::Add { name, file } => {
            // Lecture du contenu (fichier ou stdin) : spécifique au CLI, mais la
            // validation et l'écriture sont partagées.
            let content = if file == "-" {
                use std::io::Read;
                let mut s = String::new();
                std::io::stdin().read_to_string(&mut s)?;
                s
            } else {
                std::fs::read_to_string(&file)?
            };
            commands::persona_add(&name, &content)?
        }
    };
    for line in lines {
        println!("{line}");
    }
    Ok(())
}
async fn search_cmd(query: Vec<String>, max: Option<usize>) -> Result<()> {
    let q = query.join(" ");
    for line in commands::search(&q, max.unwrap_or(5)).await? {
        println!("{line}");
    }
    Ok(())
}
fn task_cmd(action: TaskAction) -> Result<()> {
    // Un seul chemin : `commands::task`, le même que `/task` dans la TUI.
    let cmd = match action {
        TaskAction::List => commands::TaskCmd::List,
        TaskAction::Add { text, role } => commands::TaskCmd::Add {
            text: text.join(" "),
            role,
        },
        TaskAction::Done { id } => commands::TaskCmd::Done(id),
        TaskAction::Claim { id } => commands::TaskCmd::Claim(id),
        TaskAction::Rm { id } => commands::TaskCmd::Rm(id),
        TaskAction::Clear => commands::TaskCmd::Clear,
    };
    for line in commands::task(cmd)? {
        println!("{line}");
    }
    Ok(())
}
fn provider_cmd(action: ProviderAction) -> Result<()> {
    // Une seule implémentation : la même que `/provider` dans la TUI.
    let lines = match action {
        ProviderAction::List => commands::provider_list(),
        ProviderAction::Keys => commands::provider_keys(),
        ProviderAction::Add { id, key } => commands::provider_add(&id, key.as_deref())?,
        ProviderAction::AddCustom {
            id,
            base_url,
            name,
            env,
        } => commands::provider_add_custom(&id, &base_url, name.as_deref(), env.as_deref())?,
        ProviderAction::Use { id } => commands::provider_use(&id)?,
        ProviderAction::Remove { id } => commands::provider_remove(&id)?,
    };
    for line in lines {
        println!("{line}");
    }
    Ok(())
}
async fn list_models_cmd(free: bool, provider_id: Option<String>) -> Result<()> {
    let id = provider_id.unwrap_or_else(|| config::resolve().provider_id);
    let p = providers::find(&id).ok_or_else(|| anyhow!("provider inconnu: {id}"))?;
    let key = config::key_for(&p);
    let mut list = models::list(&http_client::HTTP_CLIENT, &p, key.as_deref()).await?;
    if free {
        list.retain(models::is_free);
    }
    println!(
        "{} modèle(s) chez {}{}",
        list.len(),
        p.name,
        if free { " (gratuits)" } else { "" }
    );
    for m in &list {
        println!(
            "  {}{}",
            m.id,
            m.context_length
                .map(|c| format!("  (ctx {c})"))
                .unwrap_or_default()
        );
    }
    Ok(())
}

fn model_cmd(action: ModelAction) -> Result<()> {
    match action {
        ModelAction::Use { id } => {
            for line in commands::set_model(&id)? {
                println!("{line}");
            }
        }
    }
    Ok(())
}

fn embeddings_cmd(action: EmbeddingsAction) -> Result<()> {
    let lines = match action {
        EmbeddingsAction::Set { provider, model } => commands::embeddings_set(&provider, &model)?,
        EmbeddingsAction::Unset => commands::embeddings_unset()?,
        EmbeddingsAction::Show => commands::embeddings_show(),
    };
    for line in lines {
        println!("{line}");
    }
    Ok(())
}
async fn memory_cmd(action: MemoryAction) -> Result<()> {
    let lines = match action {
        MemoryAction::List { limit } => commands::memory_list(limit.unwrap_or(20)),
        MemoryAction::Save { text, trigger } => {
            commands::memory_save(&text.join(" "), trigger).await?
        }
        MemoryAction::Search { query, k } => {
            commands::memory_search(&query.join(" "), k.unwrap_or(5)).await?
        }
        MemoryAction::Stats => commands::memory_stats(),
        MemoryAction::Forget { id } => commands::memory_forget(&id)?,
        MemoryAction::Clear => commands::memory_clear()?,
    };
    for line in lines {
        println!("{line}");
    }
    Ok(())
}
fn project_cmd(action: ProjectAction) -> Result<()> {
    let p = config::load_project();
    match action {
        ProjectAction::Show => {
            println!("Projet: {}", config::project_path().display());
            println!(
                "  provider: {}",
                p.provider.unwrap_or_else(|| "(global)".into())
            );
            println!(
                "  model   : {}",
                p.model.unwrap_or_else(|| "(global)".into())
            );
            println!(
                "  routes  : {}",
                if p.routes.is_empty() {
                    "(aucune)".to_string()
                } else {
                    p.routes.join(", ")
                }
            );
            if !p.roles.is_empty() {
                println!("  roles   :");
                for (k, v) in &p.roles {
                    println!("    {k:<10} {v}");
                }
            }
            if p.system_prompt.is_some() || p.prompt_file.is_some() {
                println!("  prompt  : override projet");
            }
        }
        ProjectAction::Init => {
            let path = config::project_path();
            if path.exists() {
                println!("Existe déjà: {}", path.display());
            } else {
                config::save_project(&config::ProjectConfig::default())?;
                println!("Créé: {}", path.display());
            }
        }
    }
    Ok(())
}

async fn role_cmd(action: RoleAction) -> Result<()> {
    let lines = match action {
        RoleAction::List => {
            let mut out = vec![if commands::roles_enabled() {
                "Rôles (actifs):".to_string()
            } else {
                "Rôles (désactivés — `zer0 role on` pour réactiver) :".to_string()
            }];
            for (role, route) in commands::roles_snapshot() {
                out.push(format!(
                    "  {role:<10} {}",
                    route.unwrap_or_else(|| "—".to_string())
                ));
            }
            out
        }
        RoleAction::Off => commands::set_roles(false)?,
        RoleAction::On => commands::set_roles(true)?,
        RoleAction::Set { role, route: r } => commands::role_set(&role, &r)?,
        RoleAction::Unset { role } => commands::role_unset(&role)?,
        RoleAction::Auto { provider } => {
            let provider = provider.unwrap_or_else(|| "openrouter".to_string());
            commands::role_auto(&provider).await?
        }
    };
    for line in lines {
        println!("{line}");
    }
    Ok(())
}
async fn route_cmd(action: RouteAction) -> Result<()> {
    let lines = match action {
        RouteAction::List => commands::route_lines(&config::resolve()),
        RouteAction::Add { route: r } => commands::route_add(&r)?,
        RouteAction::Auto { provider, top } => {
            let provider = provider.unwrap_or_else(|| "openrouter".to_string());
            let top = top.unwrap_or(4);
            commands::route_auto(&provider, top).await?
        }
        RouteAction::Clear => commands::route_clear()?,
    };
    for line in lines {
        println!("{line}");
    }
    Ok(())
}

async fn select_cmd(problem: Vec<String>) -> Result<()> {
    let p = problem.join(" ");
    for line in commands::select_models(&p, 8).await? {
        println!("{line}");
    }
    Ok(())
}
/// Headless single-turn mode: prints streamed tokens to stdout.
async fn run_once(
    settings: config::Settings,
    system: String,
    prompt: String,
    all_tools: Vec<tools::ToolSpec>,
    options: ChatOptions,
    img: Option<String>,
) -> Result<()> {
    use std::io::Write;

    let mut system = enrich_system(system);

    // Liste explicite des outils disponibles dans le prompt.
    if !all_tools.is_empty() {
        system.push_str(
            "\n\n## Outils disponibles (utilise-les pour AGIR — ne dis jamais « je ne peux pas exécuter » quand un outil le permet)\n",
        );
        for t in &all_tools {
            system.push_str(&format!("- {} : {}\n", t.name, t.description));
        }
    }

    // Connaissance locale : même bloc que la TUI (`crate::knowledge_block`).
    if let Some((block, srcs)) = crate::knowledge_block(&prompt).await {
        system.push_str(&block);
        eprintln!("📚 connaissance injectée : {}", srcs.join(", "));
    }

    let role = route::auto_role(&prompt);
    let endpoints = route::chain_for_role(&settings, role);
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<UiEvent>();
    let mut user = ChatMessage::user(prompt);
    if let Some(path) = img {
        let (data_url, size) = crate::platform::image_data_url(&path).map_err(|e| anyhow!(e))?;
        eprintln!("🖼 image jointe : {path} ({:.0} Kio)", size as f64 / 1024.0);
        user.image = Some(data_url);
    }
    let messages = vec![user];
    tokio::spawn(async move {
        agent::turn::run_turn(endpoints, system, messages, all_tools, options, tx).await;
    });

    while let Some(ev) = rx.recv().await {
        match ev {
            UiEvent::Token(t) => {
                print!("{t}");
                std::io::stdout().flush().ok();
            }
            UiEvent::ToolCall { name, args } => println!("\n\n⚙ {name} {args}"),
            UiEvent::ToolResult(r) => {
                println!("⚙ → {}", r.lines().next().unwrap_or(""));
            }
            UiEvent::TurnComplete(_) => break,
            UiEvent::Active(label) => eprintln!("→ endpoint: {label}"),
            UiEvent::Error(e) => {
                eprintln!("\n⚠ erreur: {e}");
                break;
            }
            UiEvent::Info(t) => eprintln!("{t}"),
            _ => {}
        }
    }
    println!();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Anti-dérive : la référence commitée doit être exactement la sortie du CLI.
    /// Si ce test casse, régénère : `zer0 commands > docs/COMMANDS.md`.
    #[test]
    fn docs_commands_is_up_to_date() {
        // Comparaison insensible aux fins de ligne : un checkout Windows peut
        // convertir le fichier en CRLF, ce qui n'est pas une désynchronisation.
        let norm = |s: &str| s.replace("\r\n", "\n").trim_end().to_string();
        assert_eq!(
            norm(&commands_markdown()),
            norm(include_str!("../docs/COMMANDS.md")),
            "docs/COMMANDS.md est désynchronisé — régénère : `zer0 commands > docs/COMMANDS.md`"
        );
    }
}
