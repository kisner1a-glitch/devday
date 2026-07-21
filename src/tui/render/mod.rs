pub mod config;
pub mod doctor;
pub mod report;
pub mod slack;

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
        Tab::Report => report::draw(f, body, app),
        Tab::Slack => slack::draw(f, body, app),
        Tab::Config => config::draw(f, body, app),
        Tab::Doctor => doctor::draw(f, body, app),
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

    if app.show_quit_confirm {
        let area = centered(f.area(), 54, 6);
        f.render_widget(Clear, area);
        f.render_widget(
            Paragraph::new(
                "Unsaved config changes will be lost.\n\nq/Enter/y quit anyway - any other key cancels",
            )
            .block(Block::default().borders(Borders::ALL).title("Quit without saving?")),
            area,
        );
    }
}

const HELP_TEXT: &str = "1-4 switch tab   q quit   ? help\n\
Report: r regen  s window  a AI  Tab pane  Enter open  w write\n\
Slack:  v verbose  p post (confirm)\n\
Config: arrows move  Enter edit/toggle  S save\n\
Doctor: r re-run  x clear state";

pub(crate) fn centered(area: Rect, w: u16, h: u16) -> Rect {
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

    #[test]
    fn quit_confirm_modal_renders() {
        let mut app = App::new(Config::default(), None);
        app.show_quit_confirm = true;
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal.draw(|f| draw(f, &app)).unwrap();
        let text = buffer_text(&terminal);
        assert!(text.contains("Quit without saving"));
        assert!(text.contains("Unsaved config changes"));
    }

    #[test]
    fn all_tabs_render_at_multiple_sizes() {
        use crate::tui::app::Tab;
        for (w, h) in [(80u16, 24u16), (120, 40), (40, 12)] {
            for tab in Tab::ALL {
                let mut app = App::new(Config::default(), None);
                app.tab = tab;
                let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
                terminal.draw(|f| draw(f, &app)).unwrap(); // must not panic
            }
        }
    }
}
