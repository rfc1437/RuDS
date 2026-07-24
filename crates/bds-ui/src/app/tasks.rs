use super::*;

const TASK_URL_MAX_CHARS: usize = 60;

fn shorten_task_url(url: &str) -> String {
    if url.chars().count() <= TASK_URL_MAX_CHARS {
        url.to_string()
    } else {
        format!(
            "{}…",
            url.chars().take(TASK_URL_MAX_CHARS - 1).collect::<String>()
        )
    }
}

impl BdsApp {
    pub(super) fn finish_result_task<T>(
        &self,
        task_id: TaskId,
        result: &Result<T, String>,
    ) -> bool {
        let cancellation_requested = self.task_manager.is_cancelled(task_id);
        match result {
            Ok(_) => self.task_manager.complete(task_id),
            Err(error) => self.task_manager.fail(task_id, error.clone()),
        }
        cancellation_requested && result.is_err()
    }

    pub(super) fn refresh_task_snapshots(&mut self) {
        self.task_snapshots = self
            .task_manager
            .snapshots()
            .into_iter()
            .map(|snapshot| TaskSnapshot {
                id: snapshot.id,
                source: crate::state::navigation::TaskSource::Local,
                label: snapshot.label,
                group_id: snapshot.group_id,
                group_name: snapshot.group_name,
                status: snapshot.status.clone(),
                progress: snapshot.progress,
                message: snapshot.message,
                cancellation_requested: snapshot.cancellation_requested,
                is_cancellable: matches!(
                    snapshot.status,
                    TaskStatus::Pending | TaskStatus::Running
                ) && !snapshot.cancellation_requested,
            })
            .chain(self.remote_task_snapshots.iter().cloned())
            .collect();
    }

    pub(super) fn queue_site_generation(
        &mut self,
        validation: Option<engine::validate_site::SiteValidationReport>,
    ) -> Task<Message> {
        self.queue_site_generation_mode(validation, false)
    }

    pub(super) fn queue_forced_site_generation(&mut self) -> Task<Message> {
        self.queue_site_generation_mode(None, true)
    }

