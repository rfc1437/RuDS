// @generated automatically by Diesel CLI.

diesel::table! {
    ai_catalog_meta (key) {
        key -> Text,
        value -> Text,
    }
}

diesel::table! {
    ai_model_modalities (rowid) {
        rowid -> Integer,
        provider -> Text,
        model_id -> Text,
        direction -> Text,
        modality -> Text,
    }
}

diesel::table! {
    ai_models (provider, model_id) {
        provider -> Text,
        model_id -> Text,
        name -> Text,
        family -> Nullable<Text>,
        attachment -> Integer,
        reasoning -> Integer,
        tool_call -> Integer,
        structured_output -> Integer,
        temperature -> Integer,
        knowledge -> Nullable<Text>,
        release_date -> Nullable<Text>,
        last_updated_date -> Nullable<Text>,
        open_weights -> Integer,
        input_price -> Nullable<Integer>,
        output_price -> Nullable<Integer>,
        cache_read_price -> Nullable<Integer>,
        cache_write_price -> Nullable<Integer>,
        context_window -> Integer,
        max_input_tokens -> Integer,
        max_output_tokens -> Integer,
        interleaved -> Nullable<Text>,
        status -> Nullable<Text>,
        provider_package_ref -> Nullable<Text>,
        updated_at -> BigInt,
    }
}

diesel::table! {
    ai_providers (id) {
        id -> Text,
        name -> Text,
        env -> Nullable<Text>,
        package_ref -> Nullable<Text>,
        api -> Nullable<Text>,
        doc -> Nullable<Text>,
        updated_at -> BigInt,
    }
}

diesel::table! {
    chat_conversations (id) {
        id -> Text,
        title -> Text,
        model -> Nullable<Text>,
        copilot_session_id -> Nullable<Text>,
        created_at -> BigInt,
        updated_at -> BigInt,
    }
}

diesel::table! {
    chat_messages (id) {
        id -> Integer,
        conversation_id -> Text,
        role -> Text,
        content -> Nullable<Text>,
        tool_call_id -> Nullable<Text>,
        tool_calls -> Nullable<Text>,
        created_at -> BigInt,
        cache_read_tokens -> Nullable<Integer>,
        cache_write_tokens -> Nullable<Integer>,
        token_usage_input -> Nullable<Integer>,
        token_usage_output -> Nullable<Integer>,
    }
}

diesel::table! {
    db_notifications (id) {
        id -> Integer,
        entity_type -> Text,
        entity_id -> Text,
        action -> Text,
        from_cli -> Integer,
        seen_at -> Nullable<BigInt>,
        created_at -> BigInt,
    }
}

diesel::table! {
    dismissed_duplicate_pairs (id) {
        id -> Text,
        project_id -> Text,
        post_id_a -> Text,
        post_id_b -> Text,
        dismissed_at -> BigInt,
    }
}

diesel::table! {
    embedding_keys (label) {
        label -> BigInt,
        post_id -> Text,
        project_id -> Text,
        content_hash -> Text,
        vector -> Text,
    }
}

diesel::table! {
    generated_file_hashes (rowid) {
        rowid -> Integer,
        project_id -> Text,
        relative_path -> Text,
        content_hash -> Text,
        updated_at -> BigInt,
    }
}

diesel::table! {
    import_definitions (id) {
        id -> Text,
        project_id -> Text,
        name -> Text,
        wxr_file_path -> Nullable<Text>,
        uploads_folder_path -> Nullable<Text>,
        last_analysis_result -> Nullable<Text>,
        created_at -> BigInt,
        updated_at -> BigInt,
    }
}

diesel::table! {
    media (id) {
        id -> Text,
        project_id -> Text,
        filename -> Text,
        original_name -> Text,
        mime_type -> Text,
        size -> BigInt,
        width -> Nullable<Integer>,
        height -> Nullable<Integer>,
        title -> Nullable<Text>,
        alt -> Nullable<Text>,
        caption -> Nullable<Text>,
        author -> Nullable<Text>,
        file_path -> Text,
        sidecar_path -> Text,
        created_at -> BigInt,
        updated_at -> BigInt,
        checksum -> Nullable<Text>,
        tags -> Text,
        language -> Nullable<Text>,
    }
}

diesel::table! {
    media_translations (id) {
        id -> Text,
        project_id -> Text,
        translation_for -> Text,
        language -> Text,
        title -> Nullable<Text>,
        alt -> Nullable<Text>,
        caption -> Nullable<Text>,
        created_at -> BigInt,
        updated_at -> BigInt,
    }
}

