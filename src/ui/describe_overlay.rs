use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Row, Table, TableState};

use crate::analysis::approval::collapse_reviews;
use crate::analysis::mergeability::assess;
use crate::analysis::workflows::count_checks;
use crate::app::{App, DescribeFocus, DescribeSubView};
use crate::github::pr::{CheckStatus, PullRequestSnapshot};
use crate::ui::components;
use crate::ui::theme;

pub const MAX_DESC_LINES: usize = 3;
const MAX_VISIBLE_ROWS: u16 = 6;

pub fn render(frame: &mut ratatui::Frame, area: Rect, app: &App) {
    let Some(pr) = app.selected_pr() else {
        return;
    };

    let popup = centered(area, 80, 85);
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::COLOR_NONE))
        .title(Span::styled(
            format!(" {} #{} — {} ", pr.repo, pr.number, pr.title),
            Style::default().fg(theme::COLOR_NONE),
        ));
    frame.render_widget(block, popup);

    let inner = Rect {
        x: popup.x + 1,
        y: popup.y + 1,
        width: popup.width.saturating_sub(2),
        height: popup.height.saturating_sub(2),
    };

    let available = inner.height;

    let details_h: u16 = 2;
    let desc_h: u16 = (MAX_DESC_LINES as u16 + 2)
        .max(3)
        .min(available.saturating_sub(details_h));
    let remaining = available.saturating_sub(details_h + desc_h);
    let table_min = 3u16;
    let table_h: u16 = (MAX_VISIBLE_ROWS + 2).max(table_min).min(remaining / 2);

    let chunks = Layout::vertical([
        Constraint::Length(details_h),
        Constraint::Length(desc_h),
        Constraint::Length(table_h),
        Constraint::Length(table_h),
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

    frame.render_widget(Paragraph::new(vec![line]), area);
}

fn render_description_container(
    frame: &mut ratatui::Frame,
    area: Rect,
    pr: &PullRequestSnapshot,
    focused: bool,
    app: &App,
) {
    let focus_style = if focused {
        Style::default().fg(theme::COLOR_HEADER)
    } else {
        Style::default()
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(focus_style)
        .title(Span::styled(" Description ", focus_style));
    frame.render_widget(block, area);

    let inner = Rect {
        x: area.x + 1,
        y: area.y + 1,
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    };

    let cleaned = if pr.body.is_empty() {
        String::new()
    } else {
        components::strip_markdown(&pr.body)
    };

    let lines: Vec<Line> = if cleaned.is_empty() {
        vec![Line::from(Span::styled(
            "No description provided.",
            Style::default().fg(theme::COLOR_NONE),
        ))]
    } else {
        let wrapped = components::wrap_text(&cleaned, inner.width as usize);
        let total = wrapped.len();
        let scroll = if focused { app.describe_scroll } else { 0 };

        if total > MAX_DESC_LINES && scroll == 0 {
            let mut visible: Vec<Line> = wrapped
                .iter()
                .take(MAX_DESC_LINES - 1)
                .map(|s| Line::from(s.to_string()))
                .collect();
            let last = &wrapped[MAX_DESC_LINES - 1];
            let mut last_text = last.clone();
            if last_text.chars().count() > inner.width as usize - 1 {
                last_text = last_text.chars().take(inner.width as usize - 2).collect();
            }
            last_text.push('…');
            visible.push(Line::from(last_text));
            visible
        } else {
            let start = scroll.min(total.saturating_sub(MAX_DESC_LINES));
            wrapped
                .iter()
                .skip(start)
                .map(|s| Line::from(s.to_string()))
                .collect()
        }
    };

    frame.render_widget(Paragraph::new(lines), inner);

    if !pr.body.is_empty() {
        let line_count = cleaned.lines().count();
        render_total_count(frame, area, line_count);
    }
}

fn render_checks_container(
    frame: &mut ratatui::Frame,
    area: Rect,
    pr: &PullRequestSnapshot,
    focused: bool,
    app: &App,
) {
    let focus_style = if focused {
        Style::default().fg(theme::COLOR_HEADER)
    } else {
        Style::default()
    };

    let total = pr.checks.len();
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(focus_style)
        .title(Span::styled(" Checks ", focus_style));
    frame.render_widget(block, area);

    let inner = Rect {
        x: area.x + 1,
        y: area.y + 1,
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    };

    if pr.checks.is_empty() {
        frame.render_widget(
            Paragraph::new("No checks found.").style(Style::default().fg(theme::COLOR_NONE)),
            inner,
        );
        return;
    }

    let indices = components::group_checks_by_status(&pr.checks);

    let header = Row::new([
        Line::from(""),
        Line::from(Span::styled(
            "Name",
            Style::default().fg(theme::COLOR_HEADER),
        )),
        Line::from(Span::styled(
            "Duration",
            Style::default().fg(theme::COLOR_HEADER),
        )),
        Line::from(Span::styled(
            "Age",
            Style::default().fg(theme::COLOR_HEADER),
        )),
    ]);

    let selected = if focused { app.describe_scroll } else { 0 };

    let rows: Vec<Row> = indices
        .iter()
        .enumerate()
        .map(|(row_idx, &i)| {
            let check = &pr.checks[i];
            let (icon, color) = check_icon_and_color(&check.status);
            let duration = match (&check.started_at, &check.completed_at) {
                (Some(start), Some(end)) => components::format_duration(start, Some(end)),
                _ => "–".to_string(),
            };
            let age = check
                .started_at
                .as_deref()
                .map(components::format_age)
                .unwrap_or_else(|| "–".to_string());

            let row = Row::new([
                Line::from(format!(" {icon}")).style(Style::default().fg(color)),
                Line::from(check.name.clone()),
                Line::from(duration),
                Line::from(age),
            ]);

            if focused && row_idx == selected {
                row.style(
                    Style::default()
                        .bg(theme::COLOR_SELECTED_BG)
                        .fg(theme::COLOR_SELECTED),
                )
            } else {
                row
            }
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(12),
            Constraint::Length(8),
        ],
    )
    .header(header);

    let mut state = TableState::default();
    if focused {
        state.select(Some(selected));
    }

    frame.render_stateful_widget(table, inner, &mut state);

    render_total_count(frame, area, total);
}

fn render_activity_container(
    frame: &mut ratatui::Frame,
    area: Rect,
    pr: &PullRequestSnapshot,
    focused: bool,
    app: &App,
) {
    let focus_style = if focused {
        Style::default().fg(theme::COLOR_HEADER)
    } else {
        Style::default()
    };

    let entries = components::build_activity_entries(&pr.reviews, &pr.comments);
    let total = entries.len();

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(focus_style)
        .title(Span::styled(" Activity ", focus_style));
    frame.render_widget(block, area);

    let inner = Rect {
        x: area.x + 1,
        y: area.y + 1,
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    };

    if entries.is_empty() {
        frame.render_widget(
            Paragraph::new("No activity.").style(Style::default().fg(theme::COLOR_NONE)),
            inner,
        );
        return;
    }

    let header = Row::new([
        Line::from(Span::styled(
            "Type",
            Style::default().fg(theme::COLOR_HEADER),
        )),
        Line::from(Span::styled(
            "Author",
            Style::default().fg(theme::COLOR_HEADER),
        )),
        Line::from(Span::styled(
            "Label",
            Style::default().fg(theme::COLOR_HEADER),
        )),
        Line::from(Span::styled(
            "Age",
            Style::default().fg(theme::COLOR_HEADER),
        )),
    ]);

    let selected = if focused { app.describe_scroll } else { 0 };

    let rows: Vec<Row> = entries
        .iter()
        .enumerate()
        .map(|(row_idx, e)| {
            let label_color = match e.label {
                components::AuthorLabel::Ai => theme::COLOR_WAITING,
                components::AuthorLabel::Bot | components::AuthorLabel::None => theme::COLOR_NONE,
            };

            let row = Row::new([
                Line::from(e.kind.display()),
                Line::from(Span::styled(
                    e.author.clone(),
                    Style::default().add_modifier(Modifier::BOLD),
                )),
                Line::from(Span::styled(
                    e.label.display(),
                    Style::default().fg(label_color),
                )),
                Line::from(components::format_age(&e.created_at)),
            ]);

            if focused && row_idx == selected {
                row.style(
                    Style::default()
                        .bg(theme::COLOR_SELECTED_BG)
                        .fg(theme::COLOR_SELECTED),
                )
            } else {
                row
            }
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(14),
            Constraint::Min(1),
            Constraint::Length(6),
            Constraint::Length(8),
        ],
    )
    .header(header);

    let mut state = TableState::default();
    if focused {
        state.select(Some(selected));
    }

    frame.render_stateful_widget(table, inner, &mut state);

    render_total_count(frame, area, total);
}

fn render_total_count(frame: &mut ratatui::Frame, area: Rect, total: usize) {
    let total_str = format!(" {} ", total);
    let total_len = total_str.len() as u16;
    let total_y = area.y + area.height - 1;
    let total_x = area.x + area.width.saturating_sub(total_len + 1);
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
                let content = if check.annotations.is_empty() {
                    "No logs available.".to_string()
                } else {
                    check.annotations.join("\n\n")
                };
                (check.name.clone(), content)
            } else {
                ("Check".to_string(), "Check not found.".to_string())
            }
        }
        DescribeSubView::ActivityDetail { index } => {
            let entries = components::build_activity_entries(&pr.reviews, &pr.comments);
            if let Some(entry) = entries.get(*index) {
                let title = format!("{} · {}", entry.kind.display(), entry.author);
                let content = if entry.body.is_empty() {
                    "No content available.".to_string()
                } else {
                    components::strip_markdown(&entry.body)
                };
                (title, content)
            } else {
                ("Activity".to_string(), "Entry not found.".to_string())
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

    let is_placeholder = content == "No logs available."
        || content == "No description provided."
        || content == "No content available."
        || content == "Check not found."
        || content == "Entry not found.";

    let wrapped = components::wrap_text(&content, inner.width as usize);
    let lines: Vec<Line> = if is_placeholder {
        wrapped
            .into_iter()
            .map(|l| Line::from(Span::styled(l, Style::default().fg(theme::COLOR_NONE))))
            .collect()
    } else {
        wrapped.into_iter().map(Line::from).collect()
    };

    let scroll = app.describe_subview_scroll;
    let max_scroll = lines.len().saturating_sub(inner.height as usize);
    let scroll = scroll.min(max_scroll);

    frame.render_widget(Paragraph::new(lines).scroll((scroll as u16, 0)), inner);
}

fn check_icon_and_color(status: &CheckStatus) -> (&'static str, ratatui::style::Color) {
    match status {
        CheckStatus::Failed => ("●", theme::COLOR_FAILED),
        CheckStatus::Skipped => ("○", theme::COLOR_NONE),
        CheckStatus::Running => ("●", theme::COLOR_WAITING),
        CheckStatus::Success => ("●", theme::COLOR_READY),
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
