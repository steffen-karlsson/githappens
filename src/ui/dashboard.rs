use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Paragraph, Row, Table};

use crate::analysis::approval::{ApprovalState, collapse_reviews};
use crate::analysis::mergeability::{MergeReadiness, assess};
use crate::analysis::workflows::{WorkflowCounts, count_checks, render_counts};
use crate::app::{App, format_duration};
use crate::ui::components;
use crate::ui::theme;

pub fn render(frame: &mut ratatui::Frame, app: &App) {
    let area = frame.area();

    if app.help_visible {
        crate::ui::help_overlay::render(frame, area);
        return;
    }

    let rendered_dashboard = match &app.state {
        crate::app::AppState::Error(msg) => {
            crate::ui::error_screen::render(frame, area, msg);
            false
        }
        crate::app::AppState::RateLimited { retry_after_secs } => {
            let countdown = app
                .rate_limit_countdown()
                .unwrap_or_else(|| format!("{}s", retry_after_secs));
            let msg = format!("Rate limited by GitHub. Retry in {countdown}");
            crate::ui::error_screen::render(frame, area, &msg);
            false
        }
        crate::app::AppState::Loading | crate::app::AppState::Refreshing if app.prs.is_empty() => {
            let spinner = app.spinner_frame();
            let msg = format!(" {spinner}  Fetching your PRs... ");
            let paragraph = Paragraph::new(msg).centered();
            frame.render_widget(paragraph, area);
            false
        }
        _ => {
            render_dashboard(frame, area, app);
            true
        }
    };

    if rendered_dashboard && app.describe_visible {
        crate::ui::describe_overlay::render(frame, area, app);
    }
}

fn render_dashboard(frame: &mut ratatui::Frame, area: Rect, app: &App) {
    let chunks = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .split(area);

    render_header(frame, chunks[0], app);
    render_table(frame, chunks[1], app);
    render_footer(frame, chunks[2], app);
    render_status(frame, chunks[0], app);
}

fn render_header(frame: &mut ratatui::Frame, area: Rect, app: &App) {
    let scope = match &app.org {
        Some(org) => format!(" in {}", org),
        None => String::new(),
    };
    let title = format!(
        " {} — {}'s open PRs{} ",
        theme::HEADER_LABEL.trim(),
        app.viewer_login,
        scope,
    );

    let header = Block::default()
        .borders(Borders::BOTTOM)
        .title(Line::from(title.as_str()));

    frame.render_widget(header, area);
}

fn render_status(frame: &mut ratatui::Frame, area: Rect, app: &App) {
    // Status occupies the top-right corner of the header's inner area.
    // The header block's bottom border is on line 2; we stay on line 0.
    let inner = Block::default().borders(Borders::BOTTOM).inner(area);
    let status_width = 40;
    let status_area = Rect {
        x: inner.x + inner.width.saturating_sub(status_width),
        y: inner.y,
        width: status_width.min(inner.width),
        height: 1,
    };

    let grey = Style::default().fg(theme::COLOR_NONE);
    let faded_red = Style::default().fg(Color::Rgb(180, 60, 60));

    let line = if app.is_refreshing() || app.secs_until_refresh() == 0 {
        let spinner = app.spinner_frame();
        Line::styled(format!("{spinner} refreshing"), grey)
    } else if app.last_error.is_some() {
        Line::styled(
            format!(
                "refresh failed {} ago",
                format_duration(app.last_refresh_attempt_secs())
            ),
            faded_red,
        )
    } else {
        let interval = format_duration(app.refresh_interval_secs());
        let remaining = format_duration(app.secs_until_refresh());
        let bar = progress_bar(1.0 - app.refresh_progress(), 10);
        Line::styled(format!("auto {interval} {bar} {remaining}"), grey)
    };

    let paragraph = Paragraph::new(line).right_aligned();
    frame.render_widget(paragraph, status_area);
}

/// A simple ASCII progress bar: `▰▰▰▰▱▱▱▱▱▱` (filled = remaining fraction).
fn progress_bar(fraction: f64, width: usize) -> String {
    let filled = (fraction * width as f64).round() as usize;
    let filled = filled.min(width);
    let empty = width - filled;
    format!("{}{}", "▰".repeat(filled), "▱".repeat(empty))
}

