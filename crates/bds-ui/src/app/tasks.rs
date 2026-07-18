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
                label: label_for_message.clone(),
                result,
            },
        )
    }

    /// Spawn a blocking engine operation on a background thread via TaskManager.
    pub(super) fn spawn_engine_task<F>(&mut self, label_key: &str, work: F) -> Task<Message>
    where
        F: FnOnce(PathBuf, String, PathBuf, Arc<TaskManager>, TaskId) -> Result<String, String>
            + Send
            + 'static,
    {
        self.spawn_engine_task_in_group(label_key, None, work)
    }

    pub(super) fn spawn_grouped_engine_task<F>(
        &mut self,
        label_key: &str,
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
        label_key: &str,
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
                label: label_for_msg.clone(),
                result,
            },
        )
    }
}
