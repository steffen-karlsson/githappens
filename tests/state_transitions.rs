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
        org: None,
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

#[tokio::test]
async fn error_after_successful_load_stays_ready_with_stale_prs() {
    let cfg = make_config();
    let mut app = App::new(&cfg);
    let fetcher = ScriptedFetcher::new(vec![
        Ok(FetchOutcome {
            login: "ska".to_string(),
            prs: vec![make_pr(1, "Test PR")],
            truncated: false,
        }),
        Err(FetchError::Timeout),
    ]);
    // First fetch succeeds.
    let result = fetcher.fetch_open_prs(None, 500).await;
    app.apply_fetch_result(result);
    assert_eq!(app.state, AppState::Ready);
    assert_eq!(app.prs.len(), 1);
    // Second fetch times out — should stay Ready with stale PRs.
    let result = fetcher.fetch_open_prs(None, 500).await;
    app.apply_fetch_result(result);
    assert_eq!(app.state, AppState::Ready);
    assert_eq!(app.prs.len(), 1);
    assert!(app.last_error.is_some());
}

#[tokio::test]
async fn lifecycle_success_error_recovery() {
    let cfg = make_config();
    let mut app = App::new(&cfg).with_refresh_interval(1);
    let fetcher = ScriptedFetcher::new(vec![
        // Initial load succeeds.
        Ok(FetchOutcome {
            login: "ska".to_string(),
            prs: vec![make_pr(1, "PR One")],
            truncated: false,
        }),
        // Auto-refresh fails (timeout) — should stay Ready with stale data.
        Err(FetchError::Timeout),
        // Next auto-refresh succeeds — error cleared, data refreshed.
        Ok(FetchOutcome {
            login: "ska".to_string(),
            prs: vec![make_pr(2, "PR Two")],
            truncated: false,
        }),
    ]);

    // Step 1: initial load.
    app.state = AppState::Refreshing;
    let result = fetcher.fetch_open_prs(None, 500).await;
    app.apply_fetch_result(result);
    assert_eq!(app.state, AppState::Ready);
    assert_eq!(app.prs.len(), 1);
    assert_eq!(app.prs[0].number, 1);
    assert!(app.last_error.is_none());

    // Wait for interval to pass, then auto-refresh fires.
    std::thread::sleep(std::time::Duration::from_millis(1100));
    // Step 2: auto-refresh fires, times out — stays Ready with stale PRs.
    assert!(app.should_auto_refresh());
    app.state = AppState::Refreshing;
    let result = fetcher.fetch_open_prs(None, 500).await;
    app.apply_fetch_result(result);
    assert_eq!(app.state, AppState::Ready);
    assert_eq!(app.prs.len(), 1); // stale data still visible
    assert_eq!(app.prs[0].number, 1); // old PR, not replaced
    assert!(app.last_error.is_some());
    assert!(app.last_error.as_ref().unwrap().contains("timed out"));

    // Wait for interval to pass again.
    std::thread::sleep(std::time::Duration::from_millis(1100));
    // Step 3: auto-refresh fires again, succeeds — error cleared.
    assert!(app.should_auto_refresh());
    app.state = AppState::Refreshing;
    let result = fetcher.fetch_open_prs(None, 500).await;
    app.apply_fetch_result(result);
    assert_eq!(app.state, AppState::Ready);
    assert_eq!(app.prs.len(), 1);
    assert_eq!(app.prs[0].number, 2); // new data
    assert!(app.last_error.is_none());
}

