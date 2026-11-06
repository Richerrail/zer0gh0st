use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStderr, Command, Stdio};

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Off,
    Idle,
    Listening,
    Thinking,
    Speaking,
}

impl State {
    pub fn label(&self) -> &'static str {
        match self {
            State::Off => "off",
            State::Idle => "en veille",
            State::Listening => "écoute…",
            State::Thinking => "réfléchit…",
            State::Speaking => "parle…",
        }
    }
}

pub fn rec_path() -> PathBuf {
    std::env::temp_dir().join("zer0_chan_mic.wav")
}

fn has(cmd: &str) -> bool {
    crate::platform::has(cmd)
}

fn recorder(path: &Path) -> Option<(Command, &'static str)> {
    if has("pw-record") {
        let mut c = Command::new("pw-record");
        c.args(["--format", "s16", "--rate", "16000", "--channels", "1"])
            .arg(path);
        return Some((c, "pw-record"));
    }
    if has("parecord") {
        let mut c = Command::new("parecord");
        c.args([
            "--format=s16le",
            "--rate=16000",
            "--channels=1",
            "--file-format=wav",
        ])
        .arg(path);
        return Some((c, "parecord"));
    }
    if has("arecord") {
        let mut c = Command::new("arecord");
        c.args(["-f", "S16_LE", "-r", "16000", "-c", "1"]).arg(path);
        return Some((c, "arecord"));
    }

    #[cfg(windows)]
    if has("sox_ng") {
        return Some((sox_recorder("sox_ng", path), "sox_ng"));
    }

    if has("sox") {
        return Some((sox_recorder("sox", path), "sox"));
    }
    None
}

fn sox_recorder(exe: &str, path: &Path) -> Command {
    let mut c = Command::new(exe);
    c.args([
        "-d",
        "-r",
        "16000",
        "-c",
        "1",
        "-b",
        "16",
        "-e",
        "signed-integer",
        "-t",
        "wav",
    ])
    .arg(path);
    c
}

fn recorder_hint() -> &'static str {
    if cfg!(windows) {
        "installer SoX_ng (`winget install sox_ng.sox_ng`), puis redémarrer le terminal"
    } else {
        "installer pipewire / pulseaudio-utils / alsa-utils (ou sox)"
    }
}

fn stderr_text(mut stderr: Option<ChildStderr>) -> String {
    let Some(mut stderr) = stderr.take() else {
        return String::new();
    };
    let mut s = String::new();
    let _ = stderr.read_to_string(&mut s);
    s.trim().chars().take(400).collect()
}

fn drain_stderr(stderr: Option<ChildStderr>) -> Option<std::thread::JoinHandle<String>> {
    stderr.map(|mut e| {
        std::thread::spawn(move || {
            let mut s = String::new();
            let _ = e.read_to_string(&mut s);
            s
        })
    })
}

fn repair_wav(path: &Path) -> std::io::Result<()> {
    use std::io::{Error, ErrorKind};

    let mut bytes = std::fs::read(path)?;
    if bytes.len() < 44 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "enregistrement WAV absent ou invalide",
        ));
    }

    let mut pos = 12usize;
    let mut data_pos = None;
    while pos + 8 <= bytes.len() {
        let chunk_id = &bytes[pos..pos + 4];
        let chunk_size = u32::from_le_bytes([
            bytes[pos + 4],
            bytes[pos + 5],
            bytes[pos + 6],
            bytes[pos + 7],
        ]) as usize;
        if chunk_id == b"data" {
            data_pos = Some((pos, chunk_size));
            break;
        }
        let payload = pos + 8;
        let Some(next) = payload
            .checked_add(chunk_size)
            .and_then(|n| n.checked_add(chunk_size & 1))
        else {
            break;
        };
        if next <= pos || next > bytes.len() {
            break;
        }
        pos = next;
    }

    let (data_pos, declared) = data_pos
        .ok_or_else(|| Error::new(ErrorKind::InvalidData, "chunk WAV « data » introuvable"))?;
    let payload = data_pos + 8;
    let available = bytes.len().saturating_sub(payload);
    if declared == available {
        return Ok(());
    }
    let provisional = declared == 0 || declared > available || declared % 2 != 0;
    if !provisional {
        return Ok(());
    }
    let mut data_len = available;
    data_len -= data_len % 2;
    let riff_len = bytes.len() - 8;
    if riff_len > u32::MAX as usize || data_len > u32::MAX as usize {
        return Err(Error::new(ErrorKind::InvalidData, "WAV trop volumineux"));
    }

    bytes[4..8].copy_from_slice(&(riff_len as u32).to_le_bytes());
    bytes[data_pos + 4..data_pos + 8].copy_from_slice(&(data_len as u32).to_le_bytes());
    std::fs::write(path, bytes)
}

