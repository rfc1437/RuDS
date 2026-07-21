use chrono::{Datelike, TimeZone, Utc};
use diesel::prelude::*;
use diesel::sql_types::Text;
use diesel::sqlite::Sqlite;

use crate::db::DbConnection;
use crate::db::schema::posts;
use crate::model::{Post, PostStatus};
use crate::util::calendar_range_unix_ms;

diesel::define_sql_function!(fn instr(haystack: Text, needle: Text) -> Integer);
diesel::define_sql_function!(fn lower(value: Text) -> Text);

pub fn insert_post(conn: &DbConnection, post: &Post) -> QueryResult<()> {
    conn.with(|c| {
        diesel::insert_into(posts::table)
            .values(post.clone())
            .execute(c)
            .map(|_| ())
    })
}

pub fn get_post_by_id(conn: &DbConnection, id: &str) -> QueryResult<Post> {
    conn.with(|c| {
        posts::table
            .filter(posts::id.eq(id))
            .select(Post::as_select())
            .first(c)
    })
}

pub fn get_post_by_project_and_slug(
    conn: &DbConnection,
    project_id: &str,
    slug: &str,
) -> QueryResult<Post> {
    conn.with(|c| {
        posts::table
            .filter(posts::project_id.eq(project_id))
            .filter(posts::slug.eq(slug))
            .select(Post::as_select())
            .first(c)
    })
}

pub fn list_posts_by_project(conn: &DbConnection, project_id: &str) -> QueryResult<Vec<Post>> {
    conn.with(|c| {
        posts::table
            .filter(posts::project_id.eq(project_id))
            .order(posts::created_at.desc())
            .select(Post::as_select())
            .load(c)
    })
}

pub fn update_post(conn: &DbConnection, post: &Post) -> QueryResult<()> {
    conn.with(|c| {
        diesel::update(posts::table.filter(posts::id.eq(&post.id)))
            .set(post.clone())
            .execute(c)
            .map(|_| ())
    })
}

pub fn update_post_status(
    conn: &DbConnection,
    id: &str,
    status: &PostStatus,
    updated_at: i64,
) -> QueryResult<()> {
    conn.with(|c| {
        diesel::update(posts::table.filter(posts::id.eq(id)))
            .set((
                posts::status.eq(status.as_str()),
                posts::updated_at.eq(updated_at),
            ))
            .execute(c)
            .map(|_| ())
    })
}

pub fn delete_post(conn: &DbConnection, id: &str) -> QueryResult<()> {
    conn.with(|c| {
        diesel::delete(posts::table.filter(posts::id.eq(id)))
            .execute(c)
            .map(|_| ())
    })
}

pub fn count_posts_by_project(conn: &DbConnection, project_id: &str) -> QueryResult<i64> {
    conn.with(|c| {
        posts::table
            .filter(posts::project_id.eq(project_id))
            .count()
            .get_result(c)
    })
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
fn post_query<'a>(
    project_id: &'a str,
    filters: &'a PostFilterParams,
    apply_filters: bool,
    exclude_drafts: bool,
) -> posts::BoxedQuery<'a, Sqlite> {
    let mut query = posts::table
        .filter(posts::project_id.eq(project_id))
        .into_boxed();

    let page = serde_json::to_string("page").unwrap();
    if filters.pages_only {
        query = query.filter(instr(lower(posts::categories), page.clone()).gt(0));
    } else if filters.exclude_pages {
        query = query.filter(instr(lower(posts::categories), page).eq(0));
    }

    if exclude_drafts {
        query = query.filter(posts::status.ne("draft"));
    }
    if !apply_filters {
        return query;
    }

    if !filters.search_query.is_empty() {
        query = query.filter(instr(posts::title, &filters.search_query).gt(0));
    }
    if let Some(status) = &filters.status {
        query = query.filter(posts::status.eq(status));
    }
    if let Some(language) = &filters.language {
        query = query.filter(posts::language.eq(language).or(posts::language.is_null()));
    }
    if let Some(year) = filters.year
        && let Some((start, end)) = calendar_range_unix_ms(year, filters.month)
    {
        query = query.filter(posts::created_at.ge(start).and(posts::created_at.lt(end)));
    }
    for tag in &filters.tags {
        query = query.filter(instr(posts::tags, serde_json::to_string(tag).unwrap()).gt(0));
    }
    for category in &filters.categories {
        query =
            query.filter(instr(posts::categories, serde_json::to_string(category).unwrap()).gt(0));
    }
    if let Some(from) = filters.from {
        query = query.filter(posts::created_at.ge(from));
    }
    if let Some(to) = filters.to {
        query = query.filter(posts::created_at.le(to));
    }
    query
}

