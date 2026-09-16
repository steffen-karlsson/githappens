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
    pub is_draft: bool,
    pub mergeable: MergeableState,
    pub repo: String,
    pub additions: u32,
    pub deletions: u32,
    pub created_at: String,
    pub rollup_state: Option<RollupState>,
    pub checks: Vec<CheckSnapshot>,
    pub reviews: Vec<ReviewSnapshot>,
    pub up_to_date: UpToDateState,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CheckSnapshot {
    pub name: String,
    pub kind: CheckKind,
    pub completed: bool,
    pub failed: bool,
    pub skipped: bool,
    pub running: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CheckKind {
    CheckRun,
    StatusContext,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReviewSnapshot {
    pub author: String,
    pub state: ReviewState,
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
        .map(|r| r.contexts.nodes.iter().map(check_from_context).collect())
        .unwrap_or_default();

    let reviews = node.reviews.nodes.iter().map(review_from_dto).collect();

    PullRequestSnapshot {
        number: node.number,
        title: node.title.clone(),
        url: node.url.clone(),
        is_draft: node.is_draft,
        mergeable: node.mergeable.clone(),
        repo,
        additions: node.additions,
        deletions: node.deletions,
        created_at: node.created_at.clone(),
        rollup_state,
        checks,
        reviews,
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
                )
            );
            let skipped = matches!(
                cr.conclusion,
                Some(CheckRunConclusion::Skipped | CheckRunConclusion::Neutral)
            );
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
            CheckSnapshot {
                name: cr.name.clone(),
                kind: CheckKind::CheckRun,
                completed,
                failed,
                skipped,
                running,
            }
        }
        CheckContext::StatusContext(sc) => {
            let completed = matches!(
                sc.state,
                StatusState::Success | StatusState::Error | StatusState::Failure
            );
            let failed = matches!(sc.state, StatusState::Error | StatusState::Failure);
            let skipped = false;
            let running = matches!(sc.state, StatusState::Pending);
            CheckSnapshot {
                name: sc.context.clone(),
                kind: CheckKind::StatusContext,
                completed,
                failed,
                skipped,
                running,
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
        state: node.state.clone(),
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
            }
        }));
        let snap = from_dto(&node);
        assert_eq!(snap.number, 42);
        assert_eq!(snap.title, "Add feature");
        assert_eq!(snap.repo, "owner/repo");
        assert!(!snap.is_draft);
        assert_eq!(snap.mergeable, MergeableState::Mergeable);
        assert_eq!(snap.rollup_state, Some(RollupState::Success));
        assert_eq!(snap.checks.len(), 2);
        assert!(snap.checks[0].completed);
        assert!(!snap.checks[0].failed);
        assert!(snap.checks[1].completed);
        assert!(!snap.checks[1].failed);
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
            "isDraft": true,
            "mergeable": "UNKNOWN",
            "headRefOid": null,
            "additions": 0,
            "deletions": 0,
            "createdAt": "2024-01-01T00:00:00Z",
            "repository": {"nameWithOwner": "o/r"},
            "commits": {"nodes": [{"commit": {"statusCheckRollup": null}}]},
            "reviews": {"nodes": []}
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
            "reviews": {"nodes": []}
        }));
        let snap = from_dto(&node);
        assert_eq!(snap.rollup_state, Some(RollupState::Failure));
        assert_eq!(snap.checks.len(), 2);
        assert!(snap.checks[0].completed);
        assert!(snap.checks[0].failed);
        assert!(!snap.checks[1].completed);
        assert!(!snap.checks[1].failed);
    }

    #[test]
    fn from_dto_no_repository() {
        let node = make_pr_node(serde_json::json!({
            "number": 5,
            "title": "No repo",
            "url": "https://github.com/o/r/pull/5",
            "isDraft": false,
            "mergeable": "CONFLICTING",
            "headRefOid": "ghi",
            "additions": 0,
            "deletions": 0,
            "createdAt": "2024-01-01T00:00:00Z",
            "commits": {"nodes": [{"commit": {"statusCheckRollup": null}}]},
            "reviews": {"nodes": []}
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
            ]}
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
            "reviews": {"nodes": []}
        }));
        let snap = from_dto(&node);
        assert!(snap.checks[0].completed);
        assert!(snap.checks[0].failed);
        assert!(!snap.checks[1].completed);
        assert!(!snap.checks[1].failed);
    }
}