#[tokio::test]
async fn lifecycle_rate_limit_always_shows_countdown() {
    let cfg = make_config();
    let mut app = App::new(&cfg);
    let fetcher = ScriptedFetcher::new(vec![
        // Initial load succeeds.
        Ok(FetchOutcome {
            login: "ska".to_string(),
            prs: vec![make_pr(1, "PR One")],
            truncated: false,
        }),
        // Rate limited — should transition to RateLimited even with PRs.
        Err(FetchError::RateLimited {
            retry_after_secs: 60,
        }),
        // Recovery after rate limit.
        Ok(FetchOutcome {
            login: "ska".to_string(),
            prs: vec![make_pr(1, "PR One Updated")],
            truncated: false,
        }),
    ]);

    // Step 1: initial load.
    let result = fetcher.fetch_open_prs(None, 500).await;
    app.apply_fetch_result(result);
    assert_eq!(app.state, AppState::Ready);
    assert_eq!(app.prs.len(), 1);

    // Step 2: rate limited — full-screen countdown, not stay-in-Ready.
    let result = fetcher.fetch_open_prs(None, 500).await;
    app.apply_fetch_result(result);
    assert!(app.is_rate_limited());
    assert!(app.rate_limit_countdown().is_some());

    // Step 3: recovery — back to Ready.
    let result = fetcher.fetch_open_prs(None, 500).await;
    app.apply_fetch_result(result);
    assert_eq!(app.state, AppState::Ready);
    assert_eq!(app.prs[0].title, "PR One Updated");
    assert!(app.last_error.is_none());
}

#[tokio::test]
async fn lifecycle_error_with_no_prs_shows_full_error_screen() {
    let cfg = make_config();
    let mut app = App::new(&cfg);
    let fetcher = ScriptedFetcher::new(vec![
        // First fetch fails with no prior data — should show Error screen.
        Err(FetchError::GitHubUnavailable),
        // Retry succeeds.
        Ok(FetchOutcome {
            login: "ska".to_string(),
            prs: vec![make_pr(1, "Recovery")],
            truncated: false,
        }),
    ]);

    // Step 1: error with no PRs — full-screen Error.
    let result = fetcher.fetch_open_prs(None, 500).await;
    app.apply_fetch_result(result);
    assert!(matches!(app.state, AppState::Error(_)));

    // Step 2: manual retry (simulated by pressing 'r').
    let action = app.handle_key(key(KeyCode::Char('r')));
    assert_eq!(action, KeyAction::Refresh);
    app.state = AppState::Refreshing;
    let result = fetcher.fetch_open_prs(None, 500).await;
    app.apply_fetch_result(result);
    assert_eq!(app.state, AppState::Ready);
    assert_eq!(app.prs.len(), 1);
}

#[tokio::test]
async fn lifecycle_consecutive_errors_keep_stale_data() {
    let cfg = make_config();
    let mut app = App::new(&cfg).with_refresh_interval(1);
    let fetcher = ScriptedFetcher::new(vec![
        // Initial load.
        Ok(FetchOutcome {
            login: "ska".to_string(),
            prs: vec![make_pr(1, "PR One"), make_pr(2, "PR Two")],
            truncated: false,
        }),
        // Error 1: timeout.
        Err(FetchError::Timeout),
        // Error 2: network failure.
        Err(FetchError::Network("connection reset".to_string())),
        // Error 3: GitHub unavailable.
        Err(FetchError::GitHubUnavailable),
        // Recovery.
        Ok(FetchOutcome {
            login: "ska".to_string(),
            prs: vec![make_pr(3, "PR Three")],
            truncated: false,
        }),
    ]);

    // Initial load.
    let result = fetcher.fetch_open_prs(None, 500).await;
    app.apply_fetch_result(result);
    assert_eq!(app.prs.len(), 2);

    // Three consecutive errors — each stays Ready with stale 2 PRs.
    for _ in 0..3 {
        std::thread::sleep(std::time::Duration::from_millis(1100));
        assert!(app.should_auto_refresh());
        app.state = AppState::Refreshing;
        let result = fetcher.fetch_open_prs(None, 500).await;
        app.apply_fetch_result(result);
        assert_eq!(app.state, AppState::Ready);
        assert_eq!(app.prs.len(), 2); // stale data preserved
        assert!(app.last_error.is_some());
    }

    // Wait for interval, then recovery.
    std::thread::sleep(std::time::Duration::from_millis(1100));
    // Recovery — new data, error cleared.
    assert!(app.should_auto_refresh());
    app.state = AppState::Refreshing;
    let result = fetcher.fetch_open_prs(None, 500).await;
    app.apply_fetch_result(result);
    assert_eq!(app.state, AppState::Ready);
    assert_eq!(app.prs.len(), 1);
    assert_eq!(app.prs[0].number, 3);
    assert!(app.last_error.is_none());
}

