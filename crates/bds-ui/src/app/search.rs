use super::*;

impl BdsApp {
    /// Number of items to load per sidebar page.
    /// Matches the TypeScript app's limit of 500 for initial load.
    pub(super) const SIDEBAR_PAGE_SIZE: i64 = 500;

    /// Refresh only sidebar posts using current filter state (async).
    pub(super) fn refresh_sidebar_posts(&mut self) -> Task<Message> {
        let (Some(_), Some(project)) = (&self.db, &self.active_project) else {
            return Task::none();
        };

        let db_path = self.db_path.clone();
        let project_id = project.id.clone();
        let content_language = self.content_language.clone();
        let filter = match self.sidebar_view {
            SidebarView::Pages => self.page_filter.clone(),
            _ => self.post_filter.clone(),
        };
        let is_pages = self.sidebar_view == SidebarView::Pages;

        Task::perform(
            async move {
                tokio::task::spawn_blocking(move || {
                    Self::query_sidebar_posts_blocking(
                        &db_path,
                        &project_id,
                        &content_language,
                        &filter,
                        is_pages,
                        Self::SIDEBAR_PAGE_SIZE + 1,
                        0,
                    )
                })
                .await
                .unwrap_or_default()
            },
            Message::SidebarPostsLoaded,
        )
    }

    /// Refresh only sidebar media using current filter state (async).
    pub(super) fn refresh_sidebar_media(&mut self) -> Task<Message> {
        let (Some(_), Some(project)) = (&self.db, &self.active_project) else {
            return Task::none();
        };

        let db_path = self.db_path.clone();
        let project_id = project.id.clone();
        let data_dir = self.data_dir.clone();
        let content_language = self.content_language.clone();
        let filter = self.media_filter.clone();

        Task::perform(
            async move {
                tokio::task::spawn_blocking(move || {
                    let items = Self::query_sidebar_media_blocking(
                        &db_path,
                        &project_id,
                        &content_language,
                        &filter,
                        Self::SIDEBAR_PAGE_SIZE + 1,
                        0,
                    );

                    // Pre-resolve thumbnail paths off the main thread
                    let thumbs: HashMap<String, Option<std::path::PathBuf>> = items
                        .iter()
                        .map(|m| {
                            let thumb = data_dir.as_ref().and_then(|dir| {
                                if !m.mime_type.starts_with("image/") {
                                    return None;
                                }
                                let rel =
                                    bds_core::util::paths::thumbnail_path(&m.id, "small", "webp");
                                let full = dir.join(&rel);
                                if full.exists() { Some(full) } else { None }
                            });
                            (m.id.clone(), thumb)
                        })
                        .collect();

                    (items, thumbs)
                })
                .await
                .unwrap_or_default()
            },
            |(items, thumbs)| Message::SidebarMediaLoaded { items, thumbs },
        )
    }

    /// Load more posts (append to existing sidebar data).
    pub(super) fn load_more_sidebar_posts(&mut self) -> Task<Message> {
        let (Some(_), Some(project)) = (&self.db, &self.active_project) else {
            return Task::none();
        };

        let db_path = self.db_path.clone();
        let project_id = project.id.clone();
        let content_language = self.content_language.clone();
        let offset = self.sidebar_posts.len() as i64;
        let filter = match self.sidebar_view {
            SidebarView::Pages => self.page_filter.clone(),
            _ => self.post_filter.clone(),
        };
        let is_pages = self.sidebar_view == SidebarView::Pages;

        Task::perform(
            async move {
                tokio::task::spawn_blocking(move || {
                    Self::query_sidebar_posts_blocking(
                        &db_path,
                        &project_id,
                        &content_language,
                        &filter,
                        is_pages,
                        Self::SIDEBAR_PAGE_SIZE + 1,
                        offset,
                    )
                })
                .await
                .unwrap_or_default()
            },
            Message::SidebarPostsAppended,
        )
    }

