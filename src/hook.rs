//! Claude Code hook payloads (one JSON object on stdin) and the mapping from
//! them to notification title, subtitle, body and thread (spec section 6.3).

use serde::Deserialize;

use crate::text::{humanize_event, humanize_type, project_name, snippet};

/// The subset of hook fields this tool reads. Unknown fields are ignored; a
/// field of the wrong type makes the whole payload unparseable, which lands
/// in the fallback row.
#[derive(Deserialize)]
struct HookPayload {
    hook_event_name: Option<String>,
    session_id: Option<String>,
    cwd: Option<String>,
    last_assistant_message: Option<String>,
    message: Option<String>,
    title: Option<String>,
    notification_type: Option<String>,
    agent_type: Option<String>,
}

/// Notification fields derived from a hook payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Derived {
    pub title: String,
    pub subtitle: String,
    pub body: String,
    pub thread_id: Option<String>,
}

/// Maps raw stdin to notification fields per the tables in spec section 6.3.
pub fn derive(stdin: &str) -> Derived {
    let payload = serde_json::from_str::<HookPayload>(stdin).ok();
    let event = payload
        .as_ref()
        .and_then(|p| p.hook_event_name.as_deref())
        .filter(|e| !e.is_empty());
    match (payload.as_ref(), event) {
        (Some(p), Some(event)) => derive_event(p, event),
        (p, _) => fallback(stdin, p),
    }
}

fn derive_event(p: &HookPayload, event: &str) -> Derived {
    let subtitle = project_name(p.cwd.as_deref());
    let thread_id = p.session_id.clone();
    let (title, body) = match event {
        "Stop" => {
            let body = snippet(p.last_assistant_message.as_deref().unwrap_or(""));
            let body = if body.is_empty() {
                "Done.".to_string()
            } else {
                body
            };
            ("Finished".to_string(), body)
        }
        "Notification" => {
            let title = p
                .title
                .as_deref()
                .map(str::trim)
                .filter(|t| !t.is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| title_for_type(p.notification_type.as_deref()));
            (title, snippet(p.message.as_deref().unwrap_or("")))
        }
        "SubagentStop" => (
            "Subagent finished".to_string(),
            snippet(p.agent_type.as_deref().unwrap_or("")),
        ),
        other => (humanize_event(other), String::new()),
    };
    Derived {
        title,
        subtitle,
        body,
        thread_id,
    }
}

fn fallback(raw: &str, payload: Option<&HookPayload>) -> Derived {
    let mut body = snippet(raw);
    if body.is_empty() {
        body = "(empty hook payload)".to_string();
    }
    Derived {
        title: "Claude Code".to_string(),
        subtitle: String::new(),
        body,
        thread_id: payload.and_then(|p| p.session_id.clone()),
    }
}

