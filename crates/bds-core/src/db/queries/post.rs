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
            created_at = ?19, updated_at = ?20, published_at = ?21
         WHERE id = ?22",
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
            post.created_at,
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

// ── Filtered queries (per sidebar_views.allium PostsView) ───

/// Parameters for filtered post listing.
/// Drafts always shown regardless of filters per spec.
/// Published/archived respect all active filters.
#[derive(Debug, Clone, Default)]
pub struct PostFilterParams {
    /// FTS search query (empty = no search filter).
    pub search_query: String,
    /// Exact status filter.
    pub status: Option<String>,
    /// Exact language filter.
    pub language: Option<String>,
    /// Year filter from calendar archive.
    pub year: Option<i32>,
    /// Month filter (1-12) from calendar archive.
    pub month: Option<u32>,
    /// Inclusive start timestamp filter.
    pub from: Option<i64>,
    /// Inclusive end timestamp filter.
    pub to: Option<i64>,
    /// Tag filter (post must have ALL of these tags).
    pub tags: Vec<String>,
    /// Category filter (post must have at least one of these categories).
    pub categories: Vec<String>,
    /// If true, excludes posts that have category "page" (case-insensitive).
    /// Used by PostsView to hide pages from the posts list.
    pub exclude_pages: bool,
    /// If true, only includes posts that have category "page" (case-insensitive).
    /// Used by PagesView.
    pub pages_only: bool,
}

impl PostFilterParams {
    pub fn has_active_filters(&self) -> bool {
        !self.search_query.is_empty()
            || self.status.is_some()
            || self.language.is_some()
            || self.year.is_some()
            || self.from.is_some()
            || self.to.is_some()
            || !self.tags.is_empty()
            || !self.categories.is_empty()
    }
}

/// List posts with optional filters applied.
/// Per sidebar_views.allium: drafts always show regardless of filters.
/// Published/archived sections respect active filters.
///
/// Returns all matching posts (up to `limit`), ordered by created_at DESC.
/// Caller splits into draft/published/archived sections.
pub fn list_posts_filtered(
    conn: &Connection,
    project_id: &str,
    filters: &PostFilterParams,
    limit: i64,
    offset: i64,
) -> rusqlite::Result<Vec<Post>> {
    // Build dynamic WHERE clause
    let mut conditions = vec!["p.project_id = ?1".to_string()];
    let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    param_values.push(Box::new(project_id.to_string()));

    // pages_only / exclude_pages
    if filters.pages_only {
        conditions.push("LOWER(p.categories) LIKE '%\"page\"%'".to_string());
    } else if filters.exclude_pages {
        conditions.push("LOWER(p.categories) NOT LIKE '%\"page\"%'".to_string());
    }

    // For non-draft posts, apply filters. Drafts always pass.
    // We build this as: (status = 'draft') OR (filter conditions)
    let mut filter_conditions: Vec<String> = Vec::new();

    if !filters.search_query.is_empty() {
        let idx = param_values.len() + 1;
        let pattern = format!("%{}%", filters.search_query.replace('%', "\\%"));
        filter_conditions.push(format!("(p.title LIKE ?{idx} ESCAPE '\\')"));
        param_values.push(Box::new(pattern));
    }

    if let Some(status) = &filters.status {
        let idx = param_values.len() + 1;
        filter_conditions.push(format!("(p.status = ?{idx})"));
        param_values.push(Box::new(status.clone()));
    }

    if let Some(language) = &filters.language {
        let idx = param_values.len() + 1;
        filter_conditions.push(format!("(p.language = ?{idx} OR p.language IS NULL)"));
        param_values.push(Box::new(language.clone()));
    }

    if let Some(year) = filters.year {
        // created_at is unix ms; compute year range
        let start = chrono::NaiveDate::from_ymd_opt(year, 1, 1)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc()
            .timestamp() * 1000;
        let end = chrono::NaiveDate::from_ymd_opt(year + 1, 1, 1)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc()
            .timestamp() * 1000;

        if let Some(month) = filters.month {
            let m_start = chrono::NaiveDate::from_ymd_opt(year, month, 1)
                .unwrap()
                .and_hms_opt(0, 0, 0)
                .unwrap()
                .and_utc()
                .timestamp() * 1000;
            let next_month = if month == 12 {
                chrono::NaiveDate::from_ymd_opt(year + 1, 1, 1)
            } else {
                chrono::NaiveDate::from_ymd_opt(year, month + 1, 1)
            }
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc()
            .timestamp() * 1000;

            let idx1 = param_values.len() + 1;
            let idx2 = param_values.len() + 2;
            filter_conditions.push(format!("(p.created_at >= ?{idx1} AND p.created_at < ?{idx2})"));
            param_values.push(Box::new(m_start));
            param_values.push(Box::new(next_month));
        } else {
            let idx1 = param_values.len() + 1;
            let idx2 = param_values.len() + 2;
            filter_conditions.push(format!("(p.created_at >= ?{idx1} AND p.created_at < ?{idx2})"));
            param_values.push(Box::new(start));
            param_values.push(Box::new(end));
        }
    }

    for tag in &filters.tags {
        let idx = param_values.len() + 1;
        let pattern = format!("%\"{}\"%", tag.replace('"', "\\\""));
        filter_conditions.push(format!("(p.tags LIKE ?{idx})"));
        param_values.push(Box::new(pattern));
    }

    for cat in &filters.categories {
        let idx = param_values.len() + 1;
        let pattern = format!("%\"{}\"%", cat.replace('"', "\\\""));
        filter_conditions.push(format!("(p.categories LIKE ?{idx})"));
        param_values.push(Box::new(pattern));
    }

    if let Some(from) = filters.from {
        let idx = param_values.len() + 1;
        filter_conditions.push(format!("(p.created_at >= ?{idx})"));
        param_values.push(Box::new(from));
    }

    if let Some(to) = filters.to {
        let idx = param_values.len() + 1;
        filter_conditions.push(format!("(p.created_at <= ?{idx})"));
        param_values.push(Box::new(to));
    }

    // Without an explicit status filter, drafts remain visible regardless of the
    // rest of the active filters.
    if !filter_conditions.is_empty() {
        let combined = filter_conditions.join(" AND ");
        if filters.status.is_some() {
            conditions.push(format!("({combined})"));
        } else {
            conditions.push(format!("(p.status = 'draft' OR ({combined}))"));
        }
    }

    let where_clause = conditions.join(" AND ");
    let idx_limit = param_values.len() + 1;
    let idx_offset = param_values.len() + 2;
    param_values.push(Box::new(limit));
    param_values.push(Box::new(offset));

    let sql = format!(
        "SELECT {POST_COLUMNS} FROM posts p WHERE {where_clause} ORDER BY p.created_at DESC LIMIT ?{idx_limit} OFFSET ?{idx_offset}"
    );

    let params_refs: Vec<&dyn rusqlite::types::ToSql> =
        param_values.iter().map(|b| b.as_ref()).collect();

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params_refs.as_slice(), post_from_row)?;
    rows.collect()
}