pub struct ZeroChan {
    pub on: bool,
    pub state: State,
    pub frame: u32,
    pub last: String,
    rec: Option<Child>,
    rec_stderr: Option<std::thread::JoinHandle<String>>,
    rec_started: Option<std::time::Instant>,
}

pub const MAX_REC_SECS: f32 = 6.0;

impl Default for ZeroChan {
    fn default() -> Self {
        Self::new()
    }
}

impl ZeroChan {
    pub fn new() -> Self {
        ZeroChan {
            on: false,
            state: State::Off,
            frame: 0,
            last: String::new(),
            rec: None,
            rec_stderr: None,
            rec_started: None,
        }
    }

    pub fn toggle_on(&mut self) {
        self.on = !self.on;
        if self.on {
            self.state = State::Idle;
            self.last = "zero-chan activé".into();
        } else {
            self.stop_rec();
            self.state = State::Off;
        }
    }

    pub fn start_rec(&mut self) -> bool {
        if !self.on || self.rec.is_some() {
            return false;
        }
        let path = rec_path();
        let _ = std::fs::remove_file(&path);

        let Some((mut cmd, label)) = recorder(&path) else {
            self.last = format!("aucun enregistreur : {}", recorder_hint());
            return false;
        };
        let child = cmd
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn();
        let mut child = match child {
            Ok(c) => c,
            Err(e) => {
                self.last = format!("{label} : impossible de démarrer le micro ({e})");
                return false;
            }
        };

        std::thread::sleep(std::time::Duration::from_millis(120));
        if let Ok(Some(status)) = child.try_wait() {
            let detail = stderr_text(child.stderr.take());
            self.last = if detail.is_empty() {
                format!("{label} : micro indisponible (code {status})")
            } else {
                format!("{label} : micro indisponible — {detail}")
            };
            return false;
        }

        self.rec_stderr = drain_stderr(child.stderr.take());
        self.rec = Some(child);
        self.rec_started = Some(std::time::Instant::now());
        self.state = State::Listening;
        self.last = "écoute du micro…".into();
        true
    }

    pub fn stop_rec(&mut self) {
        self.rec_started = None;
        if let Some(mut c) = self.rec.take() {
            #[cfg(unix)]
            {
                let pid = c.id();
                let _ = Command::new("kill")
                    .arg("-INT")
                    .arg(pid.to_string())
                    .status();
            }
            #[cfg(not(unix))]
            {
                let _ = c.kill();
            }
            for _ in 0..20 {
                if let Ok(Some(_)) = c.try_wait() {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(25));
            }
            let _ = c.kill();
            let _ = c.wait();
        }

        let detail = self
            .rec_stderr
            .take()
            .and_then(|h| h.join().ok())
            .unwrap_or_default()
            .trim()
            .chars()
            .take(400)
            .collect::<String>();
        let path = rec_path();
        if !path.exists() {
            self.last = if detail.is_empty() {
                "micro indisponible : aucun fichier audio produit".into()
            } else {
                format!("micro indisponible — {detail}")
            };
            return;
        }
        if let Err(e) = repair_wav(&path) {
            self.last = if detail.is_empty() {
                format!("enregistrement WAV invalide : {e}")
            } else {
                format!("enregistrement WAV invalide : {e} — {detail}")
            };
        }
    }

    pub fn rec_elapsed(&self) -> Option<f32> {
        self.rec_started.map(|t| t.elapsed().as_secs_f32())
    }

    pub fn is_recording(&self) -> bool {
        self.rec.is_some()
    }

    pub fn tick(&mut self) {
        self.frame = self.frame.wrapping_add(1);
    }

