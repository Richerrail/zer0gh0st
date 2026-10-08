pub mod bintime;
pub mod pong;
pub mod radio;

use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEventKind};

/// Full-screen "fun" modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Chat,
    Radio,
    BinTime,
    Pong,
}

/// Dump a single Pong frame to stdout (diagnostic).
pub fn debug_pong_frame() -> anyhow::Result<()> {
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    let backend = TestBackend::new(80, 30);
    let mut terminal = Terminal::new(backend)?;
    let pong = pong::Pong::new();
    terminal.draw(|f| pong.draw(f, f.area()))?;
    let buf = terminal.backend().buffer().clone();
    for y in 0..buf.area.height {
        let mut line = String::new();
        for x in 0..buf.area.width {
            line.push_str(buf[(x, y)].symbol());
        }
        println!("{line}");
    }
    Ok(())
}

/// Standalone runner for previewing a mode outside the agent.
pub fn run_standalone(mode: Mode) -> anyhow::Result<()> {
    let mut terminal = ratatui::init();
    let mut bt = bintime::BinTime::new();
    let mut pong = pong::Pong::new();
    let mut radio = radio::Radio::new();

    let res = (|| -> anyhow::Result<()> {
        let tick = Duration::from_millis(45);
        let mut next = Instant::now();
        loop {
            terminal.draw(|f| {
                let area = f.area();
                match mode {
                    Mode::BinTime => bt.draw(f, area),
                    Mode::Pong => pong.draw(f, area),
                    Mode::Radio => radio.draw(f, area),
                    Mode::Chat => {}
                }
            })?;

            if event::poll(tick)? {
                if let Event::Key(k) = event::read()? {
                    if matches!(k.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
                        match mode {
                            Mode::BinTime => {
                                if matches!(k.code, KeyCode::Esc | KeyCode::Char('q')) {
                                    break;
                                }
                            }
                            Mode::Pong => match k.code {
                                KeyCode::Esc | KeyCode::Char('q') => break,
                                KeyCode::Char('r') => pong.restart(),
                                KeyCode::Up | KeyCode::Char('w') => pong.set_dir(-1),
                                KeyCode::Down | KeyCode::Char('s') => pong.set_dir(1),
                                _ => {}
                            },
                            Mode::Radio => match k.code {
                                KeyCode::Esc | KeyCode::Char('q') => {
                                    radio.stop();
                                    break;
                                }
                                KeyCode::Up | KeyCode::Char('k') => radio.prev(),
                                KeyCode::Down | KeyCode::Char('j') => radio.next(),
                                KeyCode::Enter => radio.play_selected(),
                                KeyCode::Char('n') => radio.next_play(),
                                KeyCode::Char('p') => radio.prev_play(),
                                KeyCode::Char('s') => radio.stop(),
                                _ => {}
                            },
                            Mode::Chat => break,
                        }
                    }
                }
            }

            // fixed-step: advance as many ticks as wall-clock allows (bounded catch-up)
            let now = Instant::now();
            if now.duration_since(next) > tick * 3 {
                next = now;
            }
            while now >= next {
                match mode {
                    Mode::BinTime => bt.tick(),
                    Mode::Pong => pong.step(),
                    Mode::Radio => radio.tick(),
                    Mode::Chat => {}
                }
                next += tick;
            }
        }
        Ok(())
    })();

    ratatui::restore();
    res
}
