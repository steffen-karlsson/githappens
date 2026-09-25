use std::time::{Duration, Instant};

use crate::analysis::mergeability::{MergeReadiness, assess};
use crate::config::Config;
use crate::github::client::{FetchError, FetchOutcome};
use crate::github::pr::PullRequestSnapshot;

#[derive(Debug, Clone, PartialEq)]
pub enum AppState {
    Loading,
    Ready,
    Refreshing,
    Error(String),
    RateLimited { retry_after_secs: u64 },
    Help,
}

#[derive(Debug, Clone, PartialEq)]
pub enum KeyAction {
    Quit,
    Refresh,
    ForceRefresh,
    OpenUrl(String),
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DescribeFocus {
    Description,
    Checks,
    Activity,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DescribeSubView {
    Description,
    CheckDetail { index: usize },
    ActivityDetail { index: usize },
}

pub struct App {
    pub state: AppState,
    pub prs: Vec<PullRequestSnapshot>,
    pub selected: usize,
    pub viewer_login: String,
    pub help_visible: bool,
    pub describe_visible: bool,
    pub describe_scroll: usize,
    pub describe_focus: DescribeFocus,
    pub describe_subview: Option<DescribeSubView>,
    pub describe_subview_scroll: usize,
    pub last_refresh: Option<Instant>,
    pub last_error: Option<String>,
    pub last_refresh_attempt: Option<Instant>,
    pub truncated: bool,
    pub org: Option<String>,
    refresh_interval: Duration,
    spinner_idx: usize,
}

const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// Human-readable duration: `45s`, `5m20s`, `5m`, `1h5m`, `1h`.
pub fn format_duration(secs: u64) -> String {
    if secs < 60 {
        return format!("{secs}s");
    }
    let hours = secs / 3600;
    let minutes = (secs % 3600) / 60;
    let seconds = secs % 60;
    if hours > 0 {
        if minutes > 0 {
            format!("{hours}h{minutes}m")
        } else {
            format!("{hours}h")
        }
    } else if seconds > 0 {
        format!("{minutes}m{seconds}s")
    } else {
        format!("{minutes}m")
    }
}

impl App {
    pub fn new(config: &Config) -> Self {
        Self {
            state: AppState::Loading,
            prs: Vec::new(),
            selected: 0,
            viewer_login: String::new(),
            help_visible: false,
            describe_visible: false,
            describe_scroll: 0,
            describe_focus: DescribeFocus::Description,
            describe_subview: None,
            describe_subview_scroll: 0,
            last_refresh: None,
            last_error: None,
            last_refresh_attempt: None,
            truncated: false,
            org: config.org.clone(),
            refresh_interval: Duration::from_secs(300),
            spinner_idx: 0,
        }
    }

    pub fn with_refresh_interval(mut self, secs: u64) -> Self {
        self.refresh_interval = Duration::from_secs(secs);
        self
    }

    pub fn last_refresh_secs(&self) -> u64 {
        self.last_refresh
            .map(|t| t.elapsed().as_secs())
            .unwrap_or(0)
    }

    pub fn should_auto_refresh(&self) -> bool {
        if self.state != AppState::Ready {
            return false;
        }
        self.last_refresh_attempt
            .map(|t| t.elapsed() >= self.refresh_interval)
            .unwrap_or(true)
    }

    pub fn secs_until_refresh(&self) -> u64 {
        self.last_refresh_attempt
            .map(|t| {
                self.refresh_interval
                    .as_secs()
                    .saturating_sub(t.elapsed().as_secs())
                    .saturating_sub(1)
            })
            .unwrap_or(self.refresh_interval.as_secs().saturating_sub(1))
    }

    pub fn last_refresh_attempt_secs(&self) -> u64 {
        self.last_refresh_attempt
            .map(|t| t.elapsed().as_secs())
            .unwrap_or(0)
    }

    pub fn refresh_interval_secs(&self) -> u64 {
        self.refresh_interval.as_secs()
    }

    /// Fraction of the refresh interval elapsed since last attempt (0.0–1.0).
    /// Offset by 1s so the bar starts slightly depleted, matching the -1s countdown.
    pub fn refresh_progress(&self) -> f64 {
        self.last_refresh_attempt
            .map(|t| {
                ((t.elapsed().as_secs_f64() + 1.0) / self.refresh_interval.as_secs_f64()).min(1.0)
            })
            .unwrap_or(1.0 / self.refresh_interval.as_secs_f64())
    }

    pub fn is_refreshing(&self) -> bool {
        matches!(self.state, AppState::Refreshing | AppState::Loading)
    }

    pub fn spinner_frame(&self) -> &'static str {
        SPINNER_FRAMES[self.spinner_idx % SPINNER_FRAMES.len()]
    }

    pub fn advance_spinner(&mut self) {
        self.spinner_idx = self.spinner_idx.wrapping_add(1);
    }

    pub fn spinner(&mut self) -> &'static str {
        let frame = self.spinner_frame();
        self.advance_spinner();
        frame
    }

    pub fn select_down(&mut self) {
        if !self.prs.is_empty() {
            self.selected = (self.selected + 1).min(self.prs.len() - 1);
        }
    }

