use std::path::PathBuf;
use std::process::{Command, Stdio};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    Tick,
    Select,
    Theme(String),
    Panel,
    ToggleOn,
    ToggleOff,
    Error,
    Done,
}

const WAVS: &[(&str, &[u8])] = &[
    ("tick", include_bytes!("../assets/sfx/castle/tick.wav")),
    ("select", include_bytes!("../assets/sfx/castle/select.wav")),
    (
        "theme_castle",
        include_bytes!("../assets/sfx/castle/theme_castle.wav"),
    ),
    (
        "panel_open",
        include_bytes!("../assets/sfx/castle/panel_open.wav"),
    ),
    (
        "toggle_on",
        include_bytes!("../assets/sfx/castle/toggle_on.wav"),
    ),
    (
        "toggle_off",
        include_bytes!("../assets/sfx/castle/toggle_off.wav"),
    ),
    ("error", include_bytes!("../assets/sfx/castle/error.wav")),
    ("done", include_bytes!("../assets/sfx/castle/done.wav")),
];

pub fn enabled() -> bool {
    crate::config::load_global().sound.unwrap_or(false)
}

pub fn asset(ev: &Event) -> Option<(&'static str, &'static [u8])> {
    let name = match ev {
        Event::Tick => "tick",
        Event::Select => "select",
        Event::Theme(pack) if pack == "castle" => "theme_castle",
        Event::Theme(_) => return None,
        Event::Panel => "panel_open",
        Event::ToggleOn => "toggle_on",
        Event::ToggleOff => "toggle_off",
        Event::Error => "error",
        Event::Done => "done",
    };
    WAVS.iter().find(|(n, _)| *n == name).copied()
}

pub fn event(ev: Event) {
    if !enabled() {
        return;
    }
    let Some((name, data)) = asset(&ev) else {
        return;
    };
    play(name, data);
}

fn cache_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("zer0sfx-{name}.wav"))
}

fn play(name: &str, data: &'static [u8]) {
    let path = cache_path(name);
    let fresh = std::fs::metadata(&path)
        .map(|m| m.len() as usize != data.len())
        .unwrap_or(true);
    if fresh && std::fs::write(&path, data).is_err() {
        return;
    }
    let shown = path.to_string_lossy().to_string();

    let mut cmd = if cfg!(windows) {
        let mut c = Command::new("powershell");
        c.args([
            "-NoProfile",
            "-Command",
            &format!("(New-Object Media.SoundPlayer '{shown}').PlaySync()"),
        ]);
        c
    } else if cfg!(target_os = "macos") {
        let mut c = Command::new("afplay");
        c.arg(&shown);
        c
    } else if crate::platform::find_in_path("pw-play").is_some() {
        let mut c = Command::new("pw-play");
        c.arg(&shown);
        c
    } else if crate::platform::find_in_path("paplay").is_some() {
        let mut c = Command::new("paplay");
        c.arg(&shown);
        c
    } else if crate::platform::find_in_path("aplay").is_some() {
        let mut c = Command::new("aplay");
        c.args(["-q", &shown]);
        c
    } else {
        return;
    };
    let _ = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_asset_is_a_wav() {
        for (name, data) in WAVS {
            assert!(data.len() > 1_000, "{name} trop petit");
            assert_eq!(&data[..4], b"RIFF", "{name} n'est pas un RIFF");
            assert_eq!(&data[8..12], b"WAVE", "{name} n'est pas un WAVE");
        }
    }

    #[test]
    fn theme_sound_requires_a_pack() {
        assert!(asset(&Event::Theme("castle".into())).is_some());
        assert!(asset(&Event::Theme("ghost".into())).is_none());
        assert!(asset(&Event::Tick).is_some());
        assert_eq!(asset(&Event::Tick).unwrap().0, "tick");
    }
}