    fn queue_site_generation_mode(
        &mut self,
        validation: Option<engine::validate_site::SiteValidationReport>,
        force: bool,
    ) -> Task<Message> {
        let kind = if validation.is_some() {
            SiteGenerationKind::Validation
        } else if force {
            SiteGenerationKind::Forced
        } else {
            SiteGenerationKind::Full
        };
        let Some(data_dir) = &self.data_dir else {
            if kind == SiteGenerationKind::Validation {
                self.site_validation_state.is_applying = false;
            }
            self.notify(
                ToastLevel::Error,
                &t(self.ui_locale, "engine.generateSiteNoProject"),
            );
            return Task::none();
        };
        if self.active_project.is_none() {
            if kind == SiteGenerationKind::Validation {
                self.site_validation_state.is_applying = false;
            }
            self.notify(
                ToastLevel::Error,
                &t(self.ui_locale, "engine.generateSiteNoProject"),
            );
            return Task::none();
        }
        let metadata = match engine::meta::read_project_json(data_dir) {
            Ok(metadata) => metadata,
            Err(error) => {
                if kind == SiteGenerationKind::Validation {
                    self.site_validation_state.is_applying = false;
                    self.site_validation_state.error_message = Some(error.to_string());
                }
                self.notify(ToastLevel::Error, &error.to_string());
                return Task::none();
            }
        };
        if metadata
            .public_url
            .as_deref()
            .unwrap_or("")
            .trim()
            .is_empty()
        {
            if kind == SiteGenerationKind::Validation {
                self.site_validation_state.is_applying = false;
            }
            self.notify(
                ToastLevel::Error,
                &t(self.ui_locale, "engine.publicUrlRequired"),
            );
            return Task::none();
        }

        let task_validation = validation.clone();
        let locale = self.ui_locale;
        self.spawn_result_task(
            "engine.prepareGeneration",
            move |db_path, project_id, data_dir, tm, tid| {
                tm.report_progress(
                    tid,
                    Some(0.0),
                    Some(t(locale, "engine.preparingGeneration")),
                );
                let db = Database::open(&db_path).map_err(|error| error.to_string())?;
                let metadata = engine::meta::read_project_json(&data_dir)
                    .map_err(|error| error.to_string())?;
                if tm.is_cancelled(tid) {
                    return Err("operation cancelled".into());
                }
                let posts =
                    bds_core::db::queries::post::list_posts_by_project(db.conn(), &project_id)
                        .map_err(|error| error.to_string())?
                        .into_iter()
                        .filter(engine::generation::has_published_snapshot)
                        .collect::<Vec<_>>();
                let total_posts = posts.len();
                let mut sources = Vec::with_capacity(total_posts);
                for (index, post) in posts.into_iter().enumerate() {
                    tm.report_progress(
                        tid,
                        Some(0.05 + (index + 1) as f32 / total_posts.max(1) as f32 * 0.35),
                        Some(t(locale, "engine.loadingPosts")),
                    );
                    if tm.is_cancelled(tid) {
                        return Err("operation cancelled".into());
                    }
                    if let Some(source) =
                        engine::generation::load_published_post_source(&data_dir, post)
                            .map_err(|error| error.to_string())?
                    {
                        sources.push(source);
                    }
                }
                tm.report_progress(
                    tid,
                    Some(0.45),
                    Some(t(locale, "engine.preparingGeneration")),
                );
                let prepared = Arc::new(
                    engine::generation::prepare_site_generation(
                        db.conn(),
                        &data_dir,
                        &project_id,
                        &metadata,
                        &sources,
                    )
                    .map_err(|error| error.to_string())?,
                );
                if tm.is_cancelled(tid) {
                    return Err("operation cancelled".into());
                }
                let sections = task_validation.as_ref().map_or_else(
                    || engine::generation::GenerationSection::ALL.to_vec(),
                    |report| engine::generation::sections_from_validation_report(report, &metadata),
                );
                let counts = sections
                    .into_iter()
                    .map(|section| {
                        (
                            section,
                            engine::generation::prepared_section_page_count(
                                &prepared,
                                task_validation.as_ref(),
                                section,
                            ),
                        )
                    })
                    .collect();
                tm.report_progress(tid, Some(1.0), Some(t(locale, "engine.generationPrepared")));
                Ok((prepared, counts))
            },
            move |task_id, result| Message::SiteGenerationPrepared {
                task_id,
                validation: validation.clone(),
                force,
                result,
            },
        )
    }

    pub(super) fn queue_prepared_site_generation(
        &mut self,
        validation: Option<engine::validate_site::SiteValidationReport>,
        force: bool,
        prepared: Arc<engine::generation::PreparedSiteGeneration>,
        page_estimates: HashMap<engine::generation::GenerationSection, usize>,
    ) -> Task<Message> {
        let kind = if validation.is_some() {
            SiteGenerationKind::Validation
        } else if force {
            SiteGenerationKind::Forced
        } else {
            SiteGenerationKind::Full
        };
        let (Some(project), Some(data_dir)) = (&self.active_project, &self.data_dir) else {
            return Task::none();
        };
        let sections = page_estimates.keys().copied().collect::<Vec<_>>();
        let calendar_needed = kind == SiteGenerationKind::Validation
            && sections.iter().any(|section| {
                matches!(
                    section,
                    engine::generation::GenerationSection::Core
                        | engine::generation::GenerationSection::Date
                )
            });
        let project_id = project.id.clone();
        let db_path = self.db_path.clone();
        let data_dir = data_dir.clone();
        let group_id = format!("site-generation:{}", Uuid::new_v4());
        let group_name = t(
            self.ui_locale,
            match kind {
                SiteGenerationKind::Full => "engine.renderSiteGroup",
                SiteGenerationKind::Forced => "engine.forceRenderSiteGroup",
                SiteGenerationKind::Validation => "engine.applyValidationGroup",
            },
        );
        let mut render_task_ids = Vec::new();
        let mut tasks = Vec::new();

        for section in engine::generation::GenerationSection::ALL
            .into_iter()
            .filter(|section| sections.contains(section))
        {
            let label = t(self.ui_locale, generation_section_label_key(section));
            self.add_output(&label);
            let page_work = page_estimates[&section];
            let task_id = self
                .task_manager
                .submit_grouped(&label, &group_id, &group_name);
            self.task_manager.report_progress(
                task_id,
                Some(if page_work == 0 { 1.0 } else { 0.0 }),
                Some(tw(
                    self.ui_locale,
                    "engine.renderingPages",
                    &[("current", "0"), ("total", &page_work.to_string())],
                )),
            );
            render_task_ids.push(task_id);
            let task_manager = Arc::clone(&self.task_manager);
            let task_db_path = db_path.clone();
            let task_project_id = project_id.clone();
            let task_data_dir = data_dir.clone();
            let task_group_id = group_id.clone();
            let task_validation = validation.clone();
            let task_prepared = Arc::clone(&prepared);
            let locale = self.ui_locale;
            tasks.push(Task::perform(
                async move {
                    let Some(worker) = task_manager.admit(task_id).await else {
                        return Err("cancelled".to_string());
                    };
                    tokio::task::spawn_blocking(move || {
                        let _worker = worker;
                        run_site_generation_section(
                            task_db_path,
                            task_project_id,
                            task_data_dir,
                            task_prepared,
                            task_manager,
                            task_id,
                            section,
                            task_validation,
                            force,
                            locale,
                            page_work,
                        )
                    })
                    .await
                    .unwrap_or_else(|error| Err(format!("task panicked: {error}")))
                },
                move |result| Message::SiteGenerationSectionDone {
                    group_id: task_group_id.clone(),
                    task_id,
                    result,
                },
            ));
        }

        self.site_generation_workflows.insert(
            group_id,
            SiteGenerationWorkflow {
                kind,
                db_path,
                project_id,
                data_dir,
                group_name,
                render_task_ids,
                calendar_needed,
                calendar_task_id: None,
                index_task_id: None,
                report: engine::generation::GenerationReport::default(),
            },
        );
        self.refresh_task_snapshots();
        Task::batch(tasks)
    }

