//! Moniteur système « btop-like » (sans btop) : CPU, RAM, températures,
//! processus. `sysinfo` fournit les métriques multiplateformes ; la mise en
//! forme est pure et testable.

use std::time::Duration;

use sysinfo::{Components, System};

/// Un processus tel qu'affiché (top 8).
#[derive(Debug, Clone, PartialEq)]
pub struct Proc {
    pub pid: u32,
    pub name: String,
    pub cpu: f32,
    pub mem: u64,
}

/// Photo instantanée de la machine.
#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    pub host: String,
    pub os: String,
    pub uptime: u64,
    pub cpu_total: f32,
    pub cores: Vec<f32>,
    pub mem_used: u64,
    pub mem_total: u64,
    pub swap_used: u64,
    pub swap_total: u64,
    pub load: (f64, f64, f64),
    pub procs: Vec<Proc>,
    pub temps: Vec<(String, f32)>,
}

/// État persistant : les deltas CPU n'ont de sens qu'entre deux relevés.
pub struct Monitor {
    sys: System,
    comps: Components,
}

impl Monitor {
    pub fn new() -> Self {
        let mut sys = System::new_all();
        sys.refresh_all();
        Self {
            sys,
            comps: Components::new_with_refreshed_list(),
        }
    }

    pub fn refresh(&mut self) {
        self.sys.refresh_all();
        self.comps.refresh(true);
    }

    /// Laisse à `sysinfo` le temps de calculer un delta CPU (premier relevé = 0).
    pub fn warmup(&mut self) {
        self.refresh();
        std::thread::sleep(Duration::from_millis(250));
        self.refresh();
    }

    pub fn snapshot(&self) -> Snapshot {
        let mut procs: Vec<Proc> = self
            .sys
            .processes()
            .values()
            .map(|p| Proc {
                pid: p.pid().as_u32(),
                name: p.name().to_string_lossy().to_string(),
                cpu: p.cpu_usage(),
                mem: p.memory(),
            })
            .filter(|p| p.cpu > 0.05 || p.mem > 32 * 1024 * 1024)
            .collect();
        procs.sort_by(|a, b| {
            b.cpu
                .partial_cmp(&a.cpu)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| b.mem.cmp(&a.mem))
        });
        procs.truncate(8);

        let load = System::load_average();
        Snapshot {
            host: System::host_name().unwrap_or_else(|| "?".into()),
            os: format!(
                "{} {}",
                System::name().unwrap_or_else(|| "?".into()),
                System::os_version().unwrap_or_default()
            )
            .trim()
            .to_string(),
            uptime: System::uptime(),
            cpu_total: self.sys.global_cpu_usage(),
            cores: self.sys.cpus().iter().map(|c| c.cpu_usage()).collect(),
            mem_used: self.sys.used_memory(),
            mem_total: self.sys.total_memory(),
            swap_used: self.sys.used_swap(),
            swap_total: self.sys.total_swap(),
            load: (load.one, load.five, load.fifteen),
            procs,
            temps: self
                .comps
                .list()
                .iter()
                .filter_map(|c| c.temperature().map(|t| (c.label().to_string(), t)))
                .collect(),
        }
    }
}

impl Default for Monitor {
    fn default() -> Self {
        Self::new()
    }
}

/// Octets → `12.3 MiB` (base 1024, comme btop).
pub fn bytes(n: u64) -> String {
    const U: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut v = n as f64;
    let mut i = 0;
    while v >= 1024.0 && i < U.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{n} B")
    } else {
        format!("{v:.1} {}", U[i])
    }
}

/// Barre ASCII : `[####----]  42%`.
pub fn bar(pct: f32, width: usize) -> String {
    let w = width.max(4);
    let pct = pct.clamp(0.0, 100.0);
    let filled = ((pct / 100.0) * w as f32).round() as usize;
    format!(
        "[{}{}] {:>3.0}%",
        "#".repeat(filled.min(w)),
        "-".repeat(w - filled.min(w)),
        pct
    )
}

/// Durée lisible : `3h12m`, `2j 4h`.
pub fn duration(secs: u64) -> String {
    let d = secs / 86_400;
    let h = (secs % 86_400) / 3_600;
    let m = (secs % 3_600) / 60;
    if d > 0 {
        format!("{d}j {h}h")
    } else if h > 0 {
        format!("{h}h{m:02}m")
    } else {
        format!("{m}m")
    }
}

