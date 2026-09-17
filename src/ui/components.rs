use chrono::{DateTime, Utc};

use crate::github::models::ReviewState;
use crate::github::pr::{CheckSnapshot, CheckStatus, CommentSnapshot, ReviewSnapshot};

// ---------------------------------------------------------------------------
// Formatting utilities
// ---------------------------------------------------------------------------

pub fn format_age(timestamp: &str) -> String {
    let parsed: DateTime<Utc> = match DateTime::parse_from_rfc3339(timestamp) {
        Ok(dt) => dt.with_timezone(&Utc),
        Err(_) => return "?".to_string(),
    };
    let elapsed = Utc::now().signed_duration_since(parsed);
    let secs = elapsed.num_seconds();
    if secs < 0 {
        return "?".to_string();
    }
    if secs < 60 {
        return format!("{secs}s");
    }
    let mins = secs / 60;
    if mins < 60 {
        return format!("{mins}m");
    }
    let hours = mins / 60;
    if hours < 24 {
        return format!("{hours}h");
    }
    let days = hours / 24;
    if days < 30 {
        return format!("{days}d");
    }
    let months = days / 30;
    if months < 12 {
        return format!("{months}mo");
    }
    let years = months / 12;
    format!("{years}y")
}

pub fn format_duration(start: &str, end: Option<&str>) -> String {
    let start_parsed: DateTime<Utc> = match DateTime::parse_from_rfc3339(start) {
        Ok(dt) => dt.with_timezone(&Utc),
        Err(_) => return "–".to_string(),
    };
    let end_parsed = match end {
        Some(e) => match DateTime::parse_from_rfc3339(e) {
            Ok(dt) => dt.with_timezone(&Utc),
            Err(_) => return "–".to_string(),
        },
        None => return "–".to_string(),
    };
    let elapsed = end_parsed.signed_duration_since(start_parsed);
    let secs = elapsed.num_seconds();
    if secs < 0 {
        return "–".to_string();
    }
    if secs < 60 {
        return format!("{secs}s");
    }
    let mins = secs / 60;
    let rem_secs = secs % 60;
    if mins < 60 {
        return format!("{mins}m {rem_secs}s");
    }
    let hours = mins / 60;
    let rem_mins = mins % 60;
    if hours < 24 {
        return format!("{hours}h {rem_mins}m");
    }
    let days = hours / 24;
    let rem_hours = hours % 24;
    format!("{days}d {rem_hours}h")
}

// ---------------------------------------------------------------------------
// Text utilities
// ---------------------------------------------------------------------------

pub fn strip_markdown(text: &str) -> String {
    let mut result = String::new();
    let mut in_tag = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("<!--") || trimmed.ends_with("-->") {
            continue;
        }
        if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
            result.push_str("  ");
            result.push_str(trimmed[2..].trim_start());
            result.push('\n');
            continue;
        }
        if trimmed.len() > 3 && trimmed[2..3].contains(". ") {
            result.push_str("  ");
            result.push_str(&trimmed[3..]);
            result.push('\n');
            continue;
        }
        for ch in trimmed.chars() {
            match ch {
                '<' => in_tag = true,
                '>' => {
                    in_tag = false;
                    continue;
                }
                '`' | '~' => continue,
                _ if !in_tag => result.push(ch),
                _ => {}
            }
        }
        if !in_tag {
            result.push('\n');
        }
    }
    let result = result
        .replace("### ", "")
        .replace("## ", "")
        .replace("# ", "")
        .replace("**", "")
        .replace("*", "")
        .replace("[", "")
        .replace("](", " — ")
        .replace("]", "")
        .replace(")", "");
    let mut cleaned = String::new();
    for line in result.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("https://") || trimmed.starts_with("http://") {
            if let Some(space) = trimmed.find(' ') {
                cleaned.push_str(&trimmed[..space]);
                cleaned.push('\n');
                cleaned.push_str(trimmed[space..].trim());
            } else {
                cleaned.push_str(trimmed);
            }
            cleaned.push('\n');
        } else {
            cleaned.push_str(line);
            cleaned.push('\n');
        }
    }
    cleaned.trim().to_string()
}

pub fn truncate_lines(text: &str, max_lines: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() <= max_lines {
        return text.to_string();
    }
    let truncated: Vec<&str> = lines.iter().take(max_lines).copied().collect();
    let mut result = truncated.join("\n");
    result.push('…');
    result
}