    pub(super) fn queue_site_calendar(&mut self, group_id: &str) -> Task<Message> {
        let Some(workflow) = self.site_generation_workflows.get(group_id).cloned() else {
            return Task::none();
        };
        let label = t(self.ui_locale, "engine.calendarStarted");
        let task_id = self
            .task_manager
            .submit_grouped(&label, group_id, &workflow.group_name);
        if let Some(workflow) = self.site_generation_workflows.get_mut(group_id) {
            workflow.calendar_task_id = Some(task_id);
        }
        self.refresh_task_snapshots();
        let task_manager = Arc::clone(&self.task_manager);
        let task_group_id = group_id.to_string();
        let locale = self.ui_locale;
        Task::perform(
            async move {
                let Some(worker) = task_manager.admit(task_id).await else {
                    return Err("cancelled".to_string());
                };
                tokio::task::spawn_blocking(move || {
                    let _worker = worker;
                    task_manager.report_progress(
                        task_id,
                        Some(0.0),
                        Some(t(locale, "engine.loadingPosts")),
                    );
                    let db =
                        Database::open(&workflow.db_path).map_err(|error| error.to_string())?;
                    engine::calendar::regenerate_calendar_with_progress(
                        db.conn(),
                        &workflow.data_dir,
                        &workflow.project_id,
                        |current, total, name| {
                            task_manager.report_progress(
                                task_id,
                                Some(current as f32 / total.max(1) as f32),
                                Some(tw(
                                    locale,
                                    "engine.checkingItem",
                                    &[
                                        ("current", &current.to_string()),
                                        ("total", &total.to_string()),
                                        ("name", name),
                                    ],
                                )),
                            );
                            !task_manager.is_cancelled(task_id)
                        },
                    )
                    .map_err(|error| error.to_string())?;
                    task_manager.report_progress(
                        task_id,
                        Some(1.0),
                        Some(t(locale, "engine.writingCalendar")),
                    );
                    Ok(())
                })
                .await
                .unwrap_or_else(|error| Err(format!("task panicked: {error}")))
            },
            move |result| Message::SiteGenerationCalendarDone {
                group_id: task_group_id.clone(),
                task_id,
                result,
            },
        )
    }

