use super::*;

impl BdsApp {
    pub(super) fn handle_post_editor_msg(&mut self, msg: PostEditorMsg) -> Task<Message> {
        enum DeferredPostAction {
            None,
            SyncEmbeddedPreview,
            Analyze(String),
            AnalyzeTaxonomy(String),
            AddGalleryImages(String),
            DetectLanguage(String),
            OpenTranslate(String),
            TranslateTo {
                post_id: String,
                target_language: String,
            },
            Save(String),
            Publish(String),
            Discard(String),
            ShowDelete {
                tab_id: String,
                name: String,
            },
            OpenInsertLink(String),
            OpenInsertMedia {
                post_id: String,
                link_only: bool,
            },
            OpenGallery(String),
            OpenLinkedMedia(String),
            UnlinkLinkedMedia {
                post_id: String,
                media_id: String,
            },
            InsertSelectedLink {
                post_id: String,
                linked_post_id: String,
            },
            CreateLinkedPost(String),
            InsertSelectedMedia {
                post_id: String,
                media_id: String,
            },
            SetLinkTab(modal::PostInsertLinkTab),
            SetLinkSearch(String),
            SetExternalUrl(String),
            SetExternalText(String),
            InsertExternalLink,
            SetMediaSearch(String),
            SelectGalleryImage(usize),
            GalleryPrevious,
            GalleryNext,
            GalleryCloseLightbox,
        }

        let mut deferred = DeferredPostAction::None;
        let mut refresh_linked_media: Option<(String, String)> = None;
        if let Some(tab_id) = self.active_tab.clone()
            && let Some(state) = self.post_editors.get_mut(&tab_id)
        {
            match msg {
                PostEditorMsg::ToggleQuickActions => {
                    state.quick_actions_open = !state.quick_actions_open;
                }
                PostEditorMsg::AnalyzeWithAi => {
                    state.quick_actions_open = false;
                    deferred = DeferredPostAction::Analyze(tab_id.clone());
                }
                PostEditorMsg::AnalyzeTaxonomy => {
                    state.quick_actions_open = false;
                    deferred = DeferredPostAction::AnalyzeTaxonomy(tab_id.clone());
                }
                PostEditorMsg::AddGalleryImages => {
                    state.quick_actions_open = false;
                    deferred = DeferredPostAction::AddGalleryImages(state.post_id.clone());
                }
                PostEditorMsg::SwitchEditorMode(mode) => {
                    state.set_editor_mode(&mode);
                    deferred = DeferredPostAction::SyncEmbeddedPreview;
                }
                PostEditorMsg::DetectLanguage => {
                    state.quick_actions_open = false;
                    deferred = DeferredPostAction::DetectLanguage(tab_id.clone());
                }
                PostEditorMsg::Translate => {
                    state.quick_actions_open = false;
                    deferred = DeferredPostAction::OpenTranslate(tab_id.clone());
                }
                PostEditorMsg::TranslateTo(target_language) => {
                    deferred = DeferredPostAction::TranslateTo {
                        post_id: tab_id.clone(),
                        target_language,
                    };
                }
                PostEditorMsg::TitleChanged(s) => {
                    state.title = s;
                    state.mark_dirty();
                }
                PostEditorMsg::SlugChanged(s) => {
                    state.slug = s;
                    state.mark_dirty();
                }
                PostEditorMsg::ExcerptChanged(s) => {
                    state.excerpt = s;
                    state.mark_dirty();
                }
                PostEditorMsg::ContentChanged(new_text) => {
                    state.content = new_text;
                    state.mark_dirty();
                    refresh_linked_media = Some((state.post_id.clone(), state.content.clone()));
                }
                PostEditorMsg::AuthorChanged(s) => {
                    state.author = s;
                    state.mark_dirty();
                }
                PostEditorMsg::LanguageChanged(s) => {
                    state.language = s;
                    state.mark_dirty();
                }
                PostEditorMsg::TemplateSlugChanged(s) => {
                    state.template_slug = s;
                    state.mark_dirty();
                }
                PostEditorMsg::ToggleDoNotTranslate(b) => {
                    state.do_not_translate = b;
                    state.mark_dirty();
                }
                PostEditorMsg::ToggleMetadata => {
                    state.metadata_expanded = !state.metadata_expanded;
                }
                PostEditorMsg::ToggleExcerpt => {
                    state.excerpt_expanded = !state.excerpt_expanded;
                }
                PostEditorMsg::SwitchLanguage(lang) => {
                    state.switch_language(&lang);
                    if state.editor_mode == "preview" {
                        deferred = DeferredPostAction::SyncEmbeddedPreview;
                    }
                }
                PostEditorMsg::TagsInputChanged(s) => {
                    state.tags_input = s;
                }
                PostEditorMsg::TagsInputSubmit => {
                    let tag = state.tags_input.trim().to_string();
                    if !tag.is_empty() && !state.tags.contains(&tag) {
                        state.tags.push(tag);
                        state.mark_dirty();
                    }
                    state.tags_input.clear();
                }
                PostEditorMsg::RemoveTag(tag) => {
                    state.tags.retain(|t| t != &tag);
                    state.mark_dirty();
                }
                PostEditorMsg::CategoriesInputChanged(s) => {
                    state.categories_input = s;
                }
                PostEditorMsg::CategoriesInputSubmit => {
                    let cat = state.categories_input.trim().to_string();
                    if !cat.is_empty() && !state.categories.contains(&cat) {
                        state.categories.push(cat);
                        state.mark_dirty();
                    }
                    state.categories_input.clear();
                }
                PostEditorMsg::RemoveCategory(cat) => {
                    state.categories.retain(|c| c != &cat);
                    state.mark_dirty();
                }
                PostEditorMsg::Save => {
                    deferred = DeferredPostAction::Save(tab_id.clone());
                }
                PostEditorMsg::Publish => {
                    deferred = DeferredPostAction::Publish(tab_id.clone());
                }
                PostEditorMsg::Discard => {
                    deferred = DeferredPostAction::Discard(tab_id.clone());
                }
                PostEditorMsg::Delete => {
                    deferred = DeferredPostAction::ShowDelete {
                        tab_id: tab_id.clone(),
                        name: state.title.clone(),
                    };
                }
                PostEditorMsg::InsertLink => {
                    deferred = DeferredPostAction::OpenInsertLink(state.post_id.clone());
                }
                PostEditorMsg::InsertMedia => {
                    deferred = DeferredPostAction::OpenInsertMedia {
                        post_id: state.post_id.clone(),
                        link_only: false,
                    };
                }
                PostEditorMsg::Gallery => {
                    deferred = DeferredPostAction::OpenGallery(state.post_id.clone());
                }
                PostEditorMsg::LinkExistingMedia => {
                    deferred = DeferredPostAction::OpenInsertMedia {
                        post_id: state.post_id.clone(),
                        link_only: true,
                    };
                }
                PostEditorMsg::OpenLinkedMedia(media_id) => {
                    deferred = DeferredPostAction::OpenLinkedMedia(media_id);
                }
                PostEditorMsg::UnlinkLinkedMedia(media_id) => {
                    deferred = DeferredPostAction::UnlinkLinkedMedia {
                        post_id: state.post_id.clone(),
                        media_id,
                    };
                }
                PostEditorMsg::PostInsertLinkSelected(linked_post_id) => {
                    deferred = DeferredPostAction::InsertSelectedLink {
                        post_id: state.post_id.clone(),
                        linked_post_id,
                    };
                }
                PostEditorMsg::PostInsertLinkCreate => {
                    deferred = DeferredPostAction::CreateLinkedPost(state.post_id.clone());
                }
                PostEditorMsg::PostInsertMediaSelected(media_id) => {
                    deferred = DeferredPostAction::InsertSelectedMedia {
                        post_id: state.post_id.clone(),
                        media_id,
                    };
                }
                PostEditorMsg::PostGalleryImageSelected(index) => {
                    deferred = DeferredPostAction::SelectGalleryImage(index);
                }
                PostEditorMsg::PostInsertLinkTabSwitch(tab) => {
                    deferred = DeferredPostAction::SetLinkTab(tab);
                }
                PostEditorMsg::PostInsertLinkSearch(query) => {
                    deferred = DeferredPostAction::SetLinkSearch(query);
                }
                PostEditorMsg::PostInsertLinkUrlChanged(url) => {
                    deferred = DeferredPostAction::SetExternalUrl(url);
                }
                PostEditorMsg::PostInsertLinkTextChanged(text) => {
                    deferred = DeferredPostAction::SetExternalText(text);
                }
                PostEditorMsg::PostInsertLinkExternalInsert => {
                    deferred = DeferredPostAction::InsertExternalLink;
                }
                PostEditorMsg::PostInsertMediaSearch(query) => {
                    deferred = DeferredPostAction::SetMediaSearch(query);
                }
                PostEditorMsg::PostGalleryPrevious => {
                    deferred = DeferredPostAction::GalleryPrevious;
                }
                PostEditorMsg::PostGalleryNext => {
                    deferred = DeferredPostAction::GalleryNext;
                }
                PostEditorMsg::PostGalleryCloseLightbox => {
                    deferred = DeferredPostAction::GalleryCloseLightbox;
                }
            }

            if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == state.post_id) {
                tab.is_dirty = state.is_dirty;
            }
        }

