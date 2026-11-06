use crate::config;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FreeKind {
    Local,
    FreeModels,
    FreeTier,
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

#[derive(Clone, Debug)]
pub struct Provider {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub env_key: String,
    pub signup: String,
    pub free: FreeKind,
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
    Builtin {
        id: "zai",
        name: "Z.ai (Zhipu GLM)",
        base_url: "https://api.z.ai/api/paas/v4",
        env_key: "ZAI_API_KEY",
        signup: "https://z.ai/manage-apikey",
        free: FreeKind::FreeTier,
    },
];

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

pub fn find(id: &str) -> Option<Provider> {
    let id = normalize(id);
    all().into_iter().find(|p| p.id == id)
}

fn normalize(id: &str) -> String {
    let lower = id.trim().to_ascii_lowercase();
    let lower = lower.strip_prefix("gw-").unwrap_or(&lower).to_string();
    match lower.as_str() {
        "open-router" | "open_router" => "openrouter".into(),
        "gemini" | "googleai" | "google-ai" => "google".into(),
        "moonshot" | "kimi" => "moonshotai".into(),
        "qwen" | "dashscope" | "aliyun" => "alibaba".into(),
        "grok" => "xai".into(),
        "claude" => "anthropic".into(),
        "z.ai" | "zhipu" | "zhipuai" | "bigmodel" | "glm" => "zai".into(),
        "ollama" | "llamacpp" | "llama.cpp" | "llama-server" => "local".into(),
        other => other.into(),
    }
}

pub fn default_model(id: &str) -> Option<&'static str> {
    match id {
        "local" => Some("qwen3.5-4B-super-coder"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_aliases() {
        assert_eq!(normalize("kimi"), "moonshotai");
        assert_eq!(normalize("Open-Router"), "openrouter");
        assert_eq!(normalize("gemini"), "google");
        assert_eq!(normalize("ollama"), "local");
        assert_eq!(normalize("openai"), "openai");
        assert_eq!(normalize("z.ai"), "zai");
        assert_eq!(normalize("Zhipu"), "zai");
        assert_eq!(normalize("glm"), "zai");
    }

    #[test]
    fn zai_is_in_catalog() {
        let p = find("z.ai").expect("provider zai introuvable");
        assert_eq!(p.id, "zai");
        assert_eq!(p.base_url, "https://api.z.ai/api/paas/v4");
        assert_eq!(p.env_key, "ZAI_API_KEY");
    }
}
