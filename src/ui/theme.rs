use ratatui::style::Color;
use ratatui::style::Style;
use ratatui::text::Span;

pub const GLYPH_READY: &str = "●";
pub const GLYPH_WAITING: &str = "●";
pub const GLYPH_FAILED: &str = "●";
pub const GLYPH_APPROVED: &str = "●";
pub const GLYPH_PENDING: &str = "●";
pub const GLYPH_NONE: &str = "○";
pub const GLYPH_CHANGES_REQUESTED: &str = "●";

pub const COLOR_READY: Color = Color::Rgb(46, 204, 113);
pub const COLOR_WAITING: Color = Color::Yellow;
pub const COLOR_FAILED: Color = Color::Red;
pub const COLOR_APPROVED: Color = Color::Rgb(46, 204, 113);
pub const COLOR_CHANGES_REQUESTED: Color = Color::Red;
pub const COLOR_PENDING: Color = Color::Yellow;
pub const COLOR_NONE: Color = Color::DarkGray;
pub const COLOR_HEADER: Color = Color::Cyan;
pub const COLOR_SELECTED: Color = Color::Black;
pub const COLOR_SELECTED_BG: Color = Color::White;

pub const HEADER_LABEL: &str = " githappens ";
pub const FOOTER_HINT: &str = " r refresh · d describe · Tab cycle · Enter open · ? help · q quit ";
pub const EMPTY_STATE_MSG: &str = "You have no open PRs. Go open one!";

pub const COLUMN_INDICATOR_WIDTH: usize = 3;
pub const COLUMN_NUMBER_WIDTH: usize = 6;
pub const COLUMN_CHECKS_WIDTH: usize = 14;
pub const COLUMN_COMMENTS_WIDTH: usize = 9;
pub const COLUMN_REVIEW_WIDTH: usize = 11;
pub const COLUMN_UPTODATE_WIDTH: usize = 11;
pub const COLUMN_DIFF_WIDTH: usize = 14;
pub const COLUMN_AGE_WIDTH: usize = 6;

pub fn merge_glyph_and_color(
    readiness: &crate::analysis::mergeability::MergeReadiness,
) -> (&'static str, Color) {
    match readiness {
        crate::analysis::mergeability::MergeReadiness::Ready => (GLYPH_READY, COLOR_READY),
        crate::analysis::mergeability::MergeReadiness::Waiting => (GLYPH_WAITING, COLOR_WAITING),
        crate::analysis::mergeability::MergeReadiness::Failed => (GLYPH_FAILED, COLOR_FAILED),
    }
}

pub fn approval_glyph_and_color(
    approval: &crate::analysis::approval::ApprovalState,
) -> (&'static str, Color) {
    match approval {
        crate::analysis::approval::ApprovalState::Approved => (GLYPH_APPROVED, COLOR_APPROVED),
        crate::analysis::approval::ApprovalState::ChangesRequested => {
            (GLYPH_CHANGES_REQUESTED, COLOR_CHANGES_REQUESTED)
        }
        crate::analysis::approval::ApprovalState::Pending => (GLYPH_PENDING, COLOR_PENDING),
        crate::analysis::approval::ApprovalState::None => (GLYPH_NONE, COLOR_NONE),
    }
}

pub fn up_to_date_glyph_and_color(
    state: &crate::github::pr::UpToDateState,
) -> (&'static str, Color) {
    use crate::github::pr::UpToDateState;
    match state {
        UpToDateState::UpToDate => ("●", COLOR_READY),
        UpToDateState::OutOfDate => ("●", Color::Red),
        UpToDateState::Unknown => ("○", Color::DarkGray),
    }
}

pub fn diff_spans(additions: u32, deletions: u32) -> Vec<Span<'static>> {
    vec![
        Span::styled(
            if additions > 0 {
                format!("+{}", additions)
            } else {
                "0".to_string()
            },
            if additions > 0 {
                Style::default().fg(COLOR_READY)
            } else {
                Style::default()
            },
        ),
        Span::raw("/"),
        Span::styled(
            if deletions > 0 {
                format!("-{}", deletions)
            } else {
                "0".to_string()
            },
            if deletions > 0 {
                Style::default().fg(COLOR_FAILED)
            } else {
                Style::default()
            },
        ),
    ]
}

