//! The notification to post, and the pure step that builds it from command
//! line inputs and (in hook mode) the stdin payload.

use serde::{Serialize, Serializer};

use crate::hook;

/// What `--sound` resolves to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Sound {
    Default,
    None,
    Named(String),
}

impl Sound {
    /// `default` (or empty) and `none` are keywords, case-insensitive; anything
    /// else is a system sound name passed through verbatim.
    pub fn parse(s: &str) -> Sound {
        if s.is_empty() || s.eq_ignore_ascii_case("default") {
            Sound::Default
        } else if s.eq_ignore_ascii_case("none") {
            Sound::None
        } else {
            Sound::Named(s.to_string())
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Sound::Default => "default",
            Sound::None => "none",
            Sound::Named(name) => name,
        }
    }
}

impl Serialize for Sound {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

/// Everything the macOS layer needs to post one notification. Field order is
/// the `--dry-run` JSON key order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NotificationSpec {
    pub title: String,
    pub subtitle: String,
    pub body: String,
    pub sound: Sound,
    pub thread_id: Option<String>,
}

impl NotificationSpec {
    /// One compact JSON object, no trailing newline.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("NotificationSpec is serializable")
    }
}

/// Command line inputs that influence the spec. `stdin_json` is `Some(raw)`
/// in hook mode.
pub struct Inputs<'a> {
    pub message: Option<&'a str>,
    pub title: Option<&'a str>,
    pub subtitle: Option<&'a str>,
    pub sound: &'a str,
    pub stdin_json: Option<&'a str>,
}

/// Builds the spec. In hook mode the payload supplies title, subtitle, body and
/// thread; `--title` / `--subtitle` override the first two.
pub fn build_spec(inputs: &Inputs) -> NotificationSpec {
    let sound = Sound::parse(inputs.sound);
    match inputs.stdin_json {
        Some(raw) => {
            let derived = hook::derive(raw);
            NotificationSpec {
                title: inputs.title.map(str::to_string).unwrap_or(derived.title),
                subtitle: inputs
                    .subtitle
                    .map(str::to_string)
                    .unwrap_or(derived.subtitle),
                body: derived.body,
                sound,
                thread_id: derived.thread_id,
            }
        }
        None => NotificationSpec {
            title: inputs.title.unwrap_or("").to_string(),
            subtitle: inputs.subtitle.unwrap_or("").to_string(),
            body: inputs.message.unwrap_or("").to_string(),
            sound,
            thread_id: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sound_parse_keywords_are_case_insensitive() {
        assert_eq!(Sound::parse("default"), Sound::Default);
        assert_eq!(Sound::parse("DEFAULT"), Sound::Default);
        assert_eq!(Sound::parse("none"), Sound::None);
        assert_eq!(Sound::parse("None"), Sound::None);
        assert_eq!(Sound::parse("Glass"), Sound::Named("Glass".into()));
    }

    #[test]
    fn sound_empty_is_default() {
        assert_eq!(Sound::parse(""), Sound::Default);
    }

    #[test]
    fn plain_mode_has_empty_title_and_no_thread() {
        let s = build_spec(&Inputs {
            message: Some("hello"),
            title: None,
            subtitle: None,
            sound: "default",
            stdin_json: None,
        });
        assert_eq!(
            s,
            NotificationSpec {
                title: "".into(),
                subtitle: "".into(),
                body: "hello".into(),
                sound: Sound::Default,
                thread_id: None
            }
        );
    }

    #[test]
    fn plain_mode_uses_title_subtitle_and_sound() {
        let s = build_spec(&Inputs {
            message: Some("b"),
            title: Some("T"),
            subtitle: Some("S"),
            sound: "Glass",
            stdin_json: None,
        });
        assert_eq!(
            (s.title.as_str(), s.subtitle.as_str(), s.sound.as_str()),
            ("T", "S", "Glass")
        );
    }

    #[test]
    fn hook_mode_derives_from_json() {
        let s = build_spec(&Inputs {
            message: None,
            title: None,
            subtitle: None,
            sound: "none",
            stdin_json: Some(
                r#"{"session_id":"abc","cwd":"/p/demo","hook_event_name":"Stop","last_assistant_message":"ok"}"#,
            ),
        });
        assert_eq!(
            s,
            NotificationSpec {
                title: "Finished".into(),
                subtitle: "demo".into(),
                body: "ok".into(),
                sound: Sound::None,
                thread_id: Some("abc".into())
            }
        );
    }

    #[test]
    fn hook_mode_cli_overrides_win() {
        let s = build_spec(&Inputs {
            message: None,
            title: Some("T"),
            subtitle: Some("S"),
            sound: "default",
            stdin_json: Some(r#"{"cwd":"/p/demo","hook_event_name":"Stop"}"#),
        });
        assert_eq!(
            (s.title.as_str(), s.subtitle.as_str(), s.body.as_str()),
            ("T", "S", "Done.")
        );
    }

    #[test]
    fn to_json_shape_and_key_order() {
        let s = build_spec(&Inputs {
            message: Some("hello"),
            title: None,
            subtitle: None,
            sound: "default",
            stdin_json: None,
        });
        assert_eq!(
            s.to_json(),
            r#"{"title":"","subtitle":"","body":"hello","sound":"default","thread_id":null}"#
        );
        let n = NotificationSpec {
            title: "a".into(),
            subtitle: "b".into(),
            body: "c".into(),
            sound: Sound::Named("Glass".into()),
            thread_id: Some("t".into()),
        };
        assert_eq!(
            n.to_json(),
            r#"{"title":"a","subtitle":"b","body":"c","sound":"Glass","thread_id":"t"}"#
        );
    }
}