    pub(super) fn queue_site_search_index(&mut self, group_id: &str) -> Task<Message> {
        let Some(workflow) = self.site_generation_workflows.get(group_id).cloned() else {
            return Task::none();
        };
        let label = t(self.ui_locale, "engine.buildSearchIndex");
        self.add_output(&label);
        let task_id = self
            .task_manager
            .submit_grouped(&label, group_id, &workflow.group_name);
        if let Some(workflow) = self.site_generation_workflows.get_mut(group_id) {
            workflow.index_task_id = Some(task_id);
        }
        self.refresh_task_snapshots();

        let locale = self.ui_locale;
        let task_manager = Arc::clone(&self.task_manager);
        let task_group_id = group_id.to_string();
        Task::perform(
            async move {
                let Some(worker) = task_manager.admit(task_id).await else {
                    return Err("cancelled".to_string());
                };
                tokio::task::spawn_blocking(move || {
                    let _worker = worker;
                    let db =
                        Database::open(&workflow.db_path).map_err(|error| error.to_string())?;
                    let metadata = engine::meta::read_project_json(&workflow.data_dir)
                        .map_err(|error| error.to_string())?;
                    let output_dir = workflow.data_dir.join("html");
                    let progress_manager = Arc::clone(&task_manager);
                    let cancel_manager = Arc::clone(&task_manager);
                    let on_file = move |current: usize, total: usize, path: &str| {
                        let progress = if total == 0 {
                            1.0
                        } else {
                            current as f32 / total as f32
                        };
                        progress_manager.report_progress(
                            task_id,
                            Some(progress),
                            Some(tw(
                                locale,
                                "engine.builtSearchFile",
                                &[
                                    ("path", path),
                                    ("current", &current.to_string()),
                                    ("total", &total.to_string()),
                                ],
                            )),
                        );
                    };
                    let is_cancelled = move || cancel_manager.is_cancelled(task_id);
                    if workflow.kind == SiteGenerationKind::Forced {
                        engine::generation::build_site_search_index_forced_with_progress(
                            db.conn(),
                            &output_dir,
                            &workflow.project_id,
                            &metadata,
                            on_file,
                            is_cancelled,
                        )
                    } else {
                        engine::generation::build_site_search_index_with_progress(
                            db.conn(),
                            &output_dir,
                            &workflow.project_id,
                            &metadata,
                            on_file,
                            is_cancelled,
                        )
                    }
                    .map_err(|error| error.to_string())
                })
                .await
                .unwrap_or_else(|error| Err(format!("task panicked: {error}")))
            },
            move |result| Message::SiteGenerationIndexDone {
                group_id: task_group_id.clone(),
                task_id,
                result,
            },
        )
    }

    pub(super) fn cancel_site_generation_task(&mut self, task_id: TaskId) -> bool {
        let Some(group_id) = self.task_manager.group_id(task_id) else {
            return false;
        };
        let Some(workflow) = self.site_generation_workflows.remove(&group_id) else {
            return false;
        };
        self.task_manager.cancel_group(&group_id);
        if workflow.kind == SiteGenerationKind::Validation {
            self.site_validation_state.is_applying = false;
        }
        self.add_output(&t(self.ui_locale, "engine.generationCancellationRequested"));
        self.refresh_task_snapshots();
        true
    }
    /// Rebuild the shared search index while the modal blocks editor writes.
    pub(super) fn start_search_index_rebuild(&mut self) -> Task<Message> {
        if self.db.is_none() || self.search_index_rebuild_running {
            return Task::none();
        }
        if self.task_manager.running_count() > 0 || self.task_manager.pending_count() > 0 {
            self.active_modal = Some(modal::ModalState::SearchIndexRepair);
            self.notify(
                ToastLevel::Warning,
                &t(self.ui_locale, "searchIndexRepair.waitForTasks"),
            );
            return Task::none();
        }

        self.flush_active_post_editor();
        let locale = self.ui_locale;
        let label = t(locale, "engine.reindexStarted");
        self.add_output(&label);
        let task_id = self.task_manager.submit(&label);
        self.search_index_rebuild_running = true;
        self.search_index_rebuild_task_id = Some(task_id);
        self.active_modal = Some(modal::ModalState::SearchIndexRebuilding {
            task_id,
            cancellation_requested: false,
        });
        self.refresh_task_snapshots();
        self.sync_menu_state();

        let db_path = self.db_path.clone();
        let label_for_message = label.clone();
        let task_manager = Arc::clone(&self.task_manager);
        Task::perform(
            async move {
                let Some(worker) = task_manager.admit(task_id).await else {
                    return Err("cancelled".to_string());
                };
                tokio::task::spawn_blocking(move || {
                    let _worker = worker;
                    let db = Database::open(&db_path).map_err(|error| error.to_string())?;
                    let progress_manager = Arc::clone(&task_manager);
                    let on_item: engine::search::ItemProgressFn =
                        Box::new(move |current, total, name| {
                            let progress = if total > 0 {
                                current as f32 / total as f32
                            } else {
                                1.0
                            };
                            let message = tw(
                                locale,
                                "engine.indexingItem",
                                &[
                                    ("current", &current.to_string()),
                                    ("total", &total.to_string()),
                                    ("name", name),
                                ],
                            );
                            progress_manager.report_progress(
                                task_id,
                                Some(progress),
                                Some(message),
                            );
                            !progress_manager.is_cancelled(task_id)
                        });
                    let report = engine::search::rebuild_search_index(db.conn(), Some(on_item))
                        .map_err(|error| error.to_string())?;
                    Ok(format!(
                        "posts={}, media={}",
                        report.posts_indexed, report.media_indexed
                    ))
                })
                .await
                .unwrap_or_else(|error| Err(format!("task panicked: {error}")))
            },
            move |result| Message::EngineTaskDone {
                task_id,
                operation: "engine.reindexStarted",
                label: label_for_message.clone(),
                result,
            },
        )
    }