pub fn wrap_text(text: &str, width: usize) -> Vec<String> {
    let mut result = Vec::new();
    for line in text.lines() {
        if line.chars().count() <= width {
            result.push(line.to_string());
            continue;
        }
        let mut current = String::new();
        for word in line.split_whitespace() {
            if current.is_empty() {
                if word.chars().count() <= width {
                    current = word.to_string();
                } else {
                    let mut chunk = String::new();
                    for ch in word.chars() {
                        if chunk.chars().count() >= width {
                            result.push(chunk.clone());
                            chunk.clear();
                        }
                        chunk.push(ch);
                    }
                    if !chunk.is_empty() {
                        current = chunk;
                    }
                }
            } else if current.chars().count() + 1 + word.chars().count() <= width {
                current.push(' ');
                current.push_str(word);
            } else {
                result.push(current);
                if word.chars().count() <= width {
                    current = word.to_string();
                } else {
                    let mut chunk = String::new();
                    for ch in word.chars() {
                        if chunk.chars().count() >= width {
                            result.push(chunk.clone());
                            chunk.clear();
                        }
                        chunk.push(ch);
                    }
                    current = chunk;
                }
            }
        }
        if !current.is_empty() {
            result.push(current);
        }
    }
    result
}

// ---------------------------------------------------------------------------
// Author label detection
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthorLabel {
    Ai,
    Bot,
    None,
}

impl AuthorLabel {
    pub fn display(&self) -> &'static str {
        match self {
            AuthorLabel::Ai => "AI",
            AuthorLabel::Bot => "Bot",
            AuthorLabel::None => "–",
        }
    }
}

pub fn detect_label(login: &str, is_bot: bool) -> AuthorLabel {
    if login.to_lowercase().contains("copilot") {
        AuthorLabel::Ai
    } else if is_bot {
        AuthorLabel::Bot
    } else {
        AuthorLabel::None
    }
}

// ---------------------------------------------------------------------------
// Activity entries (consolidated reviews + comments)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum EntryKind {
    Approved,
    Review,
    Comment,
}

