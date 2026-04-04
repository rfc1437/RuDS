use rusqlite::{params, Connection};

use crate::db::from_row::{post_from_row, post_status_to_str, POST_COLUMNS};
use crate::model::{Post, PostStatus};

fn tags_to_json(tags: &[String]) -> String {
    serde_json::to_string(tags).unwrap_or_else(|_| "[]".into())
}

pub fn insert_post(conn: &Connection, post: &Post) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO posts (
            id, project_id, title, slug, excerpt, content, status, author,
            language, do_not_translate, template_slug, file_path, checksum,
            tags, categories,
            published_title, published_content, published_tags,
            published_categories, published_excerpt,
            created_at, updated_at, published_at
         ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8,
            ?9, ?10, ?11, ?12, ?13,
            ?14, ?15,
            ?16, ?17, ?18, ?19, ?20,
            ?21, ?22, ?23
         )",
        params![
            post.id,
            post.project_id,
            post.title,
            post.slug,
            post.excerpt,
            post.content,
            post_status_to_str(&post.status),
            post.author,
            post.language,
            post.do_not_translate as i64,
            post.template_slug,
            post.file_path,
            post.checksum,
            tags_to_json(&post.tags),
            tags_to_json(&post.categories),
            post.published_title,
            post.published_content,
            post.published_tags,
            post.published_categories,
            post.published_excerpt,
            post.created_at,
            post.updated_at,
            post.published_at,
        ],
    )?;
    Ok(())
}

pub fn get_post_by_id(conn: &Connection, id: &str) -> rusqlite::Result<Post> {
    conn.query_row(
        &format!("SELECT {POST_COLUMNS} FROM posts WHERE id = ?1"),
        params![id],
        post_from_row,
    )
}

pub fn get_post_by_project_and_slug(
    conn: &Connection,
    project_id: &str,
    slug: &str,
) -> rusqlite::Result<Post> {
    conn.query_row(
        &format!("SELECT {POST_COLUMNS} FROM posts WHERE project_id = ?1 AND slug = ?2"),
        params![project_id, slug],
        post_from_row,
    )
}

pub fn list_posts_by_project(conn: &Connection, project_id: &str) -> rusqlite::Result<Vec<Post>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {POST_COLUMNS} FROM posts WHERE project_id = ?1 ORDER BY created_at DESC"
    ))?;
    let rows = stmt.query_map(params![project_id], post_from_row)?;
    rows.collect()
}

pub fn list_posts_by_project_limited(
    conn: &Connection,
    project_id: &str,
    limit: i64,
    offset: i64,
) -> rusqlite::Result<Vec<Post>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {POST_COLUMNS} FROM posts WHERE project_id = ?1 ORDER BY created_at DESC LIMIT ?2 OFFSET ?3"
    ))?;
    let rows = stmt.query_map(params![project_id, limit, offset], post_from_row)?;
    rows.collect()
}

pub fn update_post(conn: &Connection, post: &Post) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE posts SET
            title = ?1, slug = ?2, excerpt = ?3, content = ?4, status = ?5,
            author = ?6, language = ?7, do_not_translate = ?8, template_slug = ?9,
            file_path = ?10, checksum = ?11, tags = ?12, categories = ?13,
            published_title = ?14, published_content = ?15, published_tags = ?16,
            published_categories = ?17, published_excerpt = ?18,
            updated_at = ?19, published_at = ?20
         WHERE id = ?21",
        params![
            post.title,
            post.slug,
            post.excerpt,
            post.content,
            post_status_to_str(&post.status),
            post.author,
            post.language,
            post.do_not_translate as i64,
            post.template_slug,
            post.file_path,
            post.checksum,
            tags_to_json(&post.tags),
            tags_to_json(&post.categories),
            post.published_title,
            post.published_content,
            post.published_tags,
            post.published_categories,
            post.published_excerpt,
            post.updated_at,
            post.published_at,
            post.id,
        ],
    )?;
    Ok(())
}

pub fn update_post_status(
    conn: &Connection,
    id: &str,
    status: &PostStatus,
    updated_at: i64,
) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE posts SET status = ?1, updated_at = ?2 WHERE id = ?3",
        params![post_status_to_str(status), updated_at, id],
    )?;
    Ok(())
}

pub fn clear_post_content(conn: &Connection, id: &str, updated_at: i64) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE posts SET content = NULL, updated_at = ?1 WHERE id = ?2",
        params![updated_at, id],
    )?;
    Ok(())
}

pub fn set_post_file_path(
    conn: &Connection,
    id: &str,
    file_path: &str,
    updated_at: i64,
) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE posts SET file_path = ?1, updated_at = ?2 WHERE id = ?3",
        params![file_path, updated_at, id],
    )?;
    Ok(())
}

pub fn set_published_snapshot(
    conn: &Connection,
    id: &str,
    title: &str,
    content: &str,
    tags: &str,
    categories: &str,
    excerpt: Option<&str>,
    published_at: i64,
    updated_at: i64,
) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE posts SET
            published_title = ?1, published_content = ?2, published_tags = ?3,
            published_categories = ?4, published_excerpt = ?5,
            published_at = ?6, updated_at = ?7
         WHERE id = ?8",
        params![title, content, tags, categories, excerpt, published_at, updated_at, id],
    )?;
    Ok(())
}

pub fn delete_post(conn: &Connection, id: &str) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM posts WHERE id = ?1", params![id])?;
    Ok(())
}

