use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Row, Table};

use chrono::{DateTime, Utc};

use crate::analysis::approval::{ApprovalState, collapse_reviews};
use crate::analysis::mergeability::{MergeReadiness, assess};
use crate::analysis::workflows::{WorkflowCounts, count_checks, render_counts};
use crate::app::App;
use crate::ui::theme;

pub fn render(frame: &mut ratatui::Frame, app: &mut App) {
    let area = frame.area();

    if app.help_visible {
        crate::ui::help_overlay::render(frame, area);
        return;
    }

    match &app.state {
        crate::app::AppState::Error(msg) => {
            crate::ui::error_screen::render(frame, area, msg);
        }
        crate::app::AppState::RateLimited { retry_after_secs } => {
            let countdown = app
                .rate_limit_countdown()
                .unwrap_or_else(|| format!("{}s", retry_after_secs));
            let msg = format!("Rate limited by GitHub. Retry in {countdown}");
            crate::ui::error_screen::render(frame, area, &msg);
        }
        crate::app::AppState::Loading | crate::app::AppState::Refreshing if app.prs.is_empty() => {
            let spinner = app.spinner();
            let msg = format!(" {spinner}  Fetching your PRs... ");
            let paragraph = Paragraph::new(msg).centered();
            frame.render_widget(paragraph, area);
        }
        _ => {
            render_dashboard(frame, area, app);
        }
    }
}

fn render_dashboard(frame: &mut ratatui::Frame, area: Rect, app: &mut App) {
    let chunks = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .split(area);

    render_header(frame, chunks[0], &mut *app);
    render_table(frame, chunks[1], app);
    render_footer(frame, chunks[2], app);
}

fn render_header(frame: &mut ratatui::Frame, area: Rect, app: &mut App) {
    let title = if app.is_refreshing() {
        let spinner = app.spinner();
        format!(
            " {} {} — {}'s open PRs · refreshing... ",
            spinner,
            theme::HEADER_LABEL.trim(),
            app.viewer_login,
        )
    } else {
        format!(
            " {} — {}'s open PRs · refreshed {}s ago ",
            theme::HEADER_LABEL.trim(),
            app.viewer_login,
            app.last_refresh_secs()
        )
    };

    let header = Block::default()
        .borders(Borders::BOTTOM)
        .title(Line::from(title.as_str()));

    frame.render_widget(header, area);
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
            let checks_spans = render_checks_spans(&counts);
            let approval = collapse_reviews(&pr.reviews);
            let (rev_glyph, rev_color) = theme::approval_glyph_and_color(&approval);
            let (utd_glyph, utd_color) = theme::up_to_date_glyph_and_color(&pr.up_to_date);
            let diff_spans = vec![
                Span::styled(
                    if pr.additions > 0 {
                        format!("+{}", pr.additions)
                    } else {
                        "0".to_string()
                    },
                    if pr.additions > 0 {
                        Style::default().fg(theme::COLOR_READY)
                    } else {
                        Style::default()
                    },
                ),
                Span::raw("/"),
                Span::styled(
                    if pr.deletions > 0 {
                        format!("-{}", pr.deletions)
                    } else {
                        "0".to_string()
                    },
                    if pr.deletions > 0 {
                        Style::default().fg(Color::Red)
                    } else {
                        Style::default()
                    },
                ),
            ];
            let age_str = format_age(&pr.created_at);

            let title = if pr.is_draft {
                format!("[Draft] {}", pr.title)
            } else {
                pr.title.clone()
            };

            Row::new([
                Line::from(format!(" {glyph}")).style(Style::default().fg(color)),
                Line::from(pr.number.to_string()),
                Line::from(title),
                Line::from(diff_spans),
                checks_spans,
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
            Constraint::Length(theme::COLUMN_REVIEW_WIDTH as u16),
            Constraint::Length(theme::COLUMN_UPTODATE_WIDTH as u16),
            Constraint::Length(theme::COLUMN_AGE_WIDTH as u16),
        ],
    )
    .header(header);

    frame.render_widget(table, area);
}

fn format_age(created_at: &str) -> String {
    let parsed: DateTime<Utc> = match DateTime::parse_from_rfc3339(created_at) {
        Ok(dt) => dt.with_timezone(&Utc),
        Err(_) => return "?".to_string(),
    };
    let elapsed = Utc::now().signed_duration_since(parsed);
    let secs = elapsed.num_seconds();
    if secs < 60 {
        return format!("{secs}s");
    }
    let mins = secs / 60;
    if mins < 60 {
        return format!("{mins}m");
    }
    let hours = mins / 60;
    if hours < 24 {
        return format!("{hours}h");
    }
    let days = hours / 24;
    if days < 30 {
        return format!("{days}d");
    }
    let months = days / 30;
    if months < 12 {
        return format!("{months}mo");
    }
    let years = months / 12;
    format!("{years}y")
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

    let footer = format!(" {total} open PRs · {ready} ready · {failed} failed ");

    let paragraph = Paragraph::new(footer).style(
        Style::default()
            .add_modifier(Modifier::BOLD)
            .fg(theme::COLOR_HEADER),
    );

    frame.render_widget(paragraph, area);
}

