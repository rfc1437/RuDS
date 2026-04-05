use rusqlite::Row;

use crate::model::{
    DbNotification, GeneratedFileHash, Media, MediaTranslation, NotificationAction,
    NotificationEntity, Post, PostLink, PostMedia, PostStatus, PostTranslation, Project, Script,
    ScriptKind, ScriptStatus, Setting, Tag, Template, TemplateKind, TemplateStatus,
};

// ── helpers ──────────────────────────────────────────────────────────

fn conversion_err(msg: String) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        0,
        rusqlite::types::Type::Text,
        Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, msg)),
    )
}

fn parse_post_status(s: &str) -> rusqlite::Result<PostStatus> {
    match s {
        "draft" => Ok(PostStatus::Draft),
        "published" => Ok(PostStatus::Published),
        "archived" => Ok(PostStatus::Archived),
        _ => Err(conversion_err(format!("invalid PostStatus: {s}"))),
    }
}

fn parse_template_kind(s: &str) -> rusqlite::Result<TemplateKind> {
    match s {
        "post" => Ok(TemplateKind::Post),
        "list" => Ok(TemplateKind::List),
        "not_found" => Ok(TemplateKind::NotFound),
        "partial" => Ok(TemplateKind::Partial),
        _ => Err(conversion_err(format!("invalid TemplateKind: {s}"))),
    }
}

fn parse_template_status(s: &str) -> rusqlite::Result<TemplateStatus> {
    match s {
        "draft" => Ok(TemplateStatus::Draft),
        "published" => Ok(TemplateStatus::Published),
        _ => Err(conversion_err(format!("invalid TemplateStatus: {s}"))),
    }
}

fn parse_script_kind(s: &str) -> rusqlite::Result<ScriptKind> {
    match s {
        "macro" => Ok(ScriptKind::Macro),
        "utility" => Ok(ScriptKind::Utility),
        "transform" => Ok(ScriptKind::Transform),
        _ => Err(conversion_err(format!("invalid ScriptKind: {s}"))),
    }
}

fn parse_script_status(s: &str) -> rusqlite::Result<ScriptStatus> {
    match s {
        "draft" => Ok(ScriptStatus::Draft),
        "published" => Ok(ScriptStatus::Published),
        _ => Err(conversion_err(format!("invalid ScriptStatus: {s}"))),
    }
}

fn parse_notification_entity(s: &str) -> rusqlite::Result<NotificationEntity> {
    match s {
        "post" => Ok(NotificationEntity::Post),
        "media" => Ok(NotificationEntity::Media),
        "script" => Ok(NotificationEntity::Script),
        "template" => Ok(NotificationEntity::Template),
        _ => Err(conversion_err(format!("invalid NotificationEntity: {s}"))),
    }
}

fn parse_notification_action(s: &str) -> rusqlite::Result<NotificationAction> {
    match s {
        "created" => Ok(NotificationAction::Created),
        "updated" => Ok(NotificationAction::Updated),
        "deleted" => Ok(NotificationAction::Deleted),
        _ => Err(conversion_err(format!("invalid NotificationAction: {s}"))),
    }
}

fn json_to_vec_string(s: &str) -> rusqlite::Result<Vec<String>> {
    serde_json::from_str(s).map_err(|e| conversion_err(format!("JSON parse error: {e}")))
}

fn bool_from_i64(v: i64) -> bool {
    v != 0
}

// ── enum → DB string (for INSERT / UPDATE) ───────────────────────────

pub fn post_status_to_str(s: &PostStatus) -> &'static str {
    match s {
        PostStatus::Draft => "draft",
        PostStatus::Published => "published",
        PostStatus::Archived => "archived",
    }
}

pub fn template_kind_to_str(k: &TemplateKind) -> &'static str {
    match k {
        TemplateKind::Post => "post",
        TemplateKind::List => "list",
        TemplateKind::NotFound => "not_found",
        TemplateKind::Partial => "partial",
    }
}

pub fn template_status_to_str(s: &TemplateStatus) -> &'static str {
    match s {
        TemplateStatus::Draft => "draft",
        TemplateStatus::Published => "published",
    }
}

pub fn script_kind_to_str(k: &ScriptKind) -> &'static str {
    match k {
        ScriptKind::Macro => "macro",
        ScriptKind::Utility => "utility",
        ScriptKind::Transform => "transform",
    }
}

pub fn script_status_to_str(s: &ScriptStatus) -> &'static str {
    match s {
        ScriptStatus::Draft => "draft",
        ScriptStatus::Published => "published",
    }
}

