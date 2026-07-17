use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::tui::app::App;

pub fn draw(f: &mut Frame, area: Rect, app: &App) {
    let d = &app.doctor;

    let [checks_area, state_area] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(9)]).areas(area);

    let mut check_lines: Vec<Line> = Vec::new();
    match &d.checks {
        None => check_lines.push(Line::from(if d.running {
            "running checks..."
        } else {
            "no checks yet"
        })),
        Some(checks) => {
            if checks.is_empty() {
                check_lines.push(Line::from("no checks configured"));
            }
            for (name, ok, detail) in checks {
                if *ok {
                    check_lines.push(Line::from(format!("OK   {name} — {detail}")));
                } else {
                    check_lines.push(Line::styled(
                        format!("FAIL {name} — {detail}"),
                        Style::default().add_modifier(Modifier::REVERSED),
                    ));
                }
            }
        }
    }
    let checks_title = if d.running {
        "Checks (running...) - r re-run, x clear state"
    } else {
        "Checks - r re-run, x clear state"
    };
    f.render_widget(
        Paragraph::new(check_lines)
            .block(Block::default().borders(Borders::ALL).title(checks_title)),
        checks_area,
    );

    let mut state_lines: Vec<Line> = Vec::new();
    match &d.state_summary {
        None => state_lines.push(Line::from("state not loaded")),
        Some(st) => {
            let last_run = st
                .last_successful_run_at
                .map(|t| t.to_rfc3339())
                .unwrap_or_else(|| "-".into());
            state_lines.push(Line::from(format!("last run: {last_run}")));
            state_lines.push(Line::from(format!(
                "posted count: {}",
                st.posted_report_hashes.len()
            )));
            state_lines.push(Line::from("last hashes:"));
            let recent = st
                .posted_report_hashes
                .iter()
                .rev()
                .take(5)
                .collect::<Vec<_>>();
            if recent.is_empty() {
                state_lines.push(Line::from("  (none)"));
            } else {
                for h in recent {
                    let short: String = h.chars().take(12).collect();
                    state_lines.push(Line::from(format!("  {short}")));
                }
            }
        }
    }
    f.render_widget(
        Paragraph::new(state_lines).block(
            Block::default()
                .borders(Borders::ALL)
                .title("State summary"),
        ),
        state_area,
    );

    if let Some(buf) = &d.confirm_clear {
        let modal = super::centered(area, 60, 5);
        f.render_widget(Clear, modal);
        f.render_widget(
            Paragraph::new(format!("Type 'clear' + Enter to wipe local state: {buf}_")).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Confirm clear"),
            ),
            modal,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::tui::app::Tab;
    use crate::tui::render::{buffer_text, draw as draw_root};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    #[test]
    fn fail_check_and_env_name_render() {
        let mut app = App::new(Config::default(), None);
        app.tab = Tab::Doctor;
        app.doctor.checks = Some(vec![(
            "linear: token".into(),
            false,
            "expects env LINEAR_API_KEY".into(),
        )]);
        let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
        terminal.draw(|f| draw_root(f, &app)).unwrap();
        let text = buffer_text(&terminal);
        assert!(text.contains("FAIL"));
        assert!(text.contains("LINEAR_API_KEY"));
    }

    #[test]
    fn confirm_clear_modal_renders_buffer() {
        let mut app = App::new(Config::default(), None);
        app.tab = Tab::Doctor;
        app.doctor.confirm_clear = Some("cle".into());
        let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
        terminal.draw(|f| draw_root(f, &app)).unwrap();
        let text = buffer_text(&terminal);
        assert!(text.contains("Confirm clear"));
        assert!(text.contains("cle_"));
    }
}