    /// Spawn a blocking engine operation on a background thread via TaskManager.
    pub(super) fn spawn_engine_task<F>(&mut self, label_key: &'static str, work: F) -> Task<Message>
    where
        F: FnOnce(PathBuf, String, PathBuf, Arc<TaskManager>, TaskId) -> Result<String, String>
            + Send
            + 'static,
    {
        self.spawn_engine_task_in_group(label_key, None, work)
    }

    pub(super) fn spawn_result_task<T, F, M>(
        &mut self,
        label_key: &'static str,
        work: F,
        message: M,
    ) -> Task<Message>
    where
        T: Send + 'static,
        F: FnOnce(PathBuf, String, PathBuf, Arc<TaskManager>, TaskId) -> Result<T, String>
            + Send
            + 'static,
        M: Fn(TaskId, Result<T, String>) -> Message + Send + Sync + 'static,
    {
        let (Some(project_id), Some(data_dir)) = (
            self.active_project
                .as_ref()
                .map(|project| project.id.clone()),
            self.data_dir.clone(),
        ) else {
            return Task::none();
        };
        let label = t(self.ui_locale, label_key);
        self.add_output(&label);
        let task_id = self.task_manager.submit(&label);
        self.refresh_task_snapshots();
        let task_manager = Arc::clone(&self.task_manager);
        let db_path = self.db_path.clone();
        Task::perform(
            async move {
                let Some(worker) = task_manager.admit(task_id).await else {
                    return Err("cancelled".to_string());
                };
                tokio::task::spawn_blocking(move || {
                    let _worker = worker;
                    work(db_path, project_id, data_dir, task_manager, task_id)
                })
                .await
                .unwrap_or_else(|error| Err(format!("task panicked: {error}")))
            },
            move |result| message(task_id, result),
        )
    }

    pub(super) fn spawn_grouped_engine_task<F>(
        &mut self,
        label_key: &'static str,
        group_name: &str,
        work: F,
    ) -> Task<Message>
    where
        F: FnOnce(PathBuf, String, PathBuf, Arc<TaskManager>, TaskId) -> Result<String, String>
            + Send
            + 'static,
    {
        self.spawn_engine_task_in_group(label_key, Some(group_name), work)
    }

    pub(super) fn spawn_engine_task_in_group<F>(
        &mut self,
        label_key: &'static str,
        group_name: Option<&str>,
        work: F,
    ) -> Task<Message>
    where
        F: FnOnce(PathBuf, String, PathBuf, Arc<TaskManager>, TaskId) -> Result<String, String>
            + Send
            + 'static,
    {
        let (Some(project), Some(data_dir)) = (&self.active_project, &self.data_dir) else {
            return Task::none();
        };

        let db_path = self.db_path.clone();
        let project_id = project.id.clone();
        let data_dir = data_dir.clone();

        let label = t(self.ui_locale, label_key);
        self.add_output(&label);

        let task_id = group_name.map_or_else(
            || self.task_manager.submit(&label),
            |name| {
                self.task_manager
                    .submit_grouped(&label, &format!("{project_id}:{name}"), name)
            },
        );
        self.refresh_task_snapshots();

        let label_for_msg = label.clone();
        let tm = Arc::clone(&self.task_manager);

        Task::perform(
            async move {
                let Some(worker) = tm.admit(task_id).await else {
                    return Err("cancelled".to_string());
                };
                tokio::task::spawn_blocking(move || {
                    let _worker = worker;
                    work(db_path, project_id, data_dir, tm, task_id)
                })
                .await
                .unwrap_or_else(|e| Err(format!("task panicked: {e}")))
            },
            move |result| Message::EngineTaskDone {
                task_id,
                operation: label_key,
                label: label_for_msg.clone(),
                result,
            },
        )
    }
}

fn generation_section_label_key(section: engine::generation::GenerationSection) -> &'static str {
    match section {
        engine::generation::GenerationSection::Core => "engine.renderSiteCore",
        engine::generation::GenerationSection::Single => "engine.renderSinglePosts",
        engine::generation::GenerationSection::Category => "engine.renderCategoryArchives",
        engine::generation::GenerationSection::Tag => "engine.renderTagArchives",
        engine::generation::GenerationSection::Date => "engine.renderDateArchives",
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "task input is captured generation context"
)]
fn run_site_generation_section(
    db_path: PathBuf,
    project_id: String,
    data_dir: PathBuf,
    prepared: Arc<engine::generation::PreparedSiteGeneration>,
    task_manager: Arc<TaskManager>,
    task_id: TaskId,
    section: engine::generation::GenerationSection,
    validation: Option<engine::validate_site::SiteValidationReport>,
    force: bool,
    locale: UiLocale,
    expected_pages: usize,
) -> Result<engine::generation::GenerationReport, String> {
    task_manager.report_progress(
        task_id,
        Some(if expected_pages == 0 { 1.0 } else { 0.0 }),
        Some(tw(
            locale,
            "engine.renderingPages",
            &[("current", "0"), ("total", &expected_pages.to_string())],
        )),
    );
    let db = Database::open(&db_path).map_err(|error| error.to_string())?;
    let output_dir = data_dir.join("html");
    std::fs::create_dir_all(&output_dir).map_err(|error| error.to_string())?;
    let render_manager = Arc::clone(&task_manager);
    let cancel_manager = Arc::clone(&task_manager);
    let rendered = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let rendered_count = Arc::clone(&rendered);
    let on_page = |_current: usize, _total: usize, _url: &str| {};
    let on_rendered = move |url: &str| {
        let current = rendered_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
        let url = shorten_task_url(url);
        render_manager.report_progress(
            task_id,
            Some(if expected_pages == 0 {
                1.0
            } else {
                current.min(expected_pages) as f32 / expected_pages as f32
            }),
            Some(tw(
                locale,
                "engine.renderingPage",
                &[
                    ("url", &url),
                    ("current", &current.to_string()),
                    ("total", &expected_pages.to_string()),
                ],
            )),
        );
    };
    let is_cancelled = move || cancel_manager.is_cancelled(task_id);
    match validation {
        Some(validation) => engine::generation::apply_validation_prepared_section_with_progress(
            db.conn(),
            &output_dir,
            &project_id,
            prepared.as_ref(),
            &validation,
            section,
            on_page,
            &on_rendered,
            is_cancelled,
        ),
        None => engine::generation::render_prepared_site_section_with_progress(
            db.conn(),
            &output_dir,
            &project_id,
            prepared.as_ref(),
            section,
            force,
            &on_rendered,
            on_page,
            is_cancelled,
        ),
    }
    .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::shorten_task_url;

    #[test]
    fn task_urls_stay_single_line_without_splitting_unicode() {
        assert_eq!(shorten_task_url("/short"), "/short");
        let shortened = shorten_task_url(&format!("/{}", "ä".repeat(80)));
        assert_eq!(shortened.chars().count(), 60);
        assert!(shortened.ends_with('…'));
    }
}
