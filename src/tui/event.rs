use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::tui::app::{App, Tab};

#[derive(Debug)]
pub enum Event {
    Key(KeyEvent),
    Tick,
    // Task results are added by Tasks 3-6:
    // ReportReady(Result<crate::model::Report, String>), ...
}

#[derive(Debug, PartialEq)]
pub enum Effect {
    Quit,
    // Added by Tasks 3-6:
    // SpawnReport, OpenUrl(String), WriteReport, PostSlack(String),
    // SaveConfig, ClearState, SpawnDoctor,
}

/// Pure state transition: no I/O, no terminal, no async. Fully unit-testable.
pub fn update(app: &mut App, ev: Event) -> Vec<Effect> {
    match ev {
        Event::Tick => {
            app.spinner = app.spinner.wrapping_add(1);
            vec![]
        }
        Event::Key(key) => handle_key(app, key),
    }
}

fn handle_key(app: &mut App, key: KeyEvent) -> Vec<Effect> {
    // Ctrl-C always quits cleanly.
    if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
        return vec![Effect::Quit];
    }
    if app.show_help {
        // Any key closes the help overlay.
        app.show_help = false;
        return vec![];
    }
    match key.code {
        KeyCode::Char('q') => vec![Effect::Quit],
        KeyCode::Char('?') => {
            app.show_help = true;
            vec![]
        }
        KeyCode::Char('1') => switch_tab(app, Tab::Report),
        KeyCode::Char('2') => switch_tab(app, Tab::Slack),
        KeyCode::Char('3') => switch_tab(app, Tab::Config),
        KeyCode::Char('4') => switch_tab(app, Tab::Doctor),
        _ => tab_key(app, key),
    }
}

fn switch_tab(app: &mut App, tab: Tab) -> Vec<Effect> {
    app.tab = tab;
    vec![]
}

/// Per-tab key handling; Tasks 3-6 fill in the arms.
fn tab_key(_app: &mut App, _key: KeyEvent) -> Vec<Effect> {
    vec![]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    fn app() -> App {
        App::new(Config::default(), None)
    }
    fn key(c: char) -> Event {
        Event::Key(KeyEvent::from(KeyCode::Char(c)))
    }

    #[test]
    fn q_quits() {
        assert_eq!(update(&mut app(), key('q')), vec![Effect::Quit]);
    }

    #[test]
    fn ctrl_c_quits() {
        let mut a = app();
        let ev = Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert_eq!(update(&mut a, ev), vec![Effect::Quit]);
    }

    #[test]
    fn number_keys_switch_tabs() {
        let mut a = app();
        update(&mut a, key('2'));
        assert_eq!(a.tab, Tab::Slack);
        update(&mut a, key('4'));
        assert_eq!(a.tab, Tab::Doctor);
    }

    #[test]
    fn help_overlay_toggles_and_any_key_closes() {
        let mut a = app();
        update(&mut a, key('?'));
        assert!(a.show_help);
        update(&mut a, key('x'));
        assert!(!a.show_help);
    }

    #[test]
    fn q_inside_help_only_closes_help() {
        let mut a = app();
        update(&mut a, key('?'));
        assert_eq!(update(&mut a, key('q')), vec![]);
        assert!(!a.show_help);
    }
}