/// Year/month counts for the calendar archive widget.
/// Returns (year, month, count) tuples, ordered by year DESC, month DESC.
pub fn post_calendar_counts(
    conn: &Connection,
    project_id: &str,
    pages_only: bool,
    exclude_pages: bool,
) -> rusqlite::Result<Vec<(i32, u32, usize)>> {
    let page_filter = if pages_only {
        " AND LOWER(categories) LIKE '%\"page\"%'"
    } else if exclude_pages {
        " AND LOWER(categories) NOT LIKE '%\"page\"%'"
    } else {
        ""
    };

    let sql = format!(
        "SELECT
            CAST(strftime('%Y', created_at / 1000, 'unixepoch') AS INTEGER) AS y,
            CAST(strftime('%m', created_at / 1000, 'unixepoch') AS INTEGER) AS m,
            COUNT(*) AS cnt
         FROM posts
         WHERE project_id = ?1{page_filter}
         GROUP BY y, m
         ORDER BY y DESC, m DESC"
    );

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params![project_id], |row| {
        Ok((
            row.get::<_, i32>(0)?,
            row.get::<_, u32>(1)?,
            row.get::<_, usize>(2)?,
        ))
    })?;
    rows.collect()
}

/// Collect all distinct tag values across posts for a project.
pub fn distinct_post_tags(
    conn: &Connection,
    project_id: &str,
) -> rusqlite::Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT tags FROM posts WHERE project_id = ?1 AND tags != '[]'"
    )?;
    let rows = stmt.query_map(params![project_id], |row| {
        row.get::<_, String>(0)
    })?;
    let mut all_tags = std::collections::BTreeSet::new();
    for json_str in rows {
        if let Ok(json_str) = json_str {
            if let Ok(tags) = serde_json::from_str::<Vec<String>>(&json_str) {
                all_tags.extend(tags);
            }
        }
    }
    Ok(all_tags.into_iter().collect())
}

/// Collect all distinct category values across posts for a project.
pub fn distinct_post_categories(
    conn: &Connection,
    project_id: &str,
) -> rusqlite::Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT categories FROM posts WHERE project_id = ?1 AND categories != '[]'"
    )?;
    let rows = stmt.query_map(params![project_id], |row| {
        row.get::<_, String>(0)
    })?;
    let mut all_cats = std::collections::BTreeSet::new();
    for json_str in rows {
        if let Ok(json_str) = json_str {
            if let Ok(cats) = serde_json::from_str::<Vec<String>>(&json_str) {
                all_cats.extend(cats);
            }
        }
    }
    Ok(all_cats.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::queries::project::{insert_project, make_test_project};
    use crate::db::Database;

    fn setup() -> Database {
        let mut db = Database::open_in_memory().unwrap();
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
