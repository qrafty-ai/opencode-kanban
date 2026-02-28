//! Notification backend module for system and tmux notifications

use crate::tmux::{tmux_broadcast_to_sessions, tmux_list_sessions};
use crate::types::Task;
use std::str::FromStr;
use tracing::{debug, warn};

/// Notification backend types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NotificationBackend {
    /// No notifications
    None,
    /// Tmux notifications only (broadcast to all tmux sessions)
    #[default]
    Tmux,
    /// System notifications only (via notify-rust)
    System,
    /// Both tmux and system notifications
    Both,
}

impl NotificationBackend {
    /// Parse backend from settings value (case-insensitive)
    pub fn from_settings_value(s: &str) -> Option<Self> {
        Self::from_str(s).ok()
    }

    /// Convert backend to string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Tmux => "tmux",
            Self::System => "system",
            Self::Both => "both",
        }
    }

    /// Get the next backend in cycling order: tmux -> both -> system -> none -> tmux
    pub fn next(&self) -> Self {
        match self {
            Self::Tmux => Self::Both,
            Self::Both => Self::System,
            Self::System => Self::None,
            Self::None => Self::Tmux,
        }
    }

    /// Get the previous backend in cycling order
    pub fn previous(&self) -> Self {
        match self {
            Self::Tmux => Self::None,
            Self::None => Self::System,
            Self::System => Self::Both,
            Self::Both => Self::Tmux,
        }
    }
}

impl FromStr for NotificationBackend {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "none" => Ok(Self::None),
            "tmux" => Ok(Self::Tmux),
            "system" => Ok(Self::System),
            "both" => Ok(Self::Both),
            _ => Err(()),
        }
    }
}

/// Send task completion notification via configured backend(s)
pub fn notify_task_completion(
    task: &Task,
    backend: NotificationBackend,
    notification_display_duration_ms: u64,
) {
    let (send_tmux, send_system) = backend_targets(backend);
    if !send_tmux && !send_system {
        debug!(task_id = %task.id, "notification skipped (backend is none)");
        return;
    }

    let message = format!("✓ Task completed | {}:{}", task.branch, task.title);

    if send_tmux {
        send_tmux_notification(task, &message, notification_display_duration_ms);
    }

    if send_system {
        send_system_notification(task, &message, notification_display_duration_ms);
    }
}

fn backend_targets(backend: NotificationBackend) -> (bool, bool) {
    match backend {
        NotificationBackend::None => (false, false),
        NotificationBackend::Tmux => (true, false),
        NotificationBackend::System => (false, true),
        NotificationBackend::Both => (true, true),
    }
}

fn send_tmux_notification(task: &Task, message: &str, notification_display_duration_ms: u64) {
    let sessions = tmux_list_sessions()
        .into_iter()
        .map(|session| session.name)
        .collect::<Vec<_>>();

    if sessions.is_empty() {
        debug!(task_id = %task.id, "no tmux sessions to broadcast notification");
        return;
    }

    debug!(
        task_id = %task.id,
        session_count = sessions.len(),
        notification_display_duration_ms,
        message = %message,
        "broadcasting completion notification to tmux sessions"
    );

    if let Err(err) =
        tmux_broadcast_to_sessions(&sessions, message, notification_display_duration_ms)
    {
        warn!(error = %err, "failed to broadcast tmux notification");
    }
}

fn send_system_notification(task: &Task, message: &str, notification_display_duration_ms: u64) {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        let timeout_ms = notification_display_duration_ms.min(u32::MAX as u64) as u32;
        debug!(
            task_id = %task.id,
            message = %message,
            timeout_ms,
            "sending system notification"
        );

        let notification_result = notify_rust::Notification::new()
            .summary("OpenCode Kanban")
            .body(message)
            .icon("dialog-information")
            .timeout(notify_rust::Timeout::Milliseconds(timeout_ms))
            .show();

        match notification_result {
            Ok(_) => {
                debug!(task_id = %task.id, "system notification sent successfully");
            }
            Err(err) => {
                warn!(error = %err, "failed to send system notification");
            }
        }
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        debug!(
            task_id = %task.id,
            "system notifications not supported on this OS"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_notification_backend_from_str() {
        assert_eq!(
            NotificationBackend::from_settings_value("tmux"),
            Some(NotificationBackend::Tmux)
        );
        assert_eq!(
            NotificationBackend::from_settings_value("Tmux"),
            Some(NotificationBackend::Tmux)
        );
        assert_eq!(
            NotificationBackend::from_settings_value("TMUX"),
            Some(NotificationBackend::Tmux)
        );
        assert_eq!(
            NotificationBackend::from_settings_value("system"),
            Some(NotificationBackend::System)
        );
        assert_eq!(
            NotificationBackend::from_settings_value("System"),
            Some(NotificationBackend::System)
        );
        assert_eq!(
            NotificationBackend::from_settings_value("both"),
            Some(NotificationBackend::Both)
        );
        assert_eq!(
            NotificationBackend::from_settings_value("Both"),
            Some(NotificationBackend::Both)
        );
        assert_eq!(
            NotificationBackend::from_settings_value("none"),
            Some(NotificationBackend::None)
        );
        assert_eq!(
            NotificationBackend::from_settings_value("None"),
            Some(NotificationBackend::None)
        );
        assert_eq!(NotificationBackend::from_settings_value("invalid"), None);
        assert_eq!(NotificationBackend::from_settings_value(""), None);
    }

    #[test]
    fn test_notification_backend_as_str() {
        assert_eq!(NotificationBackend::Tmux.as_str(), "tmux");
        assert_eq!(NotificationBackend::System.as_str(), "system");
        assert_eq!(NotificationBackend::Both.as_str(), "both");
        assert_eq!(NotificationBackend::None.as_str(), "none");
    }

    #[test]
    fn test_notification_backend_next() {
        assert_eq!(NotificationBackend::Tmux.next(), NotificationBackend::Both);
        assert_eq!(
            NotificationBackend::Both.next(),
            NotificationBackend::System
        );
        assert_eq!(
            NotificationBackend::System.next(),
            NotificationBackend::None
        );
        assert_eq!(NotificationBackend::None.next(), NotificationBackend::Tmux);
    }

    #[test]
    fn test_notification_backend_previous() {
        assert_eq!(
            NotificationBackend::Tmux.previous(),
            NotificationBackend::None
        );
        assert_eq!(
            NotificationBackend::None.previous(),
            NotificationBackend::System
        );
        assert_eq!(
            NotificationBackend::System.previous(),
            NotificationBackend::Both
        );
        assert_eq!(
            NotificationBackend::Both.previous(),
            NotificationBackend::Tmux
        );
    }

    #[test]
    fn test_notification_backend_default() {
        assert_eq!(NotificationBackend::default(), NotificationBackend::Tmux);
    }

    #[test]
    fn test_backend_targets() {
        assert_eq!(backend_targets(NotificationBackend::None), (false, false));
        assert_eq!(backend_targets(NotificationBackend::Tmux), (true, false));
        assert_eq!(backend_targets(NotificationBackend::System), (false, true));
        assert_eq!(backend_targets(NotificationBackend::Both), (true, true));
    }

    #[test]
    fn test_notification_backend_roundtrip() {
        for backend in [
            NotificationBackend::None,
            NotificationBackend::Tmux,
            NotificationBackend::System,
            NotificationBackend::Both,
        ] {
            let s = backend.as_str();
            let parsed = NotificationBackend::from_settings_value(s);
            assert_eq!(parsed, Some(backend), "roundtrip failed for {}", s);
        }
    }
}
