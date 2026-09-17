use crate::github::models::ReviewState;
use crate::github::pr::ReviewSnapshot;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalState {
    Approved,
    ChangesRequested,
    Pending,
    None,
}

pub fn collapse_reviews(reviews: &[ReviewSnapshot]) -> ApprovalState {
    if reviews.is_empty() {
        return ApprovalState::None;
    }

    let mut latest_per_author: std::collections::HashMap<String, &ReviewSnapshot> =
        std::collections::HashMap::new();

    for review in reviews {
        if review.state == ReviewState::Dismissed {
            continue;
        }
        latest_per_author.insert(review.author.clone(), review);
    }

    if latest_per_author.is_empty() {
        return ApprovalState::None;
    }

    let mut has_changes_requested = false;
    let mut has_approved = false;
    let mut all_pending = true;

    for review in latest_per_author.values() {
        match review.state {
            ReviewState::ChangesRequested => {
                has_changes_requested = true;
                all_pending = false;
            }
            ReviewState::Approved => {
                has_approved = true;
                all_pending = false;
            }
            ReviewState::Pending => {}
            _ => {
                all_pending = false;
            }
        }
    }

    if has_changes_requested {
        ApprovalState::ChangesRequested
    } else if has_approved {
        ApprovalState::Approved
    } else if all_pending {
        ApprovalState::Pending
    } else {
        ApprovalState::None
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use rstest::rstest;

    fn review(author: &str, state: ReviewState) -> ReviewSnapshot {
        ReviewSnapshot {
            author: author.to_string(),
            author_is_bot: false,
            state,
            body: String::new(),
            submitted_at: None,
        }
    }

    #[rstest]
    fn single_approved() {
        let reviews = vec![review("alice", ReviewState::Approved)];
        assert_eq!(collapse_reviews(&reviews), ApprovalState::Approved);
    }

    #[rstest]
    fn single_changes_requested() {
        let reviews = vec![review("alice", ReviewState::ChangesRequested)];
        assert_eq!(collapse_reviews(&reviews), ApprovalState::ChangesRequested);
    }

    #[rstest]
    fn approved_then_changes_by_same_author() {
        let reviews = vec![
            review("alice", ReviewState::Approved),
            review("alice", ReviewState::ChangesRequested),
        ];
        assert_eq!(collapse_reviews(&reviews), ApprovalState::ChangesRequested);
    }

    #[rstest]
    fn changes_then_approved_by_same_author() {
        let reviews = vec![
            review("alice", ReviewState::ChangesRequested),
            review("alice", ReviewState::Approved),
        ];
        assert_eq!(collapse_reviews(&reviews), ApprovalState::Approved);
    }

    #[rstest]
    fn two_authors_one_approved_one_changes() {
        let reviews = vec![
            review("alice", ReviewState::Approved),
            review("bob", ReviewState::ChangesRequested),
        ];
        assert_eq!(collapse_reviews(&reviews), ApprovalState::ChangesRequested);
    }

    #[rstest]
    fn two_authors_both_approved() {
        let reviews = vec![
            review("alice", ReviewState::Approved),
            review("bob", ReviewState::Approved),
        ];
        assert_eq!(collapse_reviews(&reviews), ApprovalState::Approved);
    }

    #[rstest]
    fn only_commented() {
        let reviews = vec![review("alice", ReviewState::Commented)];
        assert_eq!(collapse_reviews(&reviews), ApprovalState::None);
    }

    #[rstest]
    fn dismissed_dropped_before_collapse() {
        let reviews = vec![review("alice", ReviewState::Dismissed)];
        assert_eq!(collapse_reviews(&reviews), ApprovalState::None);
    }

    #[rstest]
    fn dismissed_then_approved_by_same_author() {
        let reviews = vec![
            review("alice", ReviewState::Dismissed),
            review("alice", ReviewState::Approved),
        ];
        assert_eq!(collapse_reviews(&reviews), ApprovalState::Approved);
    }

    #[rstest]
    fn empty_reviews() {
        let reviews: Vec<ReviewSnapshot> = vec![];
        assert_eq!(collapse_reviews(&reviews), ApprovalState::None);
    }

    #[rstest]
    fn pending_only() {
        let reviews = vec![review("alice", ReviewState::Pending)];
        assert_eq!(collapse_reviews(&reviews), ApprovalState::Pending);
    }

    #[rstest]
    fn mixed_pending_and_approved_different_authors() {
        let reviews = vec![
            review("alice", ReviewState::Pending),
            review("bob", ReviewState::Approved),
        ];
        assert_eq!(collapse_reviews(&reviews), ApprovalState::Approved);
    }

    #[rstest]
    fn null_author_collapses_by_empty_string() {
        let reviews = vec![
            review("", ReviewState::Approved),
            review("", ReviewState::ChangesRequested),
        ];
        assert_eq!(collapse_reviews(&reviews), ApprovalState::ChangesRequested);
    }
}
