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
    bin_name = "zer0",
    version,
    about = "ZER0|GH0ST - agent IA en terminal (CLI/TUI)"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    #[arg(
        long,
        help = "Override the OpenAI-compatible base URL (includes `/v1`)"
    )]
    server: Option<String>,

    #[arg(long, help = "Override the model id")]
    model: Option<String>,

    #[arg(long, help = "Override the built-in system prompt")]
    system_prompt: Option<String>,

    #[arg(
        long,
        default_value_t = false,
        help = "Start without any tools enabled"
    )]
    no_tools: bool,

    #[arg(
        long,
        default_value_t = 8192,
        help = "Cap the number of tokens generated per model call. Kept low by default: many providers count the *reserved* output against the rate limit, so a huge value burns free-tier quota fast"
    )]
    max_tokens: u32,

    #[arg(
        long,
        default_value_t = false,
        help = "Enable the model's reasoning/thinking mode (much slower on weak CPUs)"
    )]
    think: bool,

    #[arg(
        long,
        default_value_t = false,
        help = "Start with the shell tool (`code_exec`, F4) enabled"
    )]
    allow_exec: bool,

    #[arg(
        long,
        default_value_t = false,
        help = "Reprendre la conversation active au démarrage (sinon : nouvelle conversation)"
    )]
    resume: bool,

    #[arg(
        long,
        help = "Run a single prompt headlessly, print the answer, then exit"
    )]
    once: Option<String>,

    #[arg(
        long,
        help = "Headless: attach an image (PNG/JPEG/WebP/GIF) to the prompt (vision)"
    )]
    img: Option<String>,
}