        if let Some((post_id, content)) = refresh_linked_media {
            let linked_media = self.load_post_media_items(&post_id, Some(&content));
            if let Some(state) = self.post_editors.get_mut(&post_id) {
                state.linked_media = linked_media;
            }
        }

        match deferred {
            DeferredPostAction::None => Task::none(),
            DeferredPostAction::SyncEmbeddedPreview => self.sync_embedded_preview_for_active_post(),
            DeferredPostAction::Analyze(tab_id) => self.run_post_ai_analysis(&tab_id),
            DeferredPostAction::AnalyzeTaxonomy(tab_id) => self.run_post_taxonomy_analysis(&tab_id),
            DeferredPostAction::AddGalleryImages(post_id) => self.add_gallery_images(&post_id),
            DeferredPostAction::DetectLanguage(tab_id) => self.detect_post_language(&tab_id),
            DeferredPostAction::OpenTranslate(tab_id) => self.open_post_translation_modal(&tab_id),
            DeferredPostAction::TranslateTo {
                post_id,
                target_language,
            } => self.translate_post_to(&post_id, &target_language),
            DeferredPostAction::Save(tab_id) => self.save_post_editor(&tab_id),
            DeferredPostAction::Publish(tab_id) => self.publish_post_editor(&tab_id),
            DeferredPostAction::Discard(tab_id) => self.discard_post_editor(&tab_id),
            DeferredPostAction::ShowDelete { tab_id, name } => {
                Task::done(Message::ShowModal(modal::ModalState::ConfirmDelete {
                    entity_name: name,
                    references: Vec::new(),
                    on_confirm: modal::ConfirmAction::DeletePost(tab_id),
                }))
            }
            DeferredPostAction::OpenInsertLink(post_id) => self.insert_link_modal(&post_id),
            DeferredPostAction::OpenInsertMedia { post_id, link_only } => {
                self.insert_media_modal(&post_id, link_only)
            }
            DeferredPostAction::OpenGallery(post_id) => self.post_gallery(&post_id),
            DeferredPostAction::OpenLinkedMedia(media_id) => {
                let title = self
                    .db
                    .as_ref()
                    .and_then(|db| {
                        bds_core::db::queries::media::get_media_by_id(db.conn(), &media_id)
                            .ok()
                            .map(|media| media.title.unwrap_or(media.original_name))
                    })
                    .unwrap_or_else(|| media_id.clone());
                Task::done(Message::OpenTab(Tab {
                    id: media_id,
                    tab_type: TabType::Media,
                    title,
                    is_transient: false,
                    is_dirty: false,
                }))
            }
            DeferredPostAction::UnlinkLinkedMedia { post_id, media_id } => {
                if let (Some(db), Some(data_dir)) = (&self.db, &self.data_dir) {
                    if let Err(err) = engine::post_media::unlink_media_from_post(
                        db.conn(),
                        data_dir,
                        &post_id,
                        &media_id,
                    ) {
                        self.notify_operation_failed("editor.unlinkMedia", err);
                        return Task::none();
                    }
                    self.refresh_post_relationships(&post_id);
                }
                Task::none()
            }
            DeferredPostAction::InsertSelectedLink {
                post_id,
                linked_post_id,
            } => self.insert_selected_post_link(&post_id, &linked_post_id),
            DeferredPostAction::CreateLinkedPost(post_id) => {
                self.insert_created_post_link(&post_id)
            }
            DeferredPostAction::InsertSelectedMedia { post_id, media_id } => {
                self.insert_selected_media(&post_id, &media_id)
            }
            DeferredPostAction::SetLinkTab(tab) => {
                self.refresh_post_insert_link_modal(Some(tab), None, None, None);
                Task::none()
            }
            DeferredPostAction::SetLinkSearch(query) => {
                self.refresh_post_insert_link_modal(None, Some(query), None, None);
                Task::none()
            }
            DeferredPostAction::SetExternalUrl(url) => {
                self.refresh_post_insert_link_modal(None, None, Some(url), None);
                Task::none()
            }
            DeferredPostAction::SetExternalText(text) => {
                self.refresh_post_insert_link_modal(None, None, None, Some(text));
                Task::none()
            }
            DeferredPostAction::InsertExternalLink => {
                if let Some(modal::ModalState::PostInsertLink {
                    post_id,
                    external_url,
                    external_text,
                    ..
                }) = self.active_modal.clone()
                {
                    if let Some(markdown) =
                        modal::external_link_markdown(&external_url, &external_text)
                    {
                        self.insert_markdown_into_post(&post_id, &markdown)
                    } else {
                        self.notify(
                            ToastLevel::Error,
                            &t(self.ui_locale, "modal.postInsertLink.urlRequired"),
                        );
                        Task::none()
                    }
                } else {
                    Task::none()
                }
            }
            DeferredPostAction::SetMediaSearch(query) => {
                self.refresh_insert_media_modal(query);
                Task::none()
            }
            DeferredPostAction::SelectGalleryImage(index) => {
                self.update_gallery_selection(Some(index));
                Task::none()
            }
            DeferredPostAction::GalleryPrevious => {
                self.step_gallery_selection(-1);
                Task::none()
            }
            DeferredPostAction::GalleryNext => {
                self.step_gallery_selection(1);
                Task::none()
            }
            DeferredPostAction::GalleryCloseLightbox => {
                self.update_gallery_selection(None);
                Task::none()
            }
        }
    }

    pub(super) fn handle_media_editor_msg(&mut self, msg: MediaEditorMsg) -> Task<Message> {
        enum DeferredMediaAction {
            None,
            Analyze(String),
            DetectLanguage(String),
            OpenTranslate(String),
            TranslateTo {
                media_id: String,
                target_language: String,
            },
            LinkPost {
                media_id: String,
                post_id: String,
            },
            OpenLinkedPost(String),
            UnlinkPost {
                media_id: String,
                post_id: String,
            },
            Save(String),
            Replace(String),
            Delete {
                tab_id: String,
                name: String,
            },
        }

        let mut deferred = DeferredMediaAction::None;
        let mut picker_refresh: Option<(String, String)> = None;
        if let Some(tab_id) = self.active_tab.clone()
            && let Some(state) = self.media_editors.get_mut(&tab_id)
        {
            match msg {
                MediaEditorMsg::ToggleQuickActions => {
                    state.quick_actions_open = !state.quick_actions_open;
                }
                MediaEditorMsg::AnalyzeWithAi => {
                    state.quick_actions_open = false;
                    deferred = DeferredMediaAction::Analyze(tab_id.clone());
                }
                MediaEditorMsg::DetectLanguage => {
                    state.quick_actions_open = false;
                    deferred = DeferredMediaAction::DetectLanguage(tab_id.clone());
                }
                MediaEditorMsg::TranslateMetadata => {
                    state.quick_actions_open = false;
                    deferred = DeferredMediaAction::OpenTranslate(tab_id.clone());
                }
                MediaEditorMsg::TranslateTo(target_language) => {
                    deferred = DeferredMediaAction::TranslateTo {
                        media_id: tab_id.clone(),
                        target_language,
                    };
                }
                MediaEditorMsg::TitleChanged(s) => {
                    state.title = s;
                    state.is_dirty = true;
                }
                MediaEditorMsg::AltChanged(s) => {
                    state.alt = s;
                    state.is_dirty = true;
                }
                MediaEditorMsg::CaptionChanged(s) => {
                    state.caption = s;
                    state.is_dirty = true;
                }
                MediaEditorMsg::AuthorChanged(s) => {
                    state.author = s;
                    state.is_dirty = true;
                }
                MediaEditorMsg::LanguageChanged(s) => {
                    state.language = s;
                    state.is_dirty = true;
                }
                MediaEditorMsg::TagsChanged(s) => {
                    state.tags_input = s;
                    state.is_dirty = true;
                }
                MediaEditorMsg::SwitchLanguage(lang) => {
                    state.switch_language(&lang);
                }
                MediaEditorMsg::TogglePostPicker => {
                    let next_open = !state.post_picker_open;
                    state.post_picker_open = next_open;
                    if !next_open {
                        state.post_picker_results.clear();
                    } else {
                        picker_refresh =
                            Some((state.media_id.clone(), state.post_picker_search.clone()));
                    }
                }
                MediaEditorMsg::PostPickerSearchChanged(search) => {
                    state.post_picker_search = search;
                    if state.post_picker_open {
                        picker_refresh =
                            Some((state.media_id.clone(), state.post_picker_search.clone()));
                    }
                }
                MediaEditorMsg::LinkPost(post_id) => {
                    deferred = DeferredMediaAction::LinkPost {
                        media_id: state.media_id.clone(),
                        post_id,
                    };
                }
                MediaEditorMsg::OpenLinkedPost(post_id) => {
                    deferred = DeferredMediaAction::OpenLinkedPost(post_id);
                }
                MediaEditorMsg::UnlinkPost(post_id) => {
                    deferred = DeferredMediaAction::UnlinkPost {
                        media_id: state.media_id.clone(),
                        post_id,
                    };
                }
                MediaEditorMsg::Save => {
                    deferred = DeferredMediaAction::Save(tab_id.clone());
                }
                MediaEditorMsg::ReplaceFile => {
                    deferred = DeferredMediaAction::Replace(tab_id.clone());
                }
                MediaEditorMsg::Delete => {
                    deferred = DeferredMediaAction::Delete {
                        tab_id: tab_id.clone(),
                        name: state.title.clone(),
                    };
                }
            }
            if let Some(tab) = self
                .tabs
                .iter_mut()
                .find(|t| t.id == *state.media_id.as_str())
            {
                tab.is_dirty = state.is_dirty;
            }
        }
        if let Some((media_id, search)) = picker_refresh {
            let results = self.query_media_post_picker_results(&media_id, &search);
            if let Some(state) = self.media_editors.get_mut(&media_id) {
                state.post_picker_results = results;
            }
        }
        match deferred {
            DeferredMediaAction::None => Task::none(),
            DeferredMediaAction::Analyze(media_id) => self.run_media_ai_analysis(&media_id),
            DeferredMediaAction::DetectLanguage(media_id) => self.detect_media_language(&media_id),
            DeferredMediaAction::OpenTranslate(media_id) => {
                self.open_media_translation_modal(&media_id)
            }
            DeferredMediaAction::TranslateTo {
                media_id,
                target_language,
            } => self.translate_media_to(&media_id, &target_language),
            DeferredMediaAction::LinkPost { media_id, post_id } => {
                self.link_media_to_post(&media_id, &post_id)
            }
            DeferredMediaAction::OpenLinkedPost(post_id) => {
                let title = self
                    .db
                    .as_ref()
                    .and_then(|db| {
                        bds_core::db::queries::post::get_post_by_id(db.conn(), &post_id).ok()
                    })
                    .map(|post| post.title)
                    .unwrap_or_else(|| post_id.clone());
                Task::done(Message::OpenTab(Tab {
                    id: post_id,
                    tab_type: TabType::Post,
                    title,
                    is_transient: false,
                    is_dirty: false,
                }))
            }
            DeferredMediaAction::UnlinkPost { media_id, post_id } => {
                self.unlink_media_from_post(&media_id, &post_id)
            }
            DeferredMediaAction::Save(tab_id) => self.save_media_editor(&tab_id),
            DeferredMediaAction::Replace(media_id) => {
                crate::platform::dialog::pick_media_replacement(
                    media_id,
                    t(self.ui_locale, "dialog.replaceMedia"),
                    t(self.ui_locale, "dialog.imageFilter"),
                )
            }
            DeferredMediaAction::Delete { tab_id, name } => {
                Task::done(Message::ShowModal(modal::ModalState::ConfirmDelete {
                    entity_name: name,
                    references: Vec::new(),
                    on_confirm: modal::ConfirmAction::DeleteMedia(tab_id),
                }))
            }
        }
    }

    pub(super) fn handle_template_editor_msg(&mut self, msg: TemplateEditorMsg) -> Task<Message> {
        if let Some(tab_id) = self.active_tab.clone()
            && let Some(state) = self.template_editors.get_mut(&tab_id)
        {
            match msg {
                TemplateEditorMsg::TitleChanged(s) => {
                    state.title = s;
                    state.is_dirty = true;
                }
                TemplateEditorMsg::SlugChanged(s) => {
                    state.slug = s;
                    state.is_dirty = true;
                }
                TemplateEditorMsg::KindChanged(k) => {
                    state.kind = k.0;
                    state.is_dirty = true;
                }
                TemplateEditorMsg::EnabledChanged(b) => {
                    state.enabled = b;
                    state.is_dirty = true;
                }
                TemplateEditorMsg::ContentChanged(new_text) => {
                    state.content = new_text;
                    state.is_dirty = true;
                }
                TemplateEditorMsg::Save => {
                    return self.save_template_editor(&tab_id);
                }
                TemplateEditorMsg::Validate => {
                    if let Some(st) = self.template_editors.get_mut(&tab_id) {
                        match engine::template::validate_template(&st.content) {
                            Ok(()) => {
                                st.validation_error = None;
                            }
                            Err(e) => {
                                st.validation_error = Some(e);
                            }
                        }
                    }
                }
                TemplateEditorMsg::Delete => {
                    return self.show_template_delete_confirmation(&tab_id);
                }
            }
            if let Some(st) = self.template_editors.get(&tab_id)
                && let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id)
            {
                tab.is_dirty = st.is_dirty;
            }
        }
        Task::none()
    }

    pub(super) fn handle_script_editor_msg(&mut self, msg: ScriptEditorMsg) -> Task<Message> {
        let mut run_request = None;
        if let Some(tab_id) = self.active_tab.clone()
            && let Some(state) = self.script_editors.get_mut(&tab_id)
        {
            match msg {
                ScriptEditorMsg::TitleChanged(s) => {
                    state.title = s;
                    state.is_dirty = true;
                }
                ScriptEditorMsg::SlugChanged(s) => {
                    state.slug = s;
                    state.is_dirty = true;
                }
                ScriptEditorMsg::KindChanged(k) => {
                    state.kind = k.0;
                    state.is_dirty = true;
                }
                ScriptEditorMsg::EntrypointChanged(s) => {
                    state.entrypoint = s;
                    state.is_dirty = true;
                }
                ScriptEditorMsg::EnabledChanged(b) => {
                    state.enabled = b;
                    state.is_dirty = true;
                }
                ScriptEditorMsg::ContentChanged(new_text) => {
                    state.discovered_entrypoints = engine::script::discover_entrypoints(&new_text);
                    state.content = new_text;
                    state.is_dirty = true;
                }
                ScriptEditorMsg::Save => {
                    return self.save_script_editor(&tab_id);
                }
                ScriptEditorMsg::CheckSyntax => {
                    if let Some(st) = self.script_editors.get_mut(&tab_id) {
                        match engine::script::validate_script_syntax(&st.content) {
                            Ok(()) => {
                                st.validation_error = None;
                            }
                            Err(e) => {
                                st.validation_error = Some(e);
                            }
                        }
                    }
                }
                ScriptEditorMsg::Run => {
                    run_request = Some((
                        state.content.clone(),
                        state.entrypoint.clone(),
                        state.kind.clone(),
                    ));
                }
                ScriptEditorMsg::Delete => {
                    let name = state.title.clone();
                    return Task::done(Message::ShowModal(modal::ModalState::ConfirmDelete {
                        entity_name: name,
                        references: Vec::new(),
                        on_confirm: modal::ConfirmAction::DeleteScript(tab_id),
                    }));
                }
            }
            if let Some(st) = self.script_editors.get(&tab_id)
                && let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id)
            {
                tab.is_dirty = st.is_dirty;
            }
        }
        if let Some((source, entrypoint, kind)) = run_request {
            let offline_mode = self.offline_mode;
            let app_handler = crate::platform::script_host::handler(
                Arc::clone(&self.script_menu_actions),
                t(self.ui_locale, "dialog.selectFolder"),
            );
            return self.spawn_engine_task(
                "engine.runScript",
                move |db_path, project_id, data_dir, task_manager, task_id| {
                    let execution_kind = match kind {
                        bds_core::model::ScriptKind::Macro => {
                            bds_core::scripting::ExecutionKind::Macro
                        }
                        bds_core::model::ScriptKind::Utility => {
                            bds_core::scripting::ExecutionKind::Utility
                        }
                        bds_core::model::ScriptKind::Transform => {
                            bds_core::scripting::ExecutionKind::Transform
                        }
                    };
                    let host = bds_core::scripting::CoreHost::new(db_path, project_id, data_dir)
                        .with_task(Arc::clone(&task_manager), task_id)
                        .with_offline_mode(offline_mode)
                        .with_app_handler(Arc::clone(&app_handler));
                    let control = task_manager
                        .cancellation_flag(task_id)
                        .map(bds_core::scripting::ExecutionControl::from_cancelled)
                        .unwrap_or_default();
                    let result = bds_core::scripting::execute_with_host(
                        &source,
                        &entrypoint,
                        &serde_json::json!({}),
                        execution_kind,
                        &control,
                        Arc::new(host),
                    )?;
                    let mut lines = result.output;
                    lines.extend(
                        result
                            .toasts
                            .into_iter()
                            .map(|message| format!("toast: {message}")),
                    );
                    if result.value != serde_json::Value::Null {
                        lines.push(result.value.to_string());
                    }
                    Ok(if lines.is_empty() {
                        "completed".to_string()
                    } else {
                        lines.join("\n")
                    })
                },
            );
        }
        Task::none()
    }
}
