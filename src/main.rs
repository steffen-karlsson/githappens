use std::io::{self, stdout};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use clap::Parser;
use crossterm::ExecutableCommand;
use crossterm::event::{KeyCode, KeyModifiers};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use githappens::app::{App, AppState, KeyAction};
use githappens::config;
use githappens::github::client::{FetchError, FetchOutcome, GitHubFetcher, HttpGitHubFetcher};
use githappens::log;
use githappens::ui;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

#[tokio::main]
async fn main() -> Result<()> {
    let cfg = config::Config::parse();
    config::validate(&cfg)?;

    if let Some(ref token) = cfg.token {
        log::set_redaction_token(token);
    }

    let _guard = log::init(&cfg.log_level).map_err(|e| anyhow::anyhow!("{e}"))?;

    tracing::info!("githappens starting");

    let token = cfg.token.clone().unwrap_or_default();
    let fetcher = Arc::new(HttpGitHubFetcher::new(token));
    let refresh_interval = cfg.refresh;
    let max_prs = cfg.max_prs;
    let owner = cfg.owner.clone();
    let org = cfg.org.clone();

    run_tui(fetcher, refresh_interval, max_prs, owner, org).await
}
async fn run_tui(
    fetcher: Arc<HttpGitHubFetcher>,
    refresh_interval: u64,
    max_prs: usize,
    owner: Option<String>,
    org: Option<String>,
) -> Result<()> {
    setup_terminal()?;
    let result = run_app(fetcher, refresh_interval, max_prs, owner, org).await;
    restore_terminal();
    result
}

fn setup_terminal() -> Result<()> {
    enable_raw_mode()?;
    stdout().execute(crossterm::terminal::EnterAlternateScreen)?;
    Ok(())
}

fn restore_terminal() {
    let _ = disable_raw_mode();
    let _ = stdout().execute(crossterm::terminal::LeaveAlternateScreen);
}

async fn run_app(
    fetcher: Arc<HttpGitHubFetcher>,
    refresh_interval: u64,
    max_prs: usize,
    owner: Option<String>,
    org: Option<String>,
) -> Result<()> {
    let cfg = config::Config {
        token: Some(String::new()),
        refresh: refresh_interval,
        owner: owner.clone(),
        org: org.clone(),
        max_prs,
        no_color: false,
        log_level: "info".to_string(),
    };
    let mut app = App::new(&cfg).with_refresh_interval(refresh_interval);

    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;

    let (result_tx, mut result_rx) =
        tokio::sync::mpsc::unbounded_channel::<Result<FetchOutcome, FetchError>>();

    spawn_refresh(&fetcher, &owner, max_prs, &result_tx);

    let tick_interval = Duration::from_millis(250);

    loop {
        // Process events and fetch results first, so the draw reflects
        // the latest state (e.g. Refreshing spinner).
        if let Some(event) = githappens::event::read_event(tick_interval) {
            match event {
                githappens::event::Event::Key(key) => {
                    if key.code == KeyCode::Char('c') && key.modifiers == KeyModifiers::CONTROL {
                        restore_terminal();
                        std::process::exit(130);
                    }
                    let action = app.handle_key(key);
                    match action {
                        KeyAction::Quit => break,
                        KeyAction::Refresh => {
                            if app.can_refresh() {
                                app.state = AppState::Refreshing;
                                spawn_refresh(&fetcher, &owner, max_prs, &result_tx);
                            }
                        }
                        KeyAction::ForceRefresh => {
                            app.state = AppState::Refreshing;
                            spawn_refresh(&fetcher, &owner, max_prs, &result_tx);
                        }
                        KeyAction::OpenUrl(url) => {
                            let _ = githappens::browser::open(&url);
                        }
                        KeyAction::None => {}
                    }
                }
                githappens::event::Event::Tick => {
                    app.advance_spinner();
                    if app.should_auto_refresh() && app.can_refresh() {
                        app.state = AppState::Refreshing;
                        spawn_refresh(&fetcher, &owner, max_prs, &result_tx);
                    }
                }
            }
        }

        while let Ok(result) = result_rx.try_recv() {
            app.apply_fetch_result(result);
        }

        terminal.draw(|frame| ui::render(frame, &app))?;
    }

    restore_terminal();
    Ok(())
}

fn spawn_refresh(
    fetcher: &Arc<HttpGitHubFetcher>,
    owner: &Option<String>,
    max_prs: usize,
    result_tx: &tokio::sync::mpsc::UnboundedSender<Result<FetchOutcome, FetchError>>,
) {
    let fetcher = Arc::clone(fetcher);
    let owner = owner.clone();
    let tx = result_tx.clone();
    tokio::spawn(async move {
        let result = fetcher.fetch_open_prs(owner.as_deref(), max_prs).await;
        let _ = tx.send(result);
    });
}
