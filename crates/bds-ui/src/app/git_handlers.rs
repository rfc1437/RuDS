use super::*;

use bds_core::engine::git::{
    GitEngine, GitError, GitOperation, ReconcileAction, ReconcileEntityType,
    reconcile_changed_files,
};

impl BdsApp {
    pub(super) fn handle_git_message(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::GitRefresh => self.refresh_git(),
            Message::GitLoaded {
                repository_dir,
                result,
            } => {
                if self.data_dir.as_ref() != Some(&repository_dir) {
                    return Task::none();
                }
                match result {
                    Ok(snapshot) => self.git_state.apply_snapshot(snapshot),
                    Err(error) => {
                        self.git_state.loading = false;
                        self.git_state.error = Some(error);
                    }
                }
                Task::none()
            }
            Message::GitRemoteInputChanged(value) => {
                self.git_state.remote_input = value;
                Task::none()
            }
            Message::GitCommitMessageChanged(value) => {
                self.git_state.commit_message = value;
                Task::none()
            }
            Message::GitInitialize => {
                let remote = self.git_state.remote_input.trim().to_string();
                self.run_local_git_action(GitOperation::Initialize, move |engine| {
                    engine.initialize()?;
                    if !remote.is_empty() {
                        engine.set_remote(&remote)?;
                    }
                    Ok(())
                })
            }
            Message::GitSetRemote => {
                let remote = self.git_state.remote_input.trim().to_string();
                self.run_local_git_action(GitOperation::Remote, move |engine| {
                    engine.set_remote(&remote)
                })
            }
            Message::GitCommit => {
                let message = self.git_state.commit_message.clone();
                self.run_local_git_action(GitOperation::Commit, move |engine| {
                    engine.commit_all(&message).map(|_| ())
                })
            }
            Message::GitPruneLfs => self.run_local_git_action(GitOperation::Lfs, |engine| {
                engine.prune_lfs_cache(10).map(|_| ())
            }),
            Message::GitLocalFinished {
                repository_dir,
                operation,
                result,
            } => {
                if self.data_dir.as_ref() != Some(&repository_dir) {
                    return Task::none();
                }
                match result {
                    Ok(snapshot) => {
                        self.git_state.apply_snapshot(snapshot);
                        if operation == GitOperation::Commit {
                            self.git_state.commit_message.clear();
                            self.close_git_diff_tabs();
                        }
                        self.notify(
                            ToastLevel::Success,
                            &tw(
                                self.ui_locale,
                                "git.operationDone",
                                &[("operation", &git_operation_label(self.ui_locale, operation))],
                            ),
                        );
                    }
                    Err(error) => {
                        self.git_state.loading = false;
                        self.git_state.error = Some(error.clone());
                        self.notify(ToastLevel::Error, &error);
                    }
                }
                Task::none()
            }
            Message::GitFetch => self.start_git_network(GitOperation::Fetch),
            Message::GitPull => self.start_git_network(GitOperation::Pull),
            Message::GitPush => self.start_git_network(GitOperation::Push),
            Message::GitNetworkFinished {
                repository_dir,
                task_id,
                operation,
                result,
            } => {
                if self.data_dir.as_ref() != Some(&repository_dir) {
                    self.refresh_task_snapshots();
                    return Task::none();
                }
                self.git_state.network_run = None;
                if self.task_manager.status(task_id) == Some(TaskStatus::Cancelled) {
                    self.refresh_task_snapshots();
                    return Task::none();
                }
                match result {
                    Ok(completion) => {
                        self.task_manager.complete(task_id);
                        if !completion.output.trim().is_empty() {
                            self.add_output(completion.output.trim());
                        }
                        self.git_state.apply_snapshot(completion.snapshot);
                        self.apply_git_reconcile_events(&completion.events);
                        self.notify(
                            ToastLevel::Success,
                            &tw(
                                self.ui_locale,
                                "git.operationDone",
                                &[("operation", &git_operation_label(self.ui_locale, operation))],
                            ),
                        );
                        self.refresh_task_snapshots();
                        return self.refresh_counts();
                    }
                    Err(error) => {
                        if self.task_manager.status(task_id) != Some(TaskStatus::Cancelled) {
                            self.task_manager.fail(task_id, error.clone());
                            self.notify(ToastLevel::Error, &error);
                        }
                        self.git_state.error = Some(error);
                    }
                }
                self.refresh_task_snapshots();
                Task::none()
            }
            Message::OpenGitFileDiff(path) => self.open_git_file_diff(path),
            Message::OpenGitCommitDiff { hash, subject } => {
                self.open_git_commit_diff(hash, subject)
            }
            Message::SelectGitCommitFile { hash, change } => {
                self.load_git_commit_file(hash, change)
            }
            Message::GitDiffLoaded {
                repository_dir,
                tab_id,
                result,
            } => {
                if self.data_dir.as_ref() != Some(&repository_dir) {
                    return Task::none();
                }
                if let Some(state) = self.git_diffs.get_mut(&tab_id) {
                    state.loading = false;
                    match result {
                        Ok(loaded) => {
                            state.changes = loaded.changes;
                            state.selected_path = loaded.selected_path;
                            state.diff = loaded.diff;
                            state.error = None;
                        }
                        Err(error) => state.error = Some(error),
                    }
                }
                Task::none()
            }
            Message::GitFileHistoryLoaded {
                repository_dir,
                path,
                result,
            } => {
                if self.data_dir.as_ref() != Some(&repository_dir) {
                    return Task::none();
                }
                if self.git_file_history_target.as_deref() == Some(&path) {
                    match result {
                        Ok(commits) => self.git_file_history = commits,
                        Err(error) => {
                            self.git_file_history.clear();
                            self.add_output(&error);
                        }
                    }
                }
                Task::none()
            }
            _ => Task::none(),
        }
    }

    pub(super) fn refresh_git(&mut self) -> Task<Message> {
        let Some(data_dir) = self.data_dir.clone() else {
            self.git_state = GitUiState::default();
            return Task::none();
        };
        self.git_state.loading = true;
        let repository_dir = data_dir.clone();
        Task::perform(
            async move {
                tokio::task::spawn_blocking(move || git_snapshot(&GitEngine::new(data_dir)))
                    .await
                    .unwrap_or_else(|error| Err(format!("task panicked: {error}")))
            },
            move |result| Message::GitLoaded {
                repository_dir: repository_dir.clone(),
                result,
            },
        )
    }

    fn run_local_git_action<F>(&mut self, operation: GitOperation, work: F) -> Task<Message>
    where
        F: FnOnce(&GitEngine) -> Result<(), GitError> + Send + 'static,
    {
        let Some(data_dir) = self.data_dir.clone() else {
            return Task::none();
        };
        self.git_state.loading = true;
        self.git_state.error = None;
        let repository_dir = data_dir.clone();
        Task::perform(
            async move {
                tokio::task::spawn_blocking(move || {
                    let engine = GitEngine::new(data_dir);
                    work(&engine).map_err(|error| error.to_string())?;
                    git_snapshot(&engine)
                })
                .await
                .unwrap_or_else(|error| Err(format!("task panicked: {error}")))
            },
            move |result| Message::GitLocalFinished {
                repository_dir: repository_dir.clone(),
                operation,
                result,
            },
        )
    }

    fn start_git_network(&mut self, operation: GitOperation) -> Task<Message> {
        if self.offline_mode {
            self.notify(
                ToastLevel::Warning,
                &t(self.ui_locale, "git.airplaneBlocked"),
            );
            return Task::none();
        }
        if self.git_state.network_run.is_some() {
            return Task::none();
        }
        let (Some(project), Some(data_dir)) = (&self.active_project, &self.data_dir) else {
            return Task::none();
        };
        let project_id = project.id.clone();
        let data_dir = data_dir.clone();
        let repository_dir = data_dir.clone();
        let db_path = self.db_path.clone();
        let operation_label = git_operation_label(self.ui_locale, operation);
        let label = tw(
            self.ui_locale,
            "git.runningOperation",
            &[("operation", &operation_label)],
        );
        let task_id = self.task_manager.submit(&label);
        let output = Arc::new(Mutex::new(Vec::new()));
        self.git_state.network_run = Some(crate::views::git::GitNetworkRunState {
            task_id,
            output: Arc::clone(&output),
        });
        self.git_state.error = None;
        self.refresh_task_snapshots();
        let task_manager = Arc::clone(&self.task_manager);

        Task::perform(
            async move {
                tokio::task::spawn_blocking(move || {
                    if !task_manager.wait_until_runnable(task_id) {
                        return Err("cancelled".to_string());
                    }
                    let engine = GitEngine::new(&data_dir);
                    let old_head = (operation == GitOperation::Pull)
                        .then(|| engine.head())
                        .transpose()
                        .map_err(|error| error.to_string())?
                        .flatten();
                    let output_target = Arc::clone(&output);
                    let result = match operation {
                        GitOperation::Fetch => engine.fetch(
                            || task_manager.is_cancelled(task_id),
                            move |chunk| output_target.lock().unwrap().push(chunk),
                        ),
                        GitOperation::Pull => engine.pull(
                            || task_manager.is_cancelled(task_id),
                            move |chunk| output_target.lock().unwrap().push(chunk),
                        ),
                        GitOperation::Push => engine.push(
                            || task_manager.is_cancelled(task_id),
                            move |chunk| output_target.lock().unwrap().push(chunk),
                        ),
                        _ => unreachable!("only network operations are queued"),
                    }
                    .map_err(|error| error.to_string())?;

                    let mut events = Vec::new();
                    if operation == GitOperation::Pull
                        && let (Some(old_head), Some(new_head)) =
                            (old_head, engine.head().map_err(|error| error.to_string())?)
                        && old_head != new_head
                    {
                        let changes = engine
                            .changed_files(&old_head, &new_head)
                            .map_err(|error| error.to_string())?;
                        let db = Database::open(&db_path).map_err(|error| error.to_string())?;
                        let report = reconcile_changed_files(
                            db.conn(),
                            &data_dir,
                            &project_id,
                            &changes,
                            |event| events.push(event.clone()),
                        )
                        .map_err(|error| error.to_string())?;
                        debug_assert_eq!(events, report.events);
                    }
                    Ok(GitNetworkCompletion {
                        snapshot: git_snapshot(&engine)?,
                        events,
                        output: result.output,
                    })
                })
                .await
                .unwrap_or_else(|error| Err(format!("task panicked: {error}")))
            },
            move |result| Message::GitNetworkFinished {
                repository_dir: repository_dir.clone(),
                task_id,
                operation,
                result,
            },
        )
    }

    fn open_git_file_diff(&mut self, path: String) -> Task<Message> {
        let tab = crate::views::git::file_tab(&path);
        let tab_id = tab.id.clone();
        self.activate_git_tab(tab);
        self.git_diffs
            .insert(tab_id.clone(), GitDiffState::loading_file(path.clone()));
        let Some(data_dir) = self.data_dir.clone() else {
            return Task::none();
        };
        let repository_dir = data_dir.clone();
        Task::perform(
            async move {
                tokio::task::spawn_blocking(move || {
                    let diff = GitEngine::new(data_dir)
                        .file_diff(&path)
                        .map_err(|error| error.to_string())?;
                    Ok(GitDiffLoad {
                        changes: Vec::new(),
                        selected_path: Some(path),
                        diff: Some(diff),
                    })
                })
                .await
                .unwrap_or_else(|error| Err(format!("task panicked: {error}")))
            },
            move |result| Message::GitDiffLoaded {
                repository_dir: repository_dir.clone(),
                tab_id: tab_id.clone(),
                result,
            },
        )
    }

    fn open_git_commit_diff(&mut self, hash: String, subject: String) -> Task<Message> {
        let tab = crate::views::git::commit_tab(&hash, &subject);
        let tab_id = tab.id.clone();
        self.activate_git_tab(tab);
        self.git_diffs
            .insert(tab_id.clone(), GitDiffState::loading_commit(hash.clone()));
        let Some(data_dir) = self.data_dir.clone() else {
            return Task::none();
        };
        let repository_dir = data_dir.clone();
        Task::perform(
            async move {
                tokio::task::spawn_blocking(move || {
                    let engine = GitEngine::new(data_dir);
                    let changes = engine
                        .commit_files(&hash)
                        .map_err(|error| error.to_string())?;
                    let selected = changes.first().cloned();
                    let diff = selected
                        .as_ref()
                        .map(|change| engine.commit_file_diff(&hash, change))
                        .transpose()
                        .map_err(|error| error.to_string())?;
                    Ok(GitDiffLoad {
                        selected_path: selected.map(|change| change.path),
                        changes,
                        diff,
                    })
                })
                .await
                .unwrap_or_else(|error| Err(format!("task panicked: {error}")))
            },
            move |result| Message::GitDiffLoaded {
                repository_dir: repository_dir.clone(),
                tab_id: tab_id.clone(),
                result,
            },
        )
    }

    fn load_git_commit_file(
        &mut self,
        hash: String,
        change: engine::git::ChangedFile,
    ) -> Task<Message> {
        let tab_id = format!("git-diff:commit:{hash}");
        if let Some(state) = self.git_diffs.get_mut(&tab_id) {
            state.loading = true;
            state.selected_path = Some(change.path.clone());
            state.error = None;
        }
        let Some(data_dir) = self.data_dir.clone() else {
            return Task::none();
        };
        let repository_dir = data_dir.clone();
        let changes = self
            .git_diffs
            .get(&tab_id)
            .map(|state| state.changes.clone())
            .unwrap_or_default();
        Task::perform(
            async move {
                tokio::task::spawn_blocking(move || {
                    let diff = GitEngine::new(data_dir)
                        .commit_file_diff(&hash, &change)
                        .map_err(|error| error.to_string())?;
                    Ok(GitDiffLoad {
                        changes,
                        selected_path: Some(change.path),
                        diff: Some(diff),
                    })
                })
                .await
                .unwrap_or_else(|error| Err(format!("task panicked: {error}")))
            },
            move |result| Message::GitDiffLoaded {
                repository_dir: repository_dir.clone(),
                tab_id: tab_id.clone(),
                result,
            },
        )
    }

    fn activate_git_tab(&mut self, tab: Tab) {
        self.flush_active_post_editor();
        let index = tabs::open_tab(&mut self.tabs, tab);
        self.active_tab = self.tabs.get(index).map(|tab| tab.id.clone());
        self.enforce_panel_tab_fallback();
        self.sync_menu_state();
    }

    fn close_git_diff_tabs(&mut self) {
        let ids = self
            .tabs
            .iter()
            .filter(|tab| tab.tab_type == TabType::GitDiff)
            .map(|tab| tab.id.clone())
            .collect::<Vec<_>>();
        for id in ids {
            self.git_diffs.remove(&id);
            if let Some(index) = tabs::close_tab(&mut self.tabs, &id) {
                self.active_tab = self.tabs.get(index).map(|tab| tab.id.clone());
            } else {
                self.active_tab = None;
            }
        }
    }

    pub(super) fn refresh_git_file_history(&mut self) -> Task<Message> {
        let Some(path) = self.active_git_history_path() else {
            self.git_file_history.clear();
            self.git_file_history_target = None;
            return Task::none();
        };
        let Some(data_dir) = self.data_dir.clone() else {
            return Task::none();
        };
        let repository_dir = data_dir.clone();
        self.git_file_history_target = Some(path.clone());
        self.git_file_history.clear();
        Task::perform(
            async move {
                let task_path = path.clone();
                let result = tokio::task::spawn_blocking(move || {
                    GitEngine::new(data_dir)
                        .file_history(&task_path)
                        .map_err(|error| error.to_string())
                })
                .await
                .unwrap_or_else(|error| Err(format!("task panicked: {error}")));
                (path, result)
            },
            move |(path, result)| Message::GitFileHistoryLoaded {
                repository_dir: repository_dir.clone(),
                path,
                result,
            },
        )
    }

    fn active_git_history_path(&self) -> Option<String> {
        let tab_id = self.active_tab.as_deref()?;
        let tab = self.tabs.iter().find(|tab| tab.id == tab_id)?;
        match tab.tab_type {
            TabType::Post => self
                .db
                .as_ref()
                .and_then(|db| bds_core::db::queries::post::get_post_by_id(db.conn(), tab_id).ok())
                .map(|post| post.file_path)
                .filter(|path| !path.is_empty()),
            TabType::Media => self
                .media_editors
                .get(tab_id)
                .map(|media| media.file_path.clone())
                .filter(|path| !path.is_empty()),
            _ => None,
        }
    }

    fn apply_git_reconcile_events(&mut self, events: &[engine::git::ReconcileEvent]) {
        for event in events {
            match event.entity_type {
                ReconcileEntityType::Post => {
                    self.post_editors.remove(&event.entity_id);
                }
                ReconcileEntityType::PostTranslation => self.post_editors.clear(),
                ReconcileEntityType::Script => {
                    self.script_editors.remove(&event.entity_id);
                }
                ReconcileEntityType::Template => {
                    self.template_editors.remove(&event.entity_id);
                }
            }
            if event.action == ReconcileAction::Deleted {
                let was_active = self.active_tab.as_deref() == Some(&event.entity_id);
                match tabs::close_tab(&mut self.tabs, &event.entity_id) {
                    Some(index) if was_active => {
                        self.active_tab = self.tabs.get(index).map(|tab| tab.id.clone());
                    }
                    None if was_active => self.active_tab = None,
                    _ => {}
                }
            }
        }
        self.enforce_panel_tab_fallback();
        self.sync_menu_state();
    }

    pub(super) fn reset_git_for_project_change(&mut self) {
        if let Some(run) = self.git_state.network_run.take() {
            self.task_manager.cancel(run.task_id);
        }
        self.close_git_diff_tabs();
        self.git_state = GitUiState::default();
        self.git_file_history.clear();
        self.git_file_history_target = None;
        self.refresh_task_snapshots();
    }

    pub(super) fn refresh_git_if_visible(&mut self) -> Task<Message> {
        if self.sidebar_visible && self.sidebar_view == SidebarView::Git {
            self.refresh_git()
        } else {
            Task::none()
        }
    }
}