pub fn notification_entity_to_str(e: &NotificationEntity) -> &'static str {
    match e {
        NotificationEntity::Post => "post",
        NotificationEntity::Media => "media",
        NotificationEntity::Script => "script",
        NotificationEntity::Template => "template",
    }
}

pub fn notification_action_to_str(a: &NotificationAction) -> &'static str {
    match a {
        NotificationAction::Created => "created",
        NotificationAction::Updated => "updated",
        NotificationAction::Deleted => "deleted",
    }
}

// ── column lists (keep in sync with from_row functions) ──────────────

pub const PROJECT_COLUMNS: &str =
    "id, name, slug, description, data_path, is_active, created_at, updated_at";

pub const POST_COLUMNS: &str = "\
    id, project_id, title, slug, excerpt, content, status, author, \
    language, do_not_translate, template_slug, file_path, checksum, \
    tags, categories, \
    published_title, published_content, published_tags, \
    published_categories, published_excerpt, \
    created_at, updated_at, published_at";

pub const POST_TRANSLATION_COLUMNS: &str = "\
    id, project_id, translation_for, language, title, excerpt, content, \
    status, file_path, checksum, created_at, updated_at, published_at";

pub const POST_LINK_COLUMNS: &str =
    "id, source_post_id, target_post_id, link_text, created_at";

pub const POST_MEDIA_COLUMNS: &str =
    "id, project_id, post_id, media_id, sort_order, created_at";

pub const MEDIA_COLUMNS: &str = "\
    id, project_id, filename, original_name, mime_type, size, \
    width, height, title, alt, caption, author, language, \
    file_path, sidecar_path, checksum, tags, created_at, updated_at";

pub const MEDIA_TRANSLATION_COLUMNS: &str =
    "id, project_id, translation_for, language, title, alt, caption, created_at, updated_at";

pub const TAG_COLUMNS: &str =
    "id, project_id, name, color, post_template_slug, created_at, updated_at";

pub const TEMPLATE_COLUMNS: &str = "\
    id, project_id, slug, title, kind, enabled, version, \
    file_path, status, content, created_at, updated_at";

pub const SCRIPT_COLUMNS: &str = "\
    id, project_id, slug, title, kind, entrypoint, enabled, version, \
    file_path, status, content, created_at, updated_at";

pub const SETTING_COLUMNS: &str = "key, value, updated_at";

pub const GENERATED_FILE_HASH_COLUMNS: &str =
    "project_id, relative_path, content_hash, updated_at";

pub const DB_NOTIFICATION_COLUMNS: &str =
    "id, entity_type, entity_id, action, from_cli, seen_at, created_at";

// ── from_row functions ───────────────────────────────────────────────

pub fn project_from_row(row: &Row) -> rusqlite::Result<Project> {
    Ok(Project {
        id: row.get(0)?,
        name: row.get(1)?,
        slug: row.get(2)?,
        description: row.get(3)?,
        data_path: row.get(4)?,
        is_active: bool_from_i64(row.get(5)?),
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
    })
}

pub fn post_from_row(row: &Row) -> rusqlite::Result<Post> {
    let status_str: String = row.get(6)?;
    let tags_json: String = row.get(13)?;
    let categories_json: String = row.get(14)?;
    Ok(Post {
        id: row.get(0)?,
        project_id: row.get(1)?,
        title: row.get(2)?,
        slug: row.get(3)?,
        excerpt: row.get(4)?,
        content: row.get(5)?,
        status: parse_post_status(&status_str)?,
        author: row.get(7)?,
        language: row.get(8)?,
        do_not_translate: bool_from_i64(row.get(9)?),
        template_slug: row.get(10)?,
        file_path: row.get(11)?,
        checksum: row.get(12)?,
        tags: json_to_vec_string(&tags_json)?,
        categories: json_to_vec_string(&categories_json)?,
        published_title: row.get(15)?,
        published_content: row.get(16)?,
        published_tags: row.get(17)?,
        published_categories: row.get(18)?,
        published_excerpt: row.get(19)?,
        created_at: row.get(20)?,
        updated_at: row.get(21)?,
        published_at: row.get(22)?,
    })
}