impl EntryKind {
    pub fn display(&self) -> &'static str {
        match self {
            EntryKind::Approved => "Approved",
            EntryKind::Review => "Review",
            EntryKind::Comment => "Comment",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ActivityEntry {
    pub kind: EntryKind,
    pub author: String,
    pub label: AuthorLabel,
    pub created_at: String,
    pub body: String,
}

pub fn build_activity_entries(
    reviews: &[ReviewSnapshot],
    comments: &[CommentSnapshot],
) -> Vec<ActivityEntry> {
    let mut entries: Vec<ActivityEntry> = Vec::new();

    for review in reviews {
        let kind = match review.state {
            ReviewState::Approved => EntryKind::Approved,
            _ => EntryKind::Review,
        };
        entries.push(ActivityEntry {
            kind,
            author: review.author.clone(),
            label: detect_label(&review.author, review.author_is_bot),
            created_at: review.submitted_at.clone().unwrap_or_default(),
            body: review.body.clone(),
        });
    }

    for comment in comments {
        entries.push(ActivityEntry {
            kind: EntryKind::Comment,
            author: comment.author.clone(),
            label: detect_label(&comment.author, comment.author_is_bot),
            created_at: comment.created_at.clone(),
            body: comment.body.clone(),
        });
    }

    entries.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    entries
}

// ---------------------------------------------------------------------------
// Check grouping
// ---------------------------------------------------------------------------

pub fn group_checks_by_status(checks: &[CheckSnapshot]) -> Vec<usize> {
    let mut indices: Vec<usize> = Vec::new();
    let order = [
        CheckStatus::Running,
        CheckStatus::Failed,
        CheckStatus::Success,
        CheckStatus::Skipped,
    ];
    for status in &order {
        for (i, check) in checks.iter().enumerate() {
            if check.status == *status {
                indices.push(i);
            }
        }
    }
    indices
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    // --- format_age tests ---

    #[test]
    fn format_age_seconds() {
        let ts = (Utc::now() - chrono::Duration::seconds(30)).to_rfc3339();
        assert_eq!(format_age(&ts), "30s");
    }

    #[test]
    fn format_age_minutes() {
        let ts = (Utc::now() - chrono::Duration::minutes(5)).to_rfc3339();
        assert_eq!(format_age(&ts), "5m");
    }

    #[test]
    fn format_age_hours() {
        let ts = (Utc::now() - chrono::Duration::hours(3)).to_rfc3339();
        assert_eq!(format_age(&ts), "3h");
    }

    #[test]
    fn format_age_days() {
        let ts = (Utc::now() - chrono::Duration::days(5)).to_rfc3339();
        assert_eq!(format_age(&ts), "5d");
    }

    #[test]
    fn format_age_invalid() {
        assert_eq!(format_age("not-a-date"), "?");
    }

    #[test]
    fn format_age_empty() {
        assert_eq!(format_age(""), "?");
    }

    // --- format_duration tests ---

    #[test]
    fn format_duration_seconds_range() {
        let start = (Utc::now() - chrono::Duration::seconds(45)).to_rfc3339();
        let end = Utc::now().to_rfc3339();
        assert_eq!(format_duration(&start, Some(&end)), "45s");
    }

    #[test]
    fn format_duration_minutes_seconds() {
        let start = (Utc::now() - chrono::Duration::seconds(150)).to_rfc3339();
        let end = Utc::now().to_rfc3339();
        assert_eq!(format_duration(&start, Some(&end)), "2m 30s");
    }

    #[test]
    fn format_duration_hours_minutes() {
        let start = (Utc::now() - chrono::Duration::seconds(3900)).to_rfc3339();
        let end = Utc::now().to_rfc3339();
        assert_eq!(format_duration(&start, Some(&end)), "1h 5m");
    }

    #[test]
    fn format_duration_no_end() {
        let start = (Utc::now() - chrono::Duration::seconds(30)).to_rfc3339();
        assert_eq!(format_duration(&start, None), "–");
    }

    #[test]
    fn format_duration_invalid_start() {
        let end = Utc::now().to_rfc3339();
        assert_eq!(format_duration("bad", Some(&end)), "–");
    }

    // --- strip_markdown tests ---

    #[test]
    fn strip_markdown_headers() {
        assert_eq!(strip_markdown("### Title\n**bold**"), "Title\nbold");
    }

    #[test]
    fn strip_markdown_html_tags() {
        assert_eq!(strip_markdown("<details>text</details>"), "text");
    }

    #[test]
    fn strip_markdown_html_comments() {
        assert_eq!(strip_markdown("<!-- SBOM -->\nvisible"), "visible");
    }

    #[test]
    fn strip_markdown_plain_text() {
        assert_eq!(strip_markdown("just text"), "just text");
    }

    // --- truncate_lines tests ---

    #[test]
    fn truncate_lines_within_limit() {
        assert_eq!(truncate_lines("one\ntwo", 3), "one\ntwo");
    }

    #[test]
    fn truncate_lines_exceeds_limit() {
        let result = truncate_lines("one\ntwo\nthree", 2);
        assert_eq!(result, "one\ntwo…");
    }

    #[test]
    fn truncate_lines_single_line() {
        assert_eq!(truncate_lines("only", 2), "only");
    }

    // --- wrap_text tests ---

    #[test]
    fn wrap_text_short_line() {
        assert_eq!(wrap_text("short", 20), vec!["short"]);
    }

    #[test]
    fn wrap_text_long_line() {
        let result = wrap_text("hello world foo bar", 10);
        assert_eq!(result, vec!["hello", "world foo", "bar"]);
    }

    #[test]
    fn wrap_text_preserves_newlines() {
        let result = wrap_text("line one\nline two", 20);
        assert_eq!(result, vec!["line one", "line two"]);
    }

    // --- detect_label tests ---

    #[test]
    fn detect_label_copilot() {
        assert_eq!(
            detect_label("copilot-pull-request-reviewer", true),
            AuthorLabel::Ai
        );
    }

    #[test]
    fn detect_label_bot_suffix() {
        assert_eq!(detect_label("github-actions", true), AuthorLabel::Bot);
    }

    #[test]
    fn detect_label_dependabot() {
        assert_eq!(detect_label("dependabot", true), AuthorLabel::Bot);
    }

    #[test]
    fn detect_label_human() {
        assert_eq!(detect_label("alice", false), AuthorLabel::None);
    }

    #[test]
    fn detect_label_empty() {
        assert_eq!(detect_label("", false), AuthorLabel::None);
    }

    // --- build_activity_entries tests ---

    #[test]
    fn activity_entries_merges_reviews_and_comments() {
        let reviews = vec![ReviewSnapshot {
            author: "alice".to_string(),
            author_is_bot: false,
            state: ReviewState::Approved,
            body: String::new(),
            submitted_at: Some("2024-01-01T10:00:00Z".to_string()),
        }];
        let comments = vec![CommentSnapshot {
            author: "bob".to_string(),
            author_is_bot: false,
            body: String::new(),
            created_at: "2024-01-01T12:00:00Z".to_string(),
        }];
        let entries = build_activity_entries(&reviews, &comments);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].author, "bob");
        assert_eq!(entries[1].author, "alice");
    }

