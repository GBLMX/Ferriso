//! Desktop notifications with a system backend and an in-memory test double.

use std::{error::Error as StdError, fmt, sync::Mutex};

use notify_rust::Notification as SystemNotification;

/// A notification title and body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notification {
    /// Short summary shown by the notification service.
    pub title: String,
    /// Optional detail body.
    pub body: String,
}

impl Notification {
    /// Build a notification.
    pub fn new(title: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            body: body.into(),
        }
    }
}

/// Deliver desktop notifications.
pub trait Notifier: Send + Sync + 'static {
    /// Show one notification.
    ///
    /// Some platform backends may block while contacting the desktop service;
    /// callers should run this from a blocking task when that matters.
    fn notify(&self, notification: &Notification) -> Result<(), NotificationError>;
}

/// The host desktop notification service through [`notify_rust`].
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemNotifier;

impl SystemNotifier {
    /// Create a stateless system notifier.
    pub const fn new() -> Self {
        Self
    }
}

impl Notifier for SystemNotifier {
    fn notify(&self, notification: &Notification) -> Result<(), NotificationError> {
        SystemNotification::new()
            .summary(&notification.title)
            .body(&notification.body)
            .show()
            .map(|_| ())
            .map_err(NotificationError::Backend)
    }
}

/// An in-memory notifier for headless tests and previews.
#[derive(Debug, Default)]
pub struct MemoryNotifier {
    notifications: Mutex<Vec<Notification>>,
}

impl MemoryNotifier {
    /// Create an empty in-memory notifier.
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot every notification delivered so far.
    pub fn notifications(&self) -> Result<Vec<Notification>, NotificationError> {
        self.notifications
            .lock()
            .map(|notifications| notifications.clone())
            .map_err(|_| NotificationError::Poisoned)
    }
}

impl Notifier for MemoryNotifier {
    fn notify(&self, notification: &Notification) -> Result<(), NotificationError> {
        self.notifications
            .lock()
            .map_err(|_| NotificationError::Poisoned)?
            .push(notification.clone());
        Ok(())
    }
}

/// Notification delivery failure.
#[derive(Debug)]
pub enum NotificationError {
    /// The desktop backend failed.
    Backend(notify_rust::error::Error),
    /// An internal lock was poisoned after a panic.
    Poisoned,
}

impl fmt::Display for NotificationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Backend(error) => write!(formatter, "notification backend failed: {error}"),
            Self::Poisoned => write!(formatter, "notification state is poisoned"),
        }
    }
}

impl StdError for NotificationError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::Backend(error) => Some(error),
            Self::Poisoned => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_notifier_records_notifications_in_order() {
        let notifier = MemoryNotifier::new();
        notifier
            .notify(&Notification::new("First", "body one"))
            .expect("first");
        notifier
            .notify(&Notification::new("Second", "body two"))
            .expect("second");

        assert_eq!(
            notifier.notifications().expect("notifications"),
            [
                Notification::new("First", "body one"),
                Notification::new("Second", "body two"),
            ]
        );
    }
}