fn render_table(frame: &mut ratatui::Frame, area: Rect, app: &App) {
    if app.prs.is_empty() {
        let msg = Paragraph::new(theme::EMPTY_STATE_MSG)
            .style(Style::default().add_modifier(Modifier::ITALIC))
            .centered();
        frame.render_widget(msg, area);
        return;
    }

    let header_cells: Vec<Line> = vec![
        Line::from(""),
        Line::from("#"),
        Line::from("Title"),
        Line::from("Diff"),
        Line::from("Checks"),
        Line::from("Comments"),
        Line::from("Approval"),
        Line::from("Up-to-date"),
        Line::from("Age"),
    ];
    let header = Row::new(header_cells).style(Style::default().fg(theme::COLOR_HEADER));

    let rows: Vec<Row> = app
        .prs
        .iter()
        .enumerate()
        .map(|(i, pr)| {
            let readiness = assess(pr);
            let (glyph, color) = theme::merge_glyph_and_color(&readiness);
            let counts = count_checks(&pr.checks);
            let checks_spans = Line::from(theme::checks_spans(&counts));
            let approval = collapse_reviews(&pr.reviews);
            let (rev_glyph, rev_color) = theme::approval_glyph_and_color(&approval);
            let (utd_glyph, utd_color) = theme::up_to_date_glyph_and_color(&pr.up_to_date);
            let diff_spans = Line::from(theme::diff_spans(pr.additions, pr.deletions));
            let comment_count = pr.reviews.len() + pr.comments.len();
            let age_str = components::format_age(&pr.created_at);

            let title = if pr.is_draft {
                format!("[Draft] {}", pr.title)
            } else {
                pr.title.clone()
            };

            Row::new([
                Line::from(format!(" {glyph}")).style(Style::default().fg(color)),
                Line::from(pr.number.to_string()),
                Line::from(title),
                diff_spans,
                checks_spans,
                Line::from(comment_count.to_string()),
                Line::from(format!(" {rev_glyph}")).style(Style::default().fg(rev_color)),
                Line::from(format!(" {utd_glyph}")).style(Style::default().fg(utd_color)),
                Line::from(age_str),
            ])
            .style(if i == app.selected {
                Style::default()
                    .bg(theme::COLOR_SELECTED_BG)
                    .fg(theme::COLOR_SELECTED)
            } else {
                Style::default()
            })
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(theme::COLUMN_INDICATOR_WIDTH as u16),
            Constraint::Length(theme::COLUMN_NUMBER_WIDTH as u16),
            Constraint::Min(1),
            Constraint::Length(theme::COLUMN_DIFF_WIDTH as u16),
            Constraint::Length(theme::COLUMN_CHECKS_WIDTH as u16),
            Constraint::Length(theme::COLUMN_COMMENTS_WIDTH as u16),
            Constraint::Length(theme::COLUMN_REVIEW_WIDTH as u16),
            Constraint::Length(theme::COLUMN_UPTODATE_WIDTH as u16),
            Constraint::Length(theme::COLUMN_AGE_WIDTH as u16),
        ],
    )
    .header(header);

    frame.render_widget(table, area);
}

fn render_footer(frame: &mut ratatui::Frame, area: Rect, app: &App) {
    let total = app.prs.len();
    let ready = app
        .prs
        .iter()
        .filter(|p| assess(p) == MergeReadiness::Ready)
        .count();
    let failed = app
        .prs
        .iter()
        .filter(|p| assess(p) == MergeReadiness::Failed)
        .count();

    let counts = format!(" {total} open PRs · {ready} ready · {failed} failed ");

    let chunks = Layout::horizontal([
        Constraint::Min(1),
        Constraint::Length(theme::FOOTER_HINT.chars().count() as u16),
    ])
    .split(area);

    let left = Paragraph::new(counts).style(
        Style::default()
            .add_modifier(Modifier::BOLD)
            .fg(theme::COLOR_HEADER),
    );
    frame.render_widget(left, chunks[0]);

    let right = Paragraph::new(theme::FOOTER_HINT).style(Style::default().fg(theme::COLOR_NONE));
    frame.render_widget(right, chunks[1]);
}

pub fn render_counts_string(counts: &WorkflowCounts) -> String {
    render_counts(counts)
}

pub fn assess_readiness(pr: &crate::github::pr::PullRequestSnapshot) -> MergeReadiness {
    assess(pr)
}

