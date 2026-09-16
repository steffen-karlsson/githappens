# AGENTS.md

## Project overview

`githappens` is a personal, terminal-native GitHub dashboard built in Rust with [`ratatui`](https://github.com/ratatui/ratatui). It lists your open GitHub pull requests in a TUI table with merge-readiness indicators, diff stats, workflow status, approval state, up-to-date tracking, and PR age.

- **Binary name**: `githappens`
- **Tagline**: *"Git happens. Now you can see it."*
- **PRD**: `docs/PRD.md`

## Build & test commands

- **Build**: `make build` or `cargo build`
- **Run**: `make run` or `cargo run`
- **Test**: `make test` or `cargo test`
- **Lint**: `make lint` (runs fmt-check + clippy)
- **Format**: `make fmt` or `cargo fmt`
- **Format check**: `make fmt-check` or `cargo fmt --check`
- **Clippy**: `make clippy` or `cargo clippy -- -D warnings`
- **Clippy (all targets)**: `cargo clippy --all-targets -- -D warnings`
- **Coverage**: `make coverage` or `cargo llvm-cov --workspace`
- **Audit**: `make audit` or `cargo audit`
- **CI (all)**: `make ci` (fmt-check + clippy + test + audit)
- **Clean**: `make clean`

## Architecture

```
src/
├── main.rs              # entrypoint, async TUI run loop, terminal setup/restore
├── lib.rs               # re-exports all modules for integration tests
├── app.rs               # App struct, state machine, key handling, fetch result handling
├── config.rs            # token resolution, CLI args, validation (--no-color, --log-level)
├── log.rs               # tracing setup with token redaction layer
├── event.rs             # crossterm → typed Event; poll-based read with timeout
├── browser.rs           # cross-platform open-URL via webbrowser
├── ui/
│   ├── mod.rs           # render entrypoint
│   ├── dashboard.rs     # the PR list table, header, footer, loading state
│   ├── help_overlay.rs  # ? overlay with keybindings and legend
│   ├── error_screen.rs  # error/rate-limited full-screen messages
│   └── theme.rs         # colors, glyphs, column widths, helper functions
├── github/
│   ├── mod.rs
│   ├── client.rs        # GraphQL POST client, REST mergeable_state fetch, pagination, retries
│   ├── query.graphql    # embedded via include_str!
│   ├── models.rs        # serde structs for GraphQL response (DTOs only)
│   └── pr.rs            # PullRequestSnapshot domain type, UpToDateState, DTO→domain
└── analysis/
    ├── mod.rs
    ├── mergeability.rs  # ready/waiting/failed state machine (includes up-to-date)
    ├── workflows.rs     # count completed/total, render with cap and dash
    └── approval.rs      # collapse reviews → approval state (last-wins-per-author)
```

### Module responsibilities

- `config` — token/args only; no I/O.
- `log` — tracing setup + token redaction; never logs the token.
- `github::client` — HTTP transport only (GraphQL + REST); no domain logic.
- `github::models` — DTOs only; no business rules.
- `github::pr` — pure translation DTO → domain `PullRequestSnapshot`.
- `analysis::*` — pure functions; no I/O. This is where 80% of tests live.
- `ui::*` — pure functions taking `&Frame` and `&App`. Side-effect-free.
- `app` — state machine; handles key events, applies fetch results, tracks refresh state.
- `main.rs` — wires `HttpGitHubFetcher`, spawns non-blocking refresh tasks via mpsc channel.

### Non-blocking refresh architecture

Refreshes are spawned as background `tokio::spawn` tasks. The main event loop
continues rendering (showing the spinner) while the fetch runs. Results are
communicated back via an `mpsc::unbounded_channel` and applied with
`App::apply_fetch_result()` on the next tick.

### Trait-based seams for testing

`App::apply_fetch_result(result)` decouples the app from the fetcher. `main.rs` wires `HttpGitHubFetcher`. Tests use `MockGitHubFetcher` or `ScriptedFetcher`.

### State machine

States: `Loading`, `Ready`, `Refreshing`, `Error`, `RateLimited`, `Help`.

- `Loading` → initial state, shows spinner
- `Ready` → dashboard visible, PRs loaded
- `Refreshing` → spinner in header, background fetch in progress
- `Error` → full-screen error message, press `r` to retry
- `RateLimited` → full-screen rate-limit countdown
- `Help` → help overlay (toggled with `?`)

## Merge-readiness model

The status indicator is the **worst** of all constituent signals:

| State | Conditions | Glyph | Color |
|-------|-----------|-------|-------|
| **Ready** | `mergeable == MERGEABLE` AND checks pass AND approved AND up-to-date AND not draft | `●` | green |
| **Waiting** | Not failed, not ready (pending checks, no approval, draft) | `●` | yellow |
| **Failed** | Checks failed OR changes requested OR `mergeable == CONFLICTING` OR `mergeable == UNKNOWN` OR out-of-date | `●` | red |

### Up-to-date tracking

Per-PR `mergeable_state` fetched via REST API (`/repos/{repo}/pulls/{number}`):
- `clean`, `unstable`, `has_hooks`, `blocked` → up-to-date (green)
- `behind`, `dirty` → out-of-date (red)
- API failure or unrecognized state → unknown (grey circle)

Out-of-date PRs are treated as **Failed** in the merge-readiness assessment.

### Workflow completion column

- `completed / total` from `statusCheckRollup.contexts`
- `–/–` when no checks exist (null rollup)
- `99+` cap when count exceeds 99
- CheckRun: `status == COMPLETED` counts as completed
- StatusContext: `state in (SUCCESS, ERROR, FAILURE)` counts as completed

### Approval column

Collapsed from reviews list — latest review per author (last wins). `DISMISSED`
reviews dropped before collapse. Bot authors with null login collapse by empty
string.

## Conventions

- Conventional commits for all commits.
- Clippy lints `unwrap_used`, `expect_used`, `dbg_macro`, `print_stdout`, `print_stderr` are denied in Cargo.toml.
- Coverage reported as a PR comment (no hard gate).
- No `unwrap()` or `expect()` in library code (use `?` or typed errors). `unwrap` is allowed in `#[cfg(test)]` modules via `#[allow(clippy::unwrap_used)]`.
- Token must never appear in logs, errors, or panic payloads. A `RedactingWriter` in `log.rs` scrubs the token from all log output.
- `anyhow` only at `main.rs`/CLI boundary; library code uses `thiserror` typed errors.
- `rustls-tls` (no openssl) for static binary and cross-platform builds.

## CLI flags

| Flag | Env | Default | Description |
|------|-----|---------|-------------|
| `--token <T>` | `GIT_TOKEN` | — | GitHub PAT (required) |
| `--refresh <secs>` | — | `300` | Auto-refresh interval (min 30) |
| `--owner <login>` | — | token owner | Override "me" viewer |
| `--max-prs <N>` | — | `500` | Max PRs to fetch (hard cap 1000) |
| `--no-color` | `NO_COLOR` | `false` | Disable colored output |
| `--log-level <L>` | `RUST_LOG` | `info` | Log level (trace/debug/info/warn/error) |

## GitHub API

### GraphQL (PR list + checks + reviews)

- Uses **GraphQL v4** with a single batched query (one round-trip per page).
- Query is embedded via `include_str!("query.graphql")`.
- Endpoint: `https://api.github.com/graphql`
- Pagination via `hasNextPage` / `endCursor` up to `--max-prs` (hard stop at 1000).
- Retry once on 502/503/504 with 1s backoff. No retry on 401/403/422.
- HTTP timeout: 15s.
- Honors `X-RateLimit-Remaining` / `Retry-After`.
- Null PR nodes filtered (GraphQL may return null in `nodes` array).

### REST (up-to-date state)

- After GraphQL fetch, per-PR REST call to `/repos/{repo}/pulls/{number}`
- Reads `mergeable_state` field to determine up-to-date status
- Fetched concurrently via `futures::future::join_all`
- Failures degrade gracefully to `Unknown` (grey circle)

## Dependencies

| Crate | Purpose |
|-------|---------|
| `ratatui` + `crossterm` | TUI rendering + terminal I/O |
| `tokio` | Async runtime |
| `futures` | Concurrent REST calls for mergeable_state |
| `reqwest` (rustls) | HTTP client for GraphQL + REST |
| `serde` / `serde_json` | Serialization |
| `clap` | CLI argument parsing |
| `thiserror` | Typed errors in library code |
| `anyhow` | Top-level error context in main.rs |
| `webbrowser` | Open PR URLs in default browser |
| `tracing` + `tracing-subscriber` + `tracing-appender` | Structured logging with redaction |
| `async-trait` | `GitHubFetcher` trait |
| `directories` | Platform log directory resolution |

### Dev dependencies

| Crate | Purpose |
|-------|---------|
| `rstest` | Parametric tests for analysis modules |
| `wiremock` | Mock GraphQL server for client tests |
| `pretty_assertions` | Better assert_eq output |
| `assert_cmd` | CLI integration tests |
| `insta` | Snapshot tests for rendered UI |