    pub fn select_up(&mut self) {
        if !self.prs.is_empty() {
            self.selected = self.selected.saturating_sub(1);
        }
    }

    pub fn select_top(&mut self) {
        self.selected = 0;
    }

    pub fn select_bottom(&mut self) {
        if !self.prs.is_empty() {
            self.selected = self.prs.len() - 1;
        }
    }

    pub fn toggle_help(&mut self) {
        self.help_visible = !self.help_visible;
    }

    pub fn toggle_describe(&mut self) {
        self.describe_visible = !self.describe_visible;
        self.describe_scroll = 0;
        self.describe_focus = DescribeFocus::Description;
        self.describe_subview = None;
        self.describe_subview_scroll = 0;
    }

    pub fn describe_scroll_down(&mut self) {
        if self.describe_subview.is_some() {
            self.describe_subview_scroll = self.describe_subview_scroll.saturating_add(1);
        } else {
            let max = self.describe_max_scroll();
            if self.describe_scroll < max {
                self.describe_scroll += 1;
            }
        }
    }

    pub fn describe_scroll_up(&mut self) {
        if self.describe_subview.is_some() {
            self.describe_subview_scroll = self.describe_subview_scroll.saturating_sub(1);
        } else {
            self.describe_scroll = self.describe_scroll.saturating_sub(1);
        }
    }

    fn describe_max_scroll(&self) -> usize {
        let Some(pr) = self.prs.get(self.selected) else {
            return 0;
        };
        match self.describe_focus {
            DescribeFocus::Description => {
                if pr.body.is_empty() {
                    return 0;
                }
                let stripped = crate::ui::components::strip_markdown(&pr.body);
                let line_count = stripped.lines().count();
                line_count.saturating_sub(crate::ui::describe_overlay::MAX_DESC_LINES)
            }
            DescribeFocus::Checks => pr.checks.len().saturating_sub(1),
            DescribeFocus::Activity => (pr.reviews.len() + pr.comments.len()).saturating_sub(1),
        }
    }

    pub fn describe_tab_next(&mut self) {
        self.describe_focus = match self.describe_focus {
            DescribeFocus::Description => DescribeFocus::Checks,
            DescribeFocus::Checks => DescribeFocus::Activity,
            DescribeFocus::Activity => DescribeFocus::Description,
        };
        self.describe_scroll = 0;
    }

    pub fn describe_tab_prev(&mut self) {
        self.describe_focus = match self.describe_focus {
            DescribeFocus::Description => DescribeFocus::Activity,
            DescribeFocus::Checks => DescribeFocus::Description,
            DescribeFocus::Activity => DescribeFocus::Checks,
        };
        self.describe_scroll = 0;
    }

    pub fn describe_open_subview(&mut self) {
        match self.describe_focus {
            DescribeFocus::Description => {
                let has_body = self
                    .prs
                    .get(self.selected)
                    .is_some_and(|pr| !pr.body.is_empty());
                if has_body {
                    self.describe_subview = Some(DescribeSubView::Description);
                }
            }
            DescribeFocus::Checks => {
                let index = self.describe_scroll;
                if self.prs.get(self.selected).map_or(0, |pr| pr.checks.len()) > index {
                    self.describe_subview = Some(DescribeSubView::CheckDetail { index });
                }
            }
            DescribeFocus::Activity => {
                let index = self.describe_scroll;
                let count = self
                    .prs
                    .get(self.selected)
                    .map_or(0, |pr| pr.reviews.len() + pr.comments.len());
                if count > index {
                    self.describe_subview = Some(DescribeSubView::ActivityDetail { index });
                }
            }
        }
        self.describe_subview_scroll = 0;
    }

    pub fn describe_close_subview(&mut self) {
        self.describe_subview = None;
        self.describe_subview_scroll = 0;
    }