pub fn get_approval(pr: &crate::github::pr::PullRequestSnapshot) -> ApprovalState {
    collapse_reviews(&pr.reviews)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::analysis::workflows::WorkflowCounts;
    use crate::config::Config;
    use crate::github::models::{MergeableState, ReviewState};
    use crate::github::pr::{PullRequestSnapshot, ReviewSnapshot, UpToDateState};
    use chrono::{Duration, Utc};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    #[test]
    fn render_counts_zero() {
        let counts = WorkflowCounts {
            completed: 0,
            total: 0,
            success: 0,
            failed: 0,
            running: 0,
            skipped: 0,
        };
        assert_eq!(render_counts_string(&counts), "–/–");
    }

    #[test]
    fn render_counts_normal() {
        let counts = WorkflowCounts {
            completed: 5,
            total: 7,
            success: 5,
            failed: 0,
            running: 0,
            skipped: 0,
        };
        assert_eq!(render_counts_string(&counts), "5/7");
    }

    #[test]
    fn render_counts_capped() {
        let counts = WorkflowCounts {
            completed: 100,
            total: 100,
            success: 100,
            failed: 0,
            running: 0,
            skipped: 0,
        };
        assert_eq!(render_counts_string(&counts), "99+/99+");
    }

    fn make_app_with_prs() -> App {
        let cfg = Config {
            token: Some("ghp_test".to_string()),
            refresh: 300,
            owner: None,
            org: None,
            max_prs: 500,
            no_color: false,
            log_level: "info".to_string(),
        };
        let mut app = App::new(&cfg);
        app.state = crate::app::AppState::Ready;
        app.viewer_login = "ska".to_string();
        app.prs = vec![
            PullRequestSnapshot {
                number: 42,
                title: "Add feature".to_string(),
                url: "https://github.com/o/r/pull/42".to_string(),
                body: String::new(),
                is_draft: false,
                mergeable: MergeableState::Mergeable,
                repo: "o/r".to_string(),
                additions: 10,
                deletions: 3,
                created_at: (Utc::now() - Duration::days(5)).to_rfc3339(),
                rollup_state: Some(crate::github::models::RollupState::Success),
                checks: vec![crate::github::pr::CheckSnapshot {
                    name: "CI".to_string(),
                    kind: crate::github::pr::CheckKind::CheckRun,
                    status: crate::github::pr::CheckStatus::Success,
                    started_at: None,
                    completed_at: None,
                    annotations: vec![],
                }],
                reviews: vec![ReviewSnapshot {
                    author: "alice".to_string(),
                    author_is_bot: false,
                    state: ReviewState::Approved,
                    body: String::new(),
                    submitted_at: None,
                }],
                comments: vec![],
                up_to_date: UpToDateState::UpToDate,
            },
            PullRequestSnapshot {
                number: 99,
                title: "Draft PR".to_string(),
                url: "https://github.com/o/r/pull/99".to_string(),
                body: String::new(),
                is_draft: true,
                mergeable: MergeableState::Conflicting,
                repo: "o/r".to_string(),
                additions: 0,
                deletions: 5,
                created_at: (Utc::now() - Duration::hours(2)).to_rfc3339(),
                rollup_state: None,
                checks: vec![],
                reviews: vec![],
                comments: vec![],
                up_to_date: UpToDateState::OutOfDate,
            },
        ];
        app
    }

    fn extract_text(terminal: &Terminal<TestBackend>) -> String {
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect()
    }

    #[test]
    fn render_dashboard_with_prs() {
        let app = make_app_with_prs();
        let backend = TestBackend::new(120, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, &app)).unwrap();
        let text = extract_text(&terminal);
        assert!(text.contains("Add feature"));
        assert!(text.contains("[Draft] Draft PR"));
        assert!(text.contains("42"));
        assert!(text.contains("99"));
        assert!(text.contains("open PRs"));
        assert!(text.contains("ready"));
        assert!(text.contains("failed"));
    }

    #[test]
    fn render_dashboard_empty_state() {
        let cfg = Config {
            token: Some("ghp_test".to_string()),
            refresh: 300,
            owner: None,
            org: None,
            max_prs: 500,
            no_color: false,
            log_level: "info".to_string(),
        };
        let mut app = App::new(&cfg);
        app.state = crate::app::AppState::Ready;
        app.viewer_login = "ska".to_string();
        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, &app)).unwrap();
        let text = extract_text(&terminal);
        assert!(text.contains("no open PRs"));
    }

    #[test]
    fn render_dashboard_loading_state() {
        let cfg = Config {
            token: Some("ghp_test".to_string()),
            refresh: 300,
            owner: None,
            org: None,
            max_prs: 500,
            no_color: false,
            log_level: "info".to_string(),
        };
        let mut app = App::new(&cfg);
        app.state = crate::app::AppState::Loading;
        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, &app)).unwrap();
        let text = extract_text(&terminal);
        assert!(text.contains("Fetching"));
    }

    #[test]
    fn render_dashboard_error_state() {
        let cfg = Config {
            token: Some("ghp_test".to_string()),
            refresh: 300,
            owner: None,
            org: None,
            max_prs: 500,
            no_color: false,
            log_level: "info".to_string(),
        };
        let mut app = App::new(&cfg);
        app.state = crate::app::AppState::Error("Something broke".to_string());
        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, &app)).unwrap();
        let text = extract_text(&terminal);
        assert!(text.contains("Something broke"));
        assert!(text.contains("Press r to retry"));
    }

    #[test]
    fn render_dashboard_rate_limited_state() {
        let cfg = Config {
            token: Some("ghp_test".to_string()),
            refresh: 300,
            owner: None,
            org: None,
            max_prs: 500,
            no_color: false,
            log_level: "info".to_string(),
        };
        let mut app = App::new(&cfg);
        app.state = crate::app::AppState::RateLimited {
            retry_after_secs: 60,
        };
        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, &app)).unwrap();
        let text = extract_text(&terminal);
        assert!(text.contains("Rate limited"));
        assert!(text.contains("Retry in"));
    }

    #[test]
    fn render_dashboard_help_overlay() {
        let cfg = Config {
            token: Some("ghp_test".to_string()),
            refresh: 300,
            owner: None,
            org: None,
            max_prs: 500,
            no_color: false,
            log_level: "info".to_string(),
        };
        let mut app = App::new(&cfg);
        app.help_visible = true;
        let backend = TestBackend::new(80, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, &app)).unwrap();
        let text = extract_text(&terminal);
        assert!(text.contains("Keybindings"));
    }

    #[test]
    fn render_dashboard_with_selected_pr() {
        let mut app = make_app_with_prs();
        app.selected = 1;
        let backend = TestBackend::new(120, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, &app)).unwrap();
        let text = extract_text(&terminal);
        assert!(text.contains("Draft PR"));
    }

    #[test]
    fn assess_readiness_delegates() {
        let pr = make_app_with_prs().prs[0].clone();
        assert_eq!(assess_readiness(&pr), MergeReadiness::Ready);
    }

    #[test]
    fn get_approval_delegates() {
        let pr = make_app_with_prs().prs[0].clone();
        assert_eq!(get_approval(&pr), ApprovalState::Approved);
    }

    #[test]
    fn describe_overlay_renders_over_dashboard() {
        let mut app = make_app_with_prs();
        app.selected = 0;
        app.describe_visible = true;
        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, &app)).unwrap();
        let text = extract_text(&terminal);
        assert!(text.contains("Description"));
        assert!(text.contains("Checks"));
        assert!(text.contains("Activity"));
        assert!(text.contains("#42"));
    }

    #[test]
    fn describe_overlay_renders_on_small_terminal() {
        let mut app = make_app_with_prs();
        app.selected = 0;
        app.describe_visible = true;
        let backend = TestBackend::new(80, 15);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, &app)).unwrap();
        let text = extract_text(&terminal);
        assert!(text.contains("#42"));
    }

    #[test]
    fn describe_overlay_not_rendered_in_error_state() {
        let mut app = make_app_with_prs();
        app.describe_visible = true;
        app.state = crate::app::AppState::Error("Something broke".to_string());
        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, &app)).unwrap();
        let text = extract_text(&terminal);
        assert!(text.contains("Something broke"));
        assert!(!text.contains("Description"));
    }

    #[test]
    fn render_status_shows_auto_countdown_when_ready() {
        let mut app = make_app_with_prs();
        app.state = crate::app::AppState::Ready;
        app.last_refresh_attempt = Some(std::time::Instant::now());
        let backend = TestBackend::new(120, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, &app)).unwrap();
        let text = extract_text(&terminal);
        assert!(text.contains("auto"), "should show auto countdown prefix");
    }

    #[test]
    fn render_status_shows_refreshing_when_refreshing() {
        let mut app = make_app_with_prs();
        app.state = crate::app::AppState::Refreshing;
        let backend = TestBackend::new(120, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, &app)).unwrap();
        let text = extract_text(&terminal);
        assert!(text.contains("refreshing"), "should show refreshing text");
    }

    #[test]
    fn render_status_shows_refresh_failed_when_error_present() {
        let mut app = make_app_with_prs();
        app.state = crate::app::AppState::Ready;
        app.last_error = Some("Request timed out".to_string());
        app.last_refresh_attempt = Some(std::time::Instant::now());
        let backend = TestBackend::new(120, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, &app)).unwrap();
        let text = extract_text(&terminal);
        assert!(
            text.contains("refresh failed"),
            "should show refresh failed text"
        );
        assert!(
            !text.contains("auto"),
            "should not show countdown when error present"
        );
    }

    #[test]
    fn render_status_shows_refreshing_at_zero_countdown() {
        let mut app = make_app_with_prs();
        app.state = crate::app::AppState::Ready;
        // Set last_refresh_attempt far enough back that secs_until_refresh is 0.
        app.last_refresh_attempt =
            Some(std::time::Instant::now() - std::time::Duration::from_secs(9999));
        let backend = TestBackend::new(120, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, &app)).unwrap();
        let text = extract_text(&terminal);
        assert!(
            text.contains("refreshing"),
            "should show refreshing when countdown reaches 0"
        );
    }
}