    pub(super) fn parse_filter_date(input: &str, end_of_day: bool) -> Option<i64> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return None;
        }
        let date = chrono::NaiveDate::parse_from_str(trimmed, "%Y-%m-%d").ok()?;
        let time = if end_of_day {
            date.and_hms_opt(23, 59, 59)?
        } else {
            date.and_hms_opt(0, 0, 0)?
        };
        Some(time.and_utc().timestamp_millis())
    }

    pub(super) fn build_post_filter_params(
        filter: &PostFilter,
        is_pages: bool,
    ) -> bds_core::db::queries::post::PostFilterParams {
        bds_core::db::queries::post::PostFilterParams {
            search_query: filter.search_query.clone(),
            status: filter.status_filter.clone(),
            language: filter.language_filter.clone(),
            year: filter.calendar.selected_year,
            month: filter.calendar.selected_month,
            from: Self::parse_filter_date(&filter.from_date, false),
            to: Self::parse_filter_date(&filter.to_date, true),
            tags: filter.tag_filter.clone(),
            categories: filter.category_filter.clone(),
            exclude_pages: !is_pages,
            pages_only: is_pages,
        }
    }

    pub(super) fn query_sidebar_posts_blocking(
        db_path: &Path,
        project_id: &str,
        content_language: &str,
        filter: &PostFilter,
        is_pages: bool,
        limit: i64,
        offset: i64,
    ) -> Vec<Post> {
        let Ok(db) = Database::open(db_path) else {
            return Vec::new();
        };

        let params = Self::build_post_filter_params(filter, is_pages);
        if filter.search_query.trim().is_empty() {
            return bds_core::db::queries::post::list_posts_filtered(
                db.conn(),
                project_id,
                &params,
                limit,
                offset,
            )
            .unwrap_or_default();
        }

        let fts_filters = bds_core::db::fts::PostSearchFilters {
            status: params.status.as_deref(),
            tags: (!params.tags.is_empty()).then_some(params.tags.as_slice()),
            categories: (!params.categories.is_empty()).then_some(params.categories.as_slice()),
            language: params.language.as_deref(),
            year: params.year,
            month: params.month,
            from: params.from,
            to: params.to,
            limit: Some(limit as usize),
            offset: Some(offset as usize),
            ..Default::default()
        };

        let ids = bds_core::db::fts::search_posts_filtered(
            db.conn(),
            &params.search_query,
            content_language,
            &fts_filters,
        )
        .map(|results| results.post_ids)
        .unwrap_or_default();

        ids.into_iter()
            .filter_map(|post_id| {
                bds_core::db::queries::post::get_post_by_id(db.conn(), &post_id).ok()
            })
            .filter(|post| post.project_id == project_id)
            .filter(|post| {
                let is_page_post = post
                    .categories
                    .iter()
                    .any(|category| category.eq_ignore_ascii_case("page"));
                if is_pages {
                    is_page_post
                } else {
                    !is_page_post
                }
            })
            .collect()
    }

    pub(super) fn query_sidebar_media_blocking(
        db_path: &Path,
        project_id: &str,
        content_language: &str,
        filter: &MediaFilter,
        limit: i64,
        offset: i64,
    ) -> Vec<Media> {
        let Ok(db) = Database::open(db_path) else {
            return Vec::new();
        };
        let params = bds_core::db::queries::media::MediaFilterParams {
            search_query: filter.search_query.clone(),
            year: filter.calendar.selected_year,
            month: filter.calendar.selected_month,
            tags: filter.tag_filter.clone(),
        };
        if filter.search_query.trim().is_empty() {
            return bds_core::db::queries::media::list_media_filtered(
                db.conn(),
                project_id,
                &params,
                limit,
                offset,
            )
            .unwrap_or_default();
        }

        let filters = bds_core::db::fts::MediaSearchFilters {
            project_id: Some(project_id),
            tags: (!params.tags.is_empty()).then_some(params.tags.as_slice()),
            year: params.year,
            month: params.month,
            limit: Some(limit as usize),
            offset: Some(offset as usize),
        };
        bds_core::db::fts::search_media_filtered(
            db.conn(),
            &params.search_query,
            content_language,
            &filters,
        )
        .map(|results| results.media_ids)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|media_id| {
            bds_core::db::queries::media::get_media_by_id(db.conn(), &media_id).ok()
        })
        .collect()
    }

    pub(super) fn regenerate_project_thumbnails(
        db: &Database,
        data_dir: &Path,
        project_id: &str,
        is_cancelled: impl Fn() -> bool,
        mut progress: impl FnMut(usize, usize, &str),
    ) -> Result<usize, String> {
        let media = bds_core::db::queries::media::list_media_by_project(db.conn(), project_id)
            .map_err(|e| e.to_string())?;
        let total = media.len();
        let thumbnails_dir = data_dir.join("thumbnails");
        let mut regenerated = 0usize;

        for (index, item) in media.iter().enumerate() {
            if is_cancelled() {
                return Err("cancelled".into());
            }
            progress(index, total, &item.original_name);
            if !item.mime_type.starts_with("image/") {
                continue;
            }
            let source = data_dir.join(&item.file_path);
            if !source.exists() {
                continue;
            }
            bds_core::util::thumbnail::generate_all_thumbnails(&source, &thumbnails_dir, &item.id)
                .map_err(|e| e.to_string())?;
            regenerated += 1;
        }

        Ok(regenerated)
    }

    /// Load more media (append to existing sidebar data).
    pub(super) fn load_more_sidebar_media(&mut self) -> Task<Message> {
        let (Some(_), Some(project)) = (&self.db, &self.active_project) else {
            return Task::none();
        };

        let db_path = self.db_path.clone();
        let project_id = project.id.clone();
        let offset = self.sidebar_media.len() as i64;
        let data_dir = self.data_dir.clone();
        let content_language = self.content_language.clone();
        let filter = self.media_filter.clone();

        Task::perform(
            async move {
                tokio::task::spawn_blocking(move || {
                    let items = Self::query_sidebar_media_blocking(
                        &db_path,
                        &project_id,
                        &content_language,
                        &filter,
                        Self::SIDEBAR_PAGE_SIZE + 1,
                        offset,
                    );

                    let thumbs: HashMap<String, Option<std::path::PathBuf>> = items
                        .iter()
                        .map(|m| {
                            let thumb = data_dir.as_ref().and_then(|dir| {
                                if !m.mime_type.starts_with("image/") {
                                    return None;
                                }
                                let rel =
                                    bds_core::util::paths::thumbnail_path(&m.id, "small", "webp");
                                let full = dir.join(&rel);
                                if full.exists() { Some(full) } else { None }
                            });
                            (m.id.clone(), thumb)
                        })
                        .collect();

                    (items, thumbs)
                })
                .await
                .unwrap_or_default()
            },
            |(items, thumbs)| Message::SidebarMediaAppended { items, thumbs },
        )
    }

    /// Refresh available tags, categories, and calendar data for filter widgets.
    pub(super) fn refresh_filter_metadata(&mut self) {
        if let (Some(db), Some(project)) = (&self.db, &self.active_project) {
            use bds_core::db::queries::media;
            use bds_core::db::queries::post;

            // Post filter metadata
            let all_tags = post::distinct_post_tags(db.conn(), &project.id).unwrap_or_default();
            let all_cats =
                post::distinct_post_categories(db.conn(), &project.id).unwrap_or_default();

            // Calendar counts for posts (excluding pages)
            let post_cal =
                post::post_calendar_counts(db.conn(), &project.id, false, true).unwrap_or_default();
            self.post_filter.available_tags = all_tags.clone();
            self.post_filter.available_categories = all_cats.clone();
            self.post_filter.available_languages = self.blog_languages.clone();
            self.post_filter.calendar_years = Self::build_calendar_tree(&post_cal);

            // Calendar counts for pages only
            let page_cal =
                post::post_calendar_counts(db.conn(), &project.id, true, false).unwrap_or_default();
            self.page_filter.available_tags = all_tags;
            self.page_filter.available_categories = all_cats;
            self.page_filter.available_languages = self.blog_languages.clone();
            self.page_filter.calendar_years = Self::build_calendar_tree(&page_cal);

            // Media filter metadata
            self.media_filter.available_tags =
                media::distinct_media_tags(db.conn(), &project.id).unwrap_or_default();
            let media_cal =
                media::media_calendar_counts(db.conn(), &project.id).unwrap_or_default();
            self.media_filter.calendar_years = Self::build_calendar_tree(&media_cal);
        }
    }

    /// Convert (year, month, count) tuples into CalendarYear/CalendarMonth tree.
    pub(super) fn build_calendar_tree(data: &[(i32, u32, usize)]) -> Vec<CalendarYear> {
        let mut years: Vec<CalendarYear> = Vec::new();
        for &(y, m, c) in data {
            if let Some(cy) = years.iter_mut().find(|cy| cy.year == y) {
                cy.months.push(CalendarMonth { month: m, count: c });
            } else {
                years.push(CalendarYear {
                    year: y,
                    months: vec![CalendarMonth { month: m, count: c }],
                });
            }
        }
        years
    }

    pub(super) fn handle_sidebar_message(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::PostSearchChanged(query) => {
                if self.search_index_rebuild_required && !query.is_empty() {
                    self.active_modal = Some(modal::ModalState::SearchIndexRepair);
                    return Task::none();
                }
                let filter = match self.sidebar_view {
                    SidebarView::Pages => &mut self.page_filter,
                    _ => &mut self.post_filter,
                };
                filter.search_query = query;
                self.refresh_sidebar_posts()
            }
            Message::TogglePostFilterPanel => {
                let filter = match self.sidebar_view {
                    SidebarView::Pages => &mut self.page_filter,
                    _ => &mut self.post_filter,
                };
                filter.filter_panel_visible = !filter.filter_panel_visible;
                Task::none()
            }
            Message::SetPostStatusFilter(status) => {
                let filter = match self.sidebar_view {
                    SidebarView::Pages => &mut self.page_filter,
                    _ => &mut self.post_filter,
                };
                filter.status_filter = status;
                self.refresh_sidebar_posts()
            }
            Message::SetPostLanguageFilter(language) => {
                let filter = match self.sidebar_view {
                    SidebarView::Pages => &mut self.page_filter,
                    _ => &mut self.post_filter,
                };
                filter.language_filter = language;
                self.refresh_sidebar_posts()
            }
            Message::SetPostCalendarYear(year) => {
                let filter = match self.sidebar_view {
                    SidebarView::Pages => &mut self.page_filter,
                    _ => &mut self.post_filter,
                };
                filter.calendar.selected_year = year;
                filter.calendar.selected_month = None;
                self.refresh_sidebar_posts()
            }
            Message::SetPostCalendarMonth(month) => {
                let filter = match self.sidebar_view {
                    SidebarView::Pages => &mut self.page_filter,
                    _ => &mut self.post_filter,
                };
                filter.calendar.selected_month = month;
                self.refresh_sidebar_posts()
            }
            Message::SetPostFromDate(value) => {
                let filter = match self.sidebar_view {
                    SidebarView::Pages => &mut self.page_filter,
                    _ => &mut self.post_filter,
                };
                filter.from_date = value;
                self.refresh_sidebar_posts()
            }
            Message::SetPostToDate(value) => {
                let filter = match self.sidebar_view {
                    SidebarView::Pages => &mut self.page_filter,
                    _ => &mut self.post_filter,
                };
                filter.to_date = value;
                self.refresh_sidebar_posts()
            }
            Message::TogglePostTagFilter(tag) => {
                let filter = match self.sidebar_view {
                    SidebarView::Pages => &mut self.page_filter,
                    _ => &mut self.post_filter,
                };
                if let Some(pos) = filter.tag_filter.iter().position(|t| *t == tag) {
                    filter.tag_filter.remove(pos);
                } else {
                    filter.tag_filter.push(tag);
                }
                self.refresh_sidebar_posts()
            }
            Message::TogglePostCategoryFilter(cat) => {
                let filter = match self.sidebar_view {
                    SidebarView::Pages => &mut self.page_filter,
                    _ => &mut self.post_filter,
                };
                if let Some(pos) = filter.category_filter.iter().position(|c| *c == cat) {
                    filter.category_filter.remove(pos);
                } else {
                    filter.category_filter.push(cat);
                }
                self.refresh_sidebar_posts()
            }
            Message::ClearPostFilters => {
                let filter = match self.sidebar_view {
                    SidebarView::Pages => &mut self.page_filter,
                    _ => &mut self.post_filter,
                };
                filter.clear();
                self.refresh_sidebar_posts()
            }
            Message::MediaSearchChanged(query) => {
                if self.search_index_rebuild_required && !query.is_empty() {
                    self.active_modal = Some(modal::ModalState::SearchIndexRepair);
                    return Task::none();
                }
                self.media_filter.search_query = query;
                self.refresh_sidebar_media()
            }
            Message::ToggleMediaFilterPanel => {
                self.media_filter.filter_panel_visible = !self.media_filter.filter_panel_visible;
                Task::none()
            }
            Message::SetMediaCalendarYear(year) => {
                self.media_filter.calendar.selected_year = year;
                self.media_filter.calendar.selected_month = None;
                self.refresh_sidebar_media()
            }
            Message::SetMediaCalendarMonth(month) => {
                self.media_filter.calendar.selected_month = month;
                self.refresh_sidebar_media()
            }
            Message::ToggleMediaTagFilter(tag) => {
                if let Some(pos) = self.media_filter.tag_filter.iter().position(|t| *t == tag) {
                    self.media_filter.tag_filter.remove(pos);
                } else {
                    self.media_filter.tag_filter.push(tag);
                }
                self.refresh_sidebar_media()
            }
            Message::ClearMediaFilters => {
                self.media_filter.clear();
                self.refresh_sidebar_media()
            }

            // ── Async sidebar data ──
            Message::SidebarPostsLoaded(mut posts) => {
                self.sidebar_posts_has_more = posts.len() > Self::SIDEBAR_PAGE_SIZE as usize;
                posts.truncate(Self::SIDEBAR_PAGE_SIZE as usize);
                self.sidebar_posts = posts;
                Task::none()
            }
            Message::SidebarMediaLoaded { mut items, thumbs } => {
                self.sidebar_media_has_more = items.len() > Self::SIDEBAR_PAGE_SIZE as usize;
                items.truncate(Self::SIDEBAR_PAGE_SIZE as usize);
                self.sidebar_media = items;
                self.sidebar_media_thumbs = thumbs;
                Task::none()
            }
            Message::SidebarPostsAppended(mut posts) => {
                self.sidebar_posts_has_more = posts.len() > Self::SIDEBAR_PAGE_SIZE as usize;
                posts.truncate(Self::SIDEBAR_PAGE_SIZE as usize);
                self.sidebar_posts.extend(posts);
                Task::none()
            }
            Message::SidebarMediaAppended { mut items, thumbs } => {
                self.sidebar_media_has_more = items.len() > Self::SIDEBAR_PAGE_SIZE as usize;
                items.truncate(Self::SIDEBAR_PAGE_SIZE as usize);
                self.sidebar_media.extend(items);
                self.sidebar_media_thumbs.extend(thumbs);
                Task::none()
            }
            Message::LoadMorePosts => self.load_more_sidebar_posts(),
            Message::LoadMoreMedia => self.load_more_sidebar_media(),
            _ => unreachable!("non-sidebar message routed to sidebar handler"),
        }
    }
}
