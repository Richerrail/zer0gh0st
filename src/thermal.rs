#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Guard {
    Ok,
    Warn(f32),
    Stop(f32),
    Kill(f32),
}

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

pub fn parse_milli(raw: &str) -> Option<f32> {
    let v: f32 = raw.trim().parse().ok()?;
    let c = v / 1000.0;
    (c > 1.0 && c < 150.0).then_some(c)
}

pub fn limits() -> (f32, f32, f32) {
    (
        env_celsius("ZER0_TEMP_WARN", 80.0),
        env_celsius("ZER0_TEMP_STOP", 88.0),
        env_celsius("ZER0_TEMP_KILL", 92.0),
    )
}

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
