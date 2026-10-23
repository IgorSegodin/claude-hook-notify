//! Everything that touches Objective-C: the bundle check, notification
//! settings, the permission request and posting. Completion handlers arrive on
//! a background queue, so each call waits on a channel with a timeout; no run
//! loop is needed (spec section 12).

use std::fmt;
use std::ptr::NonNull;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use block2::RcBlock;
use objc2::runtime::Bool;
use objc2_foundation::{NSBundle, NSError, NSString, NSUUID};
use objc2_user_notifications::{
    UNAuthorizationOptions, UNAuthorizationStatus, UNMutableNotificationContent,
    UNNotificationRequest, UNNotificationSettings, UNNotificationSound, UNUserNotificationCenter,
};

use crate::spec::{NotificationSpec, Sound};
use crate::verbose;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthStatus {
    NotDetermined,
    Denied,
    Authorized,
    Provisional,
    Ephemeral,
    Unknown(i64),
}

impl AuthStatus {
    pub fn can_post(&self) -> bool {
        matches!(self, Self::Authorized | Self::Provisional)
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::NotDetermined => "not determined",
            Self::Denied => "denied",
            Self::Authorized => "authorized",
            Self::Provisional => "provisional",
            Self::Ephemeral => "ephemeral",
            Self::Unknown(_) => "unknown",
        }
    }

    fn from_raw(raw: UNAuthorizationStatus) -> Self {
        match raw {
            UNAuthorizationStatus::NotDetermined => Self::NotDetermined,
            UNAuthorizationStatus::Denied => Self::Denied,
            UNAuthorizationStatus::Authorized => Self::Authorized,
            UNAuthorizationStatus::Provisional => Self::Provisional,
            UNAuthorizationStatus::Ephemeral => Self::Ephemeral,
            other => Self::Unknown(other.0 as i64),
        }
    }
}

#[derive(Debug)]
pub enum MacError {
    NotInBundle,
    Timeout(&'static str),
    Denied,
    NotRequested,
    NotGranted,
    Delivery(String),
}

impl fmt::Display for MacError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotInBundle => write!(
                f,
                "claude-hook-notify must run from inside ClaudeHookNotify.app; run `make install` and call ~/.local/bin/claude-hook-notify"
            ),
            Self::Timeout(what) => write!(f, "timed out waiting for {what}"),
            Self::Denied => write!(
                f,
                "notifications are disabled for Claude Hook Notify in System Settings > Notifications; enable them there and retry"
            ),
            Self::NotRequested => write!(
                f,
                "notification permission has not been requested yet; run `~/.local/bin/claude-hook-notify --request-permission` once and answer the dialog"
            ),
            Self::NotGranted => write!(
                f,
                "notification permission was not granted; enable Claude Hook Notify in System Settings > Notifications and retry"
            ),
            Self::Delivery(e) => write!(f, "macOS refused the notification: {e}"),
        }
    }
}

impl std::error::Error for MacError {}

/// `CFBundleIdentifier` of the bundle this process runs from, if any.
pub fn bundle_identifier() -> Option<String> {
    NSBundle::mainBundle()
        .bundleIdentifier()
        .map(|s| s.to_string())
}

/// The bundle id, or the error telling the user how to run the tool properly.
/// Must be called before any UserNotifications call: outside a bundle those
/// raise an Objective-C exception.
pub fn require_bundle() -> Result<String, MacError> {
    bundle_identifier().ok_or(MacError::NotInBundle)
}

pub fn authorization_status(timeout: Duration) -> Result<AuthStatus, MacError> {
    let center = UNUserNotificationCenter::currentNotificationCenter();
    let (tx, rx) = mpsc::channel::<UNAuthorizationStatus>();
    let block = RcBlock::new(move |settings: NonNull<UNNotificationSettings>| {
        // SAFETY: the framework passes a valid, non-null settings object that
        // outlives this call.
        let status = unsafe { settings.as_ref() }.authorizationStatus();
        let _ = tx.send(status);
    });
    let started = Instant::now();
    center.getNotificationSettingsWithCompletionHandler(&block);
    let raw = rx
        .recv_timeout(timeout)
        .map_err(|_| MacError::Timeout("notification settings"))?;
    let status = AuthStatus::from_raw(raw);
    verbose::log(format!(
        "authorization status: {} (settings handler after {:?})",
        status.label(),
        started.elapsed()
    ));
    Ok(status)
}