#[tokio::test]
async fn lifecycle_auto_refresh_uses_last_attempt_not_last_success() {
    let cfg = make_config();
    let mut app = App::new(&cfg).with_refresh_interval(2);
    let fetcher = ScriptedFetcher::new(vec![
        // Initial success.
        Ok(FetchOutcome {
            login: "ska".to_string(),
            prs: vec![make_pr(1, "PR One")],
            truncated: false,
        }),
        // Error — sets last_refresh_attempt.
        Err(FetchError::Timeout),
    ]);

    // Initial load.
    let result = fetcher.fetch_open_prs(None, 500).await;
    app.apply_fetch_result(result);
    assert_eq!(app.state, AppState::Ready);

    // Error — stays Ready, sets last_refresh_attempt to now.
    app.state = AppState::Refreshing;
    let result = fetcher.fetch_open_prs(None, 500).await;
    app.apply_fetch_result(result);
    assert_eq!(app.state, AppState::Ready);
    assert!(app.last_error.is_some());

    // should_auto_refresh should be false — last_refresh_attempt was just set.
    assert!(!app.should_auto_refresh());

    // Wait for interval to pass.
    std::thread::sleep(std::time::Duration::from_millis(2100));
    assert!(app.should_auto_refresh());
}

#[tokio::test]
async fn lifecycle_manual_refresh_during_countdown() {
    let cfg = make_config();
    let mut app = App::new(&cfg).with_refresh_interval(300);
    let fetcher = ScriptedFetcher::new(vec![
        // Initial load.
        Ok(FetchOutcome {
            login: "ska".to_string(),
            prs: vec![make_pr(1, "PR One")],
            truncated: false,
        }),
        // Manual refresh — new data.
        Ok(FetchOutcome {
            login: "ska".to_string(),
            prs: vec![make_pr(1, "PR One Updated")],
            truncated: false,
        }),
    ]);

    // Initial load.
    let result = fetcher.fetch_open_prs(None, 500).await;
    app.apply_fetch_result(result);
    assert_eq!(app.state, AppState::Ready);

    // Auto-refresh should not fire (interval is 300s).
    assert!(!app.should_auto_refresh());

    // Manual refresh via 'r' key — should transition to Refreshing.
    let action = app.handle_key(key(KeyCode::Char('r')));
    assert_eq!(action, KeyAction::Refresh);
    assert!(app.can_refresh());
    app.state = AppState::Refreshing;
    let result = fetcher.fetch_open_prs(None, 500).await;
    app.apply_fetch_result(result);
    assert_eq!(app.state, AppState::Ready);
    assert_eq!(app.prs[0].title, "PR One Updated");
    assert!(app.last_error.is_none());
}

#[tokio::test]
async fn lifecycle_token_invalid_with_existing_prs_stays_ready() {
    let cfg = make_config();
    let mut app = App::new(&cfg);
    let fetcher = ScriptedFetcher::new(vec![
        // Initial load.
        Ok(FetchOutcome {
            login: "ska".to_string(),
            prs: vec![make_pr(1, "PR One")],
            truncated: false,
        }),
        // Token expired after initial load — stays Ready with stale data.
        Err(FetchError::TokenInvalid),
    ]);

    // Initial load.
    let result = fetcher.fetch_open_prs(None, 500).await;
    app.apply_fetch_result(result);
    assert_eq!(app.state, AppState::Ready);

    // Token invalid — with existing PRs, stays Ready.
    let result = fetcher.fetch_open_prs(None, 500).await;
    app.apply_fetch_result(result);
    assert_eq!(app.state, AppState::Ready);
    assert_eq!(app.prs.len(), 1);
    assert!(app.last_error.is_some());
    assert!(app.last_error.as_ref().unwrap().contains("Token invalid"));
}
