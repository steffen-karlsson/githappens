use std::time::Duration;

use async_trait::async_trait;
use reqwest::{Client, StatusCode, header};
use serde::Deserialize;
use serde_json::json;
use thiserror::Error;

use crate::github::models::{GraphQLResponse, PullRequestNode};
use crate::github::pr::{self, PullRequestSnapshot, UpToDateState};

const GRAPHQL_ENDPOINT: &str = "https://api.github.com/graphql";
const REST_API_BASE: &str = "https://api.github.com";
const HTTP_TIMEOUT: Duration = Duration::from_secs(15);
const RETRY_BACKOFF: Duration = Duration::from_secs(1);

const QUERY: &str = include_str!("query.graphql");

#[derive(Debug, Clone, Error)]
pub enum FetchError {
    #[error("token invalid or expired")]
    TokenInvalid,
    #[error("GitHub unavailable, try again")]
    GitHubUnavailable,
    #[error("request timed out")]
    Timeout,
    #[error("rate limited, retry after {retry_after_secs}s")]
    RateLimited { retry_after_secs: u64 },
    #[error("malformed response from GitHub: {0}")]
    Parse(String),
    #[error("GraphQL errors: {0}")]
    GraphQLErrors(String),
    #[error("network error: {0}")]
    Network(String),
}

impl From<reqwest::Error> for FetchError {
    fn from(e: reqwest::Error) -> Self {
        if e.is_timeout() {
            FetchError::Timeout
        } else {
            FetchError::Network(e.to_string())
        }
    }
}

#[derive(Debug, Clone)]
pub struct FetchOutcome {
    pub login: String,
    pub prs: Vec<PullRequestSnapshot>,
    pub truncated: bool,
}

#[async_trait]
pub trait GitHubFetcher: Send + Sync {
    async fn fetch_open_prs(
        &self,
        owner: Option<&str>,
        max: usize,
    ) -> Result<FetchOutcome, FetchError>;
}

pub struct HttpGitHubFetcher {
    client: Client,
    token: String,
    endpoint: String,
    rest_base: String,
}

impl HttpGitHubFetcher {
    pub fn new(token: String) -> Self {
        Self::new_with_endpoint(token, GRAPHQL_ENDPOINT.to_string())
    }

    pub fn new_with_endpoint(token: String, endpoint: String) -> Self {
        let rest_base = endpoint
            .strip_suffix("/graphql")
            .unwrap_or(REST_API_BASE)
            .to_string();
        let client = Client::builder()
            .timeout(HTTP_TIMEOUT)
            .user_agent("githappens")
            .build()
            .unwrap_or_default();
        Self {
            client,
            token,
            endpoint,
            rest_base,
        }
    }

    async fn fetch_mergeable_state(&self, repo: &str, number: u32) -> UpToDateState {
        let url = format!("{}/repos/{repo}/pulls/{number}", self.rest_base);
        let result = self
            .client
            .get(&url)
            .header(header::AUTHORIZATION, format!("Bearer {}", self.token))
            .header(header::ACCEPT, "application/vnd.github+json")
            .send()
            .await;

        let response = match result {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("fetch_mergeable_state failed for {repo}#{number}: {e}");
                return UpToDateState::Unknown;
            }
        };

        if !response.status().is_success() {
            tracing::warn!(
                "fetch_mergeable_state {repo}#{number} returned {}",
                response.status()
            );
            return UpToDateState::Unknown;
        }

        let text = match response.text().await {
            Ok(t) => t,
            Err(_) => return UpToDateState::Unknown,
        };

        #[derive(Deserialize)]
        struct PrRestResponse {
            #[serde(default)]
            mergeable_state: Option<String>,
        }