pub fn post_translation_from_row(row: &Row) -> rusqlite::Result<PostTranslation> {
    let status_str: String = row.get(7)?;
    Ok(PostTranslation {
        id: row.get(0)?,
        project_id: row.get(1)?,
        translation_for: row.get(2)?,
        language: row.get(3)?,
        title: row.get(4)?,
        excerpt: row.get(5)?,
        content: row.get(6)?,
        status: parse_post_status(&status_str)?,
        file_path: row.get(8)?,
        checksum: row.get(9)?,
        created_at: row.get(10)?,
        updated_at: row.get(11)?,
        published_at: row.get(12)?,
    })
}

pub fn post_link_from_row(row: &Row) -> rusqlite::Result<PostLink> {
    Ok(PostLink {
        id: row.get(0)?,
        source_post_id: row.get(1)?,
        target_post_id: row.get(2)?,
        link_text: row.get(3)?,
        created_at: row.get(4)?,
    })
}

pub fn post_media_from_row(row: &Row) -> rusqlite::Result<PostMedia> {
    Ok(PostMedia {
        id: row.get(0)?,
        project_id: row.get(1)?,
        post_id: row.get(2)?,
        media_id: row.get(3)?,
        sort_order: row.get(4)?,
        created_at: row.get(5)?,
    })
}

pub fn media_from_row(row: &Row) -> rusqlite::Result<Media> {
    let tags_json: String = row.get(16)?;
    Ok(Media {
        id: row.get(0)?,
        project_id: row.get(1)?,
        filename: row.get(2)?,
        original_name: row.get(3)?,
        mime_type: row.get(4)?,
        size: row.get(5)?,
        width: row.get(6)?,
        height: row.get(7)?,
        title: row.get(8)?,
        alt: row.get(9)?,
        caption: row.get(10)?,
        author: row.get(11)?,
        language: row.get(12)?,
        file_path: row.get(13)?,
        sidecar_path: row.get(14)?,
        checksum: row.get(15)?,
        tags: json_to_vec_string(&tags_json)?,
        created_at: row.get(17)?,
        updated_at: row.get(18)?,
    })
}

pub fn media_translation_from_row(row: &Row) -> rusqlite::Result<MediaTranslation> {
    Ok(MediaTranslation {
        id: row.get(0)?,
        project_id: row.get(1)?,
        translation_for: row.get(2)?,
        language: row.get(3)?,
        title: row.get(4)?,
        alt: row.get(5)?,
        caption: row.get(6)?,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
    })
}

pub fn tag_from_row(row: &Row) -> rusqlite::Result<Tag> {
    Ok(Tag {
        id: row.get(0)?,
        project_id: row.get(1)?,
        name: row.get(2)?,
        color: row.get(3)?,
        post_template_slug: row.get(4)?,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
    })
}

pub fn template_from_row(row: &Row) -> rusqlite::Result<Template> {
    let kind_str: String = row.get(4)?;
    let status_str: String = row.get(8)?;
    Ok(Template {
        id: row.get(0)?,
        project_id: row.get(1)?,
        slug: row.get(2)?,
        title: row.get(3)?,
        kind: parse_template_kind(&kind_str)?,
        enabled: bool_from_i64(row.get(5)?),
        version: row.get(6)?,
        file_path: row.get(7)?,
        status: parse_template_status(&status_str)?,
        content: row.get(9)?,
        created_at: row.get(10)?,
        updated_at: row.get(11)?,
    })
}

pub fn script_from_row(row: &Row) -> rusqlite::Result<Script> {
    let kind_str: String = row.get(4)?;
    let status_str: String = row.get(9)?;
    Ok(Script {
        id: row.get(0)?,
        project_id: row.get(1)?,
        slug: row.get(2)?,
        title: row.get(3)?,
        kind: parse_script_kind(&kind_str)?,
        entrypoint: row.get(5)?,
        enabled: bool_from_i64(row.get(6)?),
        version: row.get(7)?,
        file_path: row.get(8)?,
        status: parse_script_status(&status_str)?,
        content: row.get(10)?,
        created_at: row.get(11)?,
        updated_at: row.get(12)?,
    })
}

pub fn setting_from_row(row: &Row) -> rusqlite::Result<Setting> {
    Ok(Setting {
        key: row.get(0)?,
        value: row.get(1)?,
        updated_at: row.get(2)?,
    })
}

pub fn generated_file_hash_from_row(row: &Row) -> rusqlite::Result<GeneratedFileHash> {
    Ok(GeneratedFileHash {
        project_id: row.get(0)?,
        relative_path: row.get(1)?,
        content_hash: row.get(2)?,
        updated_at: row.get(3)?,
    })
}