/// Rendu texte complet (partagé par `zer0 t0p` et `/t0p`).
pub fn render(s: &Snapshot) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "Système — {} ({}) · uptime {}\n",
        s.host,
        s.os,
        duration(s.uptime)
    ));
    out.push_str(&format!("CPU  {}\n", bar(s.cpu_total, 24)));
    for (i, c) in s.cores.iter().enumerate() {
        out.push_str(&format!("  cœur {i:<2} {}\n", bar(*c, 20)));
    }
    let mem_pct = if s.mem_total > 0 {
        s.mem_used as f32 * 100.0 / s.mem_total as f32
    } else {
        0.0
    };
    out.push_str(&format!(
        "RAM  {}  {} / {}",
        bar(mem_pct, 24),
        bytes(s.mem_used),
        bytes(s.mem_total)
    ));
    if s.swap_total > 0 {
        out.push_str(&format!(
            " · swap {} / {}",
            bytes(s.swap_used),
            bytes(s.swap_total)
        ));
    }
    out.push('\n');
    out.push_str(&format!(
        "Charge: {:.2} {:.2} {:.2}\n",
        s.load.0, s.load.1, s.load.2
    ));
    if !s.temps.is_empty() {
        let t: Vec<String> = s
            .temps
            .iter()
            .map(|(l, v)| format!("{l} {v:.0} °C"))
            .collect();
        out.push_str(&format!("Temp: {}\n", t.join(" · ")));
    } else {
        out.push_str("Temp: indisponible sur cette plateforme\n");
    }
    if s.procs.is_empty() {
        out.push_str("Processus: (aucun relevé)\n");
    } else {
        out.push_str("Processus (CPU):\n");
        for p in &s.procs {
            out.push_str(&format!(
                "  {:<22} pid {:<7} {:>5.1}%  {:>9}\n",
                p.name,
                p.pid,
                p.cpu,
                bytes(p.mem)
            ));
        }
    }
    out.trim_end().to_string()
}

/// `/t0p` : un relevé, prêt à afficher dans la TUI.
pub fn snapshot_text() -> String {
    let mut m = Monitor::new();
    m.warmup();
    render(&m.snapshot())
}

/// `zer0 t0p [--watch] [--interval N]`.
pub async fn cli(watch: bool, interval: u64) -> anyhow::Result<()> {
    let mut m = Monitor::new();
    m.warmup();
    loop {
        if watch {
            // Efface l'écran et remonte le curseur.
            print!("\x1b[2J\x1b[H");
        }
        println!("{}", render(&m.snapshot()));
        if !watch {
            break;
        }
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(interval.max(1))) => m.refresh(),
            _ = tokio::signal::ctrl_c() => break,
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_uses_binary_units() {
        assert_eq!(bytes(0), "0 B");
        assert_eq!(bytes(1023), "1023 B");
        assert_eq!(bytes(1024), "1.0 KiB");
        assert_eq!(bytes(15 * 1024 * 1024 * 1024), "15.0 GiB");
    }

    #[test]
    fn bar_is_bounded() {
        assert!(bar(0.0, 10).starts_with("[----------]"));
        assert!(bar(100.0, 10).starts_with("[##########]"));
        assert!(bar(150.0, 10).contains("100%"));
    }

    #[test]
    fn duration_is_readable() {
        assert_eq!(duration(59), "0m");
        assert_eq!(duration(3_600), "1h00m");
        assert_eq!(duration(86_400 + 7_200), "1j 2h");
    }

    #[test]
    fn render_contains_the_essentials() {
        let s = Snapshot {
            host: "test".into(),
            os: "linux".into(),
            uptime: 3_600,
            cpu_total: 42.0,
            cores: vec![10.0, 90.0],
            mem_used: 1024 * 1024 * 1024,
            mem_total: 2 * 1024 * 1024 * 1024,
            swap_total: 0,
            procs: vec![Proc {
                pid: 42,
                name: "firefox".into(),
                cpu: 12.3,
                mem: 512 * 1024 * 1024,
            }],
            temps: vec![("k10temp".into(), 56.0)],
            ..Default::default()
        };
        let t = render(&s);
        assert!(t.contains("test"), "{t}");
        assert!(t.contains("cœur 1"), "{t}");
        assert!(t.contains("firefox"), "{t}");
        assert!(t.contains("56 °C"), "{t}");
        assert!(t.contains("42%"), "{t}");
    }

    #[test]
    fn render_says_when_temperatures_are_unavailable() {
        let s = Snapshot {
            host: "mac".into(),
            mem_total: 1024,
            ..Default::default()
        };
        assert!(render(&s).contains("indisponible"));
    }
}