        let parsed: PrRestResponse = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return UpToDateState::Unknown,
        };

        match parsed.mergeable_state.as_deref() {
            Some("clean") | Some("unstable") | Some("has_hooks") | Some("blocked") => {
                UpToDateState::UpToDate
            }
            Some("behind") | Some("dirty") => UpToDateState::OutOfDate,
            _ => UpToDateState::Unknown,
        }
    }

    async fn fetch_page(
        &self,
        first: usize,
        after: Option<&str>,
    ) -> Result<(String, Vec<PullRequestNode>, bool, Option<String>), FetchError> {
        let variables = json!({
            "first": first,
            "after": after,
        });

        let body = json!({
            "query": QUERY,
            "variables": variables,
        });

        let response = self
            .client
            .post(&self.endpoint)
            .header(header::AUTHORIZATION, format!("Bearer {}", self.token))
            .header(header::ACCEPT, "application/vnd.github+json")
            .json(&body)
            .send()
            .await?;

        let status = response.status();

        if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
            let remaining = response
                .headers()
                .get("x-ratelimit-remaining")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok());

            if remaining == Some(0) {
                let retry_after = response
                    .headers()
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse::<u64>().ok())
                    .unwrap_or(60);
                return Err(FetchError::RateLimited {
                    retry_after_secs: retry_after,
                });
            }
            return Err(FetchError::TokenInvalid);
        }

        if status.is_server_error() {
            return Err(FetchError::GitHubUnavailable);
        }

        if status == StatusCode::UNPROCESSABLE_ENTITY {
            return Err(FetchError::TokenInvalid);
        }

        let text = response.text().await?;
        let resp: GraphQLResponse =
            serde_json::from_str(&text).map_err(|e| FetchError::Parse(e.to_string()))?;

        let data = match resp.data {
            Some(d) => d,
            None => {
                if let Some(errors) = resp.errors {
                    return Err(FetchError::GraphQLErrors(
                        errors
                            .iter()
                            .map(|e| e.message.clone())
                            .collect::<Vec<_>>()
                            .join("; "),
                    ));
                }
                return Err(FetchError::Parse("missing data field".to_string()));
            }
        };

        if let Some(errors) = resp.errors {
            tracing::warn!(
                "GraphQL partial errors: {}",
                errors
                    .iter()
                    .map(|e| e.message.clone())
                    .collect::<Vec<_>>()
                    .join("; ")
            );
        }

        let login = data.viewer.login;
        let prs: Vec<PullRequestNode> = data
            .viewer
            .pull_requests
            .nodes
            .into_iter()
            .flatten()
            .collect();
        let has_next = data.viewer.pull_requests.page_info.has_next_page;
        let end_cursor = data.viewer.pull_requests.page_info.end_cursor;

        Ok((login, prs, has_next, end_cursor))
    }
}

#[async_trait]
impl GitHubFetcher for HttpGitHubFetcher {
    async fn fetch_open_prs(
        &self,
        _owner: Option<&str>,
        max: usize,
    ) -> Result<FetchOutcome, FetchError> {
        let hard_cap = max.min(1000);
        let mut all_prs: Vec<PullRequestNode> = Vec::new();
        let mut login = String::new();
        let mut after: Option<String> = None;
        let page_size = 100usize;
        let mut truncated = false;

        loop {
            let remaining = hard_cap.saturating_sub(all_prs.len());
            if remaining == 0 {
                break;
            }

            let fetch_size = page_size.min(remaining);
            let (page_login, nodes, has_next, end_cursor) =
                match self.fetch_page(fetch_size, after.as_deref()).await {
                    Ok(result) => result,
                    Err(FetchError::GitHubUnavailable) => {
                        tokio::time::sleep(RETRY_BACKOFF).await;
                        self.fetch_page(fetch_size, after.as_deref()).await?
                    }
                    Err(e) => return Err(e),
                };
            login = page_login;

            for node in nodes {
                if !all_prs.iter().any(|p| p.number == node.number) {
                    all_prs.push(node);
                }
            }

            if all_prs.len() >= hard_cap {
                truncated = all_prs.len() >= max && max < 1000;
                break;
            }

            if !has_next {
                break;
            }

            after = end_cursor;
            if after.is_none() {
                break;
            }
        }

        if all_prs.len() >= 1000 && max >= 1000 {
            truncated = true;
        }

        let mut prs: Vec<PullRequestSnapshot> = all_prs.iter().map(pr::from_dto).collect();

        let up_to_date_futures: Vec<_> = prs
            .iter()
            .map(|p| self.fetch_mergeable_state(&p.repo, p.number))
            .collect();
        let up_to_date_results = futures::future::join_all(up_to_date_futures).await;
        for (pr, state) in prs.iter_mut().zip(up_to_date_results) {
            pr.up_to_date = state;
        }

        Ok(FetchOutcome {
            login,
            prs,
            truncated,
        })
    }
}

pub struct MockGitHubFetcher {
    pub outcome: Result<FetchOutcome, FetchError>,
}

#[async_trait]
impl GitHubFetcher for MockGitHubFetcher {
    async fn fetch_open_prs(
        &self,
        _owner: Option<&str>,
        _max: usize,
    ) -> Result<FetchOutcome, FetchError> {
        self.outcome.clone()
    }
}
