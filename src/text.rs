//! Pure text helpers used when turning hook payloads into notification fields.

use std::path::Path;

/// Maximum number of characters a snippet keeps before it is cut.
pub const SNIPPET_MAX_CHARS: usize = 200;

/// Collapses whitespace runs to one space, trims, and cuts to
/// [`SNIPPET_MAX_CHARS`] characters with a trailing ellipsis when cut.
pub fn snippet(s: &str) -> String {
    let joined = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if joined.chars().count() > SNIPPET_MAX_CHARS {
        let mut cut: String = joined.chars().take(SNIPPET_MAX_CHARS).collect();
        cut.push('\u{2026}');
        cut
    } else {
        joined
    }
}

/// `permission_prompt` -> `Permission prompt`: underscores become spaces and
/// the first character is upper-cased; the rest is left as is.
pub fn humanize_type(s: &str) -> String {
    let spaced = s.replace('_', " ");
    let mut chars = spaced.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// `PreToolUse` -> `Pre tool use`: splits CamelCase, lower-cases, then
/// applies [`humanize_type`].
pub fn humanize_event(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    let mut prev: Option<char> = None;
    for c in s.chars() {
        if c.is_uppercase() && prev.is_some_and(|p| p.is_lowercase() || p.is_ascii_digit()) {
            out.push(' ');
        }
        out.extend(c.to_lowercase());
        prev = Some(c);
    }
    humanize_type(&out)
}

/// Last path component of `cwd`, or empty when missing or when the path has
/// no final component (such as `/`).
pub fn project_name(cwd: Option<&str>) -> String {
    cwd.and_then(|c| Path::new(c).file_name())
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snippet_collapses_whitespace_and_trims() {
        assert_eq!(snippet("  a \n\n b\t c  "), "a b c");
    }

    #[test]
    fn snippet_empty_or_blank_is_empty() {
        assert_eq!(snippet(""), "");
        assert_eq!(snippet(" \n\t"), "");
    }

    #[test]
    fn snippet_exactly_200_is_unchanged() {
        let s = "x".repeat(200);
        assert_eq!(snippet(&s), s);
    }

    #[test]
    fn snippet_cuts_at_200_and_appends_ellipsis() {
        let out = snippet(&"x".repeat(250));
        assert_eq!(out.chars().count(), 201);
        assert!(out.ends_with('…'));
        assert!(out.starts_with(&"x".repeat(200)));
    }

    #[test]
    fn snippet_cuts_on_char_boundary() {
        let out = snippet(&"é".repeat(250));
        assert_eq!(out, format!("{}…", "é".repeat(200)));
    }

    #[test]
    fn humanize_type_replaces_underscores_and_capitalizes() {
        assert_eq!(humanize_type("permission_prompt"), "Permission prompt");
        assert_eq!(
            humanize_type("quota_auto_resume_fired"),
            "Quota auto resume fired"
        );
        assert_eq!(humanize_type(""), "");
    }

    #[test]
    fn humanize_event_splits_camel_case() {
        assert_eq!(humanize_event("PreToolUse"), "Pre tool use");
        assert_eq!(humanize_event("Stop"), "Stop");
        assert_eq!(humanize_event("SessionStart"), "Session start");
        assert_eq!(humanize_event("user_prompt"), "User prompt");
    }

    #[test]
    fn project_name_is_last_component() {
        assert_eq!(project_name(Some("/Users/me/projects/demo")), "demo");
        assert_eq!(project_name(Some("/Users/me/projects/demo/")), "demo");
    }

    #[test]
    fn project_name_root_is_empty() {
        assert_eq!(project_name(Some("/")), "");
        assert_eq!(project_name(Some("")), "");
    }

    #[test]
    fn project_name_missing_is_empty() {
        assert_eq!(project_name(None), "");
    }
}
