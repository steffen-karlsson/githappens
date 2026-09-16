#![allow(clippy::unwrap_used, non_snake_case)]

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use githappens::app::{App, AppState, KeyAction};
use githappens::config::Config;
use githappens::github::client::{FetchError, FetchOutcome, GitHubFetcher};
use githappens::github::models::MergeableState;
use githappens::github::pr::PullRequestSnapshot;

struct ScriptedFetcher {
    outcomes: Vec<Result<FetchOutcome, FetchError>>,
    call_count: std::sync::Mutex<usize>,
}

impl ScriptedFetcher {
    fn new(outcomes: Vec<Result<FetchOutcome, FetchError>>) -> Self {
        Self {
            outcomes,
            call_count: std::sync::Mutex::new(0),
        }
    }
}

#[async_trait::async_trait]
impl GitHubFetcher for ScriptedFetcher {
    async fn fetch_open_prs(
        &self,
        _owner: Option<&str>,
        _max: usize,
    ) -> Result<FetchOutcome, FetchError> {
        let mut count = self.call_count.lock().unwrap();
        let idx = *count;
        *count += 1;
        if idx < self.outcomes.len() {
            self.outcomes[idx].clone()
        } else {
            self.outcomes.last().cloned().unwrap_or_else(|| {
                Err(FetchError::Network(
                    "no more scripted responses".to_string(),
                ))
            })
        }
    }
}

fn make_config() -> Config {
    Config {
        token: Some("ghp_test".to_string()),
        refresh: 300,
        owner: None,
        max_prs: 500,
        no_color: false,
        log_level: "info".to_string(),
    }
}