pub fn db_notification_from_row(row: &Row) -> rusqlite::Result<DbNotification> {
    let entity_str: String = row.get(1)?;
    let action_str: String = row.get(3)?;
    Ok(DbNotification {
        id: row.get(0)?,
        entity_type: parse_notification_entity(&entity_str)?,
        entity_id: row.get(2)?,
        action: parse_notification_action(&action_str)?,
        from_cli: bool_from_i64(row.get(4)?),
        seen_at: row.get(5)?,
        created_at: row.get(6)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;

    fn setup() -> Database {
        let mut db = Database::open_in_memory().unwrap();
        db.migrate().unwrap();
        db
    }

    // ── enum parsing ─────────────────────────────────────────────────

    #[test]
    fn parse_post_status_valid() {
        assert_eq!(parse_post_status("draft").unwrap(), PostStatus::Draft);
        assert_eq!(parse_post_status("published").unwrap(), PostStatus::Published);
        assert_eq!(parse_post_status("archived").unwrap(), PostStatus::Archived);
    }

    #[test]
    fn parse_post_status_invalid() {
        assert!(parse_post_status("nope").is_err());
    }

    #[test]
    fn parse_template_kind_valid() {
        assert_eq!(parse_template_kind("post").unwrap(), TemplateKind::Post);
        assert_eq!(parse_template_kind("list").unwrap(), TemplateKind::List);
        assert_eq!(parse_template_kind("not_found").unwrap(), TemplateKind::NotFound);
        assert_eq!(parse_template_kind("partial").unwrap(), TemplateKind::Partial);
    }

    #[test]
    fn parse_template_kind_invalid() {
        assert!(parse_template_kind("unknown").is_err());
    }

    #[test]
    fn parse_template_status_valid() {
        assert_eq!(parse_template_status("draft").unwrap(), TemplateStatus::Draft);
        assert_eq!(parse_template_status("published").unwrap(), TemplateStatus::Published);
    }

    #[test]
    fn parse_script_kind_valid() {
        assert_eq!(parse_script_kind("macro").unwrap(), ScriptKind::Macro);
        assert_eq!(parse_script_kind("utility").unwrap(), ScriptKind::Utility);
        assert_eq!(parse_script_kind("transform").unwrap(), ScriptKind::Transform);
    }

    #[test]
    fn parse_script_status_valid() {
        assert_eq!(parse_script_status("draft").unwrap(), ScriptStatus::Draft);
        assert_eq!(parse_script_status("published").unwrap(), ScriptStatus::Published);
    }

    #[test]
    fn parse_notification_entity_valid() {
        assert_eq!(parse_notification_entity("post").unwrap(), NotificationEntity::Post);
        assert_eq!(parse_notification_entity("media").unwrap(), NotificationEntity::Media);
        assert_eq!(parse_notification_entity("script").unwrap(), NotificationEntity::Script);
        assert_eq!(parse_notification_entity("template").unwrap(), NotificationEntity::Template);
    }

    #[test]
    fn parse_notification_action_valid() {
        assert_eq!(parse_notification_action("created").unwrap(), NotificationAction::Created);
        assert_eq!(parse_notification_action("updated").unwrap(), NotificationAction::Updated);
        assert_eq!(parse_notification_action("deleted").unwrap(), NotificationAction::Deleted);
    }

    // ── JSON helpers ─────────────────────────────────────────────────

    #[test]
    fn json_vec_string_roundtrip() {
        let v = json_to_vec_string(r#"["a","b","c"]"#).unwrap();
        assert_eq!(v, vec!["a", "b", "c"]);
    }

    #[test]
    fn json_vec_string_empty() {
        let v = json_to_vec_string("[]").unwrap();
        assert!(v.is_empty());
    }

    #[test]
    fn json_vec_string_invalid() {
        assert!(json_to_vec_string("not json").is_err());
    }

    // ── from_row round-trips via real DB ─────────────────────────────

    #[test]
    fn project_from_row_roundtrip() {
        let db = setup();
        let c = db.conn();
        c.execute(
            "INSERT INTO projects (id, name, slug, description, data_path, is_active, created_at, updated_at)
             VALUES ('p1', 'Blog', 'blog', 'My blog', '/data', 1, 1000, 2000)",
            [],
        ).unwrap();
        let p = c.query_row(
            &format!("SELECT {PROJECT_COLUMNS} FROM projects WHERE id = 'p1'"),
            [],
            project_from_row,
        ).unwrap();
        assert_eq!(p.id, "p1");
        assert_eq!(p.name, "Blog");
        assert_eq!(p.slug, "blog");
        assert_eq!(p.description.as_deref(), Some("My blog"));
        assert_eq!(p.data_path.as_deref(), Some("/data"));
        assert!(p.is_active);
        assert_eq!(p.created_at, 1000);
        assert_eq!(p.updated_at, 2000);
    }

    #[test]
    fn post_from_row_roundtrip() {
        let db = setup();
        let c = db.conn();
        c.execute(
            "INSERT INTO projects (id, name, slug, is_active, created_at, updated_at)
             VALUES ('p1', 'B', 'b', 0, 1000, 1000)",
            [],
        ).unwrap();
        c.execute(
            "INSERT INTO posts (id, project_id, title, slug, excerpt, content, status, author,
             language, do_not_translate, template_slug, file_path, checksum,
             tags, categories,
             published_title, published_content, published_tags, published_categories, published_excerpt,
             created_at, updated_at, published_at)
             VALUES ('x', 'p1', 'Hello', 'hello', 'sum', 'body', 'draft', 'Alice',
             'en', 1, 'tpl', 'posts/hello.md', 'abc',
             '[\"rust\"]', '[\"tech\"]',
             NULL, NULL, NULL, NULL, NULL,
             1000, 2000, NULL)",
            [],
        ).unwrap();
        let p = c.query_row(
            &format!("SELECT {POST_COLUMNS} FROM posts WHERE id = 'x'"),
            [],
            post_from_row,
        ).unwrap();
        assert_eq!(p.id, "x");
        assert_eq!(p.project_id, "p1");
        assert_eq!(p.title, "Hello");
        assert_eq!(p.slug, "hello");
        assert_eq!(p.excerpt.as_deref(), Some("sum"));
        assert_eq!(p.content.as_deref(), Some("body"));
        assert_eq!(p.status, PostStatus::Draft);
        assert_eq!(p.author.as_deref(), Some("Alice"));
        assert_eq!(p.language.as_deref(), Some("en"));
        assert!(p.do_not_translate);
        assert_eq!(p.template_slug.as_deref(), Some("tpl"));
        assert_eq!(p.file_path, "posts/hello.md");
        assert_eq!(p.checksum.as_deref(), Some("abc"));
        assert_eq!(p.tags, vec!["rust"]);
        assert_eq!(p.categories, vec!["tech"]);
        assert!(p.published_title.is_none());
        assert_eq!(p.created_at, 1000);
        assert_eq!(p.updated_at, 2000);
        assert!(p.published_at.is_none());
    }

    #[test]
    fn template_from_row_roundtrip() {
        let db = setup();
        let c = db.conn();
        c.execute(
            "INSERT INTO projects (id, name, slug, is_active, created_at, updated_at)
             VALUES ('p1', 'B', 'b', 0, 1000, 1000)",
            [],
        ).unwrap();
        c.execute(
            "INSERT INTO templates (id, project_id, slug, title, kind, enabled, version,
             file_path, status, content, created_at, updated_at)
             VALUES ('t1', 'p1', 'default', 'Default', 'not_found', 0, 3,
             'templates/default.liquid', 'draft', 'html', 1000, 2000)",
            [],
        ).unwrap();
        let t = c.query_row(
            &format!("SELECT {TEMPLATE_COLUMNS} FROM templates WHERE id = 't1'"),
            [],
            template_from_row,
        ).unwrap();
        assert_eq!(t.kind, TemplateKind::NotFound);
        assert!(!t.enabled);
        assert_eq!(t.version, 3);
        assert_eq!(t.status, TemplateStatus::Draft);
        assert_eq!(t.content.as_deref(), Some("html"));
    }

    #[test]
    fn db_notification_from_row_roundtrip() {
        let db = setup();
        let c = db.conn();
        c.execute(
            "INSERT INTO db_notifications (entity_type, entity_id, action, from_cli, seen_at, created_at)
             VALUES ('media', 'm1', 'deleted', 1, 5000, 1000)",
            [],
        ).unwrap();
        let n = c.query_row(
            &format!("SELECT {DB_NOTIFICATION_COLUMNS} FROM db_notifications WHERE entity_id = 'm1'"),
            [],
            db_notification_from_row,
        ).unwrap();
        assert_eq!(n.entity_type, NotificationEntity::Media);
        assert_eq!(n.entity_id, "m1");
        assert_eq!(n.action, NotificationAction::Deleted);
        assert!(n.from_cli);
        assert_eq!(n.seen_at, Some(5000));
        assert_eq!(n.created_at, 1000);
    }
}