#[derive(Subcommand, Debug)]
enum Command {
    #[command(about = "Manage providers and their API keys")]
    Provider {
        #[command(subcommand)]
        action: ProviderAction,
    },
    #[command(about = "Update the binary from the GitHub Releases")]
    Update {
        #[arg(long, help = "Only check whether a newer version exists")]
        check: bool,
        #[arg(long, help = "Reinstall even when the version is already current")]
        force: bool,
    },
    #[command(
        name = "t0p",
        about = "Moniteur système (CPU, RAM, températures, processus)"
    )]
    Top {
        #[arg(long, help = "Rafraîchir en continu")]
        watch: bool,
        #[arg(
            long,
            default_value_t = 1,
            help = "Intervalle de rafraîchissement (secondes)"
        )]
        interval: u64,
    },
    #[command(about = "List the models of a provider")]
    Models {
        #[arg(long, help = "Only show free (`:free` / $0) models")]
        free: bool,
        #[arg(long, help = "Provider id (defaults to the active provider)")]
        provider: Option<String>,
    },
    #[command(about = "Set the active model")]
    Model {
        #[command(subcommand)]
        action: ModelAction,
    },
    #[command(about = "Recommend free models for a given problem")]
    Select {
        #[arg(help = "The problem statement")]
        problem: Vec<String>,
    },
    #[command(about = "Ordered provider failover chain")]
    Route {
        #[command(subcommand)]
        action: RouteAction,
    },
    #[command(about = "Named model presets (role -> provider/model)")]
    Role {
        #[command(subcommand)]
        action: RoleAction,
    },
    #[command(about = "Project configuration")]
    Project {
        #[command(subcommand)]
        action: ProjectAction,
    },
    #[command(about = "Persistent memory (honest store)")]
    Memory {
        #[command(subcommand)]
        action: MemoryAction,
    },
    #[command(about = "Configure the embedding backend for semantic memory")]
    Embeddings {
        #[command(subcommand)]
        action: EmbeddingsAction,
    },
    #[command(about = "Catalogue of public APIs (source: public-apis/public-apis)")]
    Apis {
        #[command(subcommand)]
        action: ApisAction,
    },
    #[command(about = "Markdown task list (division of work)")]
    Task {
        #[command(subcommand)]
        action: TaskAction,
    },
    #[command(about = "Web search (DuckDuckGo)")]
    Search {
        query: Vec<String>,
        #[arg(long)]
        max: Option<usize>,
    },
    #[command(about = "Personas (system-prompt presets)")]
    Whoami {
        #[command(subcommand)]
        action: WhoamiAction,
    },
    #[command(about = "Encrypted API-key vault")]
    Vault {
        #[command(subcommand)]
        action: VaultAction,
    },
    #[command(about = "Local PDF tools")]
    Pdf {
        #[command(subcommand)]
        action: PdfAction,
    },
    #[command(about = "Calibrated code-quality score (rubric via the review role)")]
    Jev { file: String },
    #[command(about = "Référence Markdown des commandes (générée depuis le CLI)")]
    Commands,
    #[command(
        about = "Journal d'audit des actions de l'agent (commandes exécutées, fichiers écrits)"
    )]
    Audit {
        #[arg(
            long,
            default_value_t = 50,
            help = "Nombre de lignes à afficher (les plus récentes)"
        )]
        tail: usize,
    },
    #[command(about = "Preview: CRT binary clock (full screen)")]
    Bintime,
    #[command(about = "Preview: Pong vs the agent (full screen)")]
    Pong,
    #[command(hide = true, about = "Hidden: dump one Pong frame (diagnostic)")]
    PongFrame,
    #[command(about = "Teste la transcription STT sur un fichier audio (WAV) — sans micro")]
    Stt { file: String },
    #[command(about = "Jimmy le cancre — répond « je sais pas »")]
    Jimmy { task: Vec<String> },
    #[command(about = "Jimmy d'abord, puis provoque un vrai agent (rôle `code` par défaut)")]
    Provoke {
        #[arg(default_value = "code")]
        role: String,
        task: Vec<String>,
    },
    #[command(about = "Knowledge base (SQLite + FTS5)")]
    Kb {
        #[command(subcommand)]
        action: KbAction,
    },
    #[command(about = "Bot Telegram : converse avec Zer0 à distance (long polling)")]
    Telegram {
        #[arg(long, help = "Token du bot (sinon config ou ZER0_TELEGRAM_TOKEN)")]
        token: Option<String>,
        #[arg(
            long,
            help = "Identifiants Telegram autorisés (sinon auto-claim du premier chat)"
        )]
        allow: Vec<i64>,
    },
    #[command(
        name = "0box",
        visible_alias = "zer0box",
        about = "Débat séquentiel multi-agents (0box) — headless, sans TUI"
    )]
    ZeroBox {
        #[arg(help = "Le sujet du débat (une phrase)")]
        topic: Vec<String>,
        #[arg(
            long,
            value_delimiter = ',',
            help = "Perspectives du débat, séparées par des virgules (défaut : code,reasoning,main)"
        )]
        perspectives: Vec<String>,
    },
    #[command(about = "Usage observé des modèles + solde de la clé si le provider le publie")]
    Quota {
        #[arg(long, help = "Provider à interroger (défaut : provider actif)")]
        provider: Option<String>,
    },
    #[command(
        about = "Vote pour zer0gh0st : le pouce haut = une star GitHub, le bas = son retrait"
    )]
    Vote {
        #[arg(help = "Action : `up` (star), `down` (retrait), `status`, `login`, `logout`")]
        action: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
enum ProviderAction {
    #[command(about = "List all known providers and whether a key is configured")]
    List,
    #[command(about = "Show where the keys live: zer0-gw gateway status (never the values)")]
    Keys,
    #[command(about = "Store an API key for a known provider (reads $ENV if omitted)")]
    Add { id: String, key: Option<String> },
    #[command(about = "Define a custom OpenAI-compatible provider")]
    AddCustom {
        id: String,
        #[arg(help = "Base URL including `/v1` (or equivalent), e.g. https://host/v1")]
        base_url: String,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        env: Option<String>,
    },
    #[command(about = "Set the active provider")]
    Use { id: String },
    #[command(about = "Remove a stored API key")]
    Remove { id: String },
}

#[derive(Subcommand, Debug)]
enum ModelAction {
    #[command(about = "Set the active model")]
    Use { id: String },
}

#[derive(Subcommand, Debug)]
enum RouteAction {
    #[command(about = "Show the ordered failover chain")]
    List,
    #[command(about = "Append a route: `zer0 route add openrouter/qwen/qwen3.8-27b:free`")]
    Add { route: String },
    #[command(about = "Fill the chain from the provider's free models (ranked)")]
    Auto {
        #[arg(long, help = "Provider to pull free models from (default: openrouter)")]
        provider: Option<String>,
        #[arg(long, help = "How many models to keep")]
        top: Option<usize>,
    },
    #[command(about = "Remove all failover routes")]
    Clear,
}

#[derive(Subcommand, Debug)]
enum RoleAction {
    #[command(about = "Show roles and their models")]
    List,
    #[command(about = "Disable role routing: one model (the active provider) for everything")]
    Off,
    #[command(about = "Re-enable automatic role routing")]
    On,
    #[command(
        about = "Associate a role with a model: `zer0 role set code openrouter/qwen/qwen3.8-27b:free`"
    )]
    Set { role: String, route: String },
    #[command(about = "Remove a role preset")]
    Unset { role: String },
    #[command(about = "Auto-assign each role to the best free model for its category")]
    Auto {
        #[arg(long)]
        provider: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
enum ProjectAction {
    #[command(about = "Show the current project config")]
    Show,
    #[command(about = "Create a `.zer0/project.toml` template")]
    Init,
}

#[derive(Subcommand, Debug)]
enum MemoryAction {
    #[command(about = "List stored entries")]
    List {
        #[arg(long)]
        limit: Option<usize>,
    },
    #[command(about = "Save a fact: `zer0 memory save \"text\" [--trigger \"future query\"]`")]
    Save {
        text: Vec<String>,
        #[arg(long)]
        trigger: Option<String>,
    },
    #[command(about = "Search memory: `zer0 memory search \"query\"`")]
    Search {
        query: Vec<String>,
        #[arg(long)]
        k: Option<usize>,
    },
    #[command(about = "Show the store path and counts")]
    Stats,
    #[command(about = "Delete an entry by id")]
    Forget { id: String },
    #[command(about = "Delete all entries")]
    Clear,
}

#[derive(Subcommand, Debug)]
enum EmbeddingsAction {
    #[command(about = "Configure the embedding backend")]
    Set { provider: String, model: String },
    #[command(about = "Remove the embedding backend")]
    Unset,
    #[command(about = "Show the current embedding backend")]
    Show,
}

#[derive(Subcommand, Debug)]
enum TaskAction {
    #[command(about = "List tasks")]
    List,
    #[command(about = "Add a task")]
    Add {
        text: Vec<String>,
        #[arg(long)]
        role: Option<String>,
    },
    #[command(about = "Mark a task as done")]
    Done { id: String },
    #[command(about = "Mark a task as in progress")]
    Claim { id: String },
    #[command(about = "Remove a task by id")]
    Rm { id: String },
    #[command(about = "Remove all completed ([x]) tasks")]
    Clear,
}

#[derive(Subcommand, Debug)]
enum WhoamiAction {
    #[command(about = "List available personas")]
    List,
    #[command(about = "Show a persona's content")]
    Show { name: String },
    #[command(about = "Create a persona from a file (`-` for stdin)")]
    Add { name: String, file: String },
}

#[derive(Subcommand, Debug)]
enum VaultAction {
    #[command(about = "Show vault status")]
    Status,
    #[command(about = "Encrypt the keys from config.toml into the vault")]
    Setup,
    #[command(about = "Unlock the vault for this session")]
    Unlock,
    #[command(about = "Lock the vault")]
    Lock,
    #[command(about = "Delete the vault (keys stay forgotten)")]
    Reset,
}

#[derive(Subcommand, Debug)]
enum PdfAction {
    #[command(about = "Page count and metadata")]
    Info { path: String },
    #[command(about = "Extract text")]
    Text { path: String },
    #[command(about = "Merge: `zer0 pdf merge out.pdf a.pdf b.pdf`")]
    Merge { output: String, inputs: Vec<String> },
    #[command(about = "Keep pages first..=last: `zer0 pdf split in.pdf out.pdf 1 3`")]
    Split {
        path: String,
        output: String,
        first: u32,
        last: u32,
    },
}

#[derive(Subcommand, Debug)]
enum KbAction {
    #[command(about = "Show store statistics")]
    Stats,
    #[command(about = "Search the knowledge base")]
    Search {
        query: Vec<String>,
        #[arg(long)]
        k: Option<usize>,
    },
    #[command(about = "Index a directory (default: <ZER0v1>/base_connaissance)")]
    Ingest {
        dir: Option<String>,
        #[arg(long)]
        embed: bool,
    },
    #[command(about = "Rebuild the FTS5 index")]
    Rebuild,
    #[command(about = "Retire de l'index les sources dont le fichier n'existe plus")]
    Purge,
    #[command(about = "Fusionne les sources en double (chemin absolu vs relatif)")]
    Dedupe,
    #[command(about = "Retire de l'index les sources dont le chemin contient ce motif")]
    Drop { source: String },
    #[command(about = "Show the knowledge-injection log")]
    Log {
        #[arg(long, default_value_t = 15)]
        lines: usize,
    },
}

#[derive(Subcommand, Debug)]
enum ApisAction {
    #[command(about = "Import a TSV catalogue (see scripts/kb-import-public-apis.sh)")]
    Import { path: String },
    #[command(about = "Search the catalogue")]
    Search {
        query: Vec<String>,
        #[arg(long)]
        k: Option<usize>,
        #[arg(long, help = "Only APIs usable without an API key")]
        keyless: bool,
    },
    #[command(about = "Categories and counts")]
    Categories,
    #[command(about = "Catalogue status (count, source, import date)")]
    Stats,
}

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

pub async fn entry() -> Result<()> {
    let _ = config::migrate_gateway_prefix();
    if let Some(msg) = config::corruption() {
        eprintln!("⚠ configuration : {msg}");
    }
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

    let explicit_system = cli.system_prompt.is_some();
    let mut system = cli
        .system_prompt
        .unwrap_or_else(|| DEFAULT_SYSTEM.to_string());
    system = config::system_prompt_override().unwrap_or(system);
    if !explicit_system && config::resolve().provider_id == "local" {
        system = config::local_system_prompt();
    }
    let all_tools = if cli.no_tools {
        Vec::new()
    } else {
        tools::all()
    };
    let options = ChatOptions {
        max_tokens: cli.max_tokens,
        thinking: cli.think,
        temperature: 0.3,
        tools_mode: crate::llm::llama::ToolsMode::Native,
        stream: true,
        local: false,
    };

    let mut settings = config::resolve();
    if let Some(server) = cli.server {
        settings.base_url = normalize_server(&server);
    }
    if let Some(model) = cli.model {
        settings.model = model;
    }

    if let Some(prompt) = cli.once.clone() {
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
        Command::Top { watch, interval } => crate::monitor::cli(watch, interval).await,
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
                    let creds = crate::vote::device_login(|code, url| {
                        println!("Ouvre {url} et saisis le code : {code}");
                    })
                    .await?;
                    crate::vault::put("github", &creds.access)?;
                    match creds.refresh {
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
        tools_mode: crate::llm::llama::ToolsMode::Native,
        stream: true,
        local: false,
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
    if settings.provider_id == "local" {
        anyhow::bail!(
            "0box n'est pas disponible en mode local : le débat multi-agents suppose \
             plusieurs modèles (roles). Utilise un provider cloud, ou /ask en local."
        );
    }
    let system = config::system_prompt_override().unwrap_or_else(|| DEFAULT_SYSTEM.to_string());
    let system = enrich_system(system);
    let options = ChatOptions {
        max_tokens: 8192,
        thinking: false,
        temperature: 0.3,
        tools_mode: crate::llm::llama::ToolsMode::Native,
        stream: true,
        local: false,
    };

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<UiEvent>();
    tokio::spawn(async move {
        tui::run_0box_debate(
            topic,
            perspectives,
            tui::DebateSetup {
                settings,
                system,
                options,
                read_write: true,
                code_exec: true,
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
        KbAction::Dedupe => commands::kb_dedupe()?,
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
        tools_mode: crate::llm::llama::ToolsMode::Native,
        stream: true,
        local: false,
    };
    let nchars = content.chars().count();
    eprintln!("📊 jev : {file} ({nchars} caractères)…");
    let res = agent::subagent::run_jev(endpoints, system, &file, &content, options, None).await;
    println!("{res}");
    Ok(())
}

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
    let local_mode = settings.provider_id == "local";
    let all_tools = if local_mode { Vec::new() } else { all_tools };
    let mut options = options;
    options.stream = crate::config::stream_enabled();
    options.local = local_mode;

    if !all_tools.is_empty() {
        system.push_str(
            "\n\n## Outils disponibles (utilise-les pour AGIR — ne dis jamais « je ne peux pas exécuter » quand un outil le permet)\n",
        );
        for t in &all_tools {
            system.push_str(&format!("- {} : {}\n", t.name, t.description));
        }
    }

    if !local_mode {
        if let Some((block, srcs)) = crate::knowledge_block(&prompt).await {
            system.push_str(&block);
            eprintln!("📚 connaissance injectée : {}", srcs.join(", "));
        }
    }

    if settings.provider_id == "local" {
        eprintln!("⏳ mode local : vérification de llama-server…");
        crate::local::ensure(Some(&settings.model)).await?;
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
            UiEvent::Reasoning(t) => {
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

    #[test]
    fn docs_commands_is_up_to_date() {
        let norm = |s: &str| s.replace("\r\n", "\n").trim_end().to_string();
        assert_eq!(
            norm(&commands_markdown()),
            norm(include_str!("../docs/COMMANDS.md")),
            "docs/COMMANDS.md est désynchronisé — régénère : `zer0 commands > docs/COMMANDS.md`"
        );
    }
}
