use std::fmt;

/// Which list the sidebar is currently showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidebarView {
    Posts,
    Pages,
    Media,
    Scripts,
    Templates,
    Tags,
    Chat,
    Import,
    Git,
    Settings,
}

impl fmt::Display for SidebarView {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let key = self.i18n_key();
        f.write_str(key)
    }
}

impl SidebarView {
    /// Returns the `activity.*` i18n key for this sidebar view.
    pub fn i18n_key(&self) -> &'static str {
        match self {
            Self::Posts => "activity.posts",
            Self::Pages => "activity.pages",
            Self::Media => "activity.media",
            Self::Scripts => "activity.scripts",
            Self::Templates => "activity.templates",
            Self::Tags => "activity.tags",
            Self::Chat => "activity.aiAssistant",
            Self::Import => "activity.import",
            Self::Git => "activity.sourceControl",
            Self::Settings => "common.settings",
        }
    }
}

/// Which tab is selected in the bottom panel.
///
/// Per layout.allium: tasks, output, post_links, git_log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelTab {
    Tasks,
    Output,
    PostLinks,
    GitLog,
}

/// A snapshot of a running or completed task shown in the panel.
#[derive(Debug, Clone)]
pub struct TaskSnapshot {
    pub id: u64,
    pub label: String,
    pub group_id: Option<String>,
    pub group_name: Option<String>,
    pub status: String,
    pub progress: Option<f32>,
    pub message: Option<String>,
    pub is_cancellable: bool,
}

/// A single line of output shown in the panel.
#[derive(Debug, Clone)]
pub struct OutputEntry {
    pub timestamp: i64,
    pub text: String,
}

/// Determine the next `(SidebarView, sidebar_visible)` after a user clicks
/// an activity-bar icon.
///
/// * Same icon + visible  -> toggle sidebar off
/// * Same icon + hidden   -> toggle sidebar on
/// * Different icon       -> switch view and ensure visible
pub fn handle_activity_click(
    current_view: SidebarView,
    sidebar_visible: bool,
    clicked: SidebarView,
) -> (SidebarView, bool) {
    if clicked == current_view {
        (current_view, !sidebar_visible)
    } else {
        (clicked, true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggle_off_when_same_and_visible() {
        let (view, visible) = handle_activity_click(SidebarView::Posts, true, SidebarView::Posts);
        assert_eq!(view, SidebarView::Posts);
        assert!(!visible);
    }

    #[test]
    fn toggle_on_when_same_and_hidden() {
        let (view, visible) = handle_activity_click(SidebarView::Posts, false, SidebarView::Posts);
        assert_eq!(view, SidebarView::Posts);
        assert!(visible);
    }

    #[test]
    fn switch_view_when_different() {
        let (view, visible) = handle_activity_click(SidebarView::Posts, true, SidebarView::Media);
        assert_eq!(view, SidebarView::Media);
        assert!(visible);
    }

    #[test]
    fn switch_view_when_different_and_hidden() {
        let (view, visible) =
            handle_activity_click(SidebarView::Posts, false, SidebarView::Settings);
        assert_eq!(view, SidebarView::Settings);
        assert!(visible);
    }

    #[test]
    fn display_returns_i18n_key() {
        assert_eq!(SidebarView::Posts.to_string(), "activity.posts");
        assert_eq!(SidebarView::Settings.to_string(), "common.settings");
    }
}
