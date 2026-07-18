use chrono::{Datelike, TimeZone, Utc};
use diesel::prelude::*;
use diesel::sql_types::Text;

use crate::db::DbConnection;
use crate::db::from_row::{MediaRecord, convert, convert_all};
use crate::db::schema::media;
use crate::model::Media;
use crate::util::calendar_range_unix_ms;

diesel::define_sql_function!(fn instr(haystack: Text, needle: Text) -> Integer);

pub fn insert_media(conn: &DbConnection, m: &Media) -> QueryResult<()> {
    conn.with(|c| {
        diesel::insert_into(media::table)
            .values(MediaRecord::from(m))
            .execute(c)
            .map(|_| ())
    })
}

pub fn get_media_by_id(conn: &DbConnection, id: &str) -> QueryResult<Media> {
    conn.with(|c| {
        media::table
            .filter(media::id.eq(id))
            .select(MediaRecord::as_select())
            .first(c)
            .and_then(convert)
    })
}

pub fn list_media_by_project(conn: &DbConnection, project_id: &str) -> QueryResult<Vec<Media>> {
    conn.with(|c| {
        media::table
            .filter(media::project_id.eq(project_id))
            .order(media::created_at.desc())
            .select(MediaRecord::as_select())
            .load(c)
            .and_then(convert_all)
    })
}

pub fn list_media_by_project_limited(
    conn: &DbConnection,
    project_id: &str,
    limit: i64,
    offset: i64,
) -> QueryResult<Vec<Media>> {
    conn.with(|c| {
        media::table
            .filter(media::project_id.eq(project_id))
            .order(media::created_at.desc())
            .limit(limit)
            .offset(offset)
            .select(MediaRecord::as_select())
            .load(c)
            .and_then(convert_all)
    })
}

pub fn count_media_by_project(conn: &DbConnection, project_id: &str) -> QueryResult<i64> {
    conn.with(|c| {
        media::table
            .filter(media::project_id.eq(project_id))
            .count()
            .get_result(c)
    })
}

pub fn update_media(conn: &DbConnection, m: &Media) -> QueryResult<()> {
    conn.with(|c| {
        diesel::update(media::table.filter(media::id.eq(&m.id)))
            .set(MediaRecord::from(m))
            .execute(c)
            .map(|_| ())
    })
}

pub fn delete_media(conn: &DbConnection, id: &str) -> QueryResult<()> {
    conn.with(|c| {
        diesel::delete(media::table.filter(media::id.eq(id)))
            .execute(c)
            .map(|_| ())
    })
}

// ── Filtered queries (per sidebar_views.allium MediaView) ───

/// Parameters for filtered media listing.
#[derive(Debug, Clone, Default)]
pub struct MediaFilterParams {
    /// FTS search query (empty = no search filter).
    pub search_query: String,
    /// Year filter from calendar archive.
    pub year: Option<i32>,
    /// Month filter (1-12) from calendar archive.
    pub month: Option<u32>,
    /// Tag filter (media must have ALL of these tags).
    pub tags: Vec<String>,
}

impl MediaFilterParams {
    pub fn has_active_filters(&self) -> bool {
        !self.search_query.is_empty() || self.year.is_some() || !self.tags.is_empty()
    }
}

/// List media with optional filters applied.
pub fn list_media_filtered(
    conn: &DbConnection,
    project_id: &str,
    filters: &MediaFilterParams,
    limit: i64,
    offset: i64,
) -> QueryResult<Vec<Media>> {
    conn.with(|c| {
        let mut query = media::table
            .filter(media::project_id.eq(project_id))
            .into_boxed();
        if !filters.search_query.is_empty() {
            query = query.filter(
                media::title
                    .is_not_null()
                    .and(instr(media::title.assume_not_null(), &filters.search_query).gt(0))
                    .or(media::title
                        .is_null()
                        .and(instr(media::original_name, &filters.search_query).gt(0))),
            );
        }
        if let Some(year) = filters.year {
            let (start, end) = calendar_range_unix_ms(year, filters.month).ok_or_else(|| {
                diesel::result::Error::SerializationError("invalid calendar range".into())
            })?;
            query = query.filter(media::created_at.ge(start).and(media::created_at.lt(end)));
        }
        for tag in &filters.tags {
            query = query.filter(instr(media::tags, serde_json::to_string(tag).unwrap()).gt(0));
        }
        query
            .order(media::created_at.desc())
            .limit(limit)
            .offset(offset)
            .select(MediaRecord::as_select())
            .load(c)
            .and_then(convert_all)
    })
}

