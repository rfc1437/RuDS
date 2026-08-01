/// Toast notification state.
///
/// Toasts are messages shown at the top of the workspace. Errors remain until
/// dismissed; other levels expire automatically.
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_TOAST_ID: AtomicU64 = AtomicU64::new(1);

/// Severity determines the visual style.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastLevel {
    Info,
    Success,
    Warning,
    Error,
}

/// A single toast notification.
#[derive(Debug, Clone)]
pub struct Toast {
    pub id: u64,
    pub level: ToastLevel,
    pub message: String,
    /// Unix-millis when this toast was created.
    pub created_at: u64,
}

impl Toast {
    /// Default display duration in milliseconds.
    pub const DEFAULT_DURATION_MS: u64 = 4000;

    pub fn new(level: ToastLevel, message: String) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        Self {
            id: NEXT_TOAST_ID.fetch_add(1, Ordering::Relaxed),
            level,
            message,
            created_at: now,
        }
    }

    /// Whether this toast has exceeded its display duration.
    pub fn is_expired(&self) -> bool {
        if self.level == ToastLevel::Error {
            return false;
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        now.saturating_sub(self.created_at) >= Self::DEFAULT_DURATION_MS
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toast_ids_are_unique() {
        let a = Toast::new(ToastLevel::Info, "a".into());
        let b = Toast::new(ToastLevel::Info, "b".into());
        assert_ne!(a.id, b.id);
    }

    #[test]
    fn fresh_toast_not_expired() {
        let t = Toast::new(ToastLevel::Info, "test".into());
        assert!(!t.is_expired());
    }

    #[test]
    fn error_toast_never_expires_automatically() {
        let mut toast = Toast::new(ToastLevel::Error, "important".into());
        toast.created_at = 0;
        assert!(!toast.is_expired());
    }
}
