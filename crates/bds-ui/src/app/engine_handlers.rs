use super::*;

impl BdsApp {
    pub(super) fn handle_engine_message(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::RebuildDatabase => self.spawn_engine_task(
                "engine.rebuildStarted",
                |db_path, project_id, data_dir, tm, tid| {
                    let db = Database::open(&db_path).map_err(|e| e.to_string())?;
                    let on_progress: engine::rebuild::ProgressFn = Arc::new(move |pct, msg| {
                        tm.report_progress(tid, Some(pct), Some(msg.to_string()));
                    });
                    let report = engine::rebuild::rebuild_from_filesystem_with_progress(
                        db.conn(),
                        &data_dir,
                        &project_id,
                        Some(on_progress),
                    )
                    .map_err(|e| e.to_string())?;
                    let posts = report.posts_created + report.posts_updated;
                    let media = report.media_created + report.media_updated;
                    let templates = report.templates_created + report.templates_updated;
                    let scripts = report.scripts_created + report.scripts_updated;
                    Ok(format!(
                        "posts={posts}, media={media}, templates={templates}, scripts={scripts}"
                    ))
                },
            ),
            Message::ReindexText => {
                if !self.search_index_rebuild_running {
                    self.active_modal = Some(modal::ModalState::SearchIndexRepair);
                }
                Task::none()
            }
            Message::RegenerateCalendar => {
                let locale = self.ui_locale;
                self.spawn_engine_task(
                    "engine.calendarStarted",
                    move |db_path, project_id, data_dir, tm, tid| {
                        let db = Database::open(&db_path).map_err(|e| e.to_string())?;
                        tm.report_progress(tid, Some(0.20), Some(t(locale, "engine.loadingPosts")));
                        engine::calendar::regenerate_calendar(db.conn(), &data_dir, &project_id)
                            .map_err(|e| e.to_string())?;
                        tm.report_progress(
                            tid,
                            Some(0.90),
                            Some(t(locale, "engine.writingCalendar")),
                        );
                        Ok("done".to_string())
                    },
                )
            }
            Message::ValidateTranslations => {
                self.open_singleton_tab(
                    TabType::TranslationValidation,
                    "tabBar.translationValidation",
                );
                let (Some(project), Some(data_dir)) = (&self.active_project, &self.data_dir) else {
                    return Task::none();
                };
                self.translation_validation_state.is_running = true;
                self.translation_validation_state.error_message = None;
                let db_path = self.db_path.clone();
                let project_id = project.id.clone();
                let data_dir = data_dir.clone();
                Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            let db = Database::open(&db_path).map_err(|e| e.to_string())?;
                            let meta = engine::meta::read_project_json(&data_dir)
                                .map_err(|e| e.to_string())?;
                            let main_lang = meta.main_language.as_deref().unwrap_or("en");
                            let blog_langs = meta.blog_languages.clone();
                            let on_item: engine::validate_translations::ItemProgressFn =
                                Box::new(move |_current, _total, _name| {});
                            engine::validate_translations::validate_translations_with_progress(
                                db.conn(),
                                &data_dir,
                                &project_id,
                                &blog_langs,
                                main_lang,
                                Some(on_item),
                            )
                            .map_err(|e| e.to_string())
                        })
                        .await
                        .unwrap_or_else(|error| Err(format!("task panicked: {error}")))
                    },
                    Message::TranslationValidationLoaded,
                )
            }
            Message::TranslationValidationLoaded(result) => {
                self.translation_validation_state.is_running = false;
                match result {
                    Ok(report) => {
                        self.translation_validation_state.report = Some(report);
                        self.translation_validation_state.error_message = None;
                    }
                    Err(error) => self.translation_validation_state.error_message = Some(error),
                }
                Task::none()
            }
            Message::ValidateMedia => {
                let locale = self.ui_locale;
                self.spawn_engine_task(
                    "engine.validateMediaStarted",
                    move |db_path, project_id, _data_dir, tm, tid| {
                        let db = Database::open(&db_path).map_err(|e| e.to_string())?;
                        let on_item: engine::validate_media::ProgressFn =
                            Box::new(move |current, total, name| {
                                let pct = if total > 0 {
                                    current as f32 / total as f32
                                } else {
                                    1.0
                                };
                                let msg = tw(
                                    locale,
                                    "engine.checkingItem",
                                    &[
                                        ("current", &current.to_string()),
                                        ("total", &total.to_string()),
                                        ("name", name),
                                    ],
                                );
                                tm.report_progress(tid, Some(pct), Some(msg));
                            });
                        let report = engine::validate_media::validate_media(
                            db.conn(),
                            &_data_dir,
                            &project_id,
                            Some(on_item),
                        )
                        .map_err(|e| e.to_string())?;
                        Ok(format!(
                            "checked={}, issues={}",
                            report.total_checked,
                            report.issues.len()
                        ))
                    },
                )
            }
            Message::GenerateSite => {
                let locale = self.ui_locale;
                self.spawn_engine_task(
                    "engine.generateSiteStarted",
                    move |db_path, project_id, data_dir, tm, tid| {
                        let db = Database::open(&db_path).map_err(|e| e.to_string())?;
                        let metadata = engine::meta::read_project_json(&data_dir)
                            .map_err(|e| e.to_string())?;
                        if metadata
                            .public_url
                            .as_deref()
                            .unwrap_or("")
                            .trim()
                            .is_empty()
                        {
                            return Err(
                                "public URL is required before generating the site".to_string()
                            );
                        }
                        let main_language = metadata
                            .main_language
                            .clone()
                            .unwrap_or_else(|| "en".to_string());
                        let all_posts = bds_core::db::queries::post::list_posts_by_project(
                            db.conn(),
                            &project_id,
                        )
                        .map_err(|e| e.to_string())?;
                        let published_posts = all_posts
                            .into_iter()
                            .filter(engine::generation::has_published_snapshot)
                            .collect::<Vec<_>>();
                        let total = published_posts.len().max(1) as f32;
                        let mut sources = Vec::new();
                        for (index, post) in published_posts.into_iter().enumerate() {
                            if tm.is_cancelled(tid) {
                                return Err("cancelled".to_string());
                            }
                            tm.report_progress(
                                tid,
                                Some(((index as f32) / total) * 0.7),
                                Some(tw(locale, "engine.renderingPost", &[("post", &post.slug)])),
                            );
                            if let Some(source) =
                                engine::generation::load_published_post_source(&data_dir, post)
                                    .map_err(|error| error.to_string())?
                            {
                                sources.push(source);
                            }
                        }
                        let output_dir = data_dir.join("html");
                        std::fs::create_dir_all(&output_dir).map_err(|e| e.to_string())?;
                        tm.report_progress(
                            tid,
                            Some(0.85),
                            Some(t(locale, "engine.writingGeneratedFiles")),
                        );
                        let progress_start = 0.70_f32;
                        let progress_span = 0.28_f32;
                        let task_manager = Arc::clone(&tm);
                        let report = engine::generation::generate_starter_site_with_progress(
                            db.conn(),
                            &output_dir,
                            &project_id,
                            &metadata,
                            &sources,
                            &main_language,
                            move |current, total, path| {
                                let fraction = if total == 0 {
                                    1.0
                                } else {
                                    current as f32 / total as f32
                                };
                                task_manager.report_progress(
                                    tid,
                                    Some(progress_start + fraction * progress_span),
                                    Some(tw(
                                        locale,
                                        "engine.generatedPage",
                                        &[
                                            ("url", path),
                                            ("current", &current.to_string()),
                                            ("total", &total.to_string()),
                                        ],
                                    )),
                                );
                            },
                        )
                        .map_err(|e| e.to_string())?;
                        tm.report_progress(
                            tid,
                            Some(1.0),
                            Some(t(locale, "engine.generationComplete")),
                        );
                        Ok(format!(
                            "written={}, skipped={}, output={}",
                            report.written_paths.len(),
                            report.skipped_paths.len(),
                            output_dir.display(),
                        ))
                    },
                )
            }
            Message::RunMetadataDiff => {
                self.open_singleton_tab(TabType::MetadataDiff, "tabBar.metadataDiff");
                self.start_metadata_diff()
            }
            Message::MetadataDiffLoaded(result) => {
                self.metadata_diff_state.is_running = false;
                match result {
                    Ok(report) => {
                        self.metadata_diff_state.report = Some(report);
                        self.metadata_diff_state.error_message = None;
                    }
                    Err(error) => self.metadata_diff_state.error_message = Some(error),
                }
                Task::none()
            }
            Message::RepairMetadataDiffItem { index, direction } => {
                let Some(item) = self
                    .metadata_diff_state
                    .report
                    .as_ref()
                    .and_then(|report| report.diffs.get(index))
                    .cloned()
                else {
                    return Task::none();
                };
                let (Some(project), Some(data_dir)) =
                    (self.active_project.as_ref(), self.data_dir.as_ref())
                else {
                    return Task::none();
                };
                self.metadata_diff_state.is_repairing = true;
                let db_path = self.db_path.clone();
                let project_id = project.id.clone();
                let data_dir = data_dir.clone();
                Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            let db = Database::open(&db_path).map_err(|error| error.to_string())?;
                            engine::metadata_diff::repair_metadata_diff_item(
                                db.conn(),
                                &data_dir,
                                &project_id,
                                direction,
                                &item,
                            )
                            .map_err(|error| error.to_string())
                        })
                        .await
                        .map_err(|error| error.to_string())?
                    },
                    Message::MetadataDiffItemRepaired,
                )
            }
            Message::MetadataDiffItemRepaired(result) => {
                self.metadata_diff_state.is_repairing = false;
                match result {
                    Ok(()) => {
                        self.notify(
                            ToastLevel::Success,
                            &t(self.ui_locale, "metadataDiff.repaired"),
                        );
                        self.start_metadata_diff()
                    }
                    Err(error) => {
                        self.notify(ToastLevel::Error, &error);
                        Task::none()
                    }
                }
            }
            Message::RunSiteValidation => self.start_site_validation(),
            Message::ApplySiteValidation => self.apply_site_validation(),
            Message::EngineTaskDone {
                task_id,
                label,
                result,
            } => {
                let search_rebuild_finished = self.search_index_rebuild_task_id == Some(task_id);
                let cancelled = self.task_manager.status(task_id) == Some(TaskStatus::Cancelled);
                match &result {
                    _ if cancelled => {}
                    Ok(detail) => {
                        self.task_manager.complete(task_id);
                        self.notify(ToastLevel::Success, &format!("{label}: {detail}"));
                    }
                    Err(err) => {
                        self.task_manager.fail(task_id, err.clone());
                        let message = tw(
                            self.ui_locale,
                            "common.operationFailed",
                            &[("operation", &label), ("error", err)],
                        );
                        self.notify(ToastLevel::Error, &message);
                    }
                }
                if search_rebuild_finished {
                    self.search_index_rebuild_running = false;
                    self.search_index_rebuild_task_id = None;
                    self.active_modal = None;
                    if result.is_ok() {
                        self.search_index_rebuild_required = false;
                    }
                    self.sync_menu_state();
                }
                let sidebar_task = self.refresh_counts();
                self.refresh_task_snapshots();
                sidebar_task
            }
            Message::SiteValidationLoaded(result) => {
                self.site_validation_state.is_running = false;
                self.site_validation_state.has_run = true;
                match result {
                    Ok(report) => {
                        self.site_validation_state.error_message = None;
                        self.site_validation_state.missing_files = report.missing_pages;
                        self.site_validation_state.extra_files = report.extra_pages;
                        self.site_validation_state.stale_files = report.stale_pages;
                        self.notify(
                            ToastLevel::Success,
                            &tw(
                                self.ui_locale,
                                "siteValidation.summary",
                                &[
                                    ("label", &t(self.ui_locale, "tabBar.siteValidation")),
                                    (
                                        "missing",
                                        &self.site_validation_state.missing_files.len().to_string(),
                                    ),
                                    (
                                        "extra",
                                        &self.site_validation_state.extra_files.len().to_string(),
                                    ),
                                    (
                                        "stale",
                                        &self.site_validation_state.stale_files.len().to_string(),
                                    ),
                                ],
                            ),
                        );
                    }
                    Err(error) => {
                        self.site_validation_state.error_message = Some(error.clone());
                        self.site_validation_state.missing_files.clear();
                        self.site_validation_state.extra_files.clear();
                        self.site_validation_state.stale_files.clear();
                        self.notify(ToastLevel::Error, &error);
                    }
                }
                Task::none()
            }
            Message::SiteValidationApplied(result) => {
                self.site_validation_state.is_applying = false;
                match result {
                    Ok(detail) => {
                        self.site_validation_state.error_message = None;
                        self.notify(ToastLevel::Success, &detail);
                        self.start_site_validation()
                    }
                    Err(error) => {
                        self.site_validation_state.error_message = Some(error.clone());
                        self.notify(ToastLevel::Error, &error);
                        Task::none()
                    }
                }
            }
            _ => unreachable!("non-engine message routed to engine handler"),
        }
    }
}