pub fn list_posts_filtered(
    conn: &DbConnection,
    project_id: &str,
    filters: &PostFilterParams,
    limit: i64,
    offset: i64,
) -> QueryResult<Vec<Post>> {
    conn.with(|c| {
        let content_filters = filters.has_active_filters();
        let records = if filters.status.is_some() {
            post_query(project_id, filters, true, false)
                .order(posts::created_at.desc())
                .limit(limit)
                .offset(offset)
                .select(Post::as_select())
                .load(c)?
        } else if content_filters {
            let mut records: Vec<Post> = post_query(project_id, filters, false, false)
                .filter(posts::status.eq("draft"))
                .select(Post::as_select())
                .load(c)?;
            records.extend(
                post_query(project_id, filters, true, true)
                    .select(Post::as_select())
                    .load::<Post>(c)?,
            );
            records.sort_unstable_by_key(|record| std::cmp::Reverse(record.created_at));
            records
                .into_iter()
                .skip(offset.max(0) as usize)
                .take(limit.max(0) as usize)
                .collect()
        } else {
            post_query(project_id, filters, false, false)
                .order(posts::created_at.desc())
                .limit(limit)
                .offset(offset)
                .select(Post::as_select())
                .load(c)?
        };
        Ok(records)
    })
}

/// Year/month counts for the calendar archive widget.
/// Returns (year, month, count) tuples, ordered by year DESC, month DESC.
pub fn post_calendar_counts(
    conn: &DbConnection,
    project_id: &str,
    pages_only: bool,
    exclude_pages: bool,
) -> QueryResult<Vec<(i32, u32, usize)>> {
    conn.with(|c| {
        let rows = posts::table
            .filter(posts::project_id.eq(project_id))
            .select((posts::created_at, posts::categories))
            .load::<(i64, String)>(c)?;
        let mut counts = std::collections::BTreeMap::new();
        for (timestamp, categories) in rows {
            let categories: Vec<String> = serde_json::from_str(&categories).unwrap_or_default();
            let is_page = categories
                .iter()
                .any(|category| category.eq_ignore_ascii_case("page"));
            if (pages_only && !is_page) || (exclude_pages && is_page) {
                continue;
            }
            let date = Utc
                .timestamp_millis_opt(timestamp)
                .single()
                .ok_or_else(|| {
                    diesel::result::Error::DeserializationError("invalid timestamp".into())
                })?;
            *counts.entry((date.year(), date.month())).or_insert(0) += 1;
        }
        Ok(counts
            .into_iter()
            .rev()
            .map(|((year, month), count)| (year, month, count))
            .collect())
    })
}

/// Collect all distinct tag values across posts for a project.
pub fn distinct_post_tags(conn: &DbConnection, project_id: &str) -> QueryResult<Vec<String>> {
    let rows = conn.with(|c| {
        posts::table
            .filter(posts::project_id.eq(project_id))
            .filter(posts::tags.ne("[]"))
            .select(posts::tags)
            .distinct()
            .load::<String>(c)
    })?;
    let mut all_tags = std::collections::BTreeSet::new();
    for json_str in rows {
        if let Ok(tags) = serde_json::from_str::<Vec<String>>(&json_str) {
            all_tags.extend(tags);
        }
    }
    Ok(all_tags.into_iter().collect())
}

/// Collect all distinct category values across posts for a project.
pub fn distinct_post_categories(conn: &DbConnection, project_id: &str) -> QueryResult<Vec<String>> {
    let rows = conn.with(|c| {
        posts::table
            .filter(posts::project_id.eq(project_id))
            .filter(posts::categories.ne("[]"))
            .select(posts::categories)
            .distinct()
            .load::<String>(c)
    })?;
    let mut all_cats = std::collections::BTreeSet::new();
    for json_str in rows {
        if let Ok(cats) = serde_json::from_str::<Vec<String>>(&json_str) {
            all_cats.extend(cats);
        }
    }
    Ok(all_cats.into_iter().collect())
}

#[cfg(test)]
pub fn make_test_post(id: &str, project_id: &str, slug: &str) -> Post {
    Post {
        id: id.into(),
        project_id: project_id.into(),
        title: id.into(),
        slug: slug.into(),
        excerpt: None,
        content: None,
        status: PostStatus::Draft,
        author: None,
        language: None,
        do_not_translate: false,
        template_slug: None,
        file_path: String::new(),
        checksum: None,
        tags: Vec::new(),
        categories: Vec::new(),
        published_title: None,
        published_content: None,
        published_tags: None,
        published_categories: None,
        published_excerpt: None,
        created_at: 1000,
        updated_at: 1000,
        published_at: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::db::queries::project::{insert_project, make_test_project};

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
    fn malformed_persisted_types_fail_deserialization() {
        let db = setup();
        insert_post(db.conn(), &make_post("x1", "hello")).unwrap();
        db.conn()
            .with(|connection| {
                diesel::update(posts::table.filter(posts::id.eq("x1")))
                    .set(posts::status.eq("unknown"))
                    .execute(connection)
            })
            .unwrap();
        assert!(get_post_by_id(db.conn(), "x1").is_err());

        db.conn()
            .with(|connection| {
                diesel::update(posts::table.filter(posts::id.eq("x1")))
                    .set((posts::status.eq("draft"), posts::tags.eq("not-json")))
                    .execute(connection)
            })
            .unwrap();
        assert!(get_post_by_id(db.conn(), "x1").is_err());
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