diesel::table! {
    post_links (id) {
        id -> Text,
        source_post_id -> Text,
        target_post_id -> Text,
        link_text -> Nullable<Text>,
        created_at -> BigInt,
    }
}

diesel::table! {
    post_media (id) {
        id -> Text,
        project_id -> Text,
        post_id -> Text,
        media_id -> Text,
        sort_order -> Integer,
        created_at -> BigInt,
    }
}

diesel::table! {
    post_translations (id) {
        id -> Text,
        project_id -> Text,
        translation_for -> Text,
        language -> Text,
        title -> Text,
        excerpt -> Nullable<Text>,
        content -> Nullable<Text>,
        status -> Text,
        created_at -> BigInt,
        updated_at -> BigInt,
        published_at -> Nullable<BigInt>,
        file_path -> Text,
        checksum -> Nullable<Text>,
    }
}

diesel::table! {
    posts (id) {
        id -> Text,
        project_id -> Text,
        title -> Text,
        slug -> Text,
        excerpt -> Nullable<Text>,
        content -> Nullable<Text>,
        status -> Text,
        author -> Nullable<Text>,
        created_at -> BigInt,
        updated_at -> BigInt,
        published_at -> Nullable<BigInt>,
        file_path -> Text,
        checksum -> Nullable<Text>,
        tags -> Text,
        categories -> Text,
        template_slug -> Nullable<Text>,
        language -> Nullable<Text>,
        do_not_translate -> Integer,
        published_title -> Nullable<Text>,
        published_content -> Nullable<Text>,
        published_tags -> Nullable<Text>,
        published_categories -> Nullable<Text>,
        published_excerpt -> Nullable<Text>,
    }
}

diesel::table! {
    projects (id) {
        id -> Text,
        name -> Text,
        slug -> Text,
        description -> Nullable<Text>,
        data_path -> Nullable<Text>,
        is_active -> Integer,
        created_at -> BigInt,
        updated_at -> BigInt,
    }
}

diesel::table! {
    scripts (id) {
        id -> Text,
        project_id -> Text,
        slug -> Text,
        title -> Text,
        kind -> Text,
        entrypoint -> Text,
        enabled -> Integer,
        version -> Integer,
        file_path -> Text,
        status -> Text,
        content -> Nullable<Text>,
        created_at -> BigInt,
        updated_at -> BigInt,
    }
}

diesel::table! {
    settings (key) {
        key -> Text,
        value -> Text,
        updated_at -> BigInt,
    }
}

diesel::table! {
    tags (id) {
        id -> Text,
        project_id -> Text,
        name -> Text,
        color -> Nullable<Text>,
        post_template_slug -> Nullable<Text>,
        created_at -> BigInt,
        updated_at -> BigInt,
    }
}

diesel::table! {
    templates (id) {
        id -> Text,
        project_id -> Text,
        slug -> Text,
        title -> Text,
        kind -> Text,
        enabled -> Integer,
        version -> Integer,
        file_path -> Text,
        status -> Text,
        content -> Nullable<Text>,
        created_at -> BigInt,
        updated_at -> BigInt,
    }
}

diesel::joinable!(ai_models -> ai_providers (provider));
diesel::joinable!(chat_messages -> chat_conversations (conversation_id));
diesel::joinable!(dismissed_duplicate_pairs -> projects (project_id));
diesel::joinable!(generated_file_hashes -> projects (project_id));
diesel::joinable!(import_definitions -> projects (project_id));
diesel::joinable!(media -> projects (project_id));
diesel::joinable!(media_translations -> media (translation_for));
diesel::joinable!(media_translations -> projects (project_id));
diesel::joinable!(post_media -> media (media_id));
diesel::joinable!(post_media -> posts (post_id));
diesel::joinable!(post_media -> projects (project_id));
diesel::joinable!(post_translations -> posts (translation_for));
diesel::joinable!(post_translations -> projects (project_id));
diesel::joinable!(posts -> projects (project_id));
diesel::joinable!(scripts -> projects (project_id));
diesel::joinable!(tags -> projects (project_id));
diesel::joinable!(templates -> projects (project_id));

diesel::allow_tables_to_appear_in_same_query!(
    ai_catalog_meta,
    ai_model_modalities,
    ai_models,
    ai_providers,
    chat_conversations,
    chat_messages,
    db_notifications,
    dismissed_duplicate_pairs,
    embedding_keys,
    generated_file_hashes,
    import_definitions,
    media,
    media_translations,
    post_links,
    post_media,
    post_translations,
    posts,
    projects,
    scripts,
    settings,
    tags,
    templates,
);
