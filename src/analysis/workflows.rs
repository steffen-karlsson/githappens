use crate::github::pr::CheckSnapshot;

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
    let completed = checks.iter().filter(|c| c.completed).count();
    let success = checks
        .iter()
        .filter(|c| c.completed && !c.failed && !c.skipped)
        .count();
    let failed = checks.iter().filter(|c| c.failed).count();
    let running = checks.iter().filter(|c| c.running).count();
    let skipped = checks.iter().filter(|c| c.skipped).count();
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

    fn check(completed: bool, failed: bool) -> CheckSnapshot {
        check_ext(completed, failed, false, false)
    }

    fn check_ext(completed: bool, failed: bool, skipped: bool, running: bool) -> CheckSnapshot {
        CheckSnapshot {
            name: "CI".to_string(),
            kind: CheckKind::CheckRun,
            completed,
            failed,
            skipped,
            running,
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
        let checks = vec![check(true, false), check(true, false)];
        let c = count_checks(&checks);
        assert_eq!(c, counts(2, 2));
        assert_eq!(render_counts(&c), "2/2");
    }

    #[rstest]
    fn some_pending() {
        let checks = vec![check(true, false), check(false, false), check(false, false)];
        let c = count_checks(&checks);
        assert_eq!(c, counts(1, 3));
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
        let checks: Vec<CheckSnapshot> = (0..100).map(|_| check(true, false)).collect();
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
                completed: true,
                failed: false,
                skipped: false,
                running: false,
            },
            CheckSnapshot {
                name: "travis".to_string(),
                kind: CheckKind::StatusContext,
                completed: true,
                failed: false,
                skipped: false,
                running: false,
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
            completed: false,
            failed: false,
            skipped: false,
            running: false,
        }];
        let c = count_checks(&checks);
        assert_eq!(c, counts(0, 1));
    }

    #[rstest]
    fn checkrun_completed_null_conclusion_not_completed() {
        let checks = vec![CheckSnapshot {
            name: "CI".to_string(),
            kind: CheckKind::CheckRun,
            completed: false,
            failed: false,
            skipped: false,
            running: false,
        }];
        let c = count_checks(&checks);
        assert_eq!(c, counts(0, 1));
    }

    #[rstest]
    fn skipped_checks_counted_separately() {
        let checks = vec![
            check_ext(true, false, false, false), // success
            check_ext(true, false, true, false),  // skipped
            check_ext(true, false, true, false),  // skipped
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
        let checks = vec![
            check_ext(true, false, false, false), // success
            check_ext(false, false, false, true), // running
        ];
        let c = count_checks(&checks);
        assert_eq!(c.completed, 1);
        assert_eq!(c.total, 2);
        assert_eq!(c.success, 1);
        assert_eq!(c.running, 1);
    }

    #[rstest]
    fn failed_checks_counted() {
        let checks = vec![
            check_ext(true, false, false, false), // success
            check_ext(true, true, false, false),  // failed
        ];
        let c = count_checks(&checks);
        assert_eq!(c.completed, 2);
        assert_eq!(c.total, 2);
        assert_eq!(c.success, 1);
        assert_eq!(c.failed, 1);
    }
}
