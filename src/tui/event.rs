use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::tui::app::{App, Pane, Tab};

#[derive(Debug)]
pub enum Event {
    Key(KeyEvent),
    Tick,
    ReportReady(Result<crate::model::Report, String>),
    PostResult(Result<(), String>),
    DoctorReady(Vec<(String, bool, String)>),
}

#[derive(Debug, PartialEq)]
pub enum Effect {
    Quit,
    SpawnReport,
    OpenUrl(String),
    WriteReport,
    PostSlack(String),
    SaveConfig,
    ClearState,
    SpawnDoctor,
}

/// Pure state transition: no I/O, no terminal, no async. Fully unit-testable.
pub fn update(app: &mut App, ev: Event) -> Vec<Effect> {
    match ev {
        Event::Tick => {
            app.spinner = app.spinner.wrapping_add(1);
            if !app.started {
                app.started = true;
                if app.tab == Tab::Report && app.report.report.is_none() && !app.report.loading {
                    app.report.loading = true;
                    return vec![Effect::SpawnReport];
                }
            }
            vec![]
        }
        Event::Key(key) => handle_key(app, key),
        Event::ReportReady(res) => {
            app.report.loading = false;
            match res {
                Ok(rep) => {
                    let md = crate::redact::apply(
                        &crate::report::markdown::render(&rep, true),
                        &app.cfg.redact,
                    );
                    app.report.sections = md.lines().map(String::from).collect();
                    app.report.report = Some(rep);
                    app.report.error = None;
                    app.report.selected_group = 0;
                    app.report.selected_item = 0;
                    app.status = "report ready".into();
                    app.refresh_digest();
                }
                Err(e) => {
                    app.report.error = Some(e);
                    app.status = "report failed".into();
                }
            }
            vec![]
        }
        Event::PostResult(res) => {
            app.slack.posting = false;
            app.slack.result = Some(match res {
                Ok(()) => "posted successfully".into(),
                Err(e) => format!("post failed: {e}"),
            });
            app.refresh_digest(); // picks up the new posted-hash state
            vec![]
        }
        Event::DoctorReady(checks) => {
            app.doctor.running = false;
            app.doctor.checks = Some(checks);
            vec![]
        }
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
    if tab == Tab::Report && app.report.report.is_none() && !app.report.loading {
        app.report.loading = true;
        return vec![Effect::SpawnReport];
    }
    if tab == Tab::Slack {
        app.refresh_digest();
    }
    if tab == Tab::Doctor {
        app.doctor.state_summary = Some(crate::state::load(&app.cfg.state_path()));
        if app.doctor.checks.is_none() && !app.doctor.running {
            app.doctor.running = true;
            return vec![Effect::SpawnDoctor];
        }
    }
    vec![]
}

/// Per-tab key handling.
fn tab_key(app: &mut App, key: KeyEvent) -> Vec<Effect> {
    match app.tab {
        Tab::Report => report_key(app, key),
        Tab::Slack => slack_key(app, key),
        Tab::Config => config_key(app, key),
        Tab::Doctor => doctor_key(app, key),
    }
}

fn slack_key(app: &mut App, key: KeyEvent) -> Vec<Effect> {
    if app.slack.modal {
        return match key.code {
            KeyCode::Enter | KeyCode::Char('y') => {
                app.slack.modal = false;
                if let Some(text) = app.slack.digest.clone() {
                    app.slack.posting = true;
                    app.status = "posting...".into();
                    vec![Effect::PostSlack(text)]
                } else {
                    vec![]
                }
            }
            _ => {
                app.slack.modal = false;
                app.status = "post canceled".into();
                vec![]
            }
        };
    }
    match key.code {
        KeyCode::Char('v') => {
            app.slack.verbose = !app.slack.verbose;
            app.refresh_digest();
            vec![]
        }
        KeyCode::Char('p') => {
            if app.slack.posting {
                app.status = "post already in flight".into();
            } else if app.slack.digest.is_none() {
                app.status = "no report yet - generate on the Report tab (1, then r)".into();
            } else if !app.slack_creds_configured() {
                app.status =
                    "no Slack credentials configured (set slack.webhook_env or bot_token_env + channel)"
                        .into();
            } else {
                app.slack.modal = true;
            }
            vec![]
        }
        _ => vec![],
    }
}

fn report_key(app: &mut App, key: KeyEvent) -> Vec<Effect> {
    let r = &mut app.report;
    match key.code {
        KeyCode::Char('r') => {
            if r.loading {
                app.status = "generation already in flight".into();
                vec![]
            } else {
                r.loading = true;
                app.status = "generating...".into();
                vec![Effect::SpawnReport]
            }
        }
        KeyCode::Char('s') => {
            r.since_idx = (r.since_idx + 1) % crate::tui::app::SINCE_CHOICES.len();
            app.status = format!(
                "window: {} (r to regenerate)",
                crate::tui::app::SINCE_CHOICES[r.since_idx]
            );
            vec![]
        }
        KeyCode::Char('a') => {
            r.ai_enabled = !r.ai_enabled;
            app.status = format!(
                "AI: {} (r to regenerate)",
                if r.ai_enabled { "on" } else { "off" }
            );
            vec![]
        }
        KeyCode::Char('w') => vec![Effect::WriteReport],
        KeyCode::Tab => {
            r.pane = match r.pane {
                Pane::Groups => Pane::Detail,
                Pane::Detail => Pane::Groups,
            };
            vec![]
        }
        KeyCode::Down | KeyCode::Char('j') => {
            move_selection(r, 1);
            vec![]
        }
        KeyCode::Up | KeyCode::Char('k') => {
            move_selection(r, -1);
            vec![]
        }
        KeyCode::Enter => {
            if let Some(url) = selected_url(r) {
                return vec![Effect::OpenUrl(url)];
            }
            vec![]
        }
        _ => vec![],
    }
}

fn move_selection(r: &mut crate::tui::app::ReportState, delta: isize) {
    let Some(rep) = &r.report else { return };
    match r.pane {
        Pane::Groups => {
            let len = rep.groups.len();
            if len > 0 {
                r.selected_group =
                    (r.selected_group as isize + delta).rem_euclid(len as isize) as usize;
                r.selected_item = 0;
            }
        }
        Pane::Detail => {
            let len = rep
                .groups
                .get(r.selected_group)
                .map_or(0, |g| g.items.len());
            if len > 0 {
                r.selected_item =
                    (r.selected_item as isize + delta).rem_euclid(len as isize) as usize;
            }
        }
    }
}

/// `Tab::Config` key handling: field navigation, toggles/cycles, text
/// editing, and the S -> confirm -> `Effect::SaveConfig` save flow.
///
/// Note: the text-input branch below borrows `app.config_tab.editing` fresh
/// in each arm (`if let Some(buf) = &mut app.config_tab.editing`) rather than
/// binding one `buf` up front and calling `.take()` in the Enter arm - the
/// borrow checker rejects that shape because `buf`'s borrow of
/// `app.config_tab.editing` is still live across the match while the Enter
/// arm needs its own mutable borrow of the same field. Behavior is identical.
fn config_key(app: &mut App, key: KeyEvent) -> Vec<Effect> {
    use crate::tui::app::{Field, FIELDS};
    let field = FIELDS[app.config_tab.selected];

    // Text-input mode.
    if app.config_tab.editing.is_some() {
        match key.code {
            KeyCode::Enter => {
                let value = app.config_tab.editing.take().unwrap();
                app.apply_field_edit(field, value);
                app.config_tab.dirty = true;
            }
            KeyCode::Esc => app.config_tab.editing = None,
            KeyCode::Backspace => {
                if let Some(buf) = &mut app.config_tab.editing {
                    buf.pop();
                }
            }
            KeyCode::Char(ch) => {
                if let Some(buf) = &mut app.config_tab.editing {
                    buf.push(ch);
                }
            }
            _ => {}
        }
        return vec![];
    }
    // Save-confirm modal.
    if app.config_tab.confirm_save {
        app.config_tab.confirm_save = false;
        return match key.code {
            KeyCode::Enter | KeyCode::Char('y') => vec![Effect::SaveConfig],
            _ => {
                app.status = "save canceled".into();
                vec![]
            }
        };
    }
    match key.code {
        KeyCode::Down | KeyCode::Char('j') => {
            app.config_tab.selected = (app.config_tab.selected + 1) % FIELDS.len();
            vec![]
        }
        KeyCode::Up | KeyCode::Char('k') => {
            app.config_tab.selected = (app.config_tab.selected + FIELDS.len() - 1) % FIELDS.len();
            vec![]
        }
        KeyCode::Enter => {
            match field {
                Field::SourceGithub => app.cfg.sources.github = !app.cfg.sources.github,
                Field::SourceLinear => app.cfg.sources.linear = !app.cfg.sources.linear,
                Field::SourceGit => app.cfg.sources.git = !app.cfg.sources.git,
                Field::SlackAutoPost => app.cfg.slack.auto_post = !app.cfg.slack.auto_post,
                Field::GitRootRemoveLast => {
                    app.cfg.git.roots.pop();
                }
                Field::AiProvider => {
                    app.cfg.ai.provider = match app.cfg.ai.provider.as_deref() {
                        None => Some("claude".into()),
                        Some("claude") => Some("codex".into()),
                        _ => None,
                    };
                }
                Field::GitRootAdd => {
                    app.config_tab.editing = Some(String::new());
                    return vec![];
                }
                _ => {
                    app.config_tab.editing = Some(app.field_current_text(field));
                    return vec![];
                }
            }
            app.config_tab.dirty = true;
            vec![]
        }
        KeyCode::Char('S') => {
            app.config_tab.confirm_save = true;
            vec![]
        }
        _ => vec![],
    }
}

/// `Tab::Doctor` key handling: `r` re-runs checks (ignored while a run is in
/// flight), `x` opens a typed-confirm modal that requires typing the literal
/// word "clear" before Enter emits `Effect::ClearState`. Enter always closes
/// the modal (via `.take()`), whether or not the typed word matched; on a
/// mismatch it just leaves a status hint and emits no effect.
///
/// Note: mirrors `config_key`'s text-input shape - `app.doctor.confirm_clear`
/// is re-borrowed fresh in each arm rather than bound once up front, because
/// the Enter arm needs its own `.take()` while Backspace/Char need a live
/// `&mut` into the same `Option<String>`.
fn doctor_key(app: &mut App, key: KeyEvent) -> Vec<Effect> {
    if app.doctor.confirm_clear.is_some() {
        match key.code {
            KeyCode::Enter => {
                let typed = app.doctor.confirm_clear.take().unwrap();
                if typed == "clear" {
                    return vec![Effect::ClearState];
                }
                app.status = "type 'clear' to confirm".into();
            }
            KeyCode::Esc => app.doctor.confirm_clear = None,
            KeyCode::Backspace => {
                if let Some(buf) = &mut app.doctor.confirm_clear {
                    buf.pop();
                }
            }
            KeyCode::Char(ch) => {
                if let Some(buf) = &mut app.doctor.confirm_clear {
                    buf.push(ch);
                }
            }
            _ => {}
        }
        return vec![];
    }
    match key.code {
        KeyCode::Char('r') => {
            if !app.doctor.running {
                app.doctor.running = true;
                vec![Effect::SpawnDoctor]
            } else {
                vec![]
            }
        }
        KeyCode::Char('x') => {
            app.doctor.confirm_clear = Some(String::new());
            vec![]
        }
        _ => vec![],
    }
}

fn selected_url(r: &crate::tui::app::ReportState) -> Option<String> {
    r.report
        .as_ref()?
        .groups
        .get(r.selected_group)?
        .items
        .get(r.selected_item)?
        .url
        .clone()
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

    #[test]
    fn r_spawns_report_once() {
        let mut a = app();
        assert_eq!(update(&mut a, key('r')), vec![Effect::SpawnReport]);
        assert!(a.report.loading);
        // Second r while loading is ignored.
        assert_eq!(update(&mut a, key('r')), vec![]);
    }

    #[test]
    fn report_ready_stores_redacted_sections() {
        let mut a = app();
        a.report.loading = true;
        let rep = crate::model::Report {
            window_start: chrono::Utc::now(),
            window_end: chrono::Utc::now(),
            groups: vec![],
            summary: Some("has ghp_LEAKME99 inside".into()),
            worked_on: vec![],
            next_up: vec![],
            blockers: vec![],
            source_links: vec![],
            generation_warnings: vec![],
        };
        update(&mut a, Event::ReportReady(Ok(rep)));
        assert!(!a.report.loading);
        let joined = a.report.sections.join("\n");
        assert!(joined.contains("[REDACTED]"));
        assert!(!joined.contains("ghp_LEAKME99"));
    }

    #[test]
    fn s_cycles_since_window() {
        let mut a = app();
        update(&mut a, key('s'));
        assert_eq!(a.report.since_idx, 1);
        update(&mut a, key('s'));
        update(&mut a, key('s'));
        assert_eq!(a.report.since_idx, 0);
    }

    #[test]
    fn enter_opens_selected_url() {
        let mut a = app();
        // reuse the render fixture shape: one group, one item with a url
        a.report.report = Some(crate::tui::render::report::tests_fixture());
        let fx = update(&mut a, Event::Key(KeyEvent::from(KeyCode::Enter)));
        assert_eq!(fx, vec![Effect::OpenUrl("https://example.com/x".into())]);
    }

    #[test]
    fn p_without_creds_sets_status_not_modal() {
        let mut a = app();
        a.tab = Tab::Slack;
        a.slack.digest = Some("hi".into());
        update(&mut a, key('p'));
        assert!(!a.slack.modal);
        assert!(a.status.contains("credentials"));
    }

    #[test]
    fn p_with_creds_opens_modal_and_confirm_posts() {
        let mut a = app();
        a.tab = Tab::Slack;
        a.cfg.slack.webhook_env = Some("HOOK".into());
        a.slack.digest = Some("digest text".into());
        update(&mut a, key('p'));
        assert!(a.slack.modal);
        let fx = update(&mut a, Event::Key(KeyEvent::from(KeyCode::Enter)));
        assert_eq!(fx, vec![Effect::PostSlack("digest text".into())]);
        assert!(a.slack.posting);
    }

    #[test]
    fn modal_any_other_key_cancels() {
        let mut a = app();
        a.tab = Tab::Slack;
        a.cfg.slack.webhook_env = Some("HOOK".into());
        a.slack.digest = Some("x".into());
        update(&mut a, key('p'));
        assert_eq!(update(&mut a, key('n')), vec![]);
        assert!(!a.slack.modal);
        assert!(!a.slack.posting);
    }

    #[test]
    fn post_result_recorded() {
        let mut a = app();
        a.slack.posting = true;
        update(
            &mut a,
            Event::PostResult(Err("slack webhook: network error".into())),
        );
        assert!(!a.slack.posting);
        assert!(a.slack.result.as_deref().unwrap().contains("network error"));
    }

    #[test]
    fn toggle_source_marks_dirty() {
        let mut a = app();
        a.tab = Tab::Config;
        let before = a.cfg.sources.github;
        update(&mut a, Event::Key(KeyEvent::from(KeyCode::Enter)));
        assert_eq!(a.cfg.sources.github, !before);
        assert!(a.config_tab.dirty);
    }

    #[test]
    fn edit_channel_roundtrip() {
        let mut a = app();
        a.tab = Tab::Config;
        a.config_tab.selected = 5; // SlackChannel
        update(&mut a, Event::Key(KeyEvent::from(KeyCode::Enter)));
        assert!(a.config_tab.editing.is_some());
        for ch in "#eng".chars() {
            update(&mut a, key(ch));
        }
        update(&mut a, Event::Key(KeyEvent::from(KeyCode::Enter)));
        assert_eq!(a.cfg.slack.channel.as_deref(), Some("#eng"));
    }

    #[test]
    fn capital_s_then_enter_emits_save() {
        let mut a = app();
        a.tab = Tab::Config;
        update(&mut a, key('S'));
        assert!(a.config_tab.confirm_save);
        let fx = update(&mut a, Event::Key(KeyEvent::from(KeyCode::Enter)));
        assert_eq!(fx, vec![Effect::SaveConfig]);
    }

    #[test]
    fn doctor_ready_stores_checks() {
        let mut a = app();
        a.doctor.running = true;
        update(
            &mut a,
            Event::DoctorReady(vec![("gh".into(), true, "ok".into())]),
        );
        assert!(!a.doctor.running);
        assert_eq!(a.doctor.checks.as_ref().unwrap().len(), 1);
    }

    #[test]
    fn clear_state_requires_typed_confirm() {
        let mut a = app();
        a.tab = Tab::Doctor;
        update(&mut a, key('x'));
        assert!(a.doctor.confirm_clear.is_some());
        // Wrong word does nothing.
        for ch in "nope".chars() {
            update(&mut a, key(ch));
        }
        assert_eq!(
            update(&mut a, Event::Key(KeyEvent::from(KeyCode::Enter))),
            vec![]
        );
        // Correct word emits ClearState.
        update(&mut a, key('x'));
        for ch in "clear".chars() {
            update(&mut a, key(ch));
        }
        assert_eq!(
            update(&mut a, Event::Key(KeyEvent::from(KeyCode::Enter))),
            vec![Effect::ClearState]
        );
    }
}
