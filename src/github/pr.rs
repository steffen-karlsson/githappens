use crate::github::models::{
    CheckContext, CheckRunConclusion, CheckRunStatus, MergeableState, PullRequestNode, ReviewNode,
    ReviewState, RollupState, StatusState,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpToDateState {
    UpToDate,
    OutOfDate,
    Unknown,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PullRequestSnapshot {
    pub number: u32,
    pub title: String,
    pub url: String,
    pub body: String,
    pub is_draft: bool,
    pub mergeable: MergeableState,
    pub repo: String,
    pub additions: u32,
    pub deletions: u32,
    pub created_at: String,
    pub rollup_state: Option<RollupState>,
    pub checks: Vec<CheckSnapshot>,
    pub reviews: Vec<ReviewSnapshot>,
    pub comments: Vec<CommentSnapshot>,
    pub up_to_date: UpToDateState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckStatus {
    Running,
    Failed,
    Success,
    Skipped,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CheckSnapshot {
    pub name: String,
    pub kind: CheckKind,
    pub status: CheckStatus,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub annotations: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckKind {
    CheckRun,
    StatusContext,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReviewSnapshot {
    pub author: String,
    pub author_is_bot: bool,
    pub state: ReviewState,
    pub body: String,
    pub submitted_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CommentSnapshot {
    pub author: String,
    pub author_is_bot: bool,
    pub body: String,
    pub created_at: String,
}

pub fn from_dto(node: &PullRequestNode) -> PullRequestSnapshot {
    let repo = node
        .repository
        .as_ref()
        .map(|r| r.name_with_owner.clone())
        .unwrap_or_default();

    let rollup_state = node
        .commits
        .nodes
        .first()
        .and_then(|c| c.commit.status_check_rollup.as_ref())
        .and_then(|r| r.state.clone());

    let checks = node
        .commits
        .nodes
        .first()
        .and_then(|c| c.commit.status_check_rollup.as_ref())
        .map(|r| {
            r.contexts
                .nodes
                .iter()
                .filter_map(|c| c.as_ref().map(check_from_context))
                .collect()
        })
        .unwrap_or_default();

    let reviews = node.reviews.nodes.iter().map(review_from_dto).collect();
    let comments = node
        .comments
        .nodes
        .iter()
        .map(|c| CommentSnapshot {
            author: c
                .author
                .as_ref()
                .and_then(|a| a.login.clone())
                .unwrap_or_default(),
            author_is_bot: c
                .author
                .as_ref()
                .map(|a| a.typename.as_deref() == Some("Bot"))
                .unwrap_or(false),
            body: c.body.clone(),
            created_at: c.created_at.clone().unwrap_or_default(),
        })
        .collect();

    PullRequestSnapshot {
        number: node.number,
        title: node.title.clone(),
        url: node.url.clone(),
        body: node.body.clone(),
        is_draft: node.is_draft,
        mergeable: node.mergeable.clone(),
        repo,
        additions: node.additions,
        deletions: node.deletions,
        created_at: node.created_at.clone(),
        rollup_state,
        checks,
        reviews,
        comments,
        up_to_date: UpToDateState::Unknown,
    }
}

fn check_from_context(ctx: &CheckContext) -> CheckSnapshot {
    match ctx {
        CheckContext::CheckRun(cr) => {
            let completed = cr.status == Some(CheckRunStatus::Completed);
            let failed = matches!(
                cr.conclusion,
                Some(
                    CheckRunConclusion::Failure
                        | CheckRunConclusion::TimedOut
                        | CheckRunConclusion::Cancelled
                        | CheckRunConclusion::StartupFailure
                        | CheckRunConclusion::ClusterFailure
                        | CheckRunConclusion::ActionRequired
                )
            );
            let skipped = matches!(cr.conclusion, Some(CheckRunConclusion::Skipped));
            let running = !completed
                && matches!(
                    cr.status,
                    Some(
                        CheckRunStatus::InProgress
                            | CheckRunStatus::Queued
                            | CheckRunStatus::Pending
                            | CheckRunStatus::Requested
                    )
                );
            let status = if running {
                CheckStatus::Running
            } else if failed {
                CheckStatus::Failed
            } else if skipped {
                CheckStatus::Skipped
            } else if completed && cr.conclusion.is_none() {
                CheckStatus::Running
            } else if completed {
                CheckStatus::Success
            } else {
                CheckStatus::Running
            };
            CheckSnapshot {
                name: cr.name.clone(),
                kind: CheckKind::CheckRun,
                status,
                started_at: cr.started_at.clone(),
                completed_at: cr.completed_at.clone(),
                annotations: cr
                    .annotations
                    .as_ref()
                    .map(|a| {
                        a.nodes
                            .iter()
                            .map(|n| {
                                n.path
                                    .as_deref()
                                    .map(|p| format!("{}: {}", p, n.message))
                                    .unwrap_or_else(|| n.message.clone())
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
            }
        }
        CheckContext::StatusContext(sc) => {
            let failed = matches!(sc.state, StatusState::Error | StatusState::Failure);
            let running = matches!(sc.state, StatusState::Pending | StatusState::Expected);
            let status = if running {
                CheckStatus::Running
            } else if failed {
                CheckStatus::Failed
            } else {
                CheckStatus::Success
            };
            CheckSnapshot {
                name: sc.context.clone(),
                kind: CheckKind::StatusContext,
                status,
                started_at: sc.created_at.clone(),
                completed_at: None,
                annotations: sc
                    .description
                    .as_ref()
                    .map(|d| vec![d.clone()])
                    .unwrap_or_default(),
            }
        }
    }
}

fn review_from_dto(node: &ReviewNode) -> ReviewSnapshot {
    ReviewSnapshot {
        author: node
            .author
            .as_ref()
            .and_then(|a| a.login.clone())
            .unwrap_or_default(),
        author_is_bot: node
            .author
            .as_ref()
            .map(|a| a.typename.as_deref() == Some("Bot"))
            .unwrap_or(false),
        state: node.state.clone(),
        body: node.body.clone(),
        submitted_at: node.submitted_at.clone(),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::github::models::*;
    use serde_json;

    fn make_pr_node(json: serde_json::Value) -> PullRequestNode {
        serde_json::from_value(json).unwrap()
    }

    #[test]
    fn from_dto_basic() {
        let node = make_pr_node(serde_json::json!({
            "number": 42,
            "title": "Add feature",
            "url": "https://github.com/owner/repo/pull/42",
            "body": "",
            "isDraft": false,
            "mergeable": "MERGEABLE",
            "headRefOid": "abc",
            "additions": 10,
            "deletions": 2,
            "createdAt": "2024-01-01T00:00:00Z",
            "repository": {"nameWithOwner": "owner/repo"},
            "commits": {
                "nodes": [{
                    "commit": {
                        "statusCheckRollup": {
                            "state": "SUCCESS",
                            "contexts": {
                                "nodes": [
                                    {"__typename": "CheckRun", "name": "CI", "status": "COMPLETED", "conclusion": "SUCCESS"},
                                    {"__typename": "StatusContext", "name": "travis", "state": "SUCCESS"}
                                ]
                            }
                        }
                    }
                }]
            },
            "reviews": {
                "nodes": [
                    {"author": {"login": "alice"}, "state": "APPROVED", "submittedAt": "2024-01-01T00:00:00Z"}
                ]
            },
            "comments": {"nodes": []}
        }));
        let snap = from_dto(&node);
        assert_eq!(snap.number, 42);
        assert_eq!(snap.title, "Add feature");
        assert_eq!(snap.repo, "owner/repo");
        assert!(!snap.is_draft);
        assert_eq!(snap.mergeable, MergeableState::Mergeable);
        assert_eq!(snap.rollup_state, Some(RollupState::Success));
        assert_eq!(snap.checks.len(), 2);
        assert_eq!(snap.checks[0].status, CheckStatus::Success);
        assert_eq!(snap.checks[1].status, CheckStatus::Success);
        assert_eq!(snap.reviews.len(), 1);
        assert_eq!(snap.reviews[0].author, "alice");
        assert_eq!(snap.reviews[0].state, ReviewState::Approved);
    }

    #[test]
    fn from_dto_null_rollup() {
        let node = make_pr_node(serde_json::json!({
            "number": 1,
            "title": "Test",
            "url": "https://github.com/o/r/pull/1",
            "body": "",
            "isDraft": true,
            "mergeable": "UNKNOWN",
            "headRefOid": null,
            "additions": 0,
            "deletions": 0,
            "createdAt": "2024-01-01T00:00:00Z",
            "repository": {"nameWithOwner": "o/r"},
            "commits": {"nodes": [{"commit": {"statusCheckRollup": null}}]},
            "reviews": {"nodes": []},
            "comments": {"nodes": []}
        }));
        let snap = from_dto(&node);
        assert!(snap.is_draft);
        assert_eq!(snap.mergeable, MergeableState::Unknown);
        assert!(snap.rollup_state.is_none());
        assert!(snap.checks.is_empty());
        assert!(snap.reviews.is_empty());
    }

    #[test]
    fn from_dto_failed_check_run() {
        let node = make_pr_node(serde_json::json!({
            "number": 3,
            "title": "Broken",
            "url": "https://github.com/o/r/pull/3",
            "body": "",
            "isDraft": false,
            "mergeable": "MERGEABLE",
            "headRefOid": "def",
            "additions": 5,
            "deletions": 15,
            "createdAt": "2024-01-01T00:00:00Z",
            "repository": {"nameWithOwner": "o/r"},
            "commits": {"nodes": [{"commit": {"statusCheckRollup": {
                "state": "FAILURE",
                "contexts": {"nodes": [
                    {"__typename": "CheckRun", "name": "CI", "status": "COMPLETED", "conclusion": "FAILURE"},
                    {"__typename": "CheckRun", "name": "Lint", "status": "IN_PROGRESS", "conclusion": null}
                ]}
            }}}]},
            "reviews": {"nodes": []},
            "comments": {"nodes": []}
        }));
        let snap = from_dto(&node);
        assert_eq!(snap.rollup_state, Some(RollupState::Failure));
        assert_eq!(snap.checks.len(), 2);
        assert_eq!(snap.checks[0].status, CheckStatus::Failed);
        assert_eq!(snap.checks[1].status, CheckStatus::Running);
    }

    #[test]
    fn from_dto_no_repository() {
        let node = make_pr_node(serde_json::json!({
            "number": 5,
            "title": "No repo",
            "url": "https://github.com/o/r/pull/5",
            "body": "",
            "isDraft": false,
            "mergeable": "CONFLICTING",
            "headRefOid": "ghi",
            "additions": 0,
            "deletions": 0,
            "createdAt": "2024-01-01T00:00:00Z",
            "commits": {"nodes": [{"commit": {"statusCheckRollup": null}}]},
            "reviews": {"nodes": []},
            "comments": {"nodes": []}
        }));
        let snap = from_dto(&node);
        assert_eq!(snap.repo, "");
        assert_eq!(snap.mergeable, MergeableState::Conflicting);
    }

    #[test]
    fn from_dto_null_author() {
        let node = make_pr_node(serde_json::json!({
            "number": 7,
            "title": "Bot review",
            "url": "https://github.com/o/r/pull/7",
            "body": "",
            "isDraft": false,
            "mergeable": "MERGEABLE",
            "headRefOid": "jkl",
            "additions": 0,
            "deletions": 0,
            "createdAt": "2024-01-01T00:00:00Z",
            "repository": {"nameWithOwner": "o/r"},
            "commits": {"nodes": [{"commit": {"statusCheckRollup": null}}]},
            "reviews": {"nodes": [
                {"author": null, "state": "COMMENTED", "submittedAt": null},
                {"author": {"login": null}, "state": "APPROVED", "submittedAt": "2024-01-01T00:00:00Z"}
            ]},
            "comments": {"nodes": []}
        }));
        let snap = from_dto(&node);
        assert_eq!(snap.reviews.len(), 2);
        assert_eq!(snap.reviews[0].author, "");
        assert_eq!(snap.reviews[1].author, "");
    }

    #[test]
    fn from_dto_status_context_failure() {
        let node = make_pr_node(serde_json::json!({
            "number": 8,
            "title": "Status fail",
            "url": "https://github.com/o/r/pull/8",
            "body": "",
            "isDraft": false,
            "mergeable": "MERGEABLE",
            "headRefOid": "mno",
            "additions": 0,
            "deletions": 0,
            "createdAt": "2024-01-01T00:00:00Z",
            "repository": {"nameWithOwner": "o/r"},
            "commits": {"nodes": [{"commit": {"statusCheckRollup": {
                "state": "FAILURE",
                "contexts": {"nodes": [
                    {"__typename": "StatusContext", "name": "ci/jenkins", "state": "FAILURE"},
                    {"__typename": "StatusContext", "name": "ci/travis", "state": "PENDING"}
                ]}
            }}}]},
            "reviews": {"nodes": []},
            "comments": {"nodes": []}
        }));
        let snap = from_dto(&node);
        assert_eq!(snap.checks[0].status, CheckStatus::Failed);
        assert_eq!(snap.checks[1].status, CheckStatus::Running);
    }
}