    #[test]
    fn activity_entries_ordered_latest_first() {
        let reviews = vec![
            ReviewSnapshot {
                author: "old".to_string(),
                author_is_bot: false,
                state: ReviewState::Approved,
                body: String::new(),
                submitted_at: Some("2024-01-01T10:00:00Z".to_string()),
            },
            ReviewSnapshot {
                author: "new".to_string(),
                author_is_bot: false,
                state: ReviewState::Commented,
                body: String::new(),
                submitted_at: Some("2024-01-02T10:00:00Z".to_string()),
            },
        ];
        let entries = build_activity_entries(&reviews, &[]);
        assert_eq!(entries[0].author, "new");
        assert_eq!(entries[1].author, "old");
    }

    #[test]
    fn activity_entries_empty() {
        let entries = build_activity_entries(&[], &[]);
        assert!(entries.is_empty());
    }

    #[test]
    fn activity_entries_label_detection() {
        let reviews = vec![ReviewSnapshot {
            author: "copilot-pull-request-reviewer".to_string(),
            author_is_bot: false,
            state: ReviewState::Commented,
            body: String::new(),
            submitted_at: Some("2024-01-01T10:00:00Z".to_string()),
        }];
        let entries = build_activity_entries(&reviews, &[]);
        assert_eq!(entries[0].label, AuthorLabel::Ai);
    }

    // --- group_checks_by_status tests ---

    fn make_check(name: &str, status: CheckStatus) -> CheckSnapshot {
        CheckSnapshot {
            name: name.to_string(),
            kind: crate::github::pr::CheckKind::CheckRun,
            status,
            started_at: None,
            completed_at: None,
            annotations: vec![],
        }
    }

    #[test]
    fn group_checks_running_first() {
        let checks = vec![
            make_check("A", CheckStatus::Success),
            make_check("B", CheckStatus::Running),
        ];
        let indices = group_checks_by_status(&checks);
        assert_eq!(indices, vec![1, 0]);
    }

    #[test]
    fn group_checks_failed_before_success() {
        let checks = vec![
            make_check("A", CheckStatus::Success),
            make_check("B", CheckStatus::Failed),
        ];
        let indices = group_checks_by_status(&checks);
        assert_eq!(indices, vec![1, 0]);
    }

    #[test]
    fn group_checks_all_groups() {
        let checks = vec![
            make_check("success1", CheckStatus::Success),
            make_check("running1", CheckStatus::Running),
            make_check("failed1", CheckStatus::Failed),
            make_check("skipped1", CheckStatus::Skipped),
            make_check("success2", CheckStatus::Success),
        ];
        let indices = group_checks_by_status(&checks);
        assert_eq!(indices, vec![1, 2, 0, 4, 3]);
    }

    #[test]
    fn group_checks_empty() {
        let indices = group_checks_by_status(&[]);
        assert!(indices.is_empty());
    }

    // --- EntryKind display tests ---

    #[test]
    fn entry_kind_review_display() {
        assert_eq!(EntryKind::Approved.display(), "Approved");
        assert_eq!(EntryKind::Review.display(), "Review");
    }

    #[test]
    fn entry_kind_comment_display() {
        assert_eq!(EntryKind::Comment.display(), "Comment");
    }

    // --- AuthorLabel display tests ---

    #[test]
    fn author_label_display() {
        assert_eq!(AuthorLabel::Ai.display(), "AI");
        assert_eq!(AuthorLabel::Bot.display(), "Bot");
        assert_eq!(AuthorLabel::None.display(), "–");
    }
}