pub fn count_posts_by_project(conn: &Connection, project_id: &str) -> rusqlite::Result<i64> {
    conn.query_row(
        "SELECT COUNT(*) FROM posts WHERE project_id = ?1",
        params![project_id],
        |row| row.get(0),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::queries::project::{insert_project, make_test_project};
    use crate::db::Database;

    fn setup() -> Database {
        let db = Database::open_in_memory().unwrap();
        db.migrate().unwrap();
        insert_project(db.conn(), &make_test_project("p1", "blog")).unwrap();
        db
    }

    fn make_post(id: &str, slug: &str) -> Post {
        Post {
            id: id.to_string(),
            project_id: "p1".to_string(),
            title: format!("Post {id}"),
            slug: slug.to_string(),
            excerpt: Some("excerpt".into()),
            content: Some("body".into()),
            status: PostStatus::Draft,
            author: Some("Alice".into()),
            language: Some("en".into()),
            do_not_translate: false,
            template_slug: None,
            file_path: format!("posts/{slug}.md"),
            checksum: None,
            tags: vec!["rust".into()],
            categories: vec!["tech".into()],
            published_title: None,
            published_content: None,
            published_tags: None,
            published_categories: None,
            published_excerpt: None,
            created_at: 1000,
            updated_at: 2000,
            published_at: None,
        }
    }

    #[test]
    fn insert_and_get_by_id() {
        let db = setup();
        let post = make_post("x1", "hello");
        insert_post(db.conn(), &post).unwrap();
        let fetched = get_post_by_id(db.conn(), "x1").unwrap();
        assert_eq!(fetched.title, "Post x1");
        assert_eq!(fetched.tags, vec!["rust"]);
        assert_eq!(fetched.categories, vec!["tech"]);
        assert_eq!(fetched.status, PostStatus::Draft);
        assert!(!fetched.do_not_translate);
    }

    #[test]
    fn get_by_project_and_slug() {
        let db = setup();
        insert_post(db.conn(), &make_post("x1", "hello")).unwrap();
        let fetched = get_post_by_project_and_slug(db.conn(), "p1", "hello").unwrap();
        assert_eq!(fetched.id, "x1");
    }

    #[test]
    fn list_posts_ordered_by_created_at_desc() {
        let db = setup();
        let mut p1 = make_post("x1", "first");
        p1.created_at = 1000;
        let mut p2 = make_post("x2", "second");
        p2.created_at = 2000;
        insert_post(db.conn(), &p1).unwrap();
        insert_post(db.conn(), &p2).unwrap();
        let list = list_posts_by_project(db.conn(), "p1").unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].id, "x2");
        assert_eq!(list[1].id, "x1");
    }

    #[test]
    fn update_post_fields() {
        let db = setup();
        let mut post = make_post("x1", "hello");
        insert_post(db.conn(), &post).unwrap();
        post.title = "Updated".into();
        post.tags = vec!["new-tag".into()];
        post.updated_at = 9999;
        update_post(db.conn(), &post).unwrap();
        let fetched = get_post_by_id(db.conn(), "x1").unwrap();
        assert_eq!(fetched.title, "Updated");
        assert_eq!(fetched.tags, vec!["new-tag"]);
        assert_eq!(fetched.updated_at, 9999);
    }

    #[test]
    fn update_status() {
        let db = setup();
        insert_post(db.conn(), &make_post("x1", "hello")).unwrap();
        update_post_status(db.conn(), "x1", &PostStatus::Published, 5000).unwrap();
        let fetched = get_post_by_id(db.conn(), "x1").unwrap();
        assert_eq!(fetched.status, PostStatus::Published);
        assert_eq!(fetched.updated_at, 5000);
    }

    #[test]
    fn clear_content_sets_null() {
        let db = setup();
        insert_post(db.conn(), &make_post("x1", "hello")).unwrap();
        clear_post_content(db.conn(), "x1", 5000).unwrap();
        let fetched = get_post_by_id(db.conn(), "x1").unwrap();
        assert!(fetched.content.is_none());
    }

    #[test]
    fn set_file_path_updates() {
        let db = setup();
        insert_post(db.conn(), &make_post("x1", "hello")).unwrap();
        set_post_file_path(db.conn(), "x1", "posts/2024/01/hello.md", 5000).unwrap();
        let fetched = get_post_by_id(db.conn(), "x1").unwrap();
        assert_eq!(fetched.file_path, "posts/2024/01/hello.md");
    }

    #[test]
    fn published_snapshot() {
        let db = setup();
        insert_post(db.conn(), &make_post("x1", "hello")).unwrap();
        set_published_snapshot(
            db.conn(), "x1", "Pub Title", "Pub Body",
            "[\"rust\"]", "[\"tech\"]", Some("Pub Excerpt"), 3000, 3000,
        ).unwrap();
        let fetched = get_post_by_id(db.conn(), "x1").unwrap();
        assert_eq!(fetched.published_title.as_deref(), Some("Pub Title"));
        assert_eq!(fetched.published_content.as_deref(), Some("Pub Body"));
        assert_eq!(fetched.published_at, Some(3000));
    }

    #[test]
    fn delete_removes_post() {
        let db = setup();
        insert_post(db.conn(), &make_post("x1", "hello")).unwrap();
        delete_post(db.conn(), "x1").unwrap();
        assert!(get_post_by_id(db.conn(), "x1").is_err());
    }

    #[test]
    fn count_posts() {
        let db = setup();
        assert_eq!(count_posts_by_project(db.conn(), "p1").unwrap(), 0);
        insert_post(db.conn(), &make_post("x1", "a")).unwrap();
        insert_post(db.conn(), &make_post("x2", "b")).unwrap();
        assert_eq!(count_posts_by_project(db.conn(), "p1").unwrap(), 2);
    }
}
