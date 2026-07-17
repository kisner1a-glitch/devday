use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::tui::app::{App, FIELDS};

pub fn draw(f: &mut Frame, area: Rect, app: &App) {
    let ct = &app.config_tab;

    let (list_area, input_area) = if ct.editing.is_some() {
        let [list, input] =
            Layout::vertical([Constraint::Min(0), Constraint::Length(3)]).areas(area);
        (list, Some(input))
    } else {
        (area, None)
    };

    let mut lines: Vec<Line> = Vec::new();
    for (i, field) in FIELDS.iter().enumerate() {
        let text = app.field_label(*field);
        let style = if i == ct.selected {
            Style::default().add_modifier(Modifier::REVERSED)
        } else {
            Style::default()
        };
        lines.push(Line::styled(text, style));
    }
    if let Some(err) = &ct.error {
        lines.push(Line::from(""));
        lines.push(Line::from(format!("error: {err}")));
    }

    let title = if ct.dirty {
        "Config [modified] - arrows move, Enter edit/toggle, S save"
    } else {
        "Config - arrows move, Enter edit/toggle, S save"
    };
    f.render_widget(
        Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title(title)),
        list_area,
    );

    if let (Some(input), Some(buf)) = (input_area, &ct.editing) {
        f.render_widget(
            Paragraph::new(format!("{buf}_")).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Edit (Enter confirm, Esc cancel)"),
            ),
            input,
        );
    }

    if ct.confirm_save {
        let modal = super::centered(area, 50, 6);
        f.render_widget(Clear, modal);
        f.render_widget(
            Paragraph::new("Save config?\n\nEnter/y confirm - any other key cancels")
                .block(Block::default().borders(Borders::ALL).title("Confirm save")),
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
    fn field_list_shows_webhook_env_name_not_a_value() {
        let mut cfg = Config::default();
        cfg.slack.webhook_env = Some("MY_HOOK_ENV".into());
        let mut app = App::new(cfg, None);
        app.tab = Tab::Config;
        let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
        terminal.draw(|f| draw_root(f, &app)).unwrap();
        let text = buffer_text(&terminal);
        assert!(text.contains("webhook_env"));
        assert!(text.contains("MY_HOOK_ENV"));
    }

    #[test]
    fn dirty_marker_and_confirm_modal_render() {
        let mut app = App::new(Config::default(), None);
        app.tab = Tab::Config;
        app.config_tab.dirty = true;
        app.config_tab.confirm_save = true;
        let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
        terminal.draw(|f| draw_root(f, &app)).unwrap();
        let text = buffer_text(&terminal);
        assert!(text.contains("[modified]"));
        assert!(text.contains("Confirm save"));
    }

    #[test]
    fn editing_shows_input_box_with_cursor() {
        let mut app = App::new(Config::default(), None);
        app.tab = Tab::Config;
        app.config_tab.editing = Some("#eng".into());
        let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
        terminal.draw(|f| draw_root(f, &app)).unwrap();
        let text = buffer_text(&terminal);
        assert!(text.contains("#eng_"));
    }
}
