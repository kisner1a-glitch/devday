pub mod app;
pub mod event;
pub mod render;
pub mod task;

use std::io::IsTerminal;
use std::path::Path;
use std::time::Duration;

use app::App;
use event::{update, Effect, Event};

pub async fn run(config_path: Option<&Path>) -> anyhow::Result<()> {
    if !std::io::stdout().is_terminal() {
        anyhow::bail!("devday tui requires an interactive terminal (stdout is not a TTY)");
    }
    let cfg = crate::config::Config::load(config_path)?;
    // ratatui::init() enters the alternate screen, enables raw mode, and
    // installs a panic hook that restores the terminal before the panic prints.
    let mut terminal = ratatui::init();
    let result = event_loop(&mut terminal, cfg, config_path.map(|p| p.to_path_buf())).await;
    ratatui::restore();
    result
}

async fn event_loop(
    terminal: &mut ratatui::DefaultTerminal,
    cfg: crate::config::Config,
    cfg_path: Option<std::path::PathBuf>,
) -> anyhow::Result<()> {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Event>();

    // Blocking input thread: crossterm key events -> channel.
    let input_tx = tx.clone();
    std::thread::spawn(move || loop {
        match ratatui::crossterm::event::read() {
            Ok(ratatui::crossterm::event::Event::Key(k))
                if k.kind == ratatui::crossterm::event::KeyEventKind::Press =>
            {
                if input_tx.send(Event::Key(k)).is_err() {
                    break;
                }
            }
            Ok(_) => {}
            Err(_) => break,
        }
    });

    let mut app = App::new(cfg, cfg_path);
    let mut tick = tokio::time::interval(Duration::from_millis(200));

    loop {
        terminal.draw(|f| render::draw(f, &app))?;
        let ev = tokio::select! {
            Some(e) = rx.recv() => e,
            _ = tick.tick() => Event::Tick,
        };
        for effect in update(&mut app, ev) {
            if !run_effect(&mut app, effect, &tx) {
                return Ok(());
            }
        }
    }
}

/// Executes one effect. Returns false to quit. Tasks 3-6 add arms.
fn run_effect(
    _app: &mut App,
    effect: Effect,
    _tx: &tokio::sync::mpsc::UnboundedSender<Event>,
) -> bool {
    match effect {
        Effect::Quit => false,
    }
}