    pub fn handle_key(&mut self, key: crossterm::event::KeyEvent) -> KeyAction {
        use crossterm::event::KeyCode;

        if self.help_visible {
            if key.code == KeyCode::Char('?') {
                self.toggle_help();
            }
            return KeyAction::None;
        }

        if self.describe_visible {
            if self.describe_subview.is_some() {
                match key.code {
                    KeyCode::Char('d') | KeyCode::Esc | KeyCode::Char('q') | KeyCode::Enter => {
                        self.describe_close_subview();
                    }
                    KeyCode::Char('j') | KeyCode::Down => {
                        self.describe_scroll_down();
                    }
                    KeyCode::Char('k') | KeyCode::Up => {
                        self.describe_scroll_up();
                    }
                    _ => {}
                }
                return KeyAction::None;
            }
            match key.code {
                KeyCode::Char('d') | KeyCode::Esc | KeyCode::Char('q') => {
                    self.toggle_describe();
                }
                KeyCode::Tab => {
                    self.describe_tab_next();
                }
                KeyCode::BackTab => {
                    self.describe_tab_prev();
                }
                KeyCode::Char('j') | KeyCode::Down => {
                    self.describe_scroll_down();
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    self.describe_scroll_up();
                }
                KeyCode::Enter => {
                    self.describe_open_subview();
                }
                _ => {}
            }
            return KeyAction::None;
        }

        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => KeyAction::Quit,
            KeyCode::Char('j') | KeyCode::Down => {
                self.select_down();
                KeyAction::None
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.select_up();
                KeyAction::None
            }
            KeyCode::Char('g') => {
                self.select_top();
                KeyAction::None
            }
            KeyCode::Char('G') => {
                self.select_bottom();
                KeyAction::None
            }
            KeyCode::Enter => {
                if let Some(pr) = self.selected_pr() {
                    KeyAction::OpenUrl(pr.url.clone())
                } else {
                    KeyAction::None
                }
            }
            KeyCode::Char('r') => KeyAction::Refresh,
            KeyCode::Char('R') => KeyAction::ForceRefresh,
            KeyCode::Char('d') => {
                self.toggle_describe();
                KeyAction::None
            }
            KeyCode::Char('?') => {
                self.toggle_help();
                KeyAction::None
            }
            _ => KeyAction::None,
        }
    }

    pub fn selected_pr(&self) -> Option<&PullRequestSnapshot> {
        self.prs.get(self.selected)
    }

    pub fn apply_fetch_result(&mut self, result: Result<FetchOutcome, FetchError>) {
        match result {
            Ok(outcome) => {
                self.apply_outcome(outcome);
            }
            Err(e) => {
                // Rate-limited always shows the full-screen countdown.
                if let FetchError::RateLimited { retry_after_secs } = e {
                    self.state = AppState::RateLimited { retry_after_secs };
                    self.last_refresh_attempt = Some(Instant::now());
                    return;
                }
                // With existing PRs, keep the dashboard visible.
                if !self.prs.is_empty() {
                    self.last_error = Some(error_message(&e));
                    self.last_refresh_attempt = Some(Instant::now());
                    self.state = AppState::Ready;
                    return;
                }
                // No data yet — show the full-screen error.
                self.state = AppState::Error(error_message(&e));
            }
        }
    }

    fn apply_outcome(&mut self, outcome: FetchOutcome) {
        self.prs = outcome.prs;
        if let Some(org) = &self.org {
            let prefix = format!("{}/", org.to_lowercase());
            self.prs
                .retain(|p| p.repo.to_lowercase().starts_with(&prefix));
        }
        self.viewer_login = outcome.login;
        self.truncated = outcome.truncated;
        self.last_refresh = Some(Instant::now());
        self.last_error = None;
        self.last_refresh_attempt = Some(Instant::now());
        self.state = AppState::Ready;
        if self.selected >= self.prs.len() && !self.prs.is_empty() {
            self.selected = self.prs.len() - 1;
        }
    }

    pub fn ready_count(&self) -> usize {
        self.prs
            .iter()
            .filter(|p| assess(p) == MergeReadiness::Ready)
            .count()
    }

    pub fn failed_count(&self) -> usize {
        self.prs
            .iter()
            .filter(|p| assess(p) == MergeReadiness::Failed)
            .count()
    }

    pub fn is_rate_limited(&self) -> bool {
        matches!(self.state, AppState::RateLimited { .. })
    }

    pub fn rate_limit_retry_secs(&self) -> Option<u64> {
        match &self.state {
            AppState::RateLimited { retry_after_secs } => Some(*retry_after_secs),
            _ => None,
        }
    }

    pub fn can_refresh(&self) -> bool {
        !matches!(self.state, AppState::Refreshing)
    }

    pub fn rate_limit_countdown(&self) -> Option<String> {
        match &self.state {
            AppState::RateLimited { retry_after_secs } => {
                let elapsed = self
                    .last_refresh_attempt
                    .map(|t| t.elapsed().as_secs())
                    .unwrap_or(0);
                let remaining = retry_after_secs.saturating_sub(elapsed);
                Some(format_duration(remaining))
            }
            _ => None,
        }
    }
}

