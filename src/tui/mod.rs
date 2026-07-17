pub mod app;
pub mod event;
pub mod render;
pub mod task;

use std::io::IsTerminal;
use std::path::{Path, PathBuf};
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

/// Executes one effect. Returns false to quit. Tasks 5-6 add remaining arms.
fn run_effect(
    app: &mut App,
    effect: Effect,
    tx: &tokio::sync::mpsc::UnboundedSender<Event>,
) -> bool {
    match effect {
        Effect::Quit => false,
        Effect::SpawnReport => {
            task::spawn_report(tx.clone(), app.cfg.clone(), app.report_options());
            true
        }
        Effect::OpenUrl(url) => {
            app.status = match open::that(&url) {
                Ok(()) => format!("opened {url}"),
                Err(e) => format!("open failed: {e}"),
            };
            true
        }
        Effect::WriteReport => {
            app.status = write_report(app);
            true
        }
        Effect::PostSlack(text) => {
            task::spawn_post(tx.clone(), app.cfg.clone(), text);
            true
        }
        Effect::SaveConfig => {
            let path = app.cfg_path.clone().unwrap_or_else(default_config_path);
            match save_config(&app.cfg, &path) {
                Ok(()) => {
                    app.config_tab.dirty = false;
                    app.config_tab.error = None;
                    app.status = format!("saved {}", path.display());
                }
                Err(e) => {
                    app.config_tab.error = Some(e);
                    app.status = "save failed".into();
                }
            }
            true
        }
        Effect::ClearState => {
            let path = app.cfg.state_path();
            app.status = match crate::state::save(&path, &crate::model::LocalState::default()) {
                Ok(()) => "state cleared".into(),
                Err(e) => format!("clear failed: {e}"),
            };
            app.doctor.state_summary = Some(crate::state::load(&path));
            true
        }
        Effect::SpawnDoctor => {
            task::spawn_doctor(tx.clone(), app.cfg.clone());
            true
        }
    }
}

/// Serialize -> re-parse validate -> write to a `.toml.tmp` sibling -> rename
/// into place. Validation failure writes nothing (no partial/invalid config
/// is ever left on disk).
pub fn save_config(cfg: &crate::config::Config, path: &Path) -> Result<(), String> {
    let text = toml::to_string_pretty(cfg).map_err(|e| format!("serialize: {e}"))?;
    // Validate: what we write must parse back into an identical Config.
    let _check: crate::config::Config =
        toml::from_str(&text).map_err(|e| format!("round-trip validation failed: {e}"))?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("mkdir: {e}"))?;
    }
    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, &text).map_err(|e| format!("write: {e}"))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("rename: {e}"))?;
    Ok(())
}

/// Mirrors the private helper in `config.rs`: `$HOME/.config/devday/config.toml`.
fn default_config_path() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".config")
        .join("devday")
        .join("config.toml")
}

fn write_report(app: &mut App) -> String {
    let Some(rep) = &app.report.report else {
        return "no report to write".into();
    };
    let md = crate::redact::apply(
        &crate::report::markdown::render(rep, false),
        &app.cfg.redact,
    );
    let path = app.cfg.output.clone().unwrap_or_else(|| "report.md".into());
    match std::fs::write(&path, md) {
        Ok(()) => format!("wrote {path}"),
        Err(e) => format!("write failed: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_config_is_atomic_and_reloadable() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("config.toml");
        let mut cfg = crate::config::Config::default();
        cfg.slack.channel = Some("#eng".into());
        save_config(&cfg, &path).unwrap();
        let loaded = crate::config::Config::load(Some(&path)).unwrap();
        assert_eq!(loaded.slack.channel.as_deref(), Some("#eng"));
        assert!(!path.with_extension("toml.tmp").exists());
    }
}
