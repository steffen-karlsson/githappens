use crate::analysis::approval::{ApprovalState, collapse_reviews};
use crate::github::models::{MergeableState, RollupState};
use crate::github::pr::{CheckStatus, PullRequestSnapshot, UpToDateState};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeReadiness {
    Ready,
    Waiting,
    Failed,
}

pub fn assess(pr: &PullRequestSnapshot) -> MergeReadiness {
    if pr.is_draft {
        return MergeReadiness::Waiting;
    }

    if pr.mergeable == MergeableState::Conflicting {
        return MergeReadiness::Failed;
    }

    if pr.up_to_date == UpToDateState::OutOfDate {
        return MergeReadiness::Failed;
    }

    let checks_failed = pr.checks.iter().any(|c| c.status == CheckStatus::Failed);
    if checks_failed {
        return MergeReadiness::Failed;
    }

    let approval = collapse_reviews(&pr.reviews);
    if approval == ApprovalState::ChangesRequested {
        return MergeReadiness::Failed;
    }

    if pr.mergeable == MergeableState::Unknown {
        return MergeReadiness::Failed;
    }

    let checks_pending = pr.checks.iter().any(|c| c.status == CheckStatus::Running);
    if checks_pending {
        return MergeReadiness::Waiting;
    }

    if approval != ApprovalState::Approved {
        return MergeReadiness::Waiting;
    }

    let rollup_ok = pr
        .rollup_state
        .as_ref()
        .map(|s| *s == RollupState::Success)
        .unwrap_or(true);

    if !rollup_ok {
        return MergeReadiness::Waiting;
    }

    if pr.mergeable == MergeableState::Mergeable {
        MergeReadiness::Ready
    } else {
        MergeReadiness::Waiting
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::github::models::ReviewState;
    use crate::github::pr::{CheckKind, CheckSnapshot, CheckStatus, ReviewSnapshot, UpToDateState};
    use rstest::rstest;

    fn make_check(status: CheckStatus) -> CheckSnapshot {
        CheckSnapshot {
            name: "CI".to_string(),
            kind: CheckKind::CheckRun,
            status,
            started_at: None,
            completed_at: None,
            annotations: vec![],
        }
    }

    fn make_pr() -> PullRequestSnapshot {
        PullRequestSnapshot {
            number: 1,
            title: "Test".to_string(),
            url: "https://github.com/o/r/pull/1".to_string(),
            body: String::new(),
            is_draft: false,
            mergeable: MergeableState::Mergeable,
            repo: "o/r".to_string(),
            rollup_state: Some(RollupState::Success),
            checks: vec![make_check(CheckStatus::Success)],
            reviews: vec![ReviewSnapshot {
                author: "alice".to_string(),
                author_is_bot: false,
                state: ReviewState::Approved,
                body: String::new(),
                submitted_at: None,
            }],
            up_to_date: UpToDateState::UpToDate,
            additions: 0,
            deletions: 0,
            created_at: String::new(),
            comments: vec![],
        }
    }

    #[rstest]
    fn all_green_ready() {
        let pr = make_pr();
        assert_eq!(assess(&pr), MergeReadiness::Ready);
    }

    #[rstest]
    fn draft_always_waiting() {
        let mut pr = make_pr();
        pr.is_draft = true;
        assert_eq!(assess(&pr), MergeReadiness::Waiting);
    }

    #[rstest]
    fn mergeable_unknown_failed() {
        let mut pr = make_pr();
        pr.mergeable = MergeableState::Unknown;
        assert_eq!(assess(&pr), MergeReadiness::Failed);
    }

    #[rstest]
    fn mergeable_conflicting_failed() {
        let mut pr = make_pr();
        pr.mergeable = MergeableState::Conflicting;
        assert_eq!(assess(&pr), MergeReadiness::Failed);
    }

    #[rstest]
    fn out_of_date_failed() {
        let mut pr = make_pr();
        pr.up_to_date = UpToDateState::OutOfDate;
        assert_eq!(assess(&pr), MergeReadiness::Failed);
    }

    #[rstest]
    fn up_to_date_ready() {
        let pr = make_pr();
        assert_eq!(pr.up_to_date, UpToDateState::UpToDate);
        assert_eq!(assess(&pr), MergeReadiness::Ready);
    }

    #[rstest]
    fn up_to_date_unknown_still_ready() {
        let mut pr = make_pr();
        pr.up_to_date = UpToDateState::Unknown;
        assert_eq!(assess(&pr), MergeReadiness::Ready);
    }

    #[rstest]
    fn checks_success_no_approval_waiting() {
        let mut pr = make_pr();
        pr.reviews = vec![];
        assert_eq!(assess(&pr), MergeReadiness::Waiting);
    }

    #[rstest]
    fn checks_pending_waiting() {
        let mut pr = make_pr();
        pr.checks = vec![make_check(CheckStatus::Running)];
        assert_eq!(assess(&pr), MergeReadiness::Waiting);
    }

    #[rstest]
    fn checks_failure_failed() {
        let mut pr = make_pr();
        pr.checks = vec![make_check(CheckStatus::Failed)];
        assert_eq!(assess(&pr), MergeReadiness::Failed);
    }

    #[rstest]
    fn no_ci_treated_as_success_ready() {
        let mut pr = make_pr();
        pr.checks = vec![];
        pr.rollup_state = None;
        assert_eq!(assess(&pr), MergeReadiness::Ready);
    }

    #[rstest]
    fn mixed_failure_plus_approved_failed() {
        let mut pr = make_pr();
        pr.checks = vec![
            make_check(CheckStatus::Failed),
            make_check(CheckStatus::Running),
        ];
        assert_eq!(assess(&pr), MergeReadiness::Failed);
    }

    #[rstest]
    fn mixed_pending_plus_changes_requested_failed() {
        let mut pr = make_pr();
        pr.checks = vec![make_check(CheckStatus::Running)];
        pr.reviews = vec![ReviewSnapshot {
            author: "bob".to_string(),
            author_is_bot: false,
            state: ReviewState::ChangesRequested,
            body: String::new(),
            submitted_at: None,
        }];
        assert_eq!(assess(&pr), MergeReadiness::Failed);
    }

    #[rstest]
    fn mixed_pending_no_reviews_waiting() {
        let mut pr = make_pr();
        pr.checks = vec![make_check(CheckStatus::Running)];
        pr.reviews = vec![];
        assert_eq!(assess(&pr), MergeReadiness::Waiting);
    }

    #[rstest]
    fn rollup_failure_waiting_even_with_approval() {
        let mut pr = make_pr();
        pr.rollup_state = Some(RollupState::Failure);
        assert_eq!(assess(&pr), MergeReadiness::Waiting);
    }

    #[rstest]
    fn rollup_null_no_checks_ready() {
        let mut pr = make_pr();
        pr.rollup_state = None;
        pr.checks = vec![];
        assert_eq!(assess(&pr), MergeReadiness::Ready);
    }
}