fn git_operation_label(locale: bds_core::i18n::UiLocale, operation: GitOperation) -> String {
    t(
        locale,
        match operation {
            GitOperation::Initialize => "git.initialize",
            GitOperation::Status => "git.noChanges",
            GitOperation::Diff => "git.diff",
            GitOperation::History => "git.history",
            GitOperation::Remote => "git.remoteUrl",
            GitOperation::Fetch => "git.fetch",
            GitOperation::Pull | GitOperation::Reconcile => "git.pull",
            GitOperation::Push => "git.push",
            GitOperation::Commit => "git.commit",
            GitOperation::Lfs => "git.pruneLfs",
        },
    )
}

fn git_snapshot(engine: &GitEngine) -> Result<GitSnapshot, String> {
    let repository = engine.repository().map_err(|error| error.to_string())?;
    if !repository.is_initialized {
        return Ok(GitSnapshot {
            repository,
            files: Vec::new(),
            history: Vec::new(),
            remote: bds_core::engine::git::GitRemoteState {
                local_branch: None,
                upstream_branch: None,
                has_upstream: false,
                ahead: 0,
                behind: 0,
            },
        });
    }
    let files = engine.status().map_err(|error| error.to_string())?;
    let remote = engine.remote_state().map_err(|error| error.to_string())?;
    let history = repository
        .current_branch
        .as_deref()
        .map(|branch| engine.history(branch))
        .transpose()
        .map_err(|error| error.to_string())?
        .unwrap_or_default();
    Ok(GitSnapshot {
        repository,
        files,
        history,
        remote,
    })
}
