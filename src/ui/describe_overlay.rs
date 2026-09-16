use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::analysis::approval::collapse_reviews;
use crate::analysis::mergeability::assess;
use crate::analysis::workflows::count_checks;
use crate::app::{App, DescribeFocus, DescribeSubView};
use crate::github::pr::{CheckStatus, PullRequestSnapshot};
use crate::ui::components;
use crate::ui::theme;

const MAX_DESC_LINES: usize = 2;
const MAX_VISIBLE_ROWS: usize = 5;

pub fn render(frame: &mut ratatui::Frame, area: Rect, app: &App) {
    let Some(pr) = app.selected_pr() else {
        return;
    };

    let popup = centered(area, 80, 85);
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" {} #{} — {} ", pr.repo, pr.number, pr.title));
    frame.render_widget(block, popup);

    let inner = Rect {
        x: popup.x + 1,
        y: popup.y + 1,
        width: popup.width.saturating_sub(2),
        height: popup.height.saturating_sub(2),
    };

    let chunks = Layout::vertical([
        Constraint::Length(4),
        Constraint::Length(MAX_DESC_LINES as u16 + 1),
        Constraint::Length(MAX_VISIBLE_ROWS as u16 + 2),
        Constraint::Length(MAX_VISIBLE_ROWS as u16 + 2),
        Constraint::Min(1),
    ])
    .split(inner);

    render_details(frame, chunks[0], pr);

    let desc_focused = app.describe_focus == DescribeFocus::Description;
    render_description_container(frame, chunks[1], pr, desc_focused, app);

    let checks_focused = app.describe_focus == DescribeFocus::Checks;
    render_checks_container(frame, chunks[2], pr, checks_focused, app);

    let activity_focused = app.describe_focus == DescribeFocus::Activity;
    render_activity_container(frame, chunks[3], pr, activity_focused, app);

    if let Some(ref sub) = app.describe_subview {
        render_subview(frame, area, sub, pr, app);
    }
}

