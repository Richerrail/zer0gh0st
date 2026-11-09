//! Journal structuré minimal (JSON sur stderr).
//!
//! M4 : les erreurs de diagnostic étaient éparpillées en `eprintln!` libres —
//! impossibles à filtrer, à parser ou à corréler. Ce module les remplace par un
//! objet JSON par ligne, sur le modèle déjà utilisé par `zer0-gw`
//! (`bin/zer0-gw.rs::log_line`).
//!
//! Choix : **pas de dépendance `tracing`**. Le besoin réel est « relire et
//! filtrer six messages de diagnostic », pas d'instrumenter des spans async.
//! Une soixantaine de lignes suffisent, et le binaire ne grossit pas.
//!
//! Niveau : `ZER0_LOG` = `off` | `error` | `warn` (défaut) | `info` | `debug`.
//!
//! **Ce module ne remplace pas les sorties CLI** : les messages destinés à
//! l'utilisateur (`usage: …`, `⚠ coffre verrouillé`, progression d'un tour)
//! restent des `eprintln!` lisibles. Ici, ce sont les diagnostics machine.

use std::sync::atomic::{AtomicU8, Ordering};

/// Gravité d'un événement. L'ordre est significatif : `Error < Warn < Info < Debug`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Error = 0,
    Warn = 1,
    Info = 2,
    Debug = 3,
}

impl Level {
    pub fn as_str(self) -> &'static str {
        match self {
            Level::Error => "error",
            Level::Warn => "warn",
            Level::Info => "info",
            Level::Debug => "debug",
        }
    }

    /// `None` = journalisation coupée (`off`).
    fn parse(s: &str) -> Option<Option<Level>> {
        match s.trim().to_ascii_lowercase().as_str() {
            "off" | "none" | "silent" => Some(None),
            "error" => Some(Some(Level::Error)),
            "warn" | "warning" => Some(Some(Level::Warn)),
            "info" => Some(Some(Level::Info)),
            "debug" | "trace" => Some(Some(Level::Debug)),
            _ => None,
        }
    }
}

/// Niveau courant. `OFF` = journalisation coupée.
///
/// Deux états distincts, longtemps confondus : « pas encore lu l'environnement »
/// et « coupé ». Les mélanger faisait qu'un `set_level(None)` était aussitôt
/// annulé par une relecture de `ZER0_LOG` — d'où le drapeau `INIT`.
const OFF: u8 = u8::MAX;
static LEVEL: AtomicU8 = AtomicU8::new(OFF);
static INIT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn current_level() -> Option<Level> {
    if !INIT.swap(true, Ordering::Relaxed) {
        // Première utilisation : on lit l'environnement (défaut `warn`).
        let parsed = std::env::var("ZER0_LOG")
            .ok()
            .as_deref()
            .and_then(Level::parse)
            .unwrap_or(Some(Level::Warn));
        LEVEL.store(parsed.map_or(OFF, |l| l as u8), Ordering::Relaxed);
    }
    match LEVEL.load(Ordering::Relaxed) {
        OFF => None,
        0 => Some(Level::Error),
        1 => Some(Level::Warn),
        2 => Some(Level::Info),
        _ => Some(Level::Debug),
    }
}

/// Force le niveau (tests, ou `--log-level` d'un CLI).
pub fn set_level(level: Option<Level>) {
    LEVEL.store(level.map_or(OFF, |l| l as u8), Ordering::Relaxed);
    INIT.store(true, Ordering::Relaxed);
}

/// Construit la ligne JSON (fonction pure, testable).
///
/// `fields` doit être un objet JSON ; les clés `ts`, `level` et `event` sont
/// réservées et ne peuvent pas être écrasées par un champ.
pub fn format_line(level: Level, event: &str, fields: &serde_json::Value) -> String {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let mut v = serde_json::json!({
        "ts": ts,
        "level": level.as_str(),
        "event": event,
    });
    if let (Some(obj), Some(extra)) = (v.as_object_mut(), fields.as_object()) {
        for (k, val) in extra {
            if k != "ts" && k != "level" && k != "event" {
                obj.insert(k.clone(), val.clone());
            }
        }
    }
    v.to_string()
}

/// Écrit un événement si son niveau passe le filtre. Ne panique jamais.
pub fn log(level: Level, event: &str, fields: serde_json::Value) {
    let Some(max) = current_level() else {
        return;
    };
    if level > max {
        return;
    }
    eprintln!("{}", format_line(level, event, &fields));
}

/// Raccourci : `log_error("audit_write", json!({"error": e.to_string()}))`.
pub fn log_error(event: &str, fields: serde_json::Value) {
    log(Level::Error, event, fields);
}

pub fn log_warn(event: &str, fields: serde_json::Value) {
    log(Level::Warn, event, fields);
}

pub fn log_info(event: &str, fields: serde_json::Value) {
    log(Level::Info, event, fields);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn format_line_is_valid_json_with_reserved_keys() {
        let line = format_line(Level::Warn, "telegram_poll", &json!({ "error": "timeout" }));
        let v: serde_json::Value = serde_json::from_str(&line).expect("JSON invalide");
        assert_eq!(v["level"], "warn");
        assert_eq!(v["event"], "telegram_poll");
        assert_eq!(v["error"], "timeout");
        assert!(v["ts"].is_u64());
    }

    /// Un champ utilisateur ne doit pas pouvoir usurper `level`/`event`/`ts`.
    #[test]
    fn format_line_refuses_to_override_reserved_keys() {
        let line = format_line(
            Level::Error,
            "audit_write",
            &json!({ "level": "debug", "event": "fake", "ts": 0, "error": "disk full" }),
        );
        let v: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(v["level"], "error", "niveau usurpé : {line}");
        assert_eq!(v["event"], "audit_write", "événement usurpé : {line}");
        assert_ne!(v["ts"], 0, "timestamp usurpé : {line}");
        assert_eq!(v["error"], "disk full");
    }

    #[test]
    fn level_parsing_covers_off_and_unknown() {
        assert_eq!(Level::parse("off"), Some(None));
        assert_eq!(Level::parse("WARN"), Some(Some(Level::Warn)));
        assert_eq!(Level::parse(" debug "), Some(Some(Level::Debug)));
        // Inconnu : on ne décide pas à la place de l'appelant.
        assert_eq!(Level::parse("bavard"), None);
    }

    #[test]
    fn levels_are_ordered_from_error_to_debug() {
        assert!(Level::Error < Level::Warn);
        assert!(Level::Warn < Level::Info);
        assert!(Level::Info < Level::Debug);
    }

    /// Le filtre doit couper ce qui est plus verbeux que le niveau courant.
    #[test]
    fn filter_drops_events_below_the_threshold() {
        set_level(Some(Level::Warn));
        assert!(Level::Error <= current_level().unwrap());
        assert!(Level::Warn <= current_level().unwrap());
        assert!(Level::Info > current_level().unwrap());
        assert!(Level::Debug > current_level().unwrap());

        set_level(None);
        assert!(current_level().is_none(), "off doit tout couper");

        // Remise à l'état attendu par les autres tests.
        set_level(Some(Level::Warn));
    }
}
