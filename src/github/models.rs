use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct GraphQLResponse {
    pub data: Option<ResponseData>,
    pub errors: Option<Vec<GraphQLError>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GraphQLError {
    pub message: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResponseData {
    pub viewer: Viewer,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Viewer {
    pub login: String,
    pub pull_requests: PullRequestConnection,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestConnection {
    pub page_info: PageInfo,
    pub nodes: Vec<Option<PullRequestNode>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageInfo {
    pub has_next_page: bool,
    pub end_cursor: Option<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MergeableState {
    Mergeable,
    Conflicting,
    Unknown,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestNode {
    pub number: u32,
    pub title: String,
    pub url: String,
    #[serde(default)]
    pub body: String,
    pub is_draft: bool,
    pub mergeable: MergeableState,
    pub head_ref_oid: Option<String>,
    pub additions: u32,
    pub deletions: u32,
    pub created_at: String,
    pub repository: Option<RepositoryNode>,
    pub commits: CommitConnection,
    pub reviews: ReviewConnection,
    pub comments: CommentConnection,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryNode {
    pub name_with_owner: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CommitConnection {
    pub nodes: Vec<CommitNode>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CommitNode {
    pub commit: Commit,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Commit {
    pub status_check_rollup: Option<StatusCheckRollup>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RollupState {
    Expected,
    Error,
    Failure,
    Pending,
    Success,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusCheckRollup {
    pub state: Option<RollupState>,
    pub contexts: ContextConnection,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ContextConnection {
    pub nodes: Vec<Option<CheckContext>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "__typename")]
pub enum CheckContext {
    CheckRun(CheckRun),
    StatusContext(StatusContext),
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CheckRunStatus {
    Queued,
    InProgress,
    Completed,
    Requested,
    Pending,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CheckRunConclusion {
    ActionRequired,
    Cancelled,
    ClusterFailure,
    Failure,
    Neutral,
    Skipped,
    Stale,
    StartupFailure,
    Success,
    TimedOut,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckRun {
    #[serde(default)]
    pub name: String,
    pub status: Option<CheckRunStatus>,
    pub conclusion: Option<CheckRunConclusion>,
    #[serde(default)]
    pub started_at: Option<String>,
    #[serde(default)]
    pub completed_at: Option<String>,
    #[serde(default)]
    pub annotations: Option<AnnotationConnection>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AnnotationConnection {
    pub nodes: Vec<AnnotationNode>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnnotationNode {
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub path: Option<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum StatusState {
    Error,
    Failure,
    Expected,
    Pending,
    Success,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusContext {
    #[serde(default)]
    pub context: String,
    pub state: StatusState,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReviewConnection {
    pub nodes: Vec<ReviewNode>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewNode {
    pub author: Option<ReviewAuthor>,
    pub state: ReviewState,
    pub submitted_at: Option<String>,
    #[serde(default)]
    pub body: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommentConnection {
    pub nodes: Vec<CommentNode>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommentNode {
    pub author: Option<ReviewAuthor>,
    #[serde(default)]
    pub body: String,
    pub created_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewAuthor {
    pub login: Option<String>,
    #[serde(rename = "__typename")]
    pub typename: Option<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReviewState {
    Approved,
    ChangesRequested,
    Commented,
    Dismissed,
    Pending,
    #[serde(other)]
    Other,
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_full_response() {
        let json = serde_json::json!({
            "data": {
                "viewer": {
                    "login": "ska",
                    "pullRequests": {
                        "pageInfo": {
                            "hasNextPage": false,
                            "endCursor": null
                        },
                        "nodes": [{
                            "number": 42,
                            "title": "Add feature",
                            "url": "https://github.com/owner/repo/pull/42",
                            "body": "",
                            "isDraft": false,
                            "mergeable": "MERGEABLE",
                            "headRefOid": "abc123",
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
                                                "nodes": [{
                                                    "__typename": "CheckRun",
                                                    "name": "CI",
                                                    "status": "COMPLETED",
                                                    "conclusion": "SUCCESS"
                                                }]
                                            }
                                        }
                                    }
                                }]
                            },
                            "reviews": {
                                "nodes": [{
                                    "author": {"login": "reviewer1"},
                                    "state": "APPROVED",
                                    "submittedAt": "2024-01-01T00:00:00Z"
                                }]
                            },
                            "comments": {"nodes": []}
                        }]
                    }
                }
            }
        });
        let resp: GraphQLResponse = serde_json::from_value(json).unwrap();
        let viewer = resp.data.unwrap().viewer;
        assert_eq!(viewer.login, "ska");
        let pr = viewer.pull_requests.nodes[0].as_ref().unwrap();
        assert_eq!(pr.number, 42);
        assert_eq!(pr.mergeable, MergeableState::Mergeable);
        assert!(!pr.is_draft);
    }

    #[test]
    fn deserialize_null_rollup() {
        let json = serde_json::json!({
            "data": {
                "viewer": {
                    "login": "ska",
                    "pullRequests": {
                        "pageInfo": {"hasNextPage": false, "endCursor": null},
                        "nodes": [{
                            "number": 1,
                            "title": "Test",
                            "url": "https://github.com/o/r/pull/1",
                            "body": "",
                            "isDraft": false,
                            "mergeable": "MERGEABLE",
                            "headRefOid": null,
                            "additions": 0,
                            "deletions": 0,
                            "createdAt": "2024-01-01T00:00:00Z",
                            "repository": {"nameWithOwner": "o/r"},
                            "commits": {
                                "nodes": [{
                                    "commit": {
                                        "statusCheckRollup": null
                                    }
                                }]
                            },
                            "reviews": {"nodes": []},
                            "comments": {"nodes": []}
                        }]
                    }
                }
            }
        });
        let resp: GraphQLResponse = serde_json::from_value(json).unwrap();
        let data = resp.data.unwrap();
        let pr = data.viewer.pull_requests.nodes[0].as_ref().unwrap();
        assert!(pr.commits.nodes[0].commit.status_check_rollup.is_none());
    }

    #[test]
    fn deserialize_pagination() {
        let json = serde_json::json!({
            "data": {
                "viewer": {
                    "login": "ska",
                    "pullRequests": {
                        "pageInfo": {"hasNextPage": true, "endCursor": "cursor123"},
                        "nodes": []
                    }
                }
            }
        });
        let resp: GraphQLResponse = serde_json::from_value(json).unwrap();
        let prs = resp.data.unwrap().viewer.pull_requests;
        assert!(prs.page_info.has_next_page);
        assert_eq!(prs.page_info.end_cursor.as_deref(), Some("cursor123"));
    }

    #[test]
    fn deserialize_status_context() {
        let json = serde_json::json!({
            "__typename": "StatusContext",
            "name": "continuous-integration/travis-ci",
            "state": "SUCCESS"
        });
        let ctx: CheckContext = serde_json::from_value(json).unwrap();
        match ctx {
            CheckContext::StatusContext(sc) => {
                assert_eq!(sc.state, StatusState::Success);
            }
            _ => panic!("expected StatusContext"),
        }
    }

    #[test]
    fn deserialize_unknown_review_state() {
        let json = serde_json::json!("SOME_NEW_STATE");
        let state: ReviewState = serde_json::from_value(json).unwrap();
        assert_eq!(state, ReviewState::Other);
    }
}