fn render_details(frame: &mut ratatui::Frame, area: Rect, pr: &PullRequestSnapshot) {
    let readiness = assess(pr);
    let (_, color) = theme::merge_glyph_and_color(&readiness);
    let counts = count_checks(&pr.checks);
    let approval = collapse_reviews(&pr.reviews);

    let line = Line::from({
        let mut spans = vec![
            Span::styled("Status: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::styled(format!("{:?}", readiness), Style::default().fg(color)),
            Span::raw("  ·  Diff: "),
        ];
        spans.extend(theme::diff_spans(pr.additions, pr.deletions));
        spans.push(Span::raw("  ·  Checks: "));
        spans.extend(theme::checks_spans(&counts));
        spans.push(Span::raw(format!("  ·  Approval: {:?}", approval)));
        spans
    });

    frame.render_widget(Paragraph::new(vec![line, Line::from("")]), area);
}

fn render_description_container(
    frame: &mut ratatui::Frame,
    area: Rect,
    pr: &PullRequestSnapshot,
    focused: bool,
    app: &App,
) {
    let cleaned = if pr.body.is_empty() {
        String::new()
    } else {
        components::strip_markdown(&pr.body)
    };

    let display = if cleaned.is_empty() {
        "No description provided.".to_string()
    } else {
        let wrapped = components::wrap_text(&cleaned, area.width.saturating_sub(2) as usize);
        components::truncate_lines(&wrapped.join("\n"), MAX_DESC_LINES)
    };

    let style = if focused {
        Style::default().fg(theme::COLOR_HEADER)
    } else {
        Style::default()
    };

    let block = Block::default().title(Span::styled("Description", style));
    frame.render_widget(block, area);

    let inner = Rect {
        x: area.x + 1,
        y: area.y,
        width: area.width.saturating_sub(1),
        height: area.height,
    };

    let lines: Vec<Line> = if display.is_empty() {
        vec![Line::from(Span::styled(
            "  No description provided.",
            Style::default().fg(theme::COLOR_NONE),
        ))]
    } else {
        display
            .lines()
            .map(|l| Line::from(format!(" {l}")))
            .collect()
    };

    let scroll = if focused { app.describe_scroll } else { 0 };
    let paragraph = Paragraph::new(lines).scroll((scroll as u16, 0));
    frame.render_widget(paragraph, inner);
}

fn render_checks_container(
    frame: &mut ratatui::Frame,
    area: Rect,
    pr: &PullRequestSnapshot,
    focused: bool,
    app: &App,
) {
    let style = if focused {
        Style::default().fg(theme::COLOR_HEADER)
    } else {
        Style::default()
    };

    let total = pr.checks.len();
    let block = Block::default()
        .borders(Borders::ALL)
        .title(Span::styled("Checks", style));
    frame.render_widget(block, area);

    let inner = Rect {
        x: area.x + 1,
        y: area.y + 1,
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    };

    if pr.checks.is_empty() {
        frame.render_widget(
            Paragraph::new(" No checks found.").style(Style::default().fg(theme::COLOR_NONE)),
            inner,
        );
        return;
    }

    let indices = components::group_checks_by_status(&pr.checks);

    let mut lines: Vec<Line> = vec![Line::from(vec![
        Span::styled("Status  ", Style::default().fg(theme::COLOR_HEADER)),
        Span::styled("Name", Style::default().fg(theme::COLOR_HEADER)),
    ])];

    for &i in &indices {
        let check = &pr.checks[i];
        let (icon, color) = check_icon_and_color(&check.status);
        let duration = match (&check.started_at, &check.completed_at) {
            (Some(start), Some(end)) => components::format_duration(start, Some(end)),
            (Some(_), None) => "–".to_string(),
            _ => "–".to_string(),
        };
        let age = check
            .started_at
            .as_deref()
            .map(components::format_age)
            .unwrap_or_else(|| "–".to_string());
        lines.push(Line::from(vec![
            Span::styled(format!(" {icon}"), Style::default().fg(color)),
            Span::raw("  "),
            Span::raw(check.name.clone()),
            Span::raw(format!("  {}  {}", duration, age)),
        ]));
    }

    let scroll = if focused { app.describe_scroll } else { 0 };
    frame.render_widget(Paragraph::new(lines).scroll((scroll as u16, 0)), inner);

    let total_y = area.y + area.height - 1;
    let total_str = format!(" {} ", total);
    let total_len = total_str.len() as u16;
    let total_x = area.x + area.width.saturating_sub(total_len);
    frame.render_widget(
        Paragraph::new(Span::styled(
            total_str,
            Style::default().fg(theme::COLOR_NONE),
        )),
        Rect {
            x: total_x,
            y: total_y,
            width: total_len,
            height: 1,
        },
    );
}

fn render_activity_container(
    frame: &mut ratatui::Frame,
    area: Rect,
    pr: &PullRequestSnapshot,
    focused: bool,
    app: &App,
) {
    let style = if focused {
        Style::default().fg(theme::COLOR_HEADER)
    } else {
        Style::default()
    };

    let entries = components::build_activity_entries(&pr.reviews, &pr.comments);
    let total = entries.len();

    let block = Block::default()
        .borders(Borders::ALL)
        .title(Span::styled("Activity", style));
    frame.render_widget(block, area);

    let inner = Rect {
        x: area.x + 1,
        y: area.y + 1,
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    };

    if entries.is_empty() {
        frame.render_widget(
            Paragraph::new(" No activity.").style(Style::default().fg(theme::COLOR_NONE)),
            inner,
        );
        return;
    }

    let mut lines: Vec<Line> = vec![Line::from(vec![
        Span::styled("Type         ", Style::default().fg(theme::COLOR_HEADER)),
        Span::styled(
            "Author              ",
            Style::default().fg(theme::COLOR_HEADER),
        ),
        Span::styled("Label  ", Style::default().fg(theme::COLOR_HEADER)),
        Span::styled("Age", Style::default().fg(theme::COLOR_HEADER)),
    ])];

    for e in &entries {
        let label_color = match e.label {
            components::AuthorLabel::Ai => theme::COLOR_WAITING,
            components::AuthorLabel::Bot => theme::COLOR_NONE,
            components::AuthorLabel::None => theme::COLOR_NONE,
        };
        let kind_str = format!("{:<14}", e.kind.display());
        let author_str = format!("{:<20}", e.author);
        let label_str = format!("{:<6}", e.label.display());
        let age = components::format_age(&e.created_at);

        lines.push(Line::from(vec![
            Span::styled(kind_str, Style::default()),
            Span::styled(author_str, Style::default().add_modifier(Modifier::BOLD)),
            Span::styled(label_str, Style::default().fg(label_color)),
            Span::raw(age),
        ]));
    }

    let scroll = if focused { app.describe_scroll } else { 0 };
    frame.render_widget(Paragraph::new(lines).scroll((scroll as u16, 0)), inner);

    let total_y = area.y + area.height - 1;
    let total_str = format!(" {} ", total);
    let total_len = total_str.len() as u16;
    let total_x = area.x + area.width.saturating_sub(total_len);
    frame.render_widget(
        Paragraph::new(Span::styled(
            total_str,
            Style::default().fg(theme::COLOR_NONE),
        )),
        Rect {
            x: total_x,
            y: total_y,
            width: total_len,
            height: 1,
        },
    );
}

fn render_subview(
    frame: &mut ratatui::Frame,
    area: Rect,
    sub: &DescribeSubView,
    pr: &PullRequestSnapshot,
    app: &App,
) {
    let popup = centered(area, 70, 60);
    frame.render_widget(Clear, popup);

    let (title, content) = match sub {
        DescribeSubView::Description => {
            let content = if pr.body.is_empty() {
                "No description provided.".to_string()
            } else {
                components::strip_markdown(&pr.body)
            };
            ("Description".to_string(), content)
        }
        DescribeSubView::CheckDetail { index } => {
            if let Some(check) = pr.checks.get(*index) {
                let content = check
                    .output_text
                    .as_deref()
                    .or(check.output_summary.as_deref())
                    .unwrap_or("No output available.");
                (check.name.clone(), content.to_string())
            } else {
                ("Check".to_string(), "Check not found.".to_string())
            }
        }
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" {} ", title));
    frame.render_widget(block, popup);

    let inner = Rect {
        x: popup.x + 1,
        y: popup.y + 1,
        width: popup.width.saturating_sub(2),
        height: popup.height.saturating_sub(2),
    };

    let wrapped = components::wrap_text(&content, inner.width as usize);
    let lines: Vec<Line> = wrapped.into_iter().map(Line::from).collect();

    let scroll = app.describe_subview_scroll;
    let max_scroll = lines.len().saturating_sub(inner.height as usize);
    let scroll = scroll.min(max_scroll);

    frame.render_widget(Paragraph::new(lines).scroll((scroll as u16, 0)), inner);
}

fn check_icon_and_color(status: &CheckStatus) -> (&'static str, ratatui::style::Color) {
    match status {
        CheckStatus::Failed => ("✗", theme::COLOR_FAILED),
        CheckStatus::Skipped => ("⊝", theme::COLOR_NONE),
        CheckStatus::Running => ("⏳", theme::COLOR_WAITING),
        CheckStatus::Success => ("✓", theme::COLOR_READY),
    }
}

fn centered(area: Rect, width_pct: u16, height_pct: u16) -> Rect {
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
