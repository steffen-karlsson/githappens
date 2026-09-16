use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph};

use crate::analysis::approval::collapse_reviews;
use crate::analysis::mergeability::assess;
use crate::analysis::workflows::count_checks;
use crate::app::App;
use crate::github::pr::PullRequestSnapshot;
use crate::ui::theme;

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

    let desc_line_count = if pr.body.is_empty() {
        1
    } else {
        strip_markdown(&pr.body).lines().count().max(1)
    };
    let details_height = 3u16 + desc_line_count as u16 + 1; // status + blank + header + desc + blank

    let reviews_height = if pr.reviews.is_empty() {
        1
    } else {
        let mut h = 1u16; // "Reviews:" header
        for review in &pr.reviews {
            h += 1; // state + author line
            if !review.body.is_empty() {
                let line_count = strip_markdown(&review.body).lines().count().min(3) as u16;
                h += line_count;
            }
        }
        h + 1 // trailing blank
    };

    let chunks = Layout::vertical([
        Constraint::Length(details_height),
        Constraint::Length(reviews_height),
        Constraint::Length(pr.checks.len() as u16 + 3),
        Constraint::Min(1),
    ])
    .split(inner);

    render_details(frame, chunks[0], pr);
    render_approval(frame, chunks[1], pr);
    render_checks_table(frame, chunks[2], pr);
    render_comments(frame, chunks[3], pr, app.describe_scroll);
}

fn render_details(frame: &mut ratatui::Frame, area: Rect, pr: &PullRequestSnapshot) {
    let readiness = assess(pr);
    let (_, color) = theme::merge_glyph_and_color(&readiness);
    let counts = count_checks(&pr.checks);
    let approval = collapse_reviews(&pr.reviews);

    let lines = vec![
        Line::from({
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
        }),
        Line::from(""),
        Line::from(Span::styled(
            "Description",
            Style::default().add_modifier(Modifier::BOLD),
        )),
    ];

    let desc_lines: Vec<Line> = if pr.body.is_empty() {
        vec![Line::from(Span::styled(
            "  No description provided.",
            Style::default().fg(theme::COLOR_NONE),
        ))]
    } else {
        let cleaned = strip_markdown(&pr.body);
        if cleaned.is_empty() {
            vec![Line::from(Span::styled(
                "  No description provided.",
                Style::default().fg(theme::COLOR_NONE),
            ))]
        } else {
            cleaned
                .lines()
                .map(|l| Line::from(format!("  {l}")))
                .collect()
        }
    };

    let all_lines: Vec<Line> = lines
        .into_iter()
        .chain(desc_lines)
        .chain(std::iter::once(Line::from("")))
        .collect();
    frame.render_widget(Paragraph::new(all_lines), area);
}

fn render_approval(frame: &mut ratatui::Frame, area: Rect, pr: &PullRequestSnapshot) {
    if pr.reviews.is_empty() {
        frame.render_widget(
            Paragraph::new(" No reviews.").style(Style::default().fg(theme::COLOR_NONE)),
            area,
        );
        return;
    }

    let mut lines = vec![Line::from(Span::styled(
        "Reviews:",
        Style::default().add_modifier(Modifier::BOLD),
    ))];

    for review in &pr.reviews {
        let (state_icon, state_color) = match review.state {
            crate::github::models::ReviewState::Approved => ("✓", theme::COLOR_READY),
            crate::github::models::ReviewState::ChangesRequested => ("✗", theme::COLOR_FAILED),
            _ => ("•", theme::COLOR_NONE),
        };
        lines.push(Line::from(vec![
            Span::styled(format!("  {state_icon} "), Style::default().fg(state_color)),
            Span::styled(
                review.author.clone(),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(" — {:?}", review.state),
                Style::default().fg(state_color),
            ),
        ]));
        if !review.body.is_empty() {
            let cleaned = strip_markdown(&review.body);
            for body_line in cleaned.lines().take(3) {
                lines.push(Line::from(Span::styled(
                    format!("    {}", body_line),
                    Style::default().fg(theme::COLOR_NONE),
                )));
            }
        }
    }

    frame.render_widget(Paragraph::new(lines), area);
}

fn render_checks_table(frame: &mut ratatui::Frame, area: Rect, pr: &PullRequestSnapshot) {
    if pr.checks.is_empty() {
        frame.render_widget(
            Paragraph::new(" No checks found.").style(Style::default().fg(theme::COLOR_NONE)),
            area,
        );
        return;
    }

    let mut lines = vec![Line::from(Span::styled(
        "Checks:",
        Style::default().add_modifier(Modifier::BOLD),
    ))];

    for check in &pr.checks {
        let (icon, color) = if check.failed {
            ("✗", theme::COLOR_FAILED)
        } else if check.skipped {
            ("⊝", theme::COLOR_NONE)
        } else if check.running {
            ("⏳", theme::COLOR_WAITING)
        } else if check.completed {
            ("✓", theme::COLOR_READY)
        } else {
            ("?", theme::COLOR_NONE)
        };

        lines.push(Line::from(vec![
            Span::styled(format!(" {icon} "), Style::default().fg(color)),
            Span::raw(check.name.clone()),
        ]));
    }

    frame.render_widget(
        List::new(lines.into_iter().map(ListItem::new).collect::<Vec<_>>()),
        area,
    );
}

fn render_comments(
    frame: &mut ratatui::Frame,
    area: Rect,
    pr: &PullRequestSnapshot,
    scroll: usize,
) {
    let block = Block::default().borders(Borders::TOP).title(" Comments ");

    if pr.comments.is_empty() {
        frame.render_widget(
            Paragraph::new(" No comments.")
                .block(block)
                .style(Style::default().fg(theme::COLOR_NONE)),
            area,
        );
        return;
    }

    let mut items: Vec<Line> = Vec::new();

    for comment in &pr.comments {
        items.push(Line::from(vec![
            Span::styled(
                comment.author.clone(),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("  {}", comment.created_at),
                Style::default().fg(theme::COLOR_NONE),
            ),
        ]));
        for line in comment.body.lines() {
            items.push(Line::from(format!("  {line}")));
        }
        items.push(Line::from(""));
    }

    let total_lines = items.len();
    let visible_lines = area.height as usize;
    let max_scroll = total_lines.saturating_sub(visible_lines);
    let scroll = scroll.min(max_scroll);

    let paragraph = Paragraph::new(items)
        .block(block)
        .scroll((scroll as u16, 0));

    frame.render_widget(paragraph, area);
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

fn strip_markdown(text: &str) -> String {
    let mut result = String::new();
    let mut in_html_tag = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("<!--") || trimmed.ends_with("-->") {
            continue;
        }
        for ch in trimmed.chars() {
            if ch == '<' {
                in_html_tag = true;
            }
            if !in_html_tag {
                result.push(ch);
            }
            if ch == '>' {
                in_html_tag = false;
            }
        }
        result.push('\n');
    }
    let result = result.replace("###", "").replace("**", "").replace("#", "");
    result.trim().to_string()
}