fn make_pr(number: u32, title: &str) -> PullRequestSnapshot {
    PullRequestSnapshot {
        number,
        title: title.to_string(),
        url: format!("https://github.com/o/r/pull/{number}"),
        body: String::new(),
        is_draft: false,
        mergeable: MergeableState::Mergeable,
        repo: "o/r".to_string(),
        rollup_state: None,
        checks: vec![],
        reviews: vec![],
        up_to_date: githappens::github::pr::UpToDateState::Unknown,
        additions: 0,
        deletions: 0,
        created_at: String::new(),
        comments: vec![],
    }
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

#[tokio::test]
async fn quit_on_q() {
    let cfg = make_config();
    let mut app = App::new(&cfg);
    let action = app.handle_key(key(KeyCode::Char('q')));
    assert_eq!(action, KeyAction::Quit);
}

#[tokio::test]
async fn quit_on_esc() {
    let cfg = make_config();
    let mut app = App::new(&cfg);
    let action = app.handle_key(key(KeyCode::Esc));
    assert_eq!(action, KeyAction::Quit);
}

#[tokio::test]
async fn enter_opens_url() {
    let cfg = make_config();
    let mut app = App::new(&cfg);
    app.prs = vec![make_pr(42, "Add feature")];
    let action = app.handle_key(key(KeyCode::Enter));
    match action {
        KeyAction::OpenUrl(url) => assert!(url.contains("pull/42")),
        _ => panic!("expected OpenUrl"),
    }
}

#[tokio::test]
async fn enter_empty_state_no_op() {
    let cfg = make_config();
    let mut app = App::new(&cfg);
    let action = app.handle_key(key(KeyCode::Enter));
    assert_eq!(action, KeyAction::None);
}

#[tokio::test]
async fn j_k_navigation_moves_selection() {
    let cfg = make_config();
    let mut app = App::new(&cfg);
    app.prs = vec![make_pr(1, "A"), make_pr(2, "B"), make_pr(3, "C")];
    assert_eq!(app.selected, 0);
    app.handle_key(key(KeyCode::Char('j')));
    assert_eq!(app.selected, 1);
    app.handle_key(key(KeyCode::Char('j')));
    assert_eq!(app.selected, 2);
    app.handle_key(key(KeyCode::Char('k')));
    assert_eq!(app.selected, 1);
}

#[tokio::test]
async fn arrow_keys_navigate() {
    let cfg = make_config();
    let mut app = App::new(&cfg);
    app.prs = vec![make_pr(1, "A"), make_pr(2, "B")];
    app.handle_key(key(KeyCode::Down));
    assert_eq!(app.selected, 1);
    app.handle_key(key(KeyCode::Up));
    assert_eq!(app.selected, 0);
}

#[tokio::test]
async fn g_G_top_bottom() {
    let cfg = make_config();
    let mut app = App::new(&cfg);
    app.prs = vec![make_pr(1, "A"), make_pr(2, "B"), make_pr(3, "C")];
    app.handle_key(key(KeyCode::Char('G')));
    assert_eq!(app.selected, 2);
    app.handle_key(key(KeyCode::Char('g')));
    assert_eq!(app.selected, 0);
}

#[tokio::test]
async fn clamp_at_bottom() {
    let cfg = make_config();
    let mut app = App::new(&cfg);
    app.prs = vec![make_pr(1, "A")];
    app.handle_key(key(KeyCode::Char('j')));
    assert_eq!(app.selected, 0);
    app.handle_key(key(KeyCode::Down));
    assert_eq!(app.selected, 0);
}

#[tokio::test]
async fn r_triggers_refresh() {
    let cfg = make_config();
    let mut app = App::new(&cfg);
    let action = app.handle_key(key(KeyCode::Char('r')));
    assert_eq!(action, KeyAction::Refresh);
}

#[tokio::test]
async fn R_triggers_force_refresh() {
    let cfg = make_config();
    let mut app = App::new(&cfg);
    let action = app.handle_key(key(KeyCode::Char('R')));
    assert_eq!(action, KeyAction::ForceRefresh);
}

#[tokio::test]
async fn question_toggle_help() {
    let cfg = make_config();
    let mut app = App::new(&cfg);
    assert!(!app.help_visible);
    app.handle_key(key(KeyCode::Char('?')));
    assert!(app.help_visible);
    app.handle_key(key(KeyCode::Char('?')));
    assert!(!app.help_visible);
}

#[tokio::test]
async fn refresh_transitions_ready_to_refreshing_to_ready() {
    let cfg = make_config();
    let mut app = App::new(&cfg);
    let fetcher = ScriptedFetcher::new(vec![Ok(FetchOutcome {
        login: "ska".to_string(),
        prs: vec![make_pr(1, "Test")],
        truncated: false,
    })]);
    let result = fetcher.fetch_open_prs(None, 500).await;
    app.apply_fetch_result(result);
    assert_eq!(app.state, AppState::Ready);
    assert_eq!(app.prs.len(), 1);
}

#[tokio::test]
async fn refresh_after_rate_limit_returns_to_ready() {
    let cfg = make_config();
    let mut app = App::new(&cfg);
    let fetcher = ScriptedFetcher::new(vec![
        Err(FetchError::RateLimited {
            retry_after_secs: 60,
        }),
        Ok(FetchOutcome {
            login: "ska".to_string(),
            prs: vec![make_pr(1, "Recovery")],
            truncated: false,
        }),
    ]);
    let result = fetcher.fetch_open_prs(None, 500).await;
    app.apply_fetch_result(result);
    assert!(app.is_rate_limited());
    let result = fetcher.fetch_open_prs(None, 500).await;
    app.apply_fetch_result(result);
    assert_eq!(app.state, AppState::Ready);
    assert_eq!(app.prs.len(), 1);
}

#[tokio::test]
async fn empty_state_renders() {
    let cfg = make_config();
    let mut app = App::new(&cfg);
    let fetcher = ScriptedFetcher::new(vec![Ok(FetchOutcome {
        login: "ska".to_string(),
        prs: vec![],
        truncated: false,
    })]);
    let result = fetcher.fetch_open_prs(None, 500).await;
    app.apply_fetch_result(result);
    assert_eq!(app.state, AppState::Ready);
    assert!(app.prs.is_empty());
}

#[tokio::test]
async fn refresh_while_refreshing_ignored() {
    let cfg = make_config();
    let mut app = App::new(&cfg);
    app.state = AppState::Refreshing;
    let action = app.handle_key(key(KeyCode::Char('r')));
    assert_eq!(action, KeyAction::Refresh);
}

#[tokio::test]
async fn token_not_in_error_message() {
    let cfg = make_config();
    let mut app = App::new(&cfg);
    let fetcher = ScriptedFetcher::new(vec![Err(FetchError::TokenInvalid)]);
    let result = fetcher.fetch_open_prs(None, 500).await;
    app.apply_fetch_result(result);
    match &app.state {
        AppState::Error(msg) => {
            assert!(!msg.contains("ghp_test"));
        }
        _ => panic!("expected Error state"),
    }
}
