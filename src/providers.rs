use crate::config;

/// Cost profile of a provider.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FreeKind {
    /// Runs locally, no cost.
    Local,
    /// Hosts explicit `:free` / $0 models (e.g. OpenRouter).
    FreeModels,
    /// Offers a free tier / free credits on signup.
    FreeTier,
    /// Paid only (may still have trial credits).
    Paid,
}

impl FreeKind {
    pub fn label(&self) -> &'static str {
        match self {
            FreeKind::Local => "local",
            FreeKind::FreeModels => "free models",
            FreeKind::FreeTier => "free tier",
            FreeKind::Paid => "paid",
        }
    }
}

/// A configured provider. Built-in ones come from the catalog; custom ones
/// come from `~/.config/zer0/config.toml`.
#[derive(Clone, Debug)]
pub struct Provider {
    pub id: String,
    pub name: String,
    /// Base URL including the API version prefix (contains `/chat/completions`).
    pub base_url: String,
    /// Environment variable that can hold the API key.
    pub env_key: String,
    /// Where to create a key (empty for custom providers).
    pub signup: String,
    pub free: FreeKind,
    /// True when defined by the user rather than the built-in catalog.
    pub custom: bool,
}

struct Builtin {
    id: &'static str,
    name: &'static str,
    base_url: &'static str,
    env_key: &'static str,
    signup: &'static str,
    free: FreeKind,
}

const BUILTINS: &[Builtin] = &[
    Builtin {
        id: "local",
        name: "Local (llama.cpp)",
        base_url: "http://127.0.0.1:8080/v1",
        env_key: "",
        signup: "",
        free: FreeKind::Local,
    },
    Builtin {
        id: "openrouter",
        name: "OpenRouter",
        base_url: "https://openrouter.ai/api/v1",
        env_key: "OPENROUTER_API_KEY",
        signup: "https://openrouter.ai/keys",
        free: FreeKind::FreeModels,
    },
    Builtin {
        id: "google",
        name: "Google Gemini",
        base_url: "https://generativelanguage.googleapis.com/v1beta/openai",
        env_key: "GEMINI_API_KEY",
        signup: "https://aistudio.google.com/apikey",
        free: FreeKind::FreeTier,
    },
    Builtin {
        id: "nvidia",
        name: "NVIDIA NIM",
        base_url: "https://integrate.api.nvidia.com/v1",
        env_key: "NVIDIA_API_KEY",
        signup: "https://build.nvidia.com",
        free: FreeKind::FreeTier,
    },
    Builtin {
        id: "moonshotai",
        name: "Moonshot AI (Kimi)",
        base_url: "https://api.moonshot.ai/v1",
        env_key: "MOONSHOT_API_KEY",
        signup: "https://platform.moonshot.ai/console/api-keys",
        free: FreeKind::FreeTier,
    },
    Builtin {
        id: "alibaba",
        name: "Alibaba (Qwen DashScope)",
        base_url: "https://dashscope-intl.aliyuncs.com/compatible-mode/v1",
        env_key: "DASHSCOPE_API_KEY",
        signup: "https://modelstudio.console.alibabacloud.com",
        free: FreeKind::FreeTier,
    },
    Builtin {
        id: "xai",
        name: "xAI (Grok)",
        base_url: "https://api.x.ai/v1",
        env_key: "XAI_API_KEY",
        signup: "https://console.x.ai",
        free: FreeKind::Paid,
    },
    Builtin {
        id: "openai",
        name: "OpenAI",
        base_url: "https://api.openai.com/v1",
        env_key: "OPENAI_API_KEY",
        signup: "https://platform.openai.com/api-keys",
        free: FreeKind::Paid,
    },
    Builtin {
        id: "anthropic",
        name: "Anthropic (Claude)",
        base_url: "https://api.anthropic.com/v1",
        env_key: "ANTHROPIC_API_KEY",
        signup: "https://console.anthropic.com/settings/keys",
        free: FreeKind::Paid,
    },
];

/// The built-in catalog only.
pub fn builtins() -> Vec<Provider> {
    BUILTINS
        .iter()
        .map(|b| Provider {
            id: b.id.to_string(),
            name: b.name.to_string(),
            base_url: b.base_url.to_string(),
            env_key: b.env_key.to_string(),
            signup: b.signup.to_string(),
            free: b.free,
            custom: false,
        })
        .collect()
}

/// Built-in catalog + user-defined custom providers.
pub fn all() -> Vec<Provider> {
    let mut v = builtins();
    for (id, cp) in config::load_global().custom {
        if v.iter().any(|p| p.id == id) {
            continue;
        }
        v.push(Provider {
            id,
            name: cp.name,
            base_url: cp.base_url,
            env_key: cp.env_key.unwrap_or_default(),
            signup: String::new(),
            free: FreeKind::Paid,
            custom: true,
        });
    }
    v
}

/// Resolve a provider by id, alias, or custom name.
pub fn find(id: &str) -> Option<Provider> {
    let id = normalize(id);
    all().into_iter().find(|p| p.id == id)
}

fn normalize(id: &str) -> String {
    let lower = id.trim().to_ascii_lowercase();
    match lower.as_str() {
        "open-router" | "open_router" => "openrouter".into(),
        "gemini" | "googleai" | "google-ai" => "google".into(),
        "moonshot" | "kimi" => "moonshotai".into(),
        "qwen" | "dashscope" | "aliyun" => "alibaba".into(),
        "grok" => "xai".into(),
        "claude" => "anthropic".into(),
        "ollama" | "llamacpp" | "llama.cpp" | "llama-server" => "local".into(),
        other => other.into(),
    }
}

/// Sensible default model per provider.
pub fn default_model(id: &str) -> Option<&'static str> {
    match id {
        "local" => Some("qwen3.5-4B-super-coder"),
        _ => None,
    }
}
