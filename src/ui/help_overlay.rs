use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

pub fn render(frame: &mut ratatui::Frame, area: Rect) {
    let help_text = vec![
        Line::from(""),
        Line::from("  githappens — Keybindings")
            .style(Style::default().add_modifier(Modifier::BOLD)),
        Line::from(""),
        Line::from("  j / ↓      Move selection down"),
        Line::from("  k / ↑      Move selection up"),
        Line::from("  g          Go to top"),
        Line::from("  G          Go to bottom"),
        Line::from("  Enter      Open PR in browser"),
        Line::from("  r          Refresh"),
        Line::from("  R          Force re-fetch"),
        Line::from("  ?          Toggle this help"),
        Line::from("  q / Esc    Quit"),
        Line::from("  Ctrl+C     Force quit"),
        Line::from(""),
        Line::from("  Legend:"),
        Line::from("  ● green   Ready to merge"),
        Line::from("  ● yellow  Waiting (checks/approval pending)"),
        Line::from("  ● red     Failed (checks or conflicts)"),
        Line::from(""),
        Line::from("  Approval:"),
        Line::from("  ● green   Approved"),
        Line::from("  ● red     Changes requested"),
        Line::from("  ● yellow  Pending review"),
        Line::from("  ○ gray    No reviews"),
        Line::from(""),
        Line::from("  Press ? to close"),
    ];

    let block = Block::default().borders(Borders::ALL).title(" Help ");

    let area = centered(area, 50, 70);
    frame.render_widget(Clear, area);
    let paragraph = Paragraph::new(help_text).block(block);
    frame.render_widget(paragraph, area);
}

fn centered(area: Rect, width_pct: u16, height_pct: u16) -> Rect {
    use ratatui::layout::{Constraint, Layout};
    let popup = Layout::vertical([
        Constraint::Percentage((100 - height_pct) / 2),
        Constraint::Percentage(height_pct),
        Constraint::Percentage((100 - height_pct) / 2),
    ])
    .split(area);

    Layout::horizontal([
        Constraint::Percentage((100 - width_pct) / 2),
        Constraint::Percentage(width_pct),
        Constraint::Percentage((100 - width_pct) / 2),
    ])
    .split(popup[1])[1]
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    #[test]
    fn render_help_overlay() {
        let backend = TestBackend::new(80, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, f.area())).unwrap();
        let buffer = terminal.backend().buffer();
        let text = buffer
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect::<String>();
        assert!(text.contains("Keybindings"));
        assert!(text.contains("Press ? to close"));
    }

    #[test]
    fn centered_returns_valid_rect() {
        let area = Rect::new(0, 0, 100, 100);
        let result = centered(area, 50, 70);
        assert!(result.width > 0);
        assert!(result.height > 0);
    }
}