fn error_message(e: &FetchError) -> String {
    match e {
        FetchError::RateLimited { retry_after_secs } => {
            format!("Rate limited, retry in {retry_after_secs}s")
        }
        FetchError::TokenInvalid => "Token invalid or expired".to_string(),
        FetchError::GitHubUnavailable => "GitHub unavailable".to_string(),
        FetchError::Timeout => "Request timed out".to_string(),
        other => other.to_string(),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::github::client::{GitHubFetcher, MockGitHubFetcher};

    fn make_config() -> Config {
        Config {
            token: Some("ghp_test".to_string()),
            refresh: 300,
            owner: None,
            org: None,
            max_prs: 500,
            no_color: false,
            log_level: "info".to_string(),
        }
    }

    #[tokio::test]
    async fn test_loading_to_ready() {
        let cfg = make_config();
        let mut app = App::new(&cfg);
        let fetcher = MockGitHubFetcher {
            outcome: Ok(FetchOutcome {
                login: "ska".to_string(),
                prs: vec![],
                truncated: false,
            }),
        };
        let result = fetcher.fetch_open_prs(None, 500).await;
        app.apply_fetch_result(result);
        assert_eq!(app.state, AppState::Ready);
        assert_eq!(app.viewer_login, "ska");
    }

    #[tokio::test]
    async fn test_error_state_on_token_invalid() {
        let cfg = make_config();
        let mut app = App::new(&cfg);
        let fetcher = MockGitHubFetcher {
            outcome: Err(FetchError::TokenInvalid),
        };
        let result = fetcher.fetch_open_prs(None, 500).await;
        app.apply_fetch_result(result);
        match &app.state {
            AppState::Error(msg) => assert!(msg.contains("Token invalid")),
            _ => panic!("expected Error state"),
        }
    }

    #[tokio::test]
    async fn test_rate_limited_state() {
        let cfg = make_config();
        let mut app = App::new(&cfg);
        let fetcher = MockGitHubFetcher {
            outcome: Err(FetchError::RateLimited {
                retry_after_secs: 120,
            }),
        };
        let result = fetcher.fetch_open_prs(None, 500).await;
        app.apply_fetch_result(result);
        match &app.state {
            AppState::RateLimited { retry_after_secs } => {
                assert_eq!(*retry_after_secs, 120);
            }
            _ => panic!("expected RateLimited state"),
        }
    }

    #[test]
    fn test_navigation() {
        let cfg = make_config();
        let mut app = App::new(&cfg);
        app.prs = vec![
            PullRequestSnapshot {
                number: 1,
                title: "PR 1".to_string(),
                url: "https://github.com/o/r/pull/1".to_string(),
                body: String::new(),
                is_draft: false,
                mergeable: crate::github::models::MergeableState::Mergeable,
                repo: "o/r".to_string(),
                rollup_state: None,
                checks: vec![],
                reviews: vec![],
                up_to_date: crate::github::pr::UpToDateState::Unknown,
                additions: 0,
                deletions: 0,
                created_at: String::new(),
                comments: vec![],
            },
            PullRequestSnapshot {
                number: 2,
                title: "PR 2".to_string(),
                url: "https://github.com/o/r/pull/2".to_string(),
                body: String::new(),
                is_draft: false,
                mergeable: crate::github::models::MergeableState::Mergeable,
                repo: "o/r".to_string(),
                rollup_state: None,
                checks: vec![],
                reviews: vec![],
                up_to_date: crate::github::pr::UpToDateState::Unknown,
                additions: 0,
                deletions: 0,
                created_at: String::new(),
                comments: vec![],
            },
        ];
        app.select_down();
        assert_eq!(app.selected, 1);
        app.select_down();
        assert_eq!(app.selected, 1);
        app.select_up();
        assert_eq!(app.selected, 0);
        app.select_up();
        assert_eq!(app.selected, 0);
        app.select_bottom();
        assert_eq!(app.selected, 1);
        app.select_top();
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn test_empty_navigation_no_panic() {
        let cfg = make_config();
        let mut app = App::new(&cfg);
        app.select_down();
        assert_eq!(app.selected, 0);
        app.select_up();
        assert_eq!(app.selected, 0);
        app.select_bottom();
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn test_help_toggle() {
        let cfg = make_config();
        let mut app = App::new(&cfg);
        assert!(!app.help_visible);
        app.toggle_help();
        assert!(app.help_visible);
        app.toggle_help();
        assert!(!app.help_visible);
    }

    #[test]
    fn test_with_refresh_interval() {
        let cfg = make_config();
        let app = App::new(&cfg).with_refresh_interval(60);
        assert_eq!(app.refresh_interval, Duration::from_secs(60));
    }

    #[test]
    fn test_last_refresh_secs_default_zero() {
        let cfg = make_config();
        let app = App::new(&cfg);
        assert_eq!(app.last_refresh_secs(), 0);
    }

    #[test]
    fn test_should_auto_refresh_initial_loading() {
        let cfg = make_config();
        let app = App::new(&cfg);
        assert!(!app.should_auto_refresh());
    }

    #[test]
    fn test_should_auto_refresh_ready_no_refresh() {
        let cfg = make_config();
        let mut app = App::new(&cfg);
        app.state = AppState::Ready;
        app.last_refresh_attempt = Some(Instant::now());
        assert!(!app.should_auto_refresh());
    }

    #[test]
    fn test_should_auto_refresh_ready_stale() {
        let cfg = make_config();
        let mut app = App::new(&cfg).with_refresh_interval(0);
        app.state = AppState::Ready;
        app.last_refresh_attempt = Some(Instant::now());
        std::thread::sleep(Duration::from_millis(10));
        assert!(app.should_auto_refresh());
    }

    #[test]
    fn test_should_auto_refresh_not_ready() {
        let cfg = make_config();
        let mut app = App::new(&cfg);
        app.state = AppState::Refreshing;
        assert!(!app.should_auto_refresh());
    }

    #[test]
    fn test_is_refreshing() {
        let cfg = make_config();
        let mut app = App::new(&cfg);
        assert!(app.is_refreshing());
        app.state = AppState::Ready;
        assert!(!app.is_refreshing());
        app.state = AppState::Refreshing;
        assert!(app.is_refreshing());
    }

    #[test]
    fn test_spinner_cycles() {
        let cfg = make_config();
        let mut app = App::new(&cfg);
        let first = app.spinner();
        let second = app.spinner();
        assert_ne!(first, second);
        assert!(SPINNER_FRAMES.contains(&first));
    }

    #[test]
    fn test_select_top_bottom() {
        let cfg = make_config();
        let mut app = App::new(&cfg);
        app.prs = vec![
            PullRequestSnapshot {
                number: 1,
                title: "PR 1".to_string(),
                url: "https://github.com/o/r/pull/1".to_string(),
                body: String::new(),
                is_draft: false,
                mergeable: crate::github::models::MergeableState::Mergeable,
                repo: "o/r".to_string(),
                rollup_state: None,
                checks: vec![],
                reviews: vec![],
                up_to_date: crate::github::pr::UpToDateState::Unknown,
                additions: 0,
                deletions: 0,
                created_at: String::new(),
                comments: vec![],
            },
            PullRequestSnapshot {
                number: 2,
                title: "PR 2".to_string(),
                url: "https://github.com/o/r/pull/2".to_string(),
                body: String::new(),
                is_draft: false,
                mergeable: crate::github::models::MergeableState::Mergeable,
                repo: "o/r".to_string(),
                rollup_state: None,
                checks: vec![],
                reviews: vec![],
                up_to_date: crate::github::pr::UpToDateState::Unknown,
                additions: 0,
                deletions: 0,
                created_at: String::new(),
                comments: vec![],
            },
        ];
        app.selected = 1;
        app.select_top();
        assert_eq!(app.selected, 0);
        app.select_bottom();
        assert_eq!(app.selected, 1);
    }

    #[test]
    fn test_selected_pr() {
        let cfg = make_config();
        let mut app = App::new(&cfg);
        app.prs = vec![PullRequestSnapshot {
            number: 1,
            title: "PR 1".to_string(),
            url: "https://github.com/o/r/pull/1".to_string(),
            body: String::new(),
            is_draft: false,
            mergeable: crate::github::models::MergeableState::Mergeable,
            repo: "o/r".to_string(),
            rollup_state: None,
            checks: vec![],
            reviews: vec![],
            up_to_date: crate::github::pr::UpToDateState::Unknown,
            additions: 0,
            deletions: 0,
            created_at: String::new(),
            comments: vec![],
        }];
        assert_eq!(app.selected_pr().unwrap().number, 1);
    }

    #[test]
    fn test_selected_pr_empty() {
        let cfg = make_config();
        let app = App::new(&cfg);
        assert!(app.selected_pr().is_none());
    }

    #[test]
    fn test_handle_key_quit() {
        let cfg = make_config();
        let mut app = App::new(&cfg);
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let action = app.handle_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE));
        assert_eq!(action, KeyAction::Quit);
    }

    #[test]
    fn test_handle_key_escape() {
        let cfg = make_config();
        let mut app = App::new(&cfg);
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let action = app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(action, KeyAction::Quit);
    }

    #[test]
    fn test_handle_key_j_down() {
        let cfg = make_config();
        let mut app = App::new(&cfg);
        app.prs = vec![
            PullRequestSnapshot {
                number: 1,
                title: "PR 1".to_string(),
                url: "https://github.com/o/r/pull/1".to_string(),
                body: String::new(),
                is_draft: false,
                mergeable: crate::github::models::MergeableState::Mergeable,
                repo: "o/r".to_string(),
                rollup_state: None,
                checks: vec![],
                reviews: vec![],
                up_to_date: crate::github::pr::UpToDateState::Unknown,
                additions: 0,
                deletions: 0,
                created_at: String::new(),
                comments: vec![],
            },
            PullRequestSnapshot {
                number: 2,
                title: "PR 2".to_string(),
                url: "https://github.com/o/r/pull/2".to_string(),
                body: String::new(),
                is_draft: false,
                mergeable: crate::github::models::MergeableState::Mergeable,
                repo: "o/r".to_string(),
                rollup_state: None,
                checks: vec![],
                reviews: vec![],
                up_to_date: crate::github::pr::UpToDateState::Unknown,
                additions: 0,
                deletions: 0,
                created_at: String::new(),
                comments: vec![],
            },
        ];
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let action = app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
        assert_eq!(action, KeyAction::None);
        assert_eq!(app.selected, 1);
    }

    #[test]
    fn test_handle_key_k_up() {
        let cfg = make_config();
        let mut app = App::new(&cfg);
        app.prs = vec![PullRequestSnapshot {
            number: 1,
            title: "PR 1".to_string(),
            url: "https://github.com/o/r/pull/1".to_string(),
            body: String::new(),
            is_draft: false,
            mergeable: crate::github::models::MergeableState::Mergeable,
            repo: "o/r".to_string(),
            rollup_state: None,
            checks: vec![],
            reviews: vec![],
            up_to_date: crate::github::pr::UpToDateState::Unknown,
            additions: 0,
            deletions: 0,
            created_at: String::new(),
            comments: vec![],
        }];
        app.selected = 0;
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let action = app.handle_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE));
        assert_eq!(action, KeyAction::None);
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn test_handle_key_g_top() {
        let cfg = make_config();
        let mut app = App::new(&cfg);
        app.prs = vec![PullRequestSnapshot {
            number: 1,
            title: "PR 1".to_string(),
            url: "https://github.com/o/r/pull/1".to_string(),
            body: String::new(),
            is_draft: false,
            mergeable: crate::github::models::MergeableState::Mergeable,
            repo: "o/r".to_string(),
            rollup_state: None,
            checks: vec![],
            reviews: vec![],
            up_to_date: crate::github::pr::UpToDateState::Unknown,
            additions: 0,
            deletions: 0,
            created_at: String::new(),
            comments: vec![],
        }];
        app.selected = 0;
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let action = app.handle_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE));
        assert_eq!(action, KeyAction::None);
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn test_handle_key_shift_g_bottom() {
        let cfg = make_config();
        let mut app = App::new(&cfg);
        app.prs = vec![
            PullRequestSnapshot {
                number: 1,
                title: "PR 1".to_string(),
                url: "https://github.com/o/r/pull/1".to_string(),
                body: String::new(),
                is_draft: false,
                mergeable: crate::github::models::MergeableState::Mergeable,
                repo: "o/r".to_string(),
                rollup_state: None,
                checks: vec![],
                reviews: vec![],
                up_to_date: crate::github::pr::UpToDateState::Unknown,
                additions: 0,
                deletions: 0,
                created_at: String::new(),
                comments: vec![],
            },
            PullRequestSnapshot {
                number: 2,
                title: "PR 2".to_string(),
                url: "https://github.com/o/r/pull/2".to_string(),
                body: String::new(),
                is_draft: false,
                mergeable: crate::github::models::MergeableState::Mergeable,
                repo: "o/r".to_string(),
                rollup_state: None,
                checks: vec![],
                reviews: vec![],
                up_to_date: crate::github::pr::UpToDateState::Unknown,
                additions: 0,
                deletions: 0,
                created_at: String::new(),
                comments: vec![],
            },
        ];
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let action = app.handle_key(KeyEvent::new(KeyCode::Char('G'), KeyModifiers::NONE));
        assert_eq!(action, KeyAction::None);
        assert_eq!(app.selected, 1);
    }

    #[test]
    fn test_handle_key_enter() {
        let cfg = make_config();
        let mut app = App::new(&cfg);
        app.prs = vec![PullRequestSnapshot {
            number: 1,
            title: "PR 1".to_string(),
            url: "https://github.com/o/r/pull/1".to_string(),
            body: String::new(),
            is_draft: false,
            mergeable: crate::github::models::MergeableState::Mergeable,
            repo: "o/r".to_string(),
            rollup_state: None,
            checks: vec![],
            reviews: vec![],
            up_to_date: crate::github::pr::UpToDateState::Unknown,
            additions: 0,
            deletions: 0,
            created_at: String::new(),
            comments: vec![],
        }];
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let action = app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(
            action,
            KeyAction::OpenUrl("https://github.com/o/r/pull/1".to_string())
        );
    }

    #[test]
    fn test_handle_key_enter_no_prs() {
        let cfg = make_config();
        let mut app = App::new(&cfg);
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let action = app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(action, KeyAction::None);
    }

    #[test]
    fn test_handle_key_r_refresh() {
        let cfg = make_config();
        let mut app = App::new(&cfg);
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let action = app.handle_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE));
        assert_eq!(action, KeyAction::Refresh);
    }

    #[test]
    fn test_handle_key_shift_r_force_refresh() {
        let cfg = make_config();
        let mut app = App::new(&cfg);
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let action = app.handle_key(KeyEvent::new(KeyCode::Char('R'), KeyModifiers::NONE));
        assert_eq!(action, KeyAction::ForceRefresh);
    }

    #[test]
    fn test_handle_key_question_help() {
        let cfg = make_config();
        let mut app = App::new(&cfg);
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let action = app.handle_key(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE));
        assert_eq!(action, KeyAction::None);
        assert!(app.help_visible);
    }

    #[test]
    fn test_handle_key_in_help_mode() {
        let cfg = make_config();
        let mut app = App::new(&cfg);
        app.help_visible = true;
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let action = app.handle_key(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE));
        assert_eq!(action, KeyAction::None);
        assert!(!app.help_visible);
    }

    #[test]
    fn test_handle_key_unknown_in_help() {
        let cfg = make_config();
        let mut app = App::new(&cfg);
        app.help_visible = true;
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let action = app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
        assert_eq!(action, KeyAction::None);
    }

    #[test]
    fn test_handle_key_unknown_key() {
        let cfg = make_config();
        let mut app = App::new(&cfg);
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let action = app.handle_key(KeyEvent::new(KeyCode::Char('z'), KeyModifiers::NONE));
        assert_eq!(action, KeyAction::None);
    }

    #[test]
    fn test_apply_fetch_result_github_unavailable() {
        let cfg = make_config();
        let mut app = App::new(&cfg);
        app.apply_fetch_result(Err(FetchError::GitHubUnavailable));
        match &app.state {
            AppState::Error(msg) => assert!(msg.contains("GitHub unavailable")),
            _ => panic!("expected Error state"),
        }
    }

    #[test]
    fn test_apply_fetch_result_timeout() {
        let cfg = make_config();
        let mut app = App::new(&cfg);
        app.apply_fetch_result(Err(FetchError::Timeout));
        match &app.state {
            AppState::Error(msg) => assert!(msg.contains("timed out")),
            _ => panic!("expected Error state"),
        }
    }

    #[test]
    fn test_apply_fetch_result_other_error() {
        let cfg = make_config();
        let mut app = App::new(&cfg);
        app.apply_fetch_result(Err(FetchError::Network("conn refused".to_string())));
        match &app.state {
            AppState::Error(msg) => assert!(msg.contains("conn refused")),
            _ => panic!("expected Error state"),
        }
    }

    #[test]
    fn test_apply_fetch_result_ok_adjusts_selected() {
        let cfg = make_config();
        let mut app = App::new(&cfg);
        app.selected = 5;
        app.apply_fetch_result(Ok(FetchOutcome {
            login: "ska".to_string(),
            prs: vec![PullRequestSnapshot {
                number: 1,
                title: "PR".to_string(),
                url: "https://github.com/o/r/pull/1".to_string(),
                body: String::new(),
                is_draft: false,
                mergeable: crate::github::models::MergeableState::Mergeable,
                repo: "o/r".to_string(),
                rollup_state: None,
                checks: vec![],
                reviews: vec![],
                up_to_date: crate::github::pr::UpToDateState::Unknown,
                additions: 0,
                deletions: 0,
                created_at: String::new(),
                comments: vec![],
            }],
            truncated: false,
        }));
        assert_eq!(app.state, AppState::Ready);
        assert_eq!(app.selected, 0);
        assert!(!app.truncated);
    }

    #[test]
    fn test_ready_and_failed_counts() {
        let cfg = make_config();
        let mut app = App::new(&cfg);
        app.prs = vec![
            PullRequestSnapshot {
                number: 1,
                title: "Ready PR".to_string(),
                url: String::new(),
                body: String::new(),
                is_draft: false,
                mergeable: crate::github::models::MergeableState::Mergeable,
                repo: "o/r".to_string(),
                rollup_state: Some(crate::github::models::RollupState::Success),
                checks: vec![crate::github::pr::CheckSnapshot {
                    name: "CI".to_string(),
                    kind: crate::github::pr::CheckKind::CheckRun,
                    status: crate::github::pr::CheckStatus::Success,
                    started_at: None,
                    completed_at: None,
                    annotations: vec![],
                }],
                reviews: vec![crate::github::pr::ReviewSnapshot {
                    author: "a".to_string(),
                    author_is_bot: false,
                    state: crate::github::models::ReviewState::Approved,
                    body: String::new(),
                    submitted_at: None,
                }],
                up_to_date: crate::github::pr::UpToDateState::UpToDate,
                additions: 0,
                deletions: 0,
                created_at: String::new(),
                comments: vec![],
            },
            PullRequestSnapshot {
                number: 2,
                title: "Failed PR".to_string(),
                url: String::new(),
                body: String::new(),
                is_draft: false,
                mergeable: crate::github::models::MergeableState::Conflicting,
                repo: "o/r".to_string(),
                rollup_state: None,
                checks: vec![],
                reviews: vec![],
                up_to_date: crate::github::pr::UpToDateState::OutOfDate,
                additions: 0,
                deletions: 0,
                created_at: String::new(),
                comments: vec![],
            },
        ];
        assert_eq!(app.ready_count(), 1);
        assert_eq!(app.failed_count(), 1);
    }

    #[test]
    fn test_is_rate_limited() {
        let cfg = make_config();
        let mut app = App::new(&cfg);
        assert!(!app.is_rate_limited());
        app.state = AppState::RateLimited {
            retry_after_secs: 30,
        };
        assert!(app.is_rate_limited());
    }

    #[test]
    fn test_rate_limit_retry_secs() {
        let cfg = make_config();
        let mut app = App::new(&cfg);
        app.state = AppState::RateLimited {
            retry_after_secs: 30,
        };
        assert_eq!(app.rate_limit_retry_secs(), Some(30));
    }

    #[test]
    fn test_rate_limit_retry_secs_not_limited() {
        let cfg = make_config();
        let app = App::new(&cfg);
        assert_eq!(app.rate_limit_retry_secs(), None);
    }

    #[test]
    fn test_can_refresh() {
        let cfg = make_config();
        let mut app = App::new(&cfg);
        assert!(app.can_refresh());
        app.state = AppState::Refreshing;
        assert!(!app.can_refresh());
    }

    #[test]
    fn test_rate_limit_countdown_seconds() {
        let cfg = make_config();
        let mut app = App::new(&cfg);
        app.state = AppState::RateLimited {
            retry_after_secs: 45,
        };
        app.last_refresh_attempt = Some(Instant::now());
        let countdown = app.rate_limit_countdown();
        assert!(countdown.is_some());
        assert!(countdown.unwrap().contains('s'));
    }

    #[test]
    fn test_rate_limit_countdown_not_limited() {
        let cfg = make_config();
        let app = App::new(&cfg);
        assert!(app.rate_limit_countdown().is_none());
    }

    #[test]
    fn test_should_auto_refresh_after_recent_refresh() {
        let cfg = make_config();
        let mut app = App::new(&cfg);
        app.state = AppState::Ready;
        app.last_refresh_attempt = Some(Instant::now());
        assert!(!app.should_auto_refresh());
    }

    fn make_pr_simple(number: u32) -> PullRequestSnapshot {
        PullRequestSnapshot {
            number,
            title: format!("PR {number}"),
            url: format!("https://github.com/o/r/pull/{number}"),
            body: String::new(),
            is_draft: false,
            mergeable: crate::github::models::MergeableState::Mergeable,
            repo: "o/r".to_string(),
            rollup_state: None,
            checks: vec![],
            reviews: vec![],
            up_to_date: crate::github::pr::UpToDateState::Unknown,
            additions: 0,
            deletions: 0,
            created_at: String::new(),
            comments: vec![],
        }
    }

    #[test]
    fn error_with_existing_prs_stays_ready() {
        let cfg = make_config();
        let mut app = App::new(&cfg);
        // First load PRs successfully.
        app.apply_fetch_result(Ok(FetchOutcome {
            login: "ska".to_string(),
            prs: vec![make_pr_simple(1)],
            truncated: false,
        }));
        assert_eq!(app.state, AppState::Ready);
        assert!(app.last_error.is_none());
        // Then a timeout error — dashboard should stay visible.
        app.apply_fetch_result(Err(FetchError::Timeout));
        assert_eq!(app.state, AppState::Ready);
        assert!(app.last_error.is_some());
        assert_eq!(app.prs.len(), 1);
    }

    #[test]
    fn error_with_empty_prs_transitions_to_error() {
        let cfg = make_config();
        let mut app = App::new(&cfg);
        app.apply_fetch_result(Err(FetchError::Timeout));
        assert!(matches!(app.state, AppState::Error(_)));
    }

    #[test]
    fn rate_limited_always_transitions_to_rate_limited() {
        let cfg = make_config();
        let mut app = App::new(&cfg);
        // Load PRs first.
        app.apply_fetch_result(Ok(FetchOutcome {
            login: "ska".to_string(),
            prs: vec![make_pr_simple(1)],
            truncated: false,
        }));
        // Rate limit should still go to RateLimited even with PRs.
        app.apply_fetch_result(Err(FetchError::RateLimited {
            retry_after_secs: 60,
        }));
        assert!(app.is_rate_limited());
    }

    #[test]
    fn should_auto_refresh_after_error_uses_last_attempt() {
        let cfg = make_config();
        let mut app = App::new(&cfg);
        // Load PRs first.
        app.apply_fetch_result(Ok(FetchOutcome {
            login: "ska".to_string(),
            prs: vec![make_pr_simple(1)],
            truncated: false,
        }));
        // Error refresh — stays Ready, sets last_refresh_attempt.
        app.apply_fetch_result(Err(FetchError::Timeout));
        assert_eq!(app.state, AppState::Ready);
        // Should not immediately fire again — last_refresh_attempt just set.
        assert!(!app.should_auto_refresh());
    }

    #[test]
    fn format_duration_seconds() {
        assert_eq!(format_duration(0), "0s");
        assert_eq!(format_duration(45), "45s");
        assert_eq!(format_duration(59), "59s");
    }

    #[test]
    fn format_duration_minutes() {
        assert_eq!(format_duration(60), "1m");
        assert_eq!(format_duration(320), "5m20s");
        assert_eq!(format_duration(300), "5m");
    }

    #[test]
    fn format_duration_hours() {
        assert_eq!(format_duration(3600), "1h");
        assert_eq!(format_duration(3900), "1h5m");
        assert_eq!(format_duration(7200), "2h");
    }

    fn make_pr_with_repo(number: u32, repo: &str) -> PullRequestSnapshot {
        PullRequestSnapshot {
            number,
            title: format!("PR {number}"),
            url: format!("https://github.com/{repo}/pull/{number}"),
            body: String::new(),
            is_draft: false,
            mergeable: crate::github::models::MergeableState::Mergeable,
            repo: repo.to_string(),
            rollup_state: None,
            checks: vec![],
            reviews: vec![],
            up_to_date: crate::github::pr::UpToDateState::Unknown,
            additions: 0,
            deletions: 0,
            created_at: String::new(),
            comments: vec![],
        }
    }

    #[test]
    fn org_filter_keeps_only_matching_prs() {
        let mut cfg = make_config();
        cfg.org = Some("acme".to_string());
        let mut app = App::new(&cfg);
        app.apply_fetch_result(Ok(FetchOutcome {
            login: "ska".to_string(),
            prs: vec![
                make_pr_with_repo(1, "acme/api"),
                make_pr_with_repo(2, "personal/dotfiles"),
            ],
            truncated: false,
        }));
        assert_eq!(app.prs.len(), 1);
        assert_eq!(app.prs[0].repo, "acme/api");
    }

    #[test]
    fn org_filter_case_insensitive() {
        let mut cfg = make_config();
        cfg.org = Some("AcMe".to_string());
        let mut app = App::new(&cfg);
        app.apply_fetch_result(Ok(FetchOutcome {
            login: "ska".to_string(),
            prs: vec![make_pr_with_repo(1, "acme/api")],
            truncated: false,
        }));
        assert_eq!(app.prs.len(), 1);
        assert_eq!(app.prs[0].repo, "acme/api");
    }
}