    fn face(&self) -> String {
        let f = self.frame;
        let mouth = match self.state {
            State::Speaking => ["o", "O", "‿", "_"][((f / 3) % 4) as usize],
            State::Listening => {
                if (f / 6).is_multiple_of(2) {
                    "o"
                } else {
                    "‿"
                }
            }
            State::Thinking => {
                if (f / 10).is_multiple_of(3) {
                    "~"
                } else {
                    "_"
                }
            }
            State::Idle => {
                if (f / 22).is_multiple_of(4) {
                    "_"
                } else {
                    "‿"
                }
            }
            State::Off => "_",
        };
        let left = if (f / 33).is_multiple_of(11) {
            "-"
        } else {
            "◕"
        };
        let right = if (f / 24).is_multiple_of(8) {
            "¬"
        } else {
            "◕"
        };
        format!("({left}{mouth}{right})")
    }

    pub fn draw(&self, f: &mut Frame, area: Rect) {
        let color = match self.state {
            State::Off => theme::dim(),
            State::Idle => theme::accent(),
            State::Listening => theme::danger(),
            State::Thinking => theme::warning(),
            State::Speaking => theme::success(),
        };
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(color))
            .title(" ZERO-CHAN ");
        let inner = block.inner(area);
        f.render_widget(block, area);
        if inner.height < 2 {
            return;
        }

        let bars = inner.width.saturating_sub(2) as usize / 2;
        let mut vis: Vec<Span> = Vec::new();
        for i in 0..bars {
            let h = match self.state {
                State::Off => 0,
                State::Idle => ((self.frame as i64 + i as i64 * 3) % 8 == 0) as usize,
                _ => (((self.frame as f32 * 0.6 + i as f32 * 0.9).sin().abs()) * 3.0) as usize,
            };
            let ch = [" ", "▁", "▃", "▅", "▇"][h.min(4)];
            vis.push(Span::styled(ch, Style::default().fg(color)));
            vis.push(Span::raw(" "));
        }
        let mut lines = vec![
            Line::from(vec![
                Span::styled(format!("{}  ", self.face()), Style::default().fg(color)),
                Span::styled(self.state.label(), Style::default().fg(color)),
            ]),
            Line::from(vis),
        ];
        if !self.last.is_empty() {
            let t: String = self
                .last
                .chars()
                .take(inner.width.saturating_sub(2) as usize)
                .collect();
            lines.push(Line::from(Span::styled(
                t,
                Style::default().fg(theme::dim()),
            )));
        }
        f.render_widget(Paragraph::new(lines), inner);
    }
}

