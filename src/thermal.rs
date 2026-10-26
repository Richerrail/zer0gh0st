//! Garde-fou thermique du mode local.
//!
//! Sur un portable ancien, un modèle local peut pousser le CPU à 90 °C+ et
//! provoquer un arrêt brutal. On lit les capteurs `hwmon` (Linux) et on coupe
//! **proprement** : d'abord la génération, puis le serveur si ça monte encore.
//!
//! Seuils (variables d'environnement, en °C) :
//! - `ZER0_TEMP_WARN` (80) : simple avertissement, la génération continue ;
//! - `ZER0_TEMP_STOP` (88) : génération interrompue ;
//! - `ZER0_TEMP_KILL` (92) : `llama-server` arrêté (si c'est le nôtre).
//!
//! Sur macOS/Windows, les capteurs ne sont pas exposés de façon portable :
//! `max_celsius()` renvoie `None` et le garde-fou est inactif (aucun échec).

/// Décision du garde-fou pour une température donnée.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Guard {
    /// Rien à signaler.
    Ok,
    /// Chaud : on prévient, on continue.
    Warn(f32),
    /// Trop chaud : on interrompt la génération.
    Stop(f32),
    /// Critique : on interrompt et on arrête le serveur local.
    Kill(f32),
}

/// Lit la température maximale connue (en °C), si disponible.
pub fn max_celsius() -> Option<f32> {
    #[cfg(target_os = "linux")]
    {
        let mut best: Option<f32> = None;
        let Ok(dir) = std::fs::read_dir("/sys/class/hwmon") else {
            return None;
        };
        for entry in dir.flatten() {
            let Ok(files) = std::fs::read_dir(entry.path()) else {
                continue;
            };
            for f in files.flatten() {
                let name = f.file_name();
                let name = name.to_string_lossy();
                if !(name.starts_with("temp") && name.ends_with("_input")) {
                    continue;
                }
                let Ok(raw) = std::fs::read_to_string(f.path()) else {
                    continue;
                };
                if let Some(t) = parse_milli(&raw) {
                    best = Some(best.map_or(t, |b: f32| b.max(t)));
                }
            }
        }
        best
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

/// Millidegrés → °C, avec filtre de plausibilité (ignore 0 et les valeurs
/// aberrantes de certains pilotes).
pub fn parse_milli(raw: &str) -> Option<f32> {
    let v: f32 = raw.trim().parse().ok()?;
    let c = v / 1000.0;
    (c > 1.0 && c < 150.0).then_some(c)
}

/// Lit les seuils configurés : (warn, stop, kill).
pub fn limits() -> (f32, f32, f32) {
    (
        env_celsius("ZER0_TEMP_WARN", 80.0),
        env_celsius("ZER0_TEMP_STOP", 88.0),
        env_celsius("ZER0_TEMP_KILL", 92.0),
    )
}

/// Classe une température (pure, testable).
pub fn classify(temp: f32, limits: (f32, f32, f32)) -> Guard {
    let (warn, stop, kill) = limits;
    if temp >= kill {
        Guard::Kill(temp)
    } else if temp >= stop {
        Guard::Stop(temp)
    } else if temp >= warn {
        Guard::Warn(temp)
    } else {
        Guard::Ok
    }
}

/// Vérifie l'état thermique courant.
pub fn check() -> Guard {
    match max_celsius() {
        Some(t) => classify(t, limits()),
        None => Guard::Ok,
    }
}

fn env_celsius(key: &str, default: f32) -> f32 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.trim().parse::<f32>().ok())
        .filter(|v| *v > 20.0 && *v < 150.0)
        .unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_milli_filters_junk() {
        assert_eq!(parse_milli("56000\n"), Some(56.0));
        assert_eq!(parse_milli("0"), None);
        assert_eq!(parse_milli("-5000"), None);
        assert_eq!(parse_milli("abc"), None);
        assert_eq!(parse_milli("200000"), None);
    }

    #[test]
    fn classify_uses_the_three_thresholds() {
        let l = (80.0, 88.0, 92.0);
        assert_eq!(classify(70.0, l), Guard::Ok);
        assert_eq!(classify(80.0, l), Guard::Warn(80.0));
        assert_eq!(classify(87.9, l), Guard::Warn(87.9));
        assert_eq!(classify(88.0, l), Guard::Stop(88.0));
        assert_eq!(classify(92.0, l), Guard::Kill(92.0));
    }
}