/// Shows the system permission dialog and returns whether the user granted
/// permission. An error object from the handler (macOS reports "not allowed"
/// when the user declines) is logged and counts as not granted.
pub fn request_authorization(timeout: Duration) -> Result<bool, MacError> {
    let center = UNUserNotificationCenter::currentNotificationCenter();
    let (tx, rx) = mpsc::channel::<(bool, Option<String>)>();
    let block = RcBlock::new(move |granted: Bool, error: *mut NSError| {
        let _ = tx.send((granted.as_bool(), error_description(error)));
    });
    let started = Instant::now();
    center.requestAuthorizationWithOptions_completionHandler(
        UNAuthorizationOptions::Alert | UNAuthorizationOptions::Sound,
        &block,
    );
    let (granted, error) = rx
        .recv_timeout(timeout)
        .map_err(|_| MacError::Timeout("the permission dialog"))?;
    if let Some(e) = error {
        verbose::log(format!("requestAuthorization error: {e}"));
    }
    verbose::log(format!(
        "permission granted: {granted} (handler after {:?})",
        started.elapsed()
    ));
    Ok(granted)
}

/// Spec section 6.5 step 3: decide from the recorded status whether this
/// process may post. The permission dialog is never shown here. Only
/// `--request-permission` asks, because macOS records "denied" the moment a
/// process exits with its dialog unanswered (spec section 12), and a hook run
/// gives the user `--timeout` seconds while they are, by design, looking
/// elsewhere.
pub fn check_status(status: AuthStatus) -> Result<(), MacError> {
    match status {
        status if status.can_post() => Ok(()),
        AuthStatus::NotDetermined => Err(MacError::NotRequested),
        _ => Err(MacError::Denied),
    }
}

/// Bundle guard, settings read, then [`check_status`].
pub fn ensure_authorized(timeout: Duration) -> Result<(), MacError> {
    let id = require_bundle()?;
    verbose::log(format!("bundle id: {id}"));
    check_status(authorization_status(timeout)?)
}

/// Spec section 6.5 steps 4 and 5: build the content, add the request with no
/// trigger (deliver now) and wait for the completion handler.
pub fn post(spec: &NotificationSpec, timeout: Duration) -> Result<(), MacError> {
    let content = UNMutableNotificationContent::new();
    content.setTitle(&NSString::from_str(&spec.title));
    content.setSubtitle(&NSString::from_str(&spec.subtitle));
    content.setBody(&NSString::from_str(&spec.body));
    match &spec.sound {
        Sound::Default => content.setSound(Some(&UNNotificationSound::defaultSound())),
        Sound::None => content.setSound(None),
        Sound::Named(name) => content.setSound(Some(&UNNotificationSound::soundNamed(
            &NSString::from_str(name),
        ))),
    }
    if let Some(thread) = &spec.thread_id {
        content.setThreadIdentifier(&NSString::from_str(thread));
    }
    let identifier = NSUUID::UUID().UUIDString();
    let request =
        UNNotificationRequest::requestWithIdentifier_content_trigger(&identifier, &content, None);

    let center = UNUserNotificationCenter::currentNotificationCenter();
    let (tx, rx) = mpsc::channel::<Option<String>>();
    let block = RcBlock::new(move |error: *mut NSError| {
        let _ = tx.send(error_description(error));
    });
    let started = Instant::now();
    center.addNotificationRequest_withCompletionHandler(&request, Some(&block));
    match rx.recv_timeout(timeout) {
        Ok(None) => {
            verbose::log(format!("delivery confirmed after {:?}", started.elapsed()));
            Ok(())
        }
        Ok(Some(e)) => Err(MacError::Delivery(e)),
        Err(_) => Err(MacError::Timeout("delivery confirmation")),
    }
}

fn error_description(error: *mut NSError) -> Option<String> {
    if error.is_null() {
        None
    } else {
        // SAFETY: a non-null NSError from a completion handler is a valid
        // object for the duration of the handler.
        Some(unsafe { &*error }.localizedDescription().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn not_determined_is_not_requested_error() {
        match check_status(AuthStatus::NotDetermined) {
            Err(MacError::NotRequested) => {}
            other => panic!("expected NotRequested, got {other:?}"),
        }
    }

    #[test]
    fn not_requested_message_names_the_command() {
        let msg = MacError::NotRequested.to_string();
        assert!(msg.contains("--request-permission"), "{msg}");
    }

    #[test]
    fn authorized_and_provisional_pass() {
        assert!(check_status(AuthStatus::Authorized).is_ok());
        assert!(check_status(AuthStatus::Provisional).is_ok());
    }

    #[test]
    fn denied_ephemeral_and_unknown_are_denied() {
        for status in [
            AuthStatus::Denied,
            AuthStatus::Ephemeral,
            AuthStatus::Unknown(9),
        ] {
            match check_status(status) {
                Err(MacError::Denied) => {}
                other => panic!("expected Denied for {status:?}, got {other:?}"),
            }
        }
    }
}