fn render_checks_spans(counts: &WorkflowCounts) -> Line<'static> {
    if counts.total == 0 {
        return Line::from("–/–");
    }

    let cap = |n: usize| -> String {
        if n > 99 {
            "99+".to_string()
        } else {
            n.to_string()
        }
    };

    Line::from(vec![
        Span::styled(cap(counts.success), Style::default().fg(theme::COLOR_READY)),
        Span::raw("/"),
        Span::styled(
            cap(counts.failed),
            if counts.failed > 0 {
                Style::default().fg(theme::COLOR_FAILED)
            } else {
                Style::default()
            },
        ),
        Span::raw("/"),
        Span::styled(
            cap(counts.running),
            if counts.running > 0 {
                Style::default().fg(theme::COLOR_WAITING)
            } else {
                Style::default()
            },
        ),
        Span::raw("/"),
        Span::styled(cap(counts.skipped), Style::default().fg(theme::COLOR_NONE)),
    ])
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
    fn format_age_seconds() {
        let ts = (Utc::now() - Duration::seconds(30)).to_rfc3339();
        assert_eq!(format_age(&ts), "30s");
    }

    #[test]
    fn format_age_minutes() {
        let ts = (Utc::now() - Duration::minutes(5)).to_rfc3339();
        assert_eq!(format_age(&ts), "5m");
    }

    #[test]
    fn format_age_hours() {
        let ts = (Utc::now() - Duration::hours(3)).to_rfc3339();
        assert_eq!(format_age(&ts), "3h");
    }

    #[test]
    fn format_age_days() {
        let ts = (Utc::now() - Duration::days(7)).to_rfc3339();
        assert_eq!(format_age(&ts), "7d");
    }

    #[test]
    fn format_age_months() {
        let ts = (Utc::now() - Duration::days(60)).to_rfc3339();
        assert_eq!(format_age(&ts), "2mo");
    }

    #[test]
    fn format_age_just_under_a_year() {
        let ts = (Utc::now() - Duration::days(350)).to_rfc3339();
        assert_eq!(format_age(&ts), "11mo");
    }

    #[test]
    fn format_age_twelve_months_shows_year() {
        let ts = (Utc::now() - Duration::days(360)).to_rfc3339();
        assert_eq!(format_age(&ts), "1y");
    }

    #[test]
    fn format_age_one_year() {
        let ts = (Utc::now() - Duration::days(365)).to_rfc3339();
        assert_eq!(format_age(&ts), "1y");
    }

    #[test]
    fn format_age_multiple_years() {
        let ts = (Utc::now() - Duration::days(730)).to_rfc3339();
        assert_eq!(format_age(&ts), "2y");
    }

    #[test]
    fn format_age_invalid_returns_question() {
        assert_eq!(format_age("not a date"), "?");
    }

    #[test]
    fn format_age_empty_returns_question() {
        assert_eq!(format_age(""), "?");
    }

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
                    completed: true,
                    failed: false,
                    skipped: false,
                    running: false,
                }],
                reviews: vec![ReviewSnapshot {
                    author: "alice".to_string(),
                    state: ReviewState::Approved,
                }],
                up_to_date: UpToDateState::UpToDate,
            },
            PullRequestSnapshot {
                number: 99,
                title: "Draft PR".to_string(),
                url: "https://github.com/o/r/pull/99".to_string(),
                is_draft: true,
                mergeable: MergeableState::Conflicting,
                repo: "o/r".to_string(),
                additions: 0,
                deletions: 5,
                created_at: (Utc::now() - Duration::hours(2)).to_rfc3339(),
                rollup_state: None,
                checks: vec![],
                reviews: vec![],
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
        let mut app = make_app_with_prs();
        let backend = TestBackend::new(120, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, &mut app)).unwrap();
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
            max_prs: 500,
            no_color: false,
            log_level: "info".to_string(),
        };
        let mut app = App::new(&cfg);
        app.state = crate::app::AppState::Ready;
        app.viewer_login = "ska".to_string();
        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, &mut app)).unwrap();
        let text = extract_text(&terminal);
        assert!(text.contains("no open PRs"));
    }

    #[test]
    fn render_dashboard_loading_state() {
        let cfg = Config {
            token: Some("ghp_test".to_string()),
            refresh: 300,
            owner: None,
            max_prs: 500,
            no_color: false,
            log_level: "info".to_string(),
        };
        let mut app = App::new(&cfg);
        app.state = crate::app::AppState::Loading;
        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, &mut app)).unwrap();
        let text = extract_text(&terminal);
        assert!(text.contains("Fetching"));
    }

    #[test]
    fn render_dashboard_error_state() {
        let cfg = Config {
            token: Some("ghp_test".to_string()),
            refresh: 300,
            owner: None,
            max_prs: 500,
            no_color: false,
            log_level: "info".to_string(),
        };
        let mut app = App::new(&cfg);
        app.state = crate::app::AppState::Error("Something broke".to_string());
        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, &mut app)).unwrap();
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
        terminal.draw(|f| render(f, &mut app)).unwrap();
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
            max_prs: 500,
            no_color: false,
            log_level: "info".to_string(),
        };
        let mut app = App::new(&cfg);
        app.help_visible = true;
        let backend = TestBackend::new(80, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, &mut app)).unwrap();
        let text = extract_text(&terminal);
        assert!(text.contains("Keybindings"));
    }

    #[test]
    fn render_dashboard_with_selected_pr() {
        let mut app = make_app_with_prs();
        app.selected = 1;
        let backend = TestBackend::new(120, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, &mut app)).unwrap();
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
}