/// Year/month counts for the media calendar archive widget.
pub fn media_calendar_counts(
    conn: &DbConnection,
    project_id: &str,
) -> QueryResult<Vec<(i32, u32, usize)>> {
    conn.with(|c| {
        let timestamps = media::table
            .filter(media::project_id.eq(project_id))
            .select(media::created_at)
            .load::<i64>(c)?;
        let mut counts = std::collections::BTreeMap::new();
        for timestamp in timestamps {
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

/// Collect all distinct tag values across media for a project.
pub fn distinct_media_tags(conn: &DbConnection, project_id: &str) -> QueryResult<Vec<String>> {
    let rows = conn.with(|c| {
        media::table
            .filter(media::project_id.eq(project_id))
            .filter(media::tags.ne("[]"))
            .select(media::tags)
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

/// Test helper: create a minimal Media value (available to sibling test modules).
#[cfg(test)]
pub fn make_test_media(id: &str, project_id: &str) -> Media {
    Media {
        id: id.to_string(),
        project_id: project_id.to_string(),
        filename: format!("{id}.jpg"),
        original_name: "photo.jpg".to_string(),
        mime_type: "image/jpeg".to_string(),
        size: 50000,
        width: Some(1920),
        height: Some(1080),
        title: None,
        alt: None,
        caption: None,
        author: None,
        language: None,
        file_path: format!("/media/{id}.jpg"),
        sidecar_path: format!("/media/{id}.jpg.meta"),
        checksum: None,
        tags: vec![],
        created_at: 1000,
        updated_at: 2000,
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

    fn make_media_item(id: &str) -> Media {
        Media {
            id: id.to_string(),
            project_id: "p1".to_string(),
            filename: "abc.jpg".to_string(),
            original_name: "photo.jpg".to_string(),
            mime_type: "image/jpeg".to_string(),
            size: 50000,
            width: Some(1920),
            height: Some(1080),
            title: Some("Sunset".into()),
            alt: Some("A sunset".into()),
            caption: None,
            author: Some("Bob".into()),
            language: Some("en".into()),
            file_path: "/media/abc.jpg".to_string(),
            sidecar_path: "/media/abc.jpg.meta".to_string(),
            checksum: Some("hash1".into()),
            tags: vec!["nature".into()],
            created_at: 1000,
            updated_at: 2000,
        }
    }

    #[test]
    fn insert_and_get_by_id() {
        let db = setup();
        let m = make_media_item("m1");
        insert_media(db.conn(), &m).unwrap();
        let fetched = get_media_by_id(db.conn(), "m1").unwrap();
        assert_eq!(fetched.original_name, "photo.jpg");
        assert_eq!(fetched.width, Some(1920));
        assert_eq!(fetched.height, Some(1080));
        assert_eq!(fetched.tags, vec!["nature"]);
        assert_eq!(fetched.size, 50000);
    }

    #[test]
    fn list_by_project() {
        let db = setup();
        let mut m1 = make_media_item("m1");
        m1.created_at = 1000;
        let mut m2 = make_media_item("m2");
        m2.filename = "def.jpg".into();
        m2.created_at = 2000;
        insert_media(db.conn(), &m1).unwrap();
        insert_media(db.conn(), &m2).unwrap();
        let list = list_media_by_project(db.conn(), "p1").unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].id, "m2");
    }

    #[test]
    fn update_media_fields() {
        let db = setup();
        let mut m = make_media_item("m1");
        insert_media(db.conn(), &m).unwrap();
        m.title = Some("New Title".into());
        m.tags = vec!["updated".into()];
        m.updated_at = 9999;
        update_media(db.conn(), &m).unwrap();
        let fetched = get_media_by_id(db.conn(), "m1").unwrap();
        assert_eq!(fetched.title.as_deref(), Some("New Title"));
        assert_eq!(fetched.tags, vec!["updated"]);
    }

    #[test]
    fn delete_removes_media() {
        let db = setup();
        insert_media(db.conn(), &make_media_item("m1")).unwrap();
        delete_media(db.conn(), "m1").unwrap();
        assert!(get_media_by_id(db.conn(), "m1").is_err());
    }
}
