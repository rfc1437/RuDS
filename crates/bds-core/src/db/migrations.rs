use rusqlite::Connection;

mod embedded {
    use refinery::embed_migrations;
    embed_migrations!("migrations");
}

/// Run all embedded migrations against the given connection using refinery.
///
/// Creates the full bDS schema as specified in specs/schema.allium.
/// Uses refinery for proper versioned migration tracking.
pub fn run_migrations(conn: &mut Connection) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    embedded::migrations::runner().run(conn)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        run_migrations(&mut conn).unwrap();
        conn
    }

    /// Helper: insert a project row and return its id.
    fn insert_project(conn: &Connection, id: &str, slug: &str) {
        conn.execute(
            "INSERT INTO projects (id, name, slug, is_active, created_at, updated_at)
             VALUES (?1, 'Test', ?2, 0, 1000, 1000)",
            rusqlite::params![id, slug],
        )
        .unwrap();
    }

    /// Helper: insert a post row and return its id.
    fn insert_post(conn: &Connection, id: &str, project_id: &str, slug: &str) {
        conn.execute(
            "INSERT INTO posts (id, project_id, title, slug, status, created_at, updated_at)
             VALUES (?1, ?2, 'Test Post', ?3, 'draft', 1000, 1000)",
            rusqlite::params![id, project_id, slug],
        )
        .unwrap();
    }

    /// Helper: insert a media row.
    fn insert_media(conn: &Connection, id: &str, project_id: &str) {
        conn.execute(
            "INSERT INTO media (id, project_id, filename, original_name, mime_type, size, file_path, sidecar_path, created_at, updated_at)
             VALUES (?1, ?2, 'img.jpg', 'photo.jpg', 'image/jpeg', 12345, '/media/img.jpg', '/media/img.jpg.meta', 1000, 1000)",
            rusqlite::params![id, project_id],
        )
        .unwrap();
    }

    // ================================================================
    // TABLE EXISTENCE — all tables from schema.allium must exist
    // ================================================================

    #[test]
    fn all_tables_exist() {
        let conn = setup();
        let expected = [
            "projects", "posts", "post_translations", "media", "media_translations",
            "tags", "templates", "scripts", "post_links", "post_media", "settings",
            "generated_file_hashes", "chat_conversations", "chat_messages", "ai_providers",
            "ai_models", "ai_model_modalities", "ai_catalog_meta", "embedding_keys",
            "dismissed_duplicate_pairs", "import_definitions", "db_notifications",
        ];
        for table in &expected {
            let count: i64 = conn
                .query_row(
                    &format!("SELECT COUNT(*) FROM {table}"),
                    [],
                    |row| row.get(0),
                )
                .unwrap_or_else(|e| panic!("table '{table}' should be queryable: {e}"));
            assert_eq!(count, 0, "table '{table}' should start empty");
        }
    }

    // ================================================================
    // REFINERY TRACKING — verify migration history table exists
    // ================================================================

    #[test]
    fn refinery_schema_history_exists() {
        let conn = setup();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM refinery_schema_history",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(count >= 1, "refinery should track at least one migration");
    }

    // ================================================================
    // UNIQUE INDEX ENFORCEMENT — spec invariant tests
    // ================================================================

    #[test]
    fn unique_project_slug() {
        let conn = setup();
        insert_project(&conn, "p1", "blog");
        let err = conn.execute(
            "INSERT INTO projects (id, name, slug, is_active, created_at, updated_at)
             VALUES ('p2', 'Other', 'blog', 0, 1000, 1000)",
            [],
        );
        assert!(err.is_err(), "duplicate project slug must be rejected");
    }

    #[test]
    fn unique_post_slug_per_project() {
        let conn = setup();
        insert_project(&conn, "p1", "blog");
        insert_post(&conn, "post1", "p1", "hello");
        let err = conn.execute(
            "INSERT INTO posts (id, project_id, title, slug, status, created_at, updated_at)
             VALUES ('post2', 'p1', 'Other', 'hello', 'draft', 1000, 1000)",
            [],
        );
        assert!(err.is_err(), "duplicate post slug within same project must be rejected");
    }

    #[test]
    fn same_post_slug_different_project_ok() {
        let conn = setup();
        insert_project(&conn, "p1", "blog1");
        insert_project(&conn, "p2", "blog2");
        insert_post(&conn, "post1", "p1", "hello");
        conn.execute(
            "INSERT INTO posts (id, project_id, title, slug, status, created_at, updated_at)
             VALUES ('post2', 'p2', 'Other', 'hello', 'draft', 1000, 1000)",
            [],
        )
        .expect("same slug in different project should be allowed");
    }

    #[test]
    fn unique_translation_per_post_language() {
        let conn = setup();
        insert_project(&conn, "p1", "blog");
        insert_post(&conn, "post1", "p1", "hello");
        conn.execute(
            "INSERT INTO post_translations (id, project_id, translation_for, language, title, status, created_at, updated_at)
             VALUES ('t1', 'p1', 'post1', 'de', 'Hallo', 'draft', 1000, 1000)",
            [],
        ).unwrap();
        let err = conn.execute(
            "INSERT INTO post_translations (id, project_id, translation_for, language, title, status, created_at, updated_at)
             VALUES ('t2', 'p1', 'post1', 'de', 'Hallo2', 'draft', 1000, 1000)",
            [],
        );
        assert!(err.is_err(), "duplicate (translation_for, language) must be rejected");
    }

    #[test]
    fn unique_media_translation_per_media_language() {
        let conn = setup();
        insert_project(&conn, "p1", "blog");
        insert_media(&conn, "m1", "p1");
        conn.execute(
            "INSERT INTO media_translations (id, project_id, translation_for, language, created_at, updated_at)
             VALUES ('mt1', 'p1', 'm1', 'de', 1000, 1000)",
            [],
        ).unwrap();
        let err = conn.execute(
            "INSERT INTO media_translations (id, project_id, translation_for, language, created_at, updated_at)
             VALUES ('mt2', 'p1', 'm1', 'de', 1000, 1000)",
            [],
        );
        assert!(err.is_err(), "duplicate (media translation_for, language) must be rejected");
    }

    #[test]
    fn unique_tag_name_per_project() {
        let conn = setup();
        insert_project(&conn, "p1", "blog");
        conn.execute(
            "INSERT INTO tags (id, project_id, name, created_at, updated_at)
             VALUES ('t1', 'p1', 'rust', 1000, 1000)",
            [],
        ).unwrap();
        let err = conn.execute(
            "INSERT INTO tags (id, project_id, name, created_at, updated_at)
             VALUES ('t2', 'p1', 'rust', 1000, 1000)",
            [],
        );
        assert!(err.is_err(), "duplicate tag name within same project must be rejected");
    }

    #[test]
    fn unique_template_slug_per_project() {
        let conn = setup();
        insert_project(&conn, "p1", "blog");
        conn.execute(
            "INSERT INTO templates (id, project_id, slug, title, kind, file_path, created_at, updated_at)
             VALUES ('tpl1', 'p1', 'default', 'Default', 'post', 'templates/default.liquid', 1000, 1000)",
            [],
        ).unwrap();
        let err = conn.execute(
            "INSERT INTO templates (id, project_id, slug, title, kind, file_path, created_at, updated_at)
             VALUES ('tpl2', 'p1', 'default', 'Default2', 'list', 'templates/default.liquid', 1000, 1000)",
            [],
        );
        assert!(err.is_err(), "duplicate template slug within same project must be rejected");
    }

    #[test]
    fn unique_script_slug_per_project() {
        let conn = setup();
        insert_project(&conn, "p1", "blog");
        conn.execute(
            "INSERT INTO scripts (id, project_id, slug, title, kind, file_path, created_at, updated_at)
             VALUES ('s1', 'p1', 'gallery', 'Gallery', 'macro', 'scripts/gallery.lua', 1000, 1000)",
            [],
        ).unwrap();
        let err = conn.execute(
            "INSERT INTO scripts (id, project_id, slug, title, kind, file_path, created_at, updated_at)
             VALUES ('s2', 'p1', 'gallery', 'Gallery2', 'utility', 'scripts/gallery.lua', 1000, 1000)",
            [],
        );
        assert!(err.is_err(), "duplicate script slug within same project must be rejected");
    }

    #[test]
    fn unique_post_media_link() {
        let conn = setup();
        insert_project(&conn, "p1", "blog");
        insert_post(&conn, "post1", "p1", "hello");
        insert_media(&conn, "m1", "p1");
        conn.execute(
            "INSERT INTO post_media (id, project_id, post_id, media_id, sort_order, created_at)
             VALUES ('pm1', 'p1', 'post1', 'm1', 0, 1000)",
            [],
        ).unwrap();
        let err = conn.execute(
            "INSERT INTO post_media (id, project_id, post_id, media_id, sort_order, created_at)
             VALUES ('pm2', 'p1', 'post1', 'm1', 1, 1000)",
            [],
        );
        assert!(err.is_err(), "duplicate (post_id, media_id) must be rejected");
    }

    #[test]
    fn unique_generated_file_hash() {
        let conn = setup();
        insert_project(&conn, "p1", "blog");
        conn.execute(
            "INSERT INTO generated_file_hashes (project_id, relative_path, content_hash, updated_at)
             VALUES ('p1', 'index.html', 'abc123', 1000)",
            [],
        ).unwrap();
        let err = conn.execute(
            "INSERT INTO generated_file_hashes (project_id, relative_path, content_hash, updated_at)
             VALUES ('p1', 'index.html', 'def456', 2000)",
            [],
        );
        assert!(err.is_err(), "duplicate (project_id, relative_path) must be rejected");
    }

    #[test]
    fn unique_dismissed_duplicate_pair() {
        let conn = setup();
        insert_project(&conn, "p1", "blog");
        conn.execute(
            "INSERT INTO dismissed_duplicate_pairs (id, project_id, post_id_a, post_id_b, dismissed_at)
             VALUES ('d1', 'p1', 'a', 'b', 1000)",
            [],
        ).unwrap();
        let err = conn.execute(
            "INSERT INTO dismissed_duplicate_pairs (id, project_id, post_id_a, post_id_b, dismissed_at)
             VALUES ('d2', 'p1', 'a', 'b', 2000)",
            [],
        );
        assert!(err.is_err(), "duplicate (project_id, post_id_a, post_id_b) must be rejected");
    }

    // ================================================================
    // READ/WRITE ROUND-TRIP — core entity tables
    // ================================================================

    #[test]
    fn roundtrip_project() {
        let conn = setup();
        conn.execute(
            "INSERT INTO projects (id, name, slug, description, data_path, is_active, created_at, updated_at)
             VALUES ('p1', 'My Blog', 'my-blog', 'A blog', '/data', 1, 1700000000, 1700000001)",
            [],
        ).unwrap();
        let (id, name, slug, desc, dp, active, ca, ua): (String, String, String, Option<String>, Option<String>, i64, i64, i64) = conn
            .query_row("SELECT id, name, slug, description, data_path, is_active, created_at, updated_at FROM projects WHERE id = 'p1'", [], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?, row.get(6)?, row.get(7)?))
            }).unwrap();
        assert_eq!(id, "p1");
        assert_eq!(name, "My Blog");
        assert_eq!(slug, "my-blog");
        assert_eq!(desc.as_deref(), Some("A blog"));
        assert_eq!(dp.as_deref(), Some("/data"));
        assert_eq!(active, 1);
        assert_eq!(ca, 1700000000);
        assert_eq!(ua, 1700000001);
    }

    #[test]
    fn roundtrip_post_with_all_fields() {
        let conn = setup();
        insert_project(&conn, "p1", "blog");
        conn.execute(
            "INSERT INTO posts (id, project_id, title, slug, excerpt, content, status, author,
             created_at, updated_at, published_at, file_path, checksum, tags, categories,
             template_slug, language, do_not_translate,
             published_title, published_content, published_tags, published_categories, published_excerpt)
             VALUES ('post1', 'p1', 'Hello', 'hello', 'Summary', 'Body text', 'draft', 'Alice',
             1700000000, 1700000001, NULL, '', 'abc123',
             '[\"rust\",\"blog\"]', '[\"tech\"]', 'custom-tpl', 'en', 0,
             NULL, NULL, NULL, NULL, NULL)",
            [],
        ).unwrap();

        let title: String = conn.query_row(
            "SELECT title FROM posts WHERE id = 'post1'", [], |r| r.get(0)
        ).unwrap();
        assert_eq!(title, "Hello");

        let tags: String = conn.query_row(
            "SELECT tags FROM posts WHERE id = 'post1'", [], |r| r.get(0)
        ).unwrap();
        assert_eq!(tags, "[\"rust\",\"blog\"]");

        let content: Option<String> = conn.query_row(
            "SELECT content FROM posts WHERE id = 'post1'", [], |r| r.get(0)
        ).unwrap();
        assert_eq!(content.as_deref(), Some("Body text"));
    }

    #[test]
    fn roundtrip_published_post_null_content() {
        let conn = setup();
        insert_project(&conn, "p1", "blog");
        conn.execute(
            "INSERT INTO posts (id, project_id, title, slug, content, status, file_path, created_at, updated_at, published_at)
             VALUES ('post1', 'p1', 'Published', 'pub', NULL, 'published', 'posts/2024/01/pub.md', 1700000000, 1700000001, 1700000000)",
            [],
        ).unwrap();

        let content: Option<String> = conn.query_row(
            "SELECT content FROM posts WHERE id = 'post1'", [], |r| r.get(0)
        ).unwrap();
        assert!(content.is_none(), "published post content must be null in DB");
    }

    #[test]
    fn roundtrip_post_translation() {
        let conn = setup();
        insert_project(&conn, "p1", "blog");
        insert_post(&conn, "post1", "p1", "hello");
        conn.execute(
            "INSERT INTO post_translations (id, project_id, translation_for, language, title, excerpt, content, status, created_at, updated_at, file_path, checksum)
             VALUES ('t1', 'p1', 'post1', 'de', 'Hallo', 'Zusammenfassung', 'Inhalt', 'draft', 1000, 1000, '', NULL)",
            [],
        ).unwrap();

        let (lang, title): (String, String) = conn.query_row(
            "SELECT language, title FROM post_translations WHERE id = 't1'", [], |r| Ok((r.get(0)?, r.get(1)?))
        ).unwrap();
        assert_eq!(lang, "de");
        assert_eq!(title, "Hallo");
    }

    #[test]
    fn roundtrip_media() {
        let conn = setup();
        insert_project(&conn, "p1", "blog");
        conn.execute(
            "INSERT INTO media (id, project_id, filename, original_name, mime_type, size, width, height,
             title, alt, caption, author, file_path, sidecar_path, created_at, updated_at, checksum, tags, language)
             VALUES ('m1', 'p1', 'abc.jpg', 'photo.jpg', 'image/jpeg', 50000, 1920, 1080,
             'Sunset', 'A sunset', 'Beautiful sunset', 'Bob', '/media/abc.jpg', '/media/abc.jpg.meta',
             1000, 1000, 'hash123', '[\"nature\"]', 'en')",
            [],
        ).unwrap();

        let (orig, w, h, tags): (String, Option<i32>, Option<i32>, String) = conn.query_row(
            "SELECT original_name, width, height, tags FROM media WHERE id = 'm1'", [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        ).unwrap();
        assert_eq!(orig, "photo.jpg");
        assert_eq!(w, Some(1920));
        assert_eq!(h, Some(1080));
        assert_eq!(tags, "[\"nature\"]");
    }

    #[test]
    fn roundtrip_media_translation() {
        let conn = setup();
        insert_project(&conn, "p1", "blog");
        insert_media(&conn, "m1", "p1");
        conn.execute(
            "INSERT INTO media_translations (id, project_id, translation_for, language, title, alt, caption, created_at, updated_at)
             VALUES ('mt1', 'p1', 'm1', 'de', 'Sonnenuntergang', 'Ein Sonnenuntergang', 'Schön', 1000, 1000)",
            [],
        ).unwrap();

        let (title, alt): (Option<String>, Option<String>) = conn.query_row(
            "SELECT title, alt FROM media_translations WHERE id = 'mt1'", [],
            |r| Ok((r.get(0)?, r.get(1)?))
        ).unwrap();
        assert_eq!(title.as_deref(), Some("Sonnenuntergang"));
        assert_eq!(alt.as_deref(), Some("Ein Sonnenuntergang"));
    }

    #[test]
    fn roundtrip_tag() {
        let conn = setup();
        insert_project(&conn, "p1", "blog");
        conn.execute(
            "INSERT INTO tags (id, project_id, name, color, post_template_slug, created_at, updated_at)
             VALUES ('t1', 'p1', 'rust', '#ff5733', 'tag-tpl', 1000, 1000)",
            [],
        ).unwrap();

        let (name, color, tpl): (String, Option<String>, Option<String>) = conn.query_row(
            "SELECT name, color, post_template_slug FROM tags WHERE id = 't1'", [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        ).unwrap();
        assert_eq!(name, "rust");
        assert_eq!(color.as_deref(), Some("#ff5733"));
        assert_eq!(tpl.as_deref(), Some("tag-tpl"));
    }

    #[test]
    fn roundtrip_template() {
        let conn = setup();
        insert_project(&conn, "p1", "blog");
        conn.execute(
            "INSERT INTO templates (id, project_id, slug, title, kind, enabled, version, file_path, status, content, created_at, updated_at)
             VALUES ('tpl1', 'p1', 'default', 'Default', 'post', 1, 3, 'templates/default.liquid', 'published', NULL, 1000, 1000)",
            [],
        ).unwrap();

        let (kind, ver, status, content): (String, i32, String, Option<String>) = conn.query_row(
            "SELECT kind, version, status, content FROM templates WHERE id = 'tpl1'", [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        ).unwrap();
        assert_eq!(kind, "post");
        assert_eq!(ver, 3);
        assert_eq!(status, "published");
        assert!(content.is_none(), "published template content should be null");
    }

    #[test]
    fn roundtrip_script() {
        let conn = setup();
        insert_project(&conn, "p1", "blog");
        conn.execute(
            "INSERT INTO scripts (id, project_id, slug, title, kind, entrypoint, enabled, version, file_path, status, content, created_at, updated_at)
             VALUES ('s1', 'p1', 'gallery', 'Gallery', 'macro', 'render', 1, 1, 'scripts/gallery.lua', 'draft', 'return html', 1000, 1000)",
            [],
        ).unwrap();

        let (kind, ep, content): (String, String, Option<String>) = conn.query_row(
            "SELECT kind, entrypoint, content FROM scripts WHERE id = 's1'", [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        ).unwrap();
        assert_eq!(kind, "macro");
        assert_eq!(ep, "render");
        assert_eq!(content.as_deref(), Some("return html"));
    }

    #[test]
    fn roundtrip_post_link() {
        let conn = setup();
        insert_project(&conn, "p1", "blog");
        insert_post(&conn, "post1", "p1", "hello");
        insert_post(&conn, "post2", "p1", "world");
        conn.execute(
            "INSERT INTO post_links (id, source_post_id, target_post_id, link_text, created_at)
             VALUES ('pl1', 'post1', 'post2', 'see also', 1000)",
            [],
        ).unwrap();

        let (src, tgt, txt): (String, String, Option<String>) = conn.query_row(
            "SELECT source_post_id, target_post_id, link_text FROM post_links WHERE id = 'pl1'", [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        ).unwrap();
        assert_eq!(src, "post1");
        assert_eq!(tgt, "post2");
        assert_eq!(txt.as_deref(), Some("see also"));
    }

    #[test]
    fn roundtrip_post_media_link() {
        let conn = setup();
        insert_project(&conn, "p1", "blog");
        insert_post(&conn, "post1", "p1", "hello");
        insert_media(&conn, "m1", "p1");
        conn.execute(
            "INSERT INTO post_media (id, project_id, post_id, media_id, sort_order, created_at)
             VALUES ('pm1', 'p1', 'post1', 'm1', 5, 1000)",
            [],
        ).unwrap();

        let order: i32 = conn.query_row(
            "SELECT sort_order FROM post_media WHERE id = 'pm1'", [], |r| r.get(0)
        ).unwrap();
        assert_eq!(order, 5);
    }

    #[test]
    fn roundtrip_settings() {
        let conn = setup();
        conn.execute(
            "INSERT INTO settings (key, value, updated_at) VALUES ('theme', 'dark', 1000)",
            [],
        ).unwrap();

        let val: String = conn.query_row(
            "SELECT value FROM settings WHERE key = 'theme'", [], |r| r.get(0)
        ).unwrap();
        assert_eq!(val, "dark");
    }

    #[test]
    fn roundtrip_generated_file_hash() {
        let conn = setup();
        insert_project(&conn, "p1", "blog");
        conn.execute(
            "INSERT INTO generated_file_hashes (project_id, relative_path, content_hash, updated_at)
             VALUES ('p1', 'index.html', 'sha256abc', 1000)",
            [],
        ).unwrap();

        let hash: String = conn.query_row(
            "SELECT content_hash FROM generated_file_hashes WHERE project_id = 'p1' AND relative_path = 'index.html'",
            [], |r| r.get(0)
        ).unwrap();
        assert_eq!(hash, "sha256abc");
    }

    #[test]
    fn roundtrip_chat_conversation() {
        let conn = setup();
        conn.execute(
            "INSERT INTO chat_conversations (id, title, model, created_at, updated_at)
             VALUES ('c1', 'Test Chat', 'gpt-4', 1000, 1000)",
            [],
        ).unwrap();

        let title: String = conn.query_row(
            "SELECT title FROM chat_conversations WHERE id = 'c1'", [], |r| r.get(0)
        ).unwrap();
        assert_eq!(title, "Test Chat");
    }

    #[test]
    fn roundtrip_chat_message() {
        let conn = setup();
        conn.execute(
            "INSERT INTO chat_conversations (id, title, created_at, updated_at)
             VALUES ('c1', 'Chat', 1000, 1000)",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO chat_messages (conversation_id, role, content, created_at)
             VALUES ('c1', 'user', 'Hello', 1000)",
            [],
        ).unwrap();

        let (role, content): (String, Option<String>) = conn.query_row(
            "SELECT role, content FROM chat_messages WHERE conversation_id = 'c1'", [],
            |r| Ok((r.get(0)?, r.get(1)?))
        ).unwrap();
        assert_eq!(role, "user");
        assert_eq!(content.as_deref(), Some("Hello"));
    }

    #[test]
    fn roundtrip_ai_provider_and_model() {
        let conn = setup();
        conn.execute(
            "INSERT INTO ai_providers (id, name, updated_at) VALUES ('openai', 'OpenAI', 1000)",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO ai_models (provider, model_id, name, context_window, max_input_tokens, max_output_tokens, updated_at)
             VALUES ('openai', 'gpt-4', 'GPT-4', 128000, 128000, 4096, 1000)",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO ai_model_modalities (provider, model_id, direction, modality)
             VALUES ('openai', 'gpt-4', 'input', 'text')",
            [],
        ).unwrap();

        let name: String = conn.query_row(
            "SELECT name FROM ai_models WHERE provider = 'openai' AND model_id = 'gpt-4'",
            [], |r| r.get(0)
        ).unwrap();
        assert_eq!(name, "GPT-4");

        let modality: String = conn.query_row(
            "SELECT modality FROM ai_model_modalities WHERE provider = 'openai' AND model_id = 'gpt-4'",
            [], |r| r.get(0)
        ).unwrap();
        assert_eq!(modality, "text");
    }

    #[test]
    fn roundtrip_ai_catalog_meta() {
        let conn = setup();
        conn.execute(
            "INSERT INTO ai_catalog_meta (key, value) VALUES ('etag', 'abc')",
            [],
        ).unwrap();

        let val: String = conn.query_row(
            "SELECT value FROM ai_catalog_meta WHERE key = 'etag'", [], |r| r.get(0)
        ).unwrap();
        assert_eq!(val, "abc");
    }

    #[test]
    fn roundtrip_embedding_keys() {
        let conn = setup();
        conn.execute(
            "INSERT INTO embedding_keys (label, post_id, project_id, content_hash, vector)
             VALUES (1, 'post1', 'p1', 'hash1', 'base64vector')",
            [],
        ).unwrap();

        let vec: String = conn.query_row(
            "SELECT vector FROM embedding_keys WHERE label = 1", [], |r| r.get(0)
        ).unwrap();
        assert_eq!(vec, "base64vector");
    }

    #[test]
    fn roundtrip_dismissed_duplicate_pair() {
        let conn = setup();
        insert_project(&conn, "p1", "blog");
        conn.execute(
            "INSERT INTO dismissed_duplicate_pairs (id, project_id, post_id_a, post_id_b, dismissed_at)
             VALUES ('d1', 'p1', 'a', 'b', 1000)",
            [],
        ).unwrap();

        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM dismissed_duplicate_pairs WHERE project_id = 'p1'",
            [], |r| r.get(0)
        ).unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn roundtrip_import_definition() {
        let conn = setup();
        insert_project(&conn, "p1", "blog");
        conn.execute(
            "INSERT INTO import_definitions (id, project_id, name, wxr_file_path, created_at, updated_at)
             VALUES ('i1', 'p1', 'WP Import', '/exports/wp.xml', 1000, 1000)",
            [],
        ).unwrap();

        let name: String = conn.query_row(
            "SELECT name FROM import_definitions WHERE id = 'i1'", [], |r| r.get(0)
        ).unwrap();
        assert_eq!(name, "WP Import");
    }

    #[test]
    fn roundtrip_db_notification() {
        let conn = setup();
        conn.execute(
            "INSERT INTO db_notifications (entity_type, entity_id, action, from_cli, created_at)
             VALUES ('post', 'post1', 'created', 1, 1000)",
            [],
        ).unwrap();

        let (etype, action, cli): (String, String, i64) = conn.query_row(
            "SELECT entity_type, action, from_cli FROM db_notifications WHERE entity_id = 'post1'",
            [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        ).unwrap();
        assert_eq!(etype, "post");
        assert_eq!(action, "created");
        assert_eq!(cli, 1);
    }

    // ================================================================
    // MIGRATION IDEMPOTENCY — running migrations twice must not fail
    // ================================================================

    #[test]
    fn migrations_idempotent() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        run_migrations(&mut conn).unwrap();
        run_migrations(&mut conn).expect("running migrations twice must not fail");
    }

    // ================================================================
    // DEFAULT VALUES — verify DB defaults match spec
    // ================================================================

    #[test]
    fn post_defaults() {
        let conn = setup();
        insert_project(&conn, "p1", "blog");
        conn.execute(
            "INSERT INTO posts (id, project_id, title, slug, created_at, updated_at)
             VALUES ('post1', 'p1', 'Test', 'test', 1000, 1000)",
            [],
        ).unwrap();

        let (status, file_path, tags, cats, dnt): (String, String, String, String, i64) = conn.query_row(
            "SELECT status, file_path, tags, categories, do_not_translate FROM posts WHERE id = 'post1'",
            [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        ).unwrap();
        assert_eq!(status, "draft", "default status must be 'draft'");
        assert_eq!(file_path, "", "default file_path must be empty string");
        assert_eq!(tags, "[]", "default tags must be '[]'");
        assert_eq!(cats, "[]", "default categories must be '[]'");
        assert_eq!(dnt, 0, "default do_not_translate must be 0");
    }

    #[test]
    fn template_defaults() {
        let conn = setup();
        insert_project(&conn, "p1", "blog");
        conn.execute(
            "INSERT INTO templates (id, project_id, slug, title, file_path, created_at, updated_at)
             VALUES ('tpl1', 'p1', 'test', 'Test', 'templates/test.liquid', 1000, 1000)",
            [],
        ).unwrap();

        let (kind, enabled, version, status): (String, i64, i64, String) = conn.query_row(
            "SELECT kind, enabled, version, status FROM templates WHERE id = 'tpl1'",
            [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        ).unwrap();
        assert_eq!(kind, "post", "default kind must be 'post'");
        assert_eq!(enabled, 1, "default enabled must be 1");
        assert_eq!(version, 1, "default version must be 1");
        assert_eq!(status, "published", "default status must be 'published'");
    }

    #[test]
    fn script_defaults() {
        let conn = setup();
        insert_project(&conn, "p1", "blog");
        conn.execute(
            "INSERT INTO scripts (id, project_id, slug, title, file_path, created_at, updated_at)
             VALUES ('s1', 'p1', 'test', 'Test', 'scripts/test.lua', 1000, 1000)",
            [],
        ).unwrap();

        let (kind, ep, enabled, version, status): (String, String, i64, i64, String) = conn.query_row(
            "SELECT kind, entrypoint, enabled, version, status FROM scripts WHERE id = 's1'",
            [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        ).unwrap();
        assert_eq!(kind, "utility", "default kind must be 'utility'");
        assert_eq!(ep, "render", "default entrypoint must be 'render'");
        assert_eq!(enabled, 1, "default enabled must be 1");
        assert_eq!(version, 1, "default version must be 1");
        assert_eq!(status, "published", "default status must be 'published'");
    }
}
