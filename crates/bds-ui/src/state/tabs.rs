/// The kind of content a tab holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TabType {
    Post,
    Media,
    Settings,
    Style,
    Tags,
    Chat,
    Import,
    MenuEditor,
    MetadataDiff,
    Scripts,
    Templates,
    Documentation,
    SiteValidation,
    TranslationValidation,
}

impl TabType {
    /// Singleton tabs may only appear once in the tab bar.
    /// Every type except `Post` and `Media` is a singleton.
    pub fn is_singleton(&self) -> bool {
        !matches!(self, Self::Post | Self::Media)
    }
}

/// A single tab in the editor area.
#[derive(Debug, Clone)]
pub struct Tab {
    pub id: String,
    pub tab_type: TabType,
    pub title: String,
    pub is_transient: bool,
}

/// Open (or focus) a tab in the tab list.
///
/// * **Singleton** — if a tab with the same `TabType` already exists, return
///   its index instead of inserting a duplicate.
/// * **Transient** — replace an existing transient tab of the same type, or
///   append if none exists.
/// * Otherwise — append unconditionally.
///
/// Returns the index of the resulting tab.
pub fn open_tab(tabs: &mut Vec<Tab>, new_tab: Tab) -> usize {
    if new_tab.tab_type.is_singleton() {
        if let Some(idx) = tabs.iter().position(|t| t.tab_type == new_tab.tab_type) {
            return idx;
        }
    }

    if new_tab.is_transient {
        if let Some(idx) = tabs
            .iter()
            .position(|t| t.tab_type == new_tab.tab_type && t.is_transient)
        {
            tabs[idx] = new_tab;
            return idx;
        }
    }

    tabs.push(new_tab);
    tabs.len() - 1
}

/// Remove the tab whose `id` matches and return the index that should be
/// selected next.
///
/// Prefers the same index (now occupied by the next tab), clamped to
/// `len - 1`.  Returns `None` when the list is empty after removal.
pub fn close_tab(tabs: &mut Vec<Tab>, id: &str) -> Option<usize> {
    let pos = tabs.iter().position(|t| t.id == id)?;
    tabs.remove(pos);
    if tabs.is_empty() {
        None
    } else {
        Some(pos.min(tabs.len() - 1))
    }
}

/// Mark a transient tab as permanent (non-transient).
pub fn pin_tab(tabs: &mut Vec<Tab>, id: &str) {
    if let Some(tab) = tabs.iter_mut().find(|t| t.id == id) {
        tab.is_transient = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_tab(id: &str, tab_type: TabType, transient: bool) -> Tab {
        Tab {
            id: id.to_string(),
            tab_type,
            title: id.to_string(),
            is_transient: transient,
        }
    }

    #[test]
    fn singleton_dedup() {
        let mut tabs = Vec::new();
        let idx1 = open_tab(&mut tabs, make_tab("s1", TabType::Settings, false));
        let idx2 = open_tab(&mut tabs, make_tab("s2", TabType::Settings, false));
        assert_eq!(idx1, idx2);
        assert_eq!(tabs.len(), 1);
        assert_eq!(tabs[0].id, "s1");
    }

    #[test]
    fn transient_replacement() {
        let mut tabs = Vec::new();
        open_tab(&mut tabs, make_tab("p1", TabType::Post, true));
        assert_eq!(tabs.len(), 1);
        let idx = open_tab(&mut tabs, make_tab("p2", TabType::Post, true));
        assert_eq!(tabs.len(), 1);
        assert_eq!(tabs[idx].id, "p2");
    }

    #[test]
    fn pin_makes_permanent() {
        let mut tabs = Vec::new();
        open_tab(&mut tabs, make_tab("p1", TabType::Post, true));
        assert!(tabs[0].is_transient);
        pin_tab(&mut tabs, "p1");
        assert!(!tabs[0].is_transient);
    }

    #[test]
    fn close_selects_neighbor() {
        let mut tabs = Vec::new();
        open_tab(&mut tabs, make_tab("a", TabType::Post, false));
        open_tab(&mut tabs, make_tab("b", TabType::Media, false));
        open_tab(&mut tabs, make_tab("c", TabType::Post, false));

        // Close the middle tab.
        let next = close_tab(&mut tabs, "b");
        assert_eq!(next, Some(1)); // index 1 now holds "c"
        assert_eq!(tabs[1].id, "c");

        // Close the last tab.
        let next = close_tab(&mut tabs, "c");
        assert_eq!(next, Some(0)); // clamped to len-1

        // Close the only remaining tab.
        let next = close_tab(&mut tabs, "a");
        assert_eq!(next, None);
    }

    #[test]
    fn open_appends_non_singleton_non_transient() {
        let mut tabs = Vec::new();
        let i0 = open_tab(&mut tabs, make_tab("p1", TabType::Post, false));
        let i1 = open_tab(&mut tabs, make_tab("p2", TabType::Post, false));
        assert_eq!(i0, 0);
        assert_eq!(i1, 1);
        assert_eq!(tabs.len(), 2);
    }
}
