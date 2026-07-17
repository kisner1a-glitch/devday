use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::redact::apply as redact_apply;
use crate::tui::app::{App, Pane, SINCE_CHOICES};

pub fn draw(f: &mut Frame, area: Rect, app: &App) {
    let r = &app.report;
    if r.loading {
        let spin = ["|", "/", "-", "\\"][app.spinner % 4];
        f.render_widget(
            Paragraph::new(format!(
                "{spin} generating report ({})...",
                SINCE_CHOICES[r.since_idx]
            ))
            .block(Block::default().borders(Borders::ALL).title("Report")),
            area,
        );
        return;
    }
    if let Some(err) = &r.error {
        f.render_widget(
            Paragraph::new(format!("error: {err}\n\nr to retry"))
                .block(Block::default().borders(Borders::ALL).title("Report")),
            area,
        );
        return;
    }
    let Some(rep) = &r.report else {
        f.render_widget(
            Paragraph::new("press r to generate")
                .block(Block::default().borders(Borders::ALL).title("Report")),
            area,
        );
        return;
    };

    let [left, right] =
        Layout::horizontal([Constraint::Percentage(35), Constraint::Percentage(65)]).areas(area);

    // Left: group tree.
    let mut lines: Vec<Line> = Vec::new();
    for (i, g) in rep.groups.iter().enumerate() {
        let text = format!(
            "{} ({})",
            redact_apply(&g.title, &app.cfg.redact),
            g.items.len()
        );
        let style = if i == r.selected_group {
            Style::default().add_modifier(Modifier::REVERSED)
        } else {
            Style::default()
        };
        lines.push(Line::styled(text, style));
    }
    let left_border = if r.pane == Pane::Groups {
        "Groups*"
    } else {
        "Groups"
    };
    f.render_widget(
        Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title(left_border)),
        left,
    );

    // Right: selected group's items, then the redacted sections text.
    let mut detail: Vec<Line> = Vec::new();
    if let Some(g) = rep.groups.get(r.selected_group) {
        for (i, item) in g.items.iter().enumerate() {
            let text = format!(
                "[{}] {} {}",
                item.activity_type,
                redact_apply(&item.title, &app.cfg.redact),
                redact_apply(item.status.as_deref().unwrap_or(""), &app.cfg.redact)
            );
            let style = if r.pane == Pane::Detail && i == r.selected_item {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };
            detail.push(Line::styled(text, style));
        }
        detail.push(Line::from(""));
    }
    for l in &r.sections {
        detail.push(Line::from(l.as_str()));
    }
    let right_border = if r.pane == Pane::Detail {
        "Detail*"
    } else {
        "Detail"
    };
    f.render_widget(
        Paragraph::new(detail).block(Block::default().borders(Borders::ALL).title(right_border)),
        right,
    );
}

#[cfg(test)]
pub fn tests_fixture() -> crate::model::Report {
    use crate::model::{ActivityItem, Group, Report, Source};
    use chrono::Utc;

    let item = ActivityItem {
        source: Source::Git,
        source_id: "1".into(),
        title: "token ghp_SECRET123 in title".into(),
        url: Some("https://example.com/x".into()),
        project_key: None,
        repo: Some("devday".into()),
        activity_type: "commit".into(),
        status: Some("token xoxb-STATUSSECRET in status".into()),
        actor: None,
        timestamp: Utc::now(),
        summary: None,
        signals: vec![],
    };
    Report {
        window_start: Utc::now(),
        window_end: Utc::now(),
        groups: vec![Group {
            key: "devday".into(),
            title: "devday".into(),
            items: vec![item],
        }],
        summary: None,
        worked_on: vec!["x".into()],
        next_up: vec![],
        blockers: vec![],
        source_links: vec![],
        generation_warnings: vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::tui::render::{buffer_text, draw as draw_root};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    #[test]
    fn report_tab_renders_and_scrubs_secrets() {
        let mut app = App::new(Config::default(), None);
        app.report.report = Some(tests_fixture());
        let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
        terminal.draw(|f| draw_root(f, &app)).unwrap();
        let text = buffer_text(&terminal);
        assert!(text.contains("devday (1)"));
        assert!(text.contains("[REDACTED]"));
        assert!(!text.contains("ghp_SECRET123"));
        // item.status is admin-configurable free text (Linear workflow-state
        // names) and must be scrubbed the same as the title.
        assert!(!text.contains("xoxb-STATUSSECRET"));
    }

    #[test]
    fn report_tab_honors_extra_redact_patterns() {
        use crate::model::{ActivityItem, Group, Report, Source};
        use chrono::Utc;

        let mut cfg = Config::default();
        cfg.redact.extra_patterns = vec!["SECRETCODENAME".into()];
        let mut app = App::new(cfg, None);

        let item = ActivityItem {
            source: Source::Git,
            source_id: "1".into(),
            title: "project SECRETCODENAME launch".into(),
            url: None,
            project_key: None,
            repo: Some("devday".into()),
            activity_type: "commit".into(),
            status: None,
            actor: None,
            timestamp: Utc::now(),
            summary: None,
            signals: vec![],
        };
        app.report.report = Some(Report {
            window_start: Utc::now(),
            window_end: Utc::now(),
            groups: vec![Group {
                key: "devday".into(),
                title: "devday".into(),
                items: vec![item],
            }],
            summary: None,
            worked_on: vec![],
            next_up: vec![],
            blockers: vec![],
            source_links: vec![],
            generation_warnings: vec![],
        });

        let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
        terminal.draw(|f| draw_root(f, &app)).unwrap();
        let text = buffer_text(&terminal);
        assert!(!text.contains("SECRETCODENAME"));
        assert!(text.contains("[REDACTED]"));
    }
}
