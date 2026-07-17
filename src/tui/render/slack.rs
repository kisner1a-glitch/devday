use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::tui::app::App;

pub fn draw(f: &mut Frame, area: Rect, app: &App) {
    let s = &app.slack;
    let mut lines: Vec<Line> = Vec::new();
    match &s.digest {
        None => lines.push(Line::from(
            "no report yet - generate on the Report tab (1, then r)",
        )),
        Some(text) => {
            if s.already_posted {
                lines.push(Line::from("!! identical report already posted (dedup) !!"));
                lines.push(Line::from(""));
            }
            for l in text.lines() {
                lines.push(Line::from(l.to_string()));
            }
        }
    }
    if let Some(result) = &s.result {
        lines.push(Line::from(""));
        lines.push(Line::from(result.as_str()));
    }
    let title = format!(
        "Slack preview ({}) - v verbose, p post",
        if s.verbose { "verbose" } else { "digest" }
    );
    f.render_widget(
        Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title(title)),
        area,
    );

    if s.modal {
        let modal = super::centered(area, 50, 6);
        f.render_widget(Clear, modal);
        let channel = app.cfg.slack.channel.as_deref().unwrap_or("(webhook)");
        f.render_widget(
            Paragraph::new(format!(
                "Post to {channel}?\n\nEnter/y confirm - any other key cancels"
            ))
            .block(Block::default().borders(Borders::ALL).title("Confirm post")),
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
    fn dedup_banner_and_modal_render() {
        let mut app = App::new(Config::default(), None);
        app.tab = Tab::Slack;
        app.slack.digest = Some("*devday status*".into());
        app.slack.already_posted = true;
        app.slack.modal = true;
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal.draw(|f| draw_root(f, &app)).unwrap();
        let text = buffer_text(&terminal);
        assert!(text.contains("already posted"));
        assert!(text.contains("Confirm post"));
    }
}