pub fn checks_spans(counts: &crate::analysis::workflows::WorkflowCounts) -> Vec<Span<'static>> {
    if counts.total == 0 {
        return vec![Span::raw("–/–")];
    }

    let cap = |n: usize| -> String {
        if n > 99 {
            "99+".to_string()
        } else {
            n.to_string()
        }
    };

    vec![
        Span::styled(cap(counts.success), Style::default().fg(COLOR_READY)),
        Span::raw("/"),
        Span::styled(
            cap(counts.failed),
            if counts.failed > 0 {
                Style::default().fg(COLOR_FAILED)
            } else {
                Style::default()
            },
        ),
        Span::raw("/"),
        Span::styled(
            cap(counts.running),
            if counts.running > 0 {
                Style::default().fg(COLOR_WAITING)
            } else {
                Style::default()
            },
        ),
        Span::raw("/"),
        Span::styled(cap(counts.skipped), Style::default().fg(COLOR_NONE)),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::approval::ApprovalState;
    use crate::analysis::mergeability::MergeReadiness;
    use crate::github::pr::UpToDateState;

    #[test]
    fn merge_glyph_ready() {
        let (g, c) = merge_glyph_and_color(&MergeReadiness::Ready);
        assert_eq!(g, GLYPH_READY);
        assert_eq!(c, COLOR_READY);
    }

    #[test]
    fn merge_glyph_waiting() {
        let (g, c) = merge_glyph_and_color(&MergeReadiness::Waiting);
        assert_eq!(g, GLYPH_WAITING);
        assert_eq!(c, COLOR_WAITING);
    }

    #[test]
    fn merge_glyph_failed() {
        let (g, c) = merge_glyph_and_color(&MergeReadiness::Failed);
        assert_eq!(g, GLYPH_FAILED);
        assert_eq!(c, COLOR_FAILED);
    }

    #[test]
    fn approval_glyph_approved() {
        let (g, c) = approval_glyph_and_color(&ApprovalState::Approved);
        assert_eq!(g, GLYPH_APPROVED);
        assert_eq!(c, COLOR_APPROVED);
    }

    #[test]
    fn approval_glyph_changes_requested() {
        let (g, c) = approval_glyph_and_color(&ApprovalState::ChangesRequested);
        assert_eq!(g, GLYPH_CHANGES_REQUESTED);
        assert_eq!(c, COLOR_CHANGES_REQUESTED);
    }

    #[test]
    fn approval_glyph_pending() {
        let (g, c) = approval_glyph_and_color(&ApprovalState::Pending);
        assert_eq!(g, GLYPH_PENDING);
        assert_eq!(c, COLOR_PENDING);
    }

    #[test]
    fn approval_glyph_none() {
        let (g, c) = approval_glyph_and_color(&ApprovalState::None);
        assert_eq!(g, GLYPH_NONE);
        assert_eq!(c, COLOR_NONE);
    }

    #[test]
    fn up_to_date_glyph_up_to_date() {
        let (g, c) = up_to_date_glyph_and_color(&UpToDateState::UpToDate);
        assert_eq!(g, "●");
        assert_eq!(c, COLOR_READY);
    }

    #[test]
    fn up_to_date_glyph_out_of_date() {
        let (g, c) = up_to_date_glyph_and_color(&UpToDateState::OutOfDate);
        assert_eq!(g, "●");
        assert_eq!(c, Color::Red);
    }

    #[test]
    fn up_to_date_glyph_unknown() {
        let (g, c) = up_to_date_glyph_and_color(&UpToDateState::Unknown);
        assert_eq!(g, "○");
        assert_eq!(c, Color::DarkGray);
    }
}
