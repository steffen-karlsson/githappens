use crate::github::pr::{CheckSnapshot, CheckStatus};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowCounts {
    pub completed: usize,
    pub total: usize,
    pub success: usize,
    pub failed: usize,
    pub running: usize,
    pub skipped: usize,
}

pub fn count_checks(checks: &[CheckSnapshot]) -> WorkflowCounts {
    let total = checks.len();
    let completed = checks
        .iter()
        .filter(|c| {
            c.status == CheckStatus::Success
                || c.status == CheckStatus::Failed
                || c.status == CheckStatus::Skipped
        })
        .count();
    let success = checks
        .iter()
        .filter(|c| c.status == CheckStatus::Success)
        .count();
    let failed = checks
        .iter()
        .filter(|c| c.status == CheckStatus::Failed)
        .count();
    let running = checks
        .iter()
        .filter(|c| c.status == CheckStatus::Running)
        .count();
    let skipped = checks
        .iter()
        .filter(|c| c.status == CheckStatus::Skipped)
        .count();
    WorkflowCounts {
        completed,
        total,
        success,
        failed,
        running,
        skipped,
    }
}

pub fn render_counts(counts: &WorkflowCounts) -> String {
    if counts.total == 0 {
        return "–/–".to_string();
    }
    let cap = |n: usize| -> String {
        if n > 99 {
            "99+".to_string()
        } else {
            n.to_string()
        }
    };
    format!("{}/{}", cap(counts.completed), cap(counts.total))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::github::pr::CheckKind;
    use rstest::rstest;

    fn check(status: CheckStatus) -> CheckSnapshot {
        CheckSnapshot {
            name: "CI".to_string(),
            kind: CheckKind::CheckRun,
            status,
            started_at: None,
            completed_at: None,
            annotations: vec![],
        }
    }

    fn counts(completed: usize, total: usize) -> WorkflowCounts {
        WorkflowCounts {
            completed,
            total,
            success: completed,
            failed: 0,
            running: 0,
            skipped: 0,
        }
    }

    #[rstest]
    fn all_completed() {
        let checks = vec![check(CheckStatus::Success), check(CheckStatus::Success)];
        let c = count_checks(&checks);
        assert_eq!(c, counts(2, 2));
        assert_eq!(render_counts(&c), "2/2");
    }

    #[rstest]
    fn some_pending() {
        let checks = vec![
            check(CheckStatus::Success),
            check(CheckStatus::Running),
            check(CheckStatus::Running),
        ];
        let c = count_checks(&checks);
        assert_eq!(c.completed, 1);
        assert_eq!(c.total, 3);
        assert_eq!(render_counts(&c), "1/3");
    }

    #[rstest]
    fn zero_checks_renders_dash() {
        let c = count_checks(&[]);
        assert_eq!(c, counts(0, 0));
        assert_eq!(render_counts(&c), "–/–");
    }

    #[rstest]
    fn capped_at_99() {
        let checks: Vec<CheckSnapshot> = (0..100).map(|_| check(CheckStatus::Success)).collect();
        let c = count_checks(&checks);
        assert_eq!(c, counts(100, 100));
        assert_eq!(render_counts(&c), "99+/99+");
    }

    #[rstest]
    fn mixed_checkrun_and_status_context() {
        let checks = vec![
            CheckSnapshot {
                name: "CI".to_string(),
                kind: CheckKind::CheckRun,
                status: CheckStatus::Success,
                started_at: None,
                completed_at: None,
                annotations: vec![],
            },
            CheckSnapshot {
                name: "travis".to_string(),
                kind: CheckKind::StatusContext,
                status: CheckStatus::Success,
                started_at: None,
                completed_at: None,
                annotations: vec![],
            },
        ];
        let c = count_checks(&checks);
        assert_eq!(c, counts(2, 2));
    }

    #[rstest]
    fn status_context_pending_not_completed() {
        let checks = vec![CheckSnapshot {
            name: "travis".to_string(),
            kind: CheckKind::StatusContext,
            status: CheckStatus::Running,
            started_at: None,
            completed_at: None,
            annotations: vec![],
        }];
        let c = count_checks(&checks);
        assert_eq!(c.completed, 0);
        assert_eq!(c.total, 1);
    }

    #[rstest]
    fn checkrun_completed_null_conclusion_not_completed() {
        let checks = vec![CheckSnapshot {
            name: "CI".to_string(),
            kind: CheckKind::CheckRun,
            status: CheckStatus::Running,
            started_at: None,
            completed_at: None,
            annotations: vec![],
        }];
        let c = count_checks(&checks);
        assert_eq!(c.completed, 0);
        assert_eq!(c.total, 1);
    }

    #[rstest]
    fn skipped_checks_counted_separately() {
        let checks = vec![
            check(CheckStatus::Success),
            check(CheckStatus::Skipped),
            check(CheckStatus::Skipped),
        ];
        let c = count_checks(&checks);
        assert_eq!(c.completed, 3);
        assert_eq!(c.total, 3);
        assert_eq!(c.success, 1);
        assert_eq!(c.skipped, 2);
        assert_eq!(c.failed, 0);
        assert_eq!(c.running, 0);
    }

    #[rstest]
    fn running_checks_counted() {
        let checks = vec![check(CheckStatus::Success), check(CheckStatus::Running)];
        let c = count_checks(&checks);
        assert_eq!(c.completed, 1);
        assert_eq!(c.total, 2);
        assert_eq!(c.success, 1);
        assert_eq!(c.running, 1);
    }

    #[rstest]
    fn failed_checks_counted() {
        let checks = vec![check(CheckStatus::Success), check(CheckStatus::Failed)];
        let c = count_checks(&checks);
        assert_eq!(c.completed, 2);
        assert_eq!(c.total, 2);
        assert_eq!(c.success, 1);
        assert_eq!(c.failed, 1);
    }
}
