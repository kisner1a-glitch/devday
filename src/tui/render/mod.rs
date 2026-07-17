pub mod report;

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Tabs};
use ratatui::Frame;

use crate::tui::app::{App, Tab};

pub fn draw(f: &mut Frame, app: &App) {
    let [tab_bar, body, status] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(f.area());

    let titles: Vec<Line> = Tab::ALL.iter().map(|t| Line::from(t.title())).collect();
    f.render_widget(
        Tabs::new(titles)
            .select(app.tab.index())
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED)),
        tab_bar,
    );

    match app.tab {
        // Tasks 4-6 replace the remaining placeholders with real tab renderers.
        Tab::Report => report::draw(f, body, app),
        Tab::Slack => placeholder(f, body, "Slack (coming in Task 4)"),
        Tab::Config => placeholder(f, body, "Config (coming in Task 5)"),
        Tab::Doctor => placeholder(f, body, "Doctor (coming in Task 6)"),
    }

    f.render_widget(Paragraph::new(app.status.as_str()), status);

    if app.show_help {
        let area = centered(f.area(), 60, 12);
        f.render_widget(Clear, area);
        f.render_widget(
            Paragraph::new(HELP_TEXT).block(Block::default().borders(Borders::ALL).title("Help")),
            area,
        );
    }
}

const HELP_TEXT: &str = "1-4 switch tab   q quit   ? help\n\
Report: r regen  s window  a AI  Tab pane  Enter open  w write\n\
Slack:  v verbose  p post (confirm)\n\
Config: arrows move  Enter edit/toggle  S save\n\
Doctor: r re-run  x clear state";

fn placeholder(f: &mut Frame, area: Rect, text: &str) {
    f.render_widget(
        Paragraph::new(text).block(Block::default().borders(Borders::ALL)),
        area,
    );
}

fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(area.width);
    let h = h.min(area.height);
    Rect::new(
        area.x + (area.width - w) / 2,
        area.y + (area.height - h) / 2,
        w,
        h,
    )
}

/// Test helper: flatten a TestBackend buffer to a String.
#[cfg(test)]
pub fn buffer_text(terminal: &ratatui::Terminal<ratatui::backend::TestBackend>) -> String {
    let buffer = terminal.backend().buffer();
    let mut out = String::new();
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            out.push_str(buffer[(x, y)].symbol());
        }
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    #[test]
    fn draws_tab_bar_and_status_without_panic() {
        let app = App::new(Config::default(), None);
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal.draw(|f| draw(f, &app)).unwrap();
        let text = buffer_text(&terminal);
        assert!(text.contains("1 Report"));
        assert!(text.contains("4 Doctor"));
    }

    #[test]
    fn help_overlay_renders() {
        let mut app = App::new(Config::default(), None);
        app.show_help = true;
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal.draw(|f| draw(f, &app)).unwrap();
        assert!(buffer_text(&terminal).contains("switch tab"));
    }
}