fn title_for_type(notification_type: Option<&str>) -> String {
    match notification_type {
        None => "Claude Code".to_string(),
        Some("permission_prompt") => "Permission needed".to_string(),
        Some("idle_prompt") => "Waiting for input".to_string(),
        Some("elicitation_dialog" | "elicitation_url_dialog") => "Input needed".to_string(),
        Some("agent_needs_input") => "Agent needs input".to_string(),
        Some("agent_completed") => "Agent finished".to_string(),
        Some(other) => humanize_type(other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(json: &str) -> Derived {
        derive(json)
    }

    #[test]
    fn stop_uses_finished_project_and_snippet() {
        let r = d(
            r#"{"session_id":"abc123","cwd":"/Users/me/projects/demo","hook_event_name":"Stop","last_assistant_message":"I updated the README.\n\nAll 12 tests pass."}"#,
        );
        assert_eq!(
            (r.title.as_str(), r.subtitle.as_str(), r.body.as_str()),
            (
                "Finished",
                "demo",
                "I updated the README. All 12 tests pass."
            )
        );
        assert_eq!(r.thread_id.as_deref(), Some("abc123"));
    }

    #[test]
    fn stop_without_message_says_done() {
        for payload in [
            r#"{"hook_event_name":"Stop"}"#,
            r#"{"hook_event_name":"Stop","last_assistant_message":""}"#,
            r#"{"hook_event_name":"Stop","last_assistant_message":" \n "}"#,
        ] {
            assert_eq!(d(payload).body, "Done.", "{payload}");
        }
    }

    #[test]
    fn notification_uses_payload_title_when_non_empty() {
        let r = d(
            r#"{"cwd":"/x/demo","hook_event_name":"Notification","title":"Custom","message":"hi","notification_type":"permission_prompt"}"#,
        );
        assert_eq!(
            (r.title.as_str(), r.subtitle.as_str(), r.body.as_str()),
            ("Custom", "demo", "hi")
        );
    }

    #[test]
    fn notification_empty_title_falls_back_to_type() {
        let r = d(
            r#"{"hook_event_name":"Notification","title":"  ","notification_type":"permission_prompt"}"#,
        );
        assert_eq!(r.title, "Permission needed");
    }

    #[test]
    fn notification_titles_by_type() {
        let cases = [
            ("permission_prompt", "Permission needed"),
            ("idle_prompt", "Waiting for input"),
            ("elicitation_dialog", "Input needed"),
            ("elicitation_url_dialog", "Input needed"),
            ("agent_needs_input", "Agent needs input"),
            ("agent_completed", "Agent finished"),
            ("plugin_notification", "Plugin notification"),
            ("quota_auto_resume_fired", "Quota auto resume fired"),
        ];
        for (t, want) in cases {
            let payload =
                format!(r#"{{"hook_event_name":"Notification","notification_type":"{t}"}}"#);
            assert_eq!(d(&payload).title, want, "{t}");
        }
    }

    #[test]
    fn notification_without_type_is_claude_code() {
        assert_eq!(
            d(r#"{"hook_event_name":"Notification"}"#).title,
            "Claude Code"
        );
    }

    #[test]
    fn notification_body_is_message_or_empty() {
        assert_eq!(
            d(r#"{"hook_event_name":"Notification","message":"Claude needs your permission to use Bash"}"#).body,
            "Claude needs your permission to use Bash"
        );
        assert_eq!(d(r#"{"hook_event_name":"Notification"}"#).body, "");
    }

    #[test]
    fn subagent_stop_body_is_agent_type() {
        let r = d(r#"{"cwd":"/x/demo","hook_event_name":"SubagentStop","agent_type":"Explore"}"#);
        assert_eq!(
            (r.title.as_str(), r.subtitle.as_str(), r.body.as_str()),
            ("Subagent finished", "demo", "Explore")
        );
        assert_eq!(d(r#"{"hook_event_name":"SubagentStop"}"#).body, "");
    }

    #[test]
    fn unknown_event_is_humanized_with_empty_body() {
        let r = d(r#"{"cwd":"/x/demo","hook_event_name":"PreToolUse","tool_name":"Bash"}"#);
        assert_eq!(
            (r.title.as_str(), r.subtitle.as_str(), r.body.as_str()),
            ("Pre tool use", "demo", "")
        );
    }

    #[test]
    fn missing_cwd_gives_empty_subtitle() {
        assert_eq!(d(r#"{"hook_event_name":"Stop"}"#).subtitle, "");
    }

    #[test]
    fn invalid_json_falls_back_to_raw_body() {
        let r = d("not json at all");
        assert_eq!(
            (r.title.as_str(), r.subtitle.as_str(), r.body.as_str()),
            ("Claude Code", "", "not json at all")
        );
        assert_eq!(r.thread_id, None);
    }

    #[test]
    fn non_object_json_falls_back() {
        for raw in ["[1,2]", "\"text\"", "42"] {
            let r = d(raw);
            assert_eq!(r.title, "Claude Code", "{raw}");
            assert_eq!(r.body, raw, "{raw}");
        }
    }

    #[test]
    fn object_without_event_name_falls_back_but_keeps_session() {
        let r = d(r#"{"session_id":"abc123","cwd":"/x/demo"}"#);
        assert_eq!((r.title.as_str(), r.subtitle.as_str()), ("Claude Code", ""));
        assert_eq!(r.body, r#"{"session_id":"abc123","cwd":"/x/demo"}"#);
        assert_eq!(r.thread_id.as_deref(), Some("abc123"));
    }

    #[test]
    fn empty_event_name_falls_back() {
        assert_eq!(d(r#"{"hook_event_name":""}"#).title, "Claude Code");
    }

    #[test]
    fn empty_stdin_has_placeholder_body() {
        for raw in ["", "  \n"] {
            let r = d(raw);
            assert_eq!(
                (r.title.as_str(), r.body.as_str()),
                ("Claude Code", "(empty hook payload)"),
                "{raw:?}"
            );
        }
    }

    #[test]
    fn long_raw_stdin_is_cut() {
        let r = d(&"y".repeat(300));
        assert_eq!(r.body.chars().count(), 201);
        assert!(r.body.ends_with('…'));
    }
}