pub async fn transcribe(path: &PathBuf) -> anyhow::Result<String> {
    let code = "import sys\n\
        from faster_whisper import WhisperModel\n\
        m = WhisperModel('base', device='cpu', compute_type='int8')\n\
        segs, _ = m.transcribe(sys.argv[1], language='fr')\n\
        print(''.join(s.text for s in segs).strip())\n";
    let out = tokio::process::Command::new(crate::platform::python())
        .arg("-c")
        .arg(code)
        .arg(path)
        .output()
        .await?;
    if !out.status.success() {
        let err: String = String::from_utf8_lossy(&out.stderr)
            .chars()
            .take(300)
            .collect();
        return Err(anyhow::anyhow!(
            "Échec STT (faster-whisper). Interpréteur : `{}`.\n\
             Installe-le : `pip install \"faster-whisper\" \"av<16\"` (dans un venv, puis \
             `ZER0_PYTHON=/chemin/venv/bin/python`).\nDétail : {err}",
            crate::platform::python()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

pub fn draw_buttons(f: &mut Frame, area: Rect, zc: &ZeroChan) -> (Option<Rect>, Option<Rect>) {
    if area.width < 13 || area.height < 3 {
        return (None, None);
    }
    let border = if zc.on { theme::accent() } else { theme::dim() };
    let dim = Style::default().fg(border);
    let on_color = if zc.on {
        theme::success()
    } else {
        theme::dim()
    };
    let mic_color = if zc.is_recording() {
        theme::danger()
    } else if zc.on {
        theme::highlight()
    } else {
        theme::dim()
    };
    let s0 = format!("{:<3}", if zc.on { "on" } else { "off" });
    let s1 = format!(
        "{:<3}",
        if zc.is_recording() {
            "REC"
        } else if zc.on {
            "on"
        } else {
            "off"
        }
    );

    let title = " 0chan/mic ";
    let top = Line::from(vec![
        Span::styled("┌", dim),
        Span::styled(
            title,
            Style::default().fg(border).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "─".repeat(area.width as usize - 2 - title.chars().count()),
            dim,
        ),
        Span::styled("┐", dim),
    ]);
    let mid = Line::from(vec![
        Span::styled("│ ", dim),
        Span::styled(s0, Style::default().fg(on_color)),
        Span::styled(" / ", dim),
        Span::styled(s1, Style::default().fg(mic_color)),
        Span::styled(" │", dim),
    ]);
    let bottom = Line::from(vec![
        Span::styled("└", dim),
        Span::styled("─".repeat(area.width as usize - 2), dim),
        Span::styled("┘", dim),
    ]);
    let block_area = Rect {
        x: area.x,
        y: area.y,
        width: area.width,
        height: 3,
    };
    f.render_widget(Paragraph::new(vec![top, mid, bottom]), block_area);

    let r1 = Rect {
        x: area.x + 2,
        y: area.y + 1,
        width: 3,
        height: 1,
    };
    let r2 = Rect {
        x: area.x + 8,
        y: area.y + 1,
        width: 3,
        height: 1,
    };
    (Some(r1), Some(r2))
}

pub fn block_layout(banner_area: Rect, on: bool) -> (Rect, Rect) {
    if on && banner_area.width >= 96 {
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(70), Constraint::Length(26)])
            .split(banner_area);
        (cols[0], cols[1])
    } else {
        (
            banner_area,
            Rect {
                x: 0,
                y: 0,
                width: 0,
                height: 0,
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_wav(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("zer0-{name}-{nanos}.wav"))
    }

    #[test]
    fn repair_wav_fixes_truncated_riff_sizes() {
        let path = temp_wav("repair-wav");
        let mut wav = vec![0u8; 44];
        wav[0..4].copy_from_slice(b"RIFF");
        wav[8..12].copy_from_slice(b"WAVE");
        wav[12..16].copy_from_slice(b"fmt ");
        wav[16..20].copy_from_slice(&16u32.to_le_bytes());
        wav[20..22].copy_from_slice(&1u16.to_le_bytes());
        wav[22..24].copy_from_slice(&1u16.to_le_bytes());
        wav[24..28].copy_from_slice(&16_000u32.to_le_bytes());
        wav[28..32].copy_from_slice(&32_000u32.to_le_bytes());
        wav[32..34].copy_from_slice(&2u16.to_le_bytes());
        wav[34..36].copy_from_slice(&16u16.to_le_bytes());
        wav[36..40].copy_from_slice(b"data");
        wav[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
        wav[40..44].copy_from_slice(&u32::MAX.to_le_bytes());
        wav.extend_from_slice(&[0u8; 100]);
        std::fs::write(&path, &wav).unwrap();

        repair_wav(&path).unwrap();
        let fixed = std::fs::read(&path).unwrap();
        assert_eq!(u32::from_le_bytes(fixed[4..8].try_into().unwrap()), 136);
        assert_eq!(u32::from_le_bytes(fixed[40..44].try_into().unwrap()), 100);

        let wave = std::fs::File::open(&path).unwrap();
        let mut reader = std::io::BufReader::new(wave);
        let mut header = [0u8; 44];
        std::io::Read::read_exact(&mut reader, &mut header).unwrap();
        assert_eq!(&header[0..4], b"RIFF");
        assert_eq!(&header[8..12], b"WAVE");
        assert_eq!(&header[36..40], b"data");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn repair_wav_leaves_a_coherent_file_untouched() {
        let path = temp_wav("coherent");
        let mut wav = vec![0u8; 44];
        wav[0..4].copy_from_slice(b"RIFF");
        wav[8..12].copy_from_slice(b"WAVE");
        wav[12..16].copy_from_slice(b"fmt ");
        wav[16..20].copy_from_slice(&16u32.to_le_bytes());
        wav[36..40].copy_from_slice(b"data");
        wav.extend_from_slice(&[1u8; 100]);
        let riff_size = (wav.len() - 8) as u32;
        let data_size = (wav.len() - 44) as u32;
        wav[4..8].copy_from_slice(&riff_size.to_le_bytes());
        wav[40..44].copy_from_slice(&data_size.to_le_bytes());
        std::fs::write(&path, &wav).unwrap();

        let before = std::fs::read(&path).unwrap();
        repair_wav(&path).unwrap();
        assert_eq!(
            std::fs::read(&path).unwrap(),
            before,
            "fichier modifié inutilement"
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn repair_wav_rejects_non_wav() {
        let path = temp_wav("not-wav");
        std::fs::write(&path, b"not a wav file").unwrap();
        assert!(repair_wav(&path).is_err());
        let _ = std::fs::remove_file(path);
    }
}
