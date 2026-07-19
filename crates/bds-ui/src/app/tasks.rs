use super::*;

impl BdsApp {
    pub(super) fn refresh_task_snapshots(&mut self) {
        self.task_snapshots = self
            .task_manager
            .snapshots()
            .into_iter()
            .map(|snapshot| {
                let status_str = match &snapshot.status {
                    TaskStatus::Pending => t(self.ui_locale, "tasks.statusPending"),
                    TaskStatus::Running => t(self.ui_locale, "tasks.statusRunning"),
                    TaskStatus::Completed => t(self.ui_locale, "tasks.statusCompleted"),
                    TaskStatus::Failed(error) => {
                        tw(self.ui_locale, "tasks.statusFailed", &[("error", error)])
                    }
                    TaskStatus::Cancelled => t(self.ui_locale, "tasks.statusCancelled"),
                };
                TaskSnapshot {
                    id: snapshot.id,
                    label: snapshot.label,
                    group_id: snapshot.group_id,
                    group_name: snapshot.group_name,
                    status: status_str,
                    progress: snapshot.progress,
                    message: snapshot.message,
                    is_cancellable: matches!(
                        snapshot.status,
                        TaskStatus::Pending | TaskStatus::Running
                    ),
                }
            })
            .collect();
    }

    pub(super) fn queue_site_generation(
        &mut self,
        validation: Option<engine::validate_site::SiteValidationReport>,
    ) -> Task<Message> {
        let kind = if validation.is_some() {
            SiteGenerationKind::Validation
        } else {
            SiteGenerationKind::Full
        };
        let (Some(project), Some(data_dir)) = (&self.active_project, &self.data_dir) else {
            if kind == SiteGenerationKind::Validation {
                self.site_validation_state.is_applying = false;
            }
            self.notify(
                ToastLevel::Error,
                &t(self.ui_locale, "engine.generateSiteNoProject"),
            );
            return Task::none();
        };
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

        let sections = validation.as_ref().map_or_else(
            || engine::generation::GenerationSection::ALL.to_vec(),
            engine::generation::sections_from_validation_report,
        );
        if sections.is_empty() {
            return Task::none();
        }

        let project_id = project.id.clone();
        let db_path = self.db_path.clone();
        let data_dir = data_dir.clone();
        let group_id = format!("site-generation:{}", Uuid::new_v4());
        let group_name = t(
            self.ui_locale,
            if kind == SiteGenerationKind::Full {
                "engine.renderSiteGroup"
            } else {
                "engine.applyValidationGroup"
            },
        );
        let mut render_task_ids = Vec::new();
        let mut tasks = Vec::new();

        for section in sections {
            let label = t(self.ui_locale, generation_section_label_key(section));
            self.add_output(&label);
            let task_id = self
                .task_manager
                .submit_grouped(&label, &group_id, &group_name);
            render_task_ids.push(task_id);
            let task_manager = Arc::clone(&self.task_manager);
            let task_db_path = db_path.clone();
            let task_project_id = project_id.clone();
            let task_data_dir = data_dir.clone();
            let task_group_id = group_id.clone();
            let task_validation = validation.clone();
            let locale = self.ui_locale;
            tasks.push(Task::perform(
                async move {
                    tokio::task::spawn_blocking(move || {
                        run_site_generation_section(
                            task_db_path,
                            task_project_id,
                            task_data_dir,
                            task_manager,
                            task_id,
                            section,
                            task_validation,
                            locale,
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
                index_task_id: None,
                report: engine::generation::GenerationReport::default(),
            },
        );
        self.refresh_task_snapshots();
        Task::batch(tasks)
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
                tokio::task::spawn_blocking(move || {
                    if !task_manager.wait_until_runnable(task_id) {
                        return Err("cancelled".to_string());
                    }
                    let db =
                        Database::open(&workflow.db_path).map_err(|error| error.to_string())?;
                    let metadata = engine::meta::read_project_json(&workflow.data_dir)
                        .map_err(|error| error.to_string())?;
                    let output_dir = workflow.data_dir.join("html");
                    let progress_manager = Arc::clone(&task_manager);
                    let cancel_manager = Arc::clone(&task_manager);
                    engine::generation::build_site_search_index_with_progress(
                        db.conn(),
                        &output_dir,
                        &workflow.project_id,
                        &metadata,
                        move |current, total, path| {
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
                        },
                        move || cancel_manager.is_cancelled(task_id),
                    )
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
        self.add_output(&t(self.ui_locale, "engine.generationCancelled"));
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
        self.active_modal = Some(modal::ModalState::SearchIndexRebuilding);
        self.refresh_task_snapshots();
        self.sync_menu_state();

        let db_path = self.db_path.clone();
        let label_for_message = label.clone();
        let task_manager = Arc::clone(&self.task_manager);
        Task::perform(
            async move {
                tokio::task::spawn_blocking(move || {
                    if !task_manager.wait_until_runnable(task_id) {
                        return Err("cancelled".to_string());
                    }
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
                tokio::task::spawn_blocking(move || {
                    if !tm.wait_until_runnable(task_id) {
                        return Err("cancelled".to_string());
                    }
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
    task_manager: Arc<TaskManager>,
    task_id: TaskId,
    section: engine::generation::GenerationSection,
    validation: Option<engine::validate_site::SiteValidationReport>,
    locale: UiLocale,
) -> Result<engine::generation::GenerationReport, String> {
    if !task_manager.wait_until_runnable(task_id) {
        return Err("cancelled".to_string());
    }
    let db = Database::open(&db_path).map_err(|error| error.to_string())?;
    let metadata = engine::meta::read_project_json(&data_dir).map_err(|error| error.to_string())?;
    let posts = bds_core::db::queries::post::list_posts_by_project(db.conn(), &project_id)
        .map_err(|error| error.to_string())?;
    let mut sources = Vec::new();
    for post in posts
        .into_iter()
        .filter(engine::generation::has_published_snapshot)
    {
        if task_manager.is_cancelled(task_id) {
            return Err("cancelled".to_string());
        }
        if let Some(source) = engine::generation::load_published_post_source(&data_dir, post)
            .map_err(|error| error.to_string())?
        {
            sources.push(source);
        }
    }
    let output_dir = data_dir.join("html");
    std::fs::create_dir_all(&output_dir).map_err(|error| error.to_string())?;
    let progress_manager = Arc::clone(&task_manager);
    let cancel_manager = Arc::clone(&task_manager);
    let progress_key = if validation.is_some() {
        "engine.rewrotePage"
    } else {
        "engine.generatedPage"
    };
    let on_page = move |current: usize, total: usize, url: &str| {
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
                progress_key,
                &[
                    ("url", url),
                    ("current", &current.to_string()),
                    ("total", &total.to_string()),
                ],
            )),
        );
    };
    let is_cancelled = move || cancel_manager.is_cancelled(task_id);
    match validation {
        Some(validation) => engine::generation::apply_validation_section_with_progress(
            db.conn(),
            &output_dir,
            &project_id,
            &metadata,
            &sources,
            &validation,
            section,
            on_page,
            is_cancelled,
        ),
        None => engine::generation::render_site_section_with_progress(
            db.conn(),
            &output_dir,
            &project_id,
            &metadata,
            &sources,
            section,
            on_page,
            is_cancelled,
        ),
    }
    .map_err(|error| error.to_string())
}
