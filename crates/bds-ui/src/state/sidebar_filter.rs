//! Sidebar filter state per sidebar_views.allium PostsView / MediaView.
//!
//! Per ui_data_flow.allium SidebarFilterIsolation:
//! "Sidebar search/filter state is local to the sidebar component.
//!  Filtering never affects: active tab, editor content, selectedPostId.
//!  Only the visible list of items changes."

/// Calendar year/month archive filter.
/// Per sidebar_views.allium CalendarFilter.
#[derive(Debug, Clone, Default)]
pub struct CalendarFilter {
    pub selected_year: Option<i32>,
    pub selected_month: Option<u32>, // 1-12
}

/// A year in the calendar archive tree, with per-month counts.
#[derive(Debug, Clone)]
pub struct CalendarYear {
    pub year: i32,
    pub months: Vec<CalendarMonth>,
}

/// A month in the calendar archive tree.
#[derive(Debug, Clone)]
pub struct CalendarMonth {
    pub month: u32, // 1-12
    pub count: usize,
}

/// Filter state for the Posts sidebar (and Pages, which is Posts with
/// category_filter pre-set to ["page"]).
#[derive(Debug, Clone, Default)]
pub struct PostFilter {
    /// FTS search query, per sidebar_views.allium PostsView.search_query.
    pub search_query: String,
    /// Whether the collapsible filter panel is visible.
    pub filter_panel_visible: bool,
    /// Exact status filter.
    pub status_filter: Option<String>,
    /// Exact language filter.
    pub language_filter: Option<String>,
    /// Year/month archive filter.
    pub calendar: CalendarFilter,
    /// Inclusive start date input (`YYYY-MM-DD`).
    pub from_date: String,
    /// Inclusive end date input (`YYYY-MM-DD`).
    pub to_date: String,
    /// Selected tag names (multi-select).
    pub tag_filter: Vec<String>,
    /// Selected category names (multi-select).
    pub category_filter: Vec<String>,
    /// Calendar tree for the toggle widget.
    pub calendar_years: Vec<CalendarYear>,
    /// All available tags for the chip selector.
    pub available_tags: Vec<String>,
    /// All available categories for the chip selector.
    pub available_categories: Vec<String>,
    /// Available languages for the chip selector.
    pub available_languages: Vec<String>,
}

/// Filter state for the Media sidebar.
#[derive(Debug, Clone, Default)]
pub struct MediaFilter {
    /// FTS search query.
    pub search_query: String,
    /// Whether the collapsible filter panel is visible.
    pub filter_panel_visible: bool,
    /// Year/month archive filter.
    pub calendar: CalendarFilter,
    /// Selected tag names (multi-select).
    pub tag_filter: Vec<String>,
    /// Calendar tree for the toggle widget.
    pub calendar_years: Vec<CalendarYear>,
    /// All available tags for the chip selector.
    pub available_tags: Vec<String>,
}

impl PostFilter {
    /// Returns true if any filter is active (for "Clear All" visibility).
    pub fn has_active_filters(&self) -> bool {
        !self.search_query.is_empty()
            || self.status_filter.is_some()
            || self.language_filter.is_some()
            || self.calendar.selected_year.is_some()
            || !self.from_date.trim().is_empty()
            || !self.to_date.trim().is_empty()
            || !self.tag_filter.is_empty()
            || !self.category_filter.is_empty()
    }

    /// Reset all filters to defaults.
    pub fn clear(&mut self) {
        self.search_query.clear();
        self.status_filter = None;
        self.language_filter = None;
        self.calendar = CalendarFilter::default();
        self.from_date.clear();
        self.to_date.clear();
        self.tag_filter.clear();
        self.category_filter.clear();
    }
}

impl MediaFilter {
    /// Returns true if any filter is active.
    pub fn has_active_filters(&self) -> bool {
        !self.search_query.is_empty()
            || self.calendar.selected_year.is_some()
            || !self.tag_filter.is_empty()
    }

    /// Reset all filters to defaults.
    pub fn clear(&mut self) {
        self.search_query.clear();
        self.calendar = CalendarFilter::default();
        self.tag_filter.clear();
    }
}
